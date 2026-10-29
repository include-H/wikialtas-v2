//! 工具实现与工具面（按阶段给不同的手，`docs/design.md §2` · `docs/tools.md`）。
//!
//! 三段的工具面**故意不同**：研究段有检索但没有写正文；生成段有写正文但没有检索
//! ——这是从结构上消灭「只查不写」。工具 schema 由这里手写、由 `tool_names` 测试守一致性
//! （`tools.md §0`：schema 与实现的漂移要靠测试钉住）。

use crate::guard::{self, DupCallGuard, Verdict};
use crate::ledger::{Ledger, Source, Status};

use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain::Medium;
use wikiatlas_store::content;

/// 整篇写（保留给「修订 / 小任务」；生成段默认走 `patch_section` 按章写）。
pub const WRITE_TOOL: &str = "write_content";
/// 按章写（生成段的唯一写工具）。
pub const PATCH_SECTION_TOOL: &str = "patch_section";
/// **整篇撰写 wiki**（研究→骨架→按章生成，落草稿）——通用 harness 里模型按需调用的写作工具（D71）。
pub const WRITE_WIKI_TOOL: &str = "write_wiki";
/// 读技能正文（L0 目录常驻，正文按需读，`prompts.md §4`）。
pub const READ_SKILL_TOOL: &str = "read_skill";
/// 读账本正文（生成段把账本当上下文；账本本身也常驻一段，这个工具用于取最新）。
pub const READ_LEDGER_TOOL: &str = "read_ledger";
/// 联网检索（研究段）。
pub const WEB_SEARCH_TOOL: &str = "web_search";
/// 打开一页读正文（研究段）。
pub const WEB_FETCH_TOOL: &str = "web_fetch";
/// 记一条事实账本（研究段唯一的"写"）。
pub const LEDGER_PUT_TOOL: &str = "ledger_put";

/// 研究段工具面。
pub fn research_tool_defs() -> Value {
    json!([
        def(READ_SKILL_TOOL,
            "读一个技能（写作规范）的正文。先用目录里的 name；section 可选，只取那一节。\
             做研究前至少读 fact-check 与该介质那一份。",
            json!({
                "name": { "type": "string", "description": "技能名，如 fact-check" },
                "section": { "type": "string", "description": "只取某节（可选）" }
            }),
            &["name"]),
        def(WEB_SEARCH_TOOL,
            "联网检索。优先官方 / 专业数据库 / 权威媒体；社区与自媒体只能当线索。\
             结果里的 title/url/snippet 就是来源，**别凭记忆下断言**。",
            json!({
                "query": { "type": "string", "description": "检索词（中/英都可）" },
                "num": { "type": "integer", "description": "最终融合结果数，默认 20，上限 50；provider 会自动拉更大的候选池" },
                "providers": { "type": "array", "items": { "type": "string", "enum": ["exa","brave","deepseek"] }, "description": "可选：指定本轮 provider；默认聚合全部已配置 provider" }
            }),
            &["query"]),
        def(WEB_FETCH_TOOL,
            "打开一个网页读正文（纯文本）。用于把检索结果里的一页读全，核对日期 / 版本 / 剧情。\
             超出 12k 字会被截断并明确标注。",
            json!({ "url": { "type": "string", "description": "要读的页面 URL" } }),
            &["url"]),
        def(LEDGER_PUT_TOOL,
            "把一条事实记进事实账本（**每查到一条就记一条**，不要攒到最后）。\
             状态只能是 已核实 / 冲突 / 待核实；查不到就记 待核实，绝不编造。",
            json!({
                "field": { "type": "string", "description": "字段名，如 首发日期 / 导演 / 主要角色·克劳德" },
                "claim": { "type": "string", "description": "断言内容（纯值，不含判断句）" },
                "status": { "type": "string", "enum": ["已核实", "冲突", "待核实"], "description": "缺省按有无来源判断" },
                "source_title": { "type": "string", "description": "来源标题 / 站点名（可空）" },
                "source_url": { "type": "string", "description": "来源 URL（可空）" },
                "quote": { "type": "string", "description": "原文摘录（≤300 字，可空）" }
            }),
            &["field", "claim"]),
    ])
}

/// 生成段工具面：**没有检索工具**，只有按章写 + 只读。
pub fn generation_tool_defs() -> Value {
    json!([
        def(READ_SKILL_TOOL,
            "读一个技能（写作规范）的正文。写作前至少读 wiki-write / wiki-skeleton / 该介质那一份。",
            json!({
                "name": { "type": "string", "description": "技能名，如 wiki-skeleton" },
                "section": { "type": "string", "description": "只取某节（可选）" }
            }),
            &["name"]),
        def(READ_LEDGER_TOOL,
            "读这篇的事实账本正文（事实只能来自账本）。",
            json!({}),
            &[]),
        def(PATCH_SECTION_TOOL,
            "写**一章**：锚点就是这一章（或文档头）的标题行，`new_markdown` 必须以锚点开头、只含这一章。\
             宿主按骨架顺序请你逐章写；不要试图一次写完整篇，也不要只把正文写在回复文字里。",
            json!({
                "node_id": { "type": "string", "description": "目标节点 id" },
                "anchor": { "type": "string", "description": "本章标题行，如 `## 4. 剧情概要`（文档头是 `# …`）" },
                "new_markdown": { "type": "string", "description": "这一章的完整 markdown（含标题行）" }
            }),
            &["node_id", "anchor", "new_markdown"]),
    ])
}

/// `write_wiki`：整篇撰写当前节点（研究 → 骨架 → 按章生成，只落草稿）。
/// 通用 harness 里这是模型**按需调用**的写作工具（`design.md §2` 的三段式收成一个工具，D71）。
pub fn write_wiki_def() -> Value {
    def(
        WRITE_WIKI_TOOL,
        "为**当前这个作品节点**整篇撰写 wiki 正文（内部会先研究、再定骨架、再按章写，最后把正文落到草稿）。\
         只在用户明确要你「写 / 补全 / 重写这篇 wiki」时调用。\
         只是问问题、查资料、看看库里有什么 → 用读工具 + answer，**不要**调它。\
         只要改一两句 → 也别用它整篇重写（修订路径随后补）。",
        json!({
            "node_id": { "type": "string", "description": "目标作品节点 id（就是当前节点；可省略）" }
        }),
        &[],
    )
}

pub(crate) fn def(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "function",
        "name": name,
        "description": description,
        "parameters": { "type": "object", "properties": properties, "required": required }
    })
}

/// 所有工具名（一致性测试用：工具面里出现的名字必须都有实现）。
pub fn known_tool_names() -> Vec<&'static str> {
    let mut names = vec![
        READ_SKILL_TOOL,
        READ_LEDGER_TOOL,
        WEB_SEARCH_TOOL,
        WEB_FETCH_TOOL,
        LEDGER_PUT_TOOL,
        PATCH_SECTION_TOOL,
        WRITE_TOOL,
        WRITE_WIKI_TOOL,
    ];
    names.extend(crate::read_tools::read_tool_names());
    names.extend(crate::harness_tools::names());
    names
}

/// `read_skill`：从技能根读正文（找不到就明确说没这个技能，别编）。
pub fn call_read_skill(arguments: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct Args {
        name: String,
        #[serde(default)]
        section: String,
    }
    let args: Args = match serde_json::from_str(arguments) {
        Ok(a) => a,
        Err(e) => return json!({ "ok": false, "reason": format!("参数不是合法 JSON：{e}") }),
    };
    let section = (!args.section.is_empty()).then(|| args.section.as_str());
    match crate::prompt::read_skill(&crate::prompt::skills_root(), &args.name, section) {
        Some(text) => json!({ "ok": true, "name": args.name, "text": text }),
        None => json!({ "ok": false, "reason": format!("没有这个技能：{}", args.name) }),
    }
}

/// `read_ledger`：把当前账本正文还给模型。
pub fn call_read_ledger(ledger: &Ledger) -> Value {
    json!({ "ok": true, "markdown": ledger.render() })
}

/// `ledger_put`：记一条事实（合并到内存账本；落库由调用方按节流做）。
pub fn call_ledger_put(arguments: &str, medium: Medium, ledger: &mut Ledger) -> Value {
    #[derive(serde::Deserialize)]
    struct Args {
        field: String,
        claim: String,
        #[serde(default)]
        status: String,
        #[serde(default)]
        source_title: String,
        #[serde(default)]
        source_url: String,
        #[serde(default)]
        quote: String,
    }
    let args: Args = match serde_json::from_str(arguments) {
        Ok(a) => a,
        Err(e) => return json!({ "ok": false, "reason": format!("参数不是合法 JSON：{e}") }),
    };
    if args.field.trim().is_empty() {
        return json!({ "ok": false, "reason": "field 不能为空" });
    }
    if args.claim.trim().is_empty() {
        return json!({ "ok": false, "reason": "claim 不能为空" });
    }
    // 来源：有 URL 就挂；同一 URL 复用同一个编号
    let mut sources = vec![];
    if !args.source_url.trim().is_empty() {
        let id = match ledger.sources.iter().find(|s| s.url.trim() == args.source_url.trim()) {
            Some(s) => s.id.clone(),
            None => {
                let id = next_source_id(ledger);
                ledger.put_source(Source {
                    id: id.clone(),
                    title: args.source_title.trim().to_string(),
                    url: args.source_url.trim().to_string(),
                    fetched: today(),
                    quote: args.quote.trim().to_string(),
                });
                id
            }
        };
        sources.push(id);
    }
    let status = Status::parse(&args.status).unwrap_or(if sources.is_empty() {
        Status::Pending
    } else {
        Status::Verified
    });
    let no = ledger.put(&args.field, &args.claim, sources, status);
    let cov = ledger.coverage(medium);
    json!({
        "ok": true, "no": no, "status": status.as_str(),
        "coverage": (cov.ratio() * 100.0).round() as u32,
        "missing": cov.missing,
    })
}

/// 下一条来源编号（`S1`, `S2`…）。
fn next_source_id(ledger: &Ledger) -> String {
    let max = ledger
        .sources
        .iter()
        .filter_map(|s| s.id.strip_prefix('S'))
        .filter_map(|n| n.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("S{}", max + 1)
}

/// 当前日期（UTC，`YYYY-MM-DD`）——账本的「抓取时间」。
fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

/// `web_fetch`：抓一页 → 抽纯文本 → 截断并明说。
pub async fn call_web_fetch(arguments: &str) -> Value {
    crate::websearch::fetch(arguments).await
}

/// `web_search`：走 Exa（`.secrets/credentials.md` 里那个 key）。
/// 没配 key 就明确说"检索不可用"，**不让模型假装查过**。
pub async fn call_web_search(arguments: &str) -> Value {
    crate::websearch::search(arguments).await
}

/// 整篇写工具（修订 / 兜底用）。生成段默认不用它。
pub async fn call_write(
    pool: &PgPool,
    node_id: Uuid,
    base_ver: i32,
    arguments: &str,
    status: &wikiatlas_domain::NodeStatus,
    old_body: &str,
    run_id: &str,
    dup: &mut DupCallGuard,
) -> Value {
    #[derive(serde::Deserialize)]
    struct Args {
        #[serde(default)]
        node_id: String,
        content_md: String,
        #[serde(default)]
        summary: String,
    }
    if let Verdict::Reject(reason) = dup.check(WRITE_TOOL, arguments) {
        return json!({ "ok": false, "reason": reason });
    }
    let args: Args = match guard::parse_args(arguments) {
        Ok(a) => a,
        Err(Verdict::Reject(r)) => return json!({ "ok": false, "reason": r }),
        Err(Verdict::Allow) => unreachable!(),
    };
    if !args.node_id.is_empty() && args.node_id != node_id.to_string() {
        return json!({ "ok": false, "reason": "只允许写当前工单的目标节点" });
    }
    if let Verdict::Reject(reason) = guard::check_write(*status, old_body, &args.content_md) {
        return json!({ "ok": false, "reason": reason });
    }
    let by = format!("llm:{run_id}");
    match content::put_draft(pool, node_id, &args.content_md, base_ver, &by).await {
        Ok(d) => json!({ "ok": true, "wrote": true, "base_ver": d.base_ver, "summary": args.summary }),
        Err(e) => json!({ "ok": false, "reason": format!("落草稿失败：{e}") }),
    }
}

/// 极简 HTML → 纯文本（不引新依赖）：去 script/style，去标签，解几个常见实体，压空白。
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let lower = html.to_ascii_lowercase();
    let mut i = 0usize;
    let bytes = html.len();
    // 用字节游标走一遍（ASCII 标签；中文正文里的 '<' 极少，容错即可）
    while i < bytes {
        if lower[i..].starts_with("<script") {
            if let Some(end) = lower[i..].find("</script>") {
                i += end + 9;
                continue;
            } else {
                break;
            }
        }
        if lower[i..].starts_with("<style") {
            if let Some(end) = lower[i..].find("</style>") {
                i += end + 8;
                continue;
            } else {
                break;
            }
        }
        let c = html[i..].chars().next().unwrap_or(' ');
        if c == '<' {
            // 跳过整个标签
            if let Some(end) = html[i..].find('>') {
                // 块级标签补换行
                let tag = lower[i + 1..i + end].split_whitespace().next().unwrap_or("");
                let block = matches!(
                    tag.trim_start_matches('/'),
                    "p" | "div" | "br" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "section" | "article"
                );
                i += end + 1;
                if block {
                    out.push('\n');
                }
                continue;
            } else {
                break;
            }
        }
        out.push(c);
        i += c.len_utf8();
    }
    let decoded = decode_entities(&out);
    // 压空白：连续空行折一个，行首尾去空
    let mut text = String::new();
    let mut blank = false;
    for line in decoded.lines() {
        let t = line.trim();
        if t.is_empty() {
            if !blank && !text.is_empty() {
                text.push('\n');
            }
            blank = true;
        } else {
            text.push_str(t);
            text.push('\n');
            blank = false;
        }
    }
    text.trim().to_string()
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 工具面里的名字都有实现() {
        let known = known_tool_names();
        for defs in [research_tool_defs(), generation_tool_defs()] {
            for t in defs.as_array().unwrap() {
                let name = t.get("name").and_then(Value::as_str).unwrap();
                assert!(known.contains(&name), "工具 {name} 不在已知名单里");
            }
        }
    }

    #[test]
    fn 生成段没有检索工具() {
        let names: Vec<String> = generation_tool_defs()
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.get("name").unwrap().as_str().unwrap().to_string())
            .collect();
        assert!(!names.contains(&WEB_SEARCH_TOOL.to_string()), "生成段不许有检索");
        assert!(!names.contains(&WEB_FETCH_TOOL.to_string()));
        assert!(names.contains(&PATCH_SECTION_TOOL.to_string()));
    }

    #[test]
    fn ledger_put_记一条并给覆盖率() {
        let mut l = Ledger::default();
        let out = call_ledger_put(
            r#"{"field":"首发日期","claim":"2006-03-09，Windows","source_url":"https://x","source_title":"官方"}"#,
            Medium::Game,
            &mut l,
        );
        assert_eq!(out.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(out.get("no").and_then(Value::as_u64), Some(1));
        assert_eq!(out.get("status").and_then(Value::as_str), Some("已核实"));
        assert_eq!(l.sources.len(), 1);
        assert_eq!(l.entries[0].sources, vec!["S1"]);
    }

    #[test]
    fn ledger_put_同URL复用来源编号() {
        let mut l = Ledger::default();
        call_ledger_put(r#"{"field":"a","claim":"1","source_url":"https://x"}"#, Medium::Game, &mut l);
        call_ledger_put(r#"{"field":"b","claim":"2","source_url":"https://x"}"#, Medium::Game, &mut l);
        assert_eq!(l.sources.len(), 1, "同一 URL 复用来源");
        assert_eq!(l.entries[1].sources, vec!["S1"]);
    }

    #[test]
    fn ledger_put_无来源默认待核实() {
        let mut l = Ledger::default();
        let out = call_ledger_put(r#"{"field":"x","claim":"y"}"#, Medium::Game, &mut l);
        assert_eq!(out.get("status").and_then(Value::as_str), Some("待核实"));
    }

    #[test]
    fn 非法JSON_报错不猜() {
        let mut l = Ledger::default();
        let out = call_ledger_put("{坏}", Medium::Game, &mut l);
        assert_eq!(out.get("ok").and_then(Value::as_bool), Some(false));
    }

    #[test]
    fn html抽文本去标签去脚本() {
        let html = "<html><head><style>.a{}</style></head><body><h1>标题</h1><p>正文&nbsp;一</p><script>var x=1</script><p>第二段</p></body></html>";
        let t = html_to_text(html);
        assert!(t.contains("标题"));
        assert!(t.contains("正文 一"));
        assert!(t.contains("第二段"));
        assert!(!t.contains("var x"));
        assert!(!t.contains("<p>"));
    }

    #[tokio::test]
    async fn web_fetch_拒绝非http() {
        let out = call_web_fetch(r#"{"url":"ftp://x"}"#).await;
        assert_eq!(out.get("ok").and_then(Value::as_bool), Some(false));
    }
}

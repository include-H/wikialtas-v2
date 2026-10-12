//! 工具实现（除写正文外的那几个）。写正文在 `runtime::call_write`（它要落库 + 过护栏）。
//!
//! 从 `runtime.rs` 拆出来（单文件 ≤300 行，D41）。

use crate::guard::{self, DupCallGuard, Verdict};

use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_store::content;

/// 写正文工具（`docs/tools.md §2.1`）。
pub const WRITE_TOOL: &str = "write_content";
/// 读技能正文（`docs/prompts.md §4`：L0 目录常驻，正文按需读）。
pub const READ_SKILL_TOOL: &str = "read_skill";
/// 联网检索（`docs/prompts.md §2.3` 第 2 步：**必做**）。
pub const WEB_SEARCH_TOOL: &str = "web_search";


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


/// 工具面（schema 直接给 Responses API 的 `tools`）。
pub fn tool_defs() -> Value {
    json!([{
        "type": "function",
        "name": READ_SKILL_TOOL,
        "description": "读一个技能（写作规范）的正文。先用目录里的 name；section 可选，只取那一节。                        写正文前至少读 wiki-write / wiki-skeleton / 该介质那一份。",
        "parameters": {
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "技能名，如 wiki-skeleton" },
                "section": { "type": "string", "description": "只取某节（可选）" }
            },
            "required": ["name"]
        }
    }, {
        "type": "function",
        "name": WEB_SEARCH_TOOL,
        "description": "联网检索。写正文前必做：查首发日期与平台、版本关系、销量/评分、可指出处的台词。                        结果里的 title/url/snippet 就是你的事实账本，**别凭记忆下断言**。",
        "parameters": {
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "检索词（中/英都可）" },
                "num": { "type": "integer", "description": "要几条，默认 5，上限 10" }
            },
            "required": ["query"]
        }
    }, {
        "type": "function",
        "name": WRITE_TOOL,
        "description": "把整篇正文写入目标节点的草稿。改一句用 patch 类工具，别为一句重写整章；\
                        内容为空或明显被截断会被拒绝。",
        "parameters": {
            "type": "object",
            "properties": {
                "node_id": { "type": "string", "description": "目标节点 id" },
                "content_md": { "type": "string", "description": "完整 markdown 正文" },
                "summary": { "type": "string", "description": "这次写入的摘要（可选）" }
            },
            "required": ["node_id", "content_md"]
        }
    }])
}

/// 系统提示。**M3 先用最小版**：分段组装（`prompts.md §1` 的 section 表）随 M4 落，
/// 现在只保身份 + 写工具的边界（`behavior-notes.md §0`：一个事实只有一个 owner）。
pub fn system_prompt() -> String {
    "你是 WikiAtlas 的写作助手，服务于一个中文 ACG 资料库。\n\
     **写正文只有一个渠道：调用 write_content 工具。** 正文必须放在工具的 content_md 参数里，\n\
     用 markdown（## 编号章节）。不要只把正文写在回复文字里——那样等于什么都没写。\n\
     不要用空内容调用写工具；调用失败就按失败原因改，别重复同样的调用。\n\
     写完可以再用一两句话说明你写了什么。"
        .to_string()
}

/// 工具调用：解析参数 → 护栏 → 落草稿。
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

/// `web_search`：走 Exa（`.secrets/credentials.md` 里那个 key）。
/// 没配 key 就明确说"检索不可用"，**不让模型假装查过**。
pub async fn call_web_search(arguments: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct Args {
        query: String,
        #[serde(default)]
        num: Option<u32>,
    }
    let args: Args = match serde_json::from_str(arguments) {
        Ok(a) => a,
        Err(e) => return json!({ "ok": false, "reason": format!("参数不是合法 JSON：{e}") }),
    };
    let key = match exa_key().await {
        Some(k) => k,
        None => return json!({ "ok": false, "reason": "检索不可用：没配 exa.api_key" }),
    };
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(40)).build() {
        Ok(c) => c,
        Err(e) => return json!({ "ok": false, "reason": format!("HTTP 客户端建不起来：{e}") }),
    };
    let body = json!({
        "query": args.query,
        "numResults": args.num.unwrap_or(5).min(10),
        "contents": { "text": { "maxCharacters": 1500 } }
    });
    let resp = client
        .post("https://api.exa.ai/search")
        .header("x-api-key", key)
        .json(&body)
        .send()
        .await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => return json!({ "ok": false, "reason": format!("检索失败：{e}") }),
    };
    let code = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !code.is_success() {
        let msg: String = text.chars().take(160).collect();
        return json!({ "ok": false, "reason": format!("检索 HTTP {code}：{msg}") });
    }
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let results: Vec<Value> = v
        .get("results")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|r| {
                    json!({
                        "title": r.get("title").cloned().unwrap_or(Value::Null),
                        "url": r.get("url").cloned().unwrap_or(Value::Null),
                        "snippet": r.get("text").and_then(Value::as_str).unwrap_or("").chars().take(700).collect::<String>(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    json!({ "ok": true, "count": results.len(), "results": results })
}

/// Exa key：secrets 表优先（`exa.api_key`），环境变量兜底。**值绝不外泄**。
pub async fn exa_key() -> Option<String> {
    if let Ok(k) = std::env::var("WIKIATLAS_EXA_API_KEY") {
        if !k.trim().is_empty() {
            return Some(k);
        }
    }
    let url = std::env::var("WIKIATLAS_DB_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()?;
    let pool = wikiatlas_store::connect(&url).await.ok()?;
    wikiatlas_store::secrets::get_secret(&pool, "exa.api_key")
        .await
        .ok()
        .flatten()
        .filter(|v| !v.trim().is_empty())
}

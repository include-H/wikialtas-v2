//! 工具实现（除写正文外的那几个）。写正文在 `runtime::call_write`（它要落库 + 过护栏）。
//!
//! 从 `runtime.rs` 拆出来（单文件 ≤300 行，D41）。

use crate::guard::{self, DupCallGuard, Verdict};
use crate::Result;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_store::content;

/// 写正文工具（`docs/tools.md §2.1`）。
pub const WRITE_TOOL: &str = "write_content";
/// 读技能正文（`docs/prompts.md §4`：L0 目录常驻，正文按需读）。
pub const READ_SKILL_TOOL: &str = "read_skill";


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

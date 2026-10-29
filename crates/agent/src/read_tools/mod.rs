//! 读工具面 + 控制工具（`docs/tools.md §1.1 §1.3`）。
//!
//! 通用 harness 里这一面**总是可用**：模型自己决定是查库、答问题还是动手写
//! （写动作 `write_wiki` 在 `tools.rs`；能不能做由 `policy` 在动作层判）。
//! `answer` 交付回答并带可点引用（D19：问答产物不直接改正文，要留就存成资料）。
//!
//! 拆成两个文件（D41：单文件 ≤300 行）：本文件放**工具面契约**（schema / 派发 / 名单），
//! `calls` 放**各工具实现**。工具怎么用写进 `description`（`behavior-notes.md §0`）。

mod calls;

use crate::runtime::common::ToolCall;
use crate::tools::{def, READ_LEDGER_TOOL, READ_SKILL_TOOL};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

pub const GET_TREE_TOOL: &str = "get_tree";
pub const LIST_WORKS_TOOL: &str = "list_works";
pub const READ_NODE_TOOL: &str = "read_node";
pub const READ_DOC_TOOL: &str = "read_doc";
pub const SEARCH_WORKS_TOOL: &str = "search_works";
pub const ANSWER_TOOL: &str = "answer";
pub const FINISH_TOOL: &str = "finish";

/// 读工具名（一致性测试用）。
pub fn read_tool_names() -> Vec<&'static str> {
    vec![
        GET_TREE_TOOL,
        LIST_WORKS_TOOL,
        READ_NODE_TOOL,
        READ_DOC_TOOL,
        SEARCH_WORKS_TOOL,
        READ_SKILL_TOOL,
        READ_LEDGER_TOOL,
        ANSWER_TOOL,
        FINISH_TOOL,
    ]
}

/// 读 + 控制工具面（`answer` / `finish` 也在内）。写工具（`write_wiki`）在 `tools.rs`。
pub fn read_tool_defs() -> Value {
    json!([
        def(
            READ_SKILL_TOOL,
            "读一个技能的正文（写作/资料规范）。查资料规范（fact-check）或某介质的写法时用；纯问答一般不必读。",
            json!({
                "name": { "type": "string", "description": "技能名，如 fact-check / media-game" },
                "section": { "type": "string", "description": "只取某节（可选）" }
            }),
            &["name"]
        ),
        def(
            GET_TREE_TOOL,
            "取库里的节点树（id / 标题 / 种类 / 介质 / 状态 / 父节点）。用来找「某个词对应哪个节点」「它旁边有什么」。",
            json!({
                "root_id": { "type": "string", "description": "只取这个节点的子树（可选；不给就整棵树）" }
            }),
            &[]
        ),
        def(
            LIST_WORKS_TOOL,
            "列作品节点（可按介质 / 状态过滤）。想知道「库里有哪些作品」时用。",
            json!({
                "medium": { "type": "string", "description": "game / movie / tv / manga / book（可选）" },
                "status": { "type": "string", "description": "draft / ready（可选）" },
                "limit": { "type": "integer", "description": "最多几条，默认 30，上限 100" }
            }),
            &[]
        ),
        def(
            SEARCH_WORKS_TOOL,
            "按标题 / 别名 / 正文检索节点（归一化 + 模糊 + bigram）。用户提一个名字或事实，先用它定位节点与原文片段。",
            json!({
                "query": { "type": "string", "description": "要查的名字 / 关键词" },
                "limit": { "type": "integer", "description": "最多几条，默认 10，上限 50" }
            }),
            &["query"]
        ),
        def(
            READ_NODE_TOOL,
            "读一个作品的元数据 + **定稿正文**。正文超 12k 字会截断并明说（`truncated`/`notice`）。",
            json!({
                "node_id": { "type": "string", "description": "节点 id（先用 search_works / get_tree 拿到）" }
            }),
            &["node_id"]
        ),
        def(
            READ_DOC_TOOL,
            "读一篇**资料**（doc 节点）的正文，如事实账本 / 角色页 / 时间线。要引具体事实时用。",
            json!({ "node_id": { "type": "string", "description": "资料节点 id" } }),
            &["node_id"]
        ),
        def(
            READ_LEDGER_TOOL,
            "读**当前作品**的事实账本（研究时记下的可核实事实 + 来源）。答具体事实（日期 / 销量 / 角色）时优先查它。",
            json!({}),
            &[]
        ),
        def(
            ANSWER_TOOL,
            "**交付回答（最后一件事）**。`text` 是给用户看的回答（中文、简洁、可分点）；`refs` 列出你引用到的节点 id，用户能点回去。答完就停，别再调其他工具。",
            json!({
                "text": { "type": "string", "description": "回答正文（markdown）" },
                "refs": {
                    "type": "array",
                    "description": "引用到的节点（可空）",
                    "items": {
                        "type": "object",
                        "properties": { "node_id": { "type": "string" } },
                        "required": ["node_id"]
                    }
                }
            }),
            &["text"]
        ),
        def(FINISH_TOOL, "没有更多可说的就收尾（没东西可答时用）。", json!({}), &[]),
    ])
}

/// 跑一个读 / 控制工具。`target` 是当前作品节点（`read_ledger` 用）。
pub async fn dispatch(pool: &PgPool, target: Uuid, call: &ToolCall) -> Value {
    match call.name.as_str() {
        READ_SKILL_TOOL => crate::tools::call_read_skill(&call.arguments),
        GET_TREE_TOOL => calls::get_tree(pool, &call.arguments).await,
        LIST_WORKS_TOOL => calls::list_works(pool, &call.arguments).await,
        SEARCH_WORKS_TOOL => calls::search(pool, &call.arguments).await,
        READ_NODE_TOOL => calls::read_node(pool, &call.arguments, false).await,
        READ_DOC_TOOL => calls::read_node(pool, &call.arguments, true).await,
        READ_LEDGER_TOOL => calls::read_ledger(pool, target).await,
        ANSWER_TOOL => calls::answer(pool, &call.arguments).await,
        FINISH_TOOL => json!({ "ok": true }),
        other => json!({ "ok": false, "reason": format!("问答没有这个工具：{other}") }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 读工具面没有写工具() {
        let names: Vec<String> = read_tool_defs()
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.get("name").unwrap().as_str().unwrap().to_string())
            .collect();
        for banned in [
            crate::tools::WRITE_TOOL,
            crate::tools::PATCH_SECTION_TOOL,
            crate::tools::LEDGER_PUT_TOOL,
            crate::tools::WRITE_WIKI_TOOL,
        ] {
            assert!(!names.contains(&banned.to_string()), "读工具面不该有写工具 {banned}");
        }
        assert!(names.contains(&ANSWER_TOOL.to_string()));
        assert!(names.contains(&READ_NODE_TOOL.to_string()));
    }

    #[test]
    fn 工具面里的名字都在名单里() {
        let known = read_tool_names();
        for t in read_tool_defs().as_array().unwrap() {
            let name = t.get("name").and_then(Value::as_str).unwrap();
            assert!(known.contains(&name), "工具 {name} 不在读工具名单里");
        }
    }
}

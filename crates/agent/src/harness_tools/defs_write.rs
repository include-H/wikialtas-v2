//! 通用 Harness 的写工具 schema。每个 description 都说清边界与失败条件。

use crate::tools::def;
use serde_json::{json, Value};

pub fn defs() -> Value {
    json!([
        def(
            "upsert_work",
            "创建或修改节点元数据。新建时 title 必填；修改时传 id。parent_id 用于移动，move_to_root=true 才移到根。只改用户明确要求的字段，别顺手重排整棵树。",
            json!({
                "id": { "type": "string", "description": "修改已有节点时传" },
                "parent_id": { "type": "string" },
                "move_to_root": { "type": "boolean" },
                "kind": { "type": "string", "enum": ["universe","series","work","doc"] },
                "medium": { "type": ["string","null"], "enum": ["game","movie","tv","manga","book","other",null] },
                "title": { "type": "string" },
                "aliases": { "type": "array", "items": { "type": "string" } },
                "tags": { "type": "array", "items": { "type": "string" } },
                "status": { "type": "string", "enum": ["draft","ready","archived"] },
                "visibility": { "type": "string", "enum": ["private","public"] }
            }),
            &[],
        ),
        def(
            "write_content",
            "用完整 markdown 覆盖目标节点的草稿。用于用户明确要求整篇重写或修订小任务；空写和“原文很长却突然只剩很短”都会被拦。绝不直接定稿。",
            json!({
                "node_id": { "type": "string" },
                "content_md": { "type": "string" },
                "summary": { "type": "string" }
            }),
            &["content_md"],
        ),
        def(
            "patch_section",
            "按唯一章节锚点替换一章。anchor 必须唯一命中；new_markdown 必须以 anchor 开头。只改这一章，不整篇重写。",
            json!({
                "node_id": { "type": "string" },
                "anchor": { "type": "string" },
                "new_markdown": { "type": "string" }
            }),
            &["anchor","new_markdown"],
        ),
        def(
            "patch_sections",
            "批量替换章节。先预检所有锚点，再一次性落草稿；任一锚点不唯一或缺失则全部拒绝。单次最多 20 节。",
            json!({
                "node_id": { "type": "string" },
                "sections": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "anchor": { "type": "string" },
                            "new_markdown": { "type": "string" }
                        },
                        "required": ["anchor","new_markdown"]
                    }
                }
            }),
            &["sections"],
        ),
        def(
            "edit",
            "按字面串做局部替换。old_string 必须唯一出现；适合改一句、修一个错字、改题记中的一处。找不到或多处命中都拒绝。",
            json!({
                "node_id": { "type": "string" },
                "old_string": { "type": "string" },
                "new_string": { "type": "string" }
            }),
            &["old_string","new_string"],
        ),
        def(
            "upsert_relation",
            "建立一条类型化有向关系。type 必须来自 relation_types；同类型成环、自指和重复关系会拒绝。",
            json!({
                "from_id": { "type": "string" },
                "to_id": { "type": "string" },
                "type": { "type": "string", "description": "如 sequel_of / adaptation_of / references" }
            }),
            &["from_id","to_id","type"],
        ),
        def(
            "upsert_relations",
            "批量建立关系。先预检全部节点、类型与环，一个事务全成或全不成；单次最多 50 条。",
            json!({
                "relations": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "from_id": { "type": "string" },
                            "to_id": { "type": "string" },
                            "type": { "type": "string" }
                        },
                        "required": ["from_id","to_id","type"]
                    }
                }
            }),
            &["relations"],
        ),
        def(
            "detach_relation",
            "删除一条关系边。只删这条边，不删两端节点。",
            json!({ "relation_id": { "type": "string" } }),
            &["relation_id"],
        ),
        def(
            "attach_library_link",
            "把清单条目挂到已有节点。身份是 source+external_id；已挂过或正片位冲突会拒绝。",
            json!({
                "node_id": { "type": "string" },
                "source": { "type": "string", "enum": ["gameatlas","emby","komga"] },
                "external_id": { "type": "string" },
                "role": { "type": "string", "enum": ["work","mix"] }
            }),
            &["node_id","source","external_id"],
        ),
        def(
            "detach_link",
            "解除一条媒体链。文档、正文、关系和资料都保留；文档会回到池子的“无文档”。",
            json!({ "link_id": { "type": "string" } }),
            &["link_id"],
        ),
        def(
            "relink",
            "把一条 active/stale 链换到同源另一条当前清单条目。旧链保留为 replaced，新链 active，文档不动。",
            json!({
                "link_id": { "type": "string" },
                "external_id": { "type": "string" }
            }),
            &["link_id","external_id"],
        ),
        def(
            "rebuild_doc",
            "从媒体库条目按落位规则重建文档并挂链。用于用户明确说“重新建档”；已有文档优先复用。",
            json!({
                "source": { "type": "string", "enum": ["gameatlas","emby","komga"] },
                "external_id": { "type": "string" },
                "role": { "type": "string", "enum": ["work","mix"] },
                "parent_id": { "type": "string" }
            }),
            &["source","external_id"],
        ),
        def(
            "create_doc",
            "在节点下创建资料。带 doc_type + work_ids 时创建跨作品综合页并自动引用这些作品；否则创建普通资料。",
            json!({
                "parent_id": { "type": "string" },
                "title": { "type": "string" },
                "doc_type": { "type": "string", "enum": ["role","worldbuilding","timeline","theme","creator"] },
                "work_ids": { "type": "array", "items": { "type": "string" } },
                "tags": { "type": "array", "items": { "type": "string" } }
            }),
            &["title"],
        ),
        def(
            "archive_node",
            "归档节点（默认删除策略）。归档后所有写工具都会拒绝；只有用户明确要求归档时调用。",
            json!({ "node_id": { "type": "string" } }),
            &["node_id"],
        ),
    ])
}

pub fn control_defs() -> Value {
    json!([
        def(
            "todo_write",
            "整份提交当前任务清单。多步任务先列清单，执行中更新状态；它不是写库动作，不参与重复调用拦截。",
            json!({
                "todos": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "text": { "type": "string" },
                            "status": { "type": "string", "enum": ["pending","in_progress","completed"] }
                        },
                        "required": ["text","status"]
                    }
                }
            }),
            &["todos"],
        ),
        def(
            "answer",
            "交付最终回答。text 要给用户看；refs 只放真正读过的节点 id，用户可点击回源。答完停止。",
            json!({
                "text": { "type": "string" },
                "refs": { "type": "array", "items": { "type": "object", "properties": { "node_id": { "type": "string" } }, "required": ["node_id"] } }
            }),
            &["text"],
        ),
        def(
            "ask_user",
            "只有缺少会改变执行结果的必要信息时追问。问题要具体，说明为什么要问；不要用它确认已经明确的事。",
            json!({ "question": { "type": "string" } }),
            &["question"],
        ),
        def("finish", "任务已完成且没有新的用户可见内容时收尾。", json!({}), &[]),
    ])
}

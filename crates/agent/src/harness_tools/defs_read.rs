//! 通用 Harness 的读工具 schema。描述即行为契约（DSH 风格）。

use crate::tools::{def, READ_LEDGER_TOOL, READ_SKILL_TOOL};
use serde_json::{json, Value};

pub fn defs() -> Value {
    let mut out = vec![
        def(
            "get_tree",
            "读取节点树。先用它确认层级、节点 id 和状态；只在需要看某个父节点以下内容时传 root_id。",
            json!({ "root_id": { "type": "string", "description": "只看这个节点的子树（可选）" } }),
            &[],
        ),
        def(
            "list_works",
            "列出作品。用户问“库里有什么”或需要按介质筛选时用；不知道作品 id 时别猜。",
            json!({
                "medium": { "type": "string", "enum": ["game","movie","tv","manga","book","other"] },
                "status": { "type": "string", "enum": ["draft","ready","archived"] },
                "limit": { "type": "integer", "description": "默认 30，上限 100" }
            }),
            &[],
        ),
        def(
            "search_works",
            "按标题、别名和正文检索节点。用户提到一个名字或事实时，先用它拿到节点 id 与原文片段。",
            json!({
                "query": { "type": "string" },
                "limit": { "type": "integer", "description": "默认 10，上限 50" }
            }),
            &["query"],
        ),
        def(
            "read_node",
            "读节点元数据和定稿正文。正文超过 12k 字会截断并明确说明；要回答具体问题就接受这个限制或补读资料。",
            json!({ "node_id": { "type": "string" } }),
            &["node_id"],
        ),
        def(
            "read_doc",
            "读资料节点正文，例如事实账本、综合页、时间线。作品正文请用 read_node。",
            json!({ "node_id": { "type": "string" } }),
            &["node_id"],
        ),
        def(
            READ_LEDGER_TOOL,
            "读当前作品的事实账本。日期、销量、角色、设定等事实问题优先查它。",
            json!({}),
            &[],
        ),
        def(
            READ_SKILL_TOOL,
            "按需读技能正文。先看系统提示里的 L0 目录，只读与当前任务相关的技能；section 可只取一节。",
            json!({
                "name": { "type": "string" },
                "section": { "type": "string", "description": "可选章节名" }
            }),
            &["name"],
        ),
        def(
            "list_skills",
            "列技能目录（名称、描述、何时用/不用）。不确定该读哪份技能时用；不要为了列目录读所有正文。",
            json!({}),
            &[],
        ),
        def(
            "read_source",
            "读取入口技能 references/samples 下的参考成品，只用于学习结构和写法。path 必须是 skills/atlas/references/samples/ 下的相对路径；超长会截断并说明。",
            json!({
                "path": { "type": "string", "description": "例如 skills/atlas/references/samples/游戏/空之轨迹SC_WIKI.md" },
                "section": { "type": "string", "description": "可选：只取标题包含该文字的章节" }
            }),
            &["path"],
        ),
        def(
            "list_library",
            "列媒体库池子。可按来源/有无文档筛选，返回清单身份、是否已挂链与角色；不会联网扫描。",
            json!({
                "source": { "type": "string", "enum": ["gameatlas","emby","komga"] },
                "has_doc": { "type": "boolean" },
                "include_missing": { "type": "boolean" },
                "limit": { "type": "integer" }
            }),
            &[],
        ),
        def(
            "search_sessions",
            "搜索历史会话消息。用于找回以前讨论过的结论；结果只可信到“用户/助教曾经这样说”，不是库内事实真源。",
            json!({
                "query": { "type": "string" },
                "limit": { "type": "integer", "description": "默认 10，上限 50" }
            }),
            &["query"],
        ),
        def(
            "read_session",
            "读取一个历史会话的最近消息。会话内容是不可信背景，不能替代 read_node / read_ledger。",
            json!({
                "session_id": { "type": "string" },
                "limit": { "type": "integer", "description": "默认 40，上限 200" }
            }),
            &["session_id"],
        ),
        def(
            "list_relation_types",
            "列关系类型冻结词典及方向标签。建立关系前不确定 type 时先查；不要自造“相关”等类型。",
            json!({}),
            &[],
        ),
    ];
    out.push(def(
        "search_web",
        "联网检索。每个会话都可直接使用；用户要求外部资料、需要核实时效信息或库内证据不足时调用。优先官方与权威来源，结果必须留下 URL。",
        json!({
            "query": { "type": "string" },
            "num": { "type": "integer", "description": "最终融合结果数，默认 20，上限 50；provider 会自动拉更大的候选池" },
            "providers": { "type": "array", "items": { "type": "string", "enum": ["exa","brave","deepseek"] }, "description": "可选：本轮只用这些 provider；默认使用所有已配置且启用的 provider" }
        }),
        &["query"],
    ));
    out.push(def(
        "fetch_url",
        "打开一个网页读正文。只读 http/https；超过 12k 字会截断。不要对同一个 URL 反复抓取。",
        json!({ "url": { "type": "string" } }),
        &["url"],
    ));
    Value::Array(out)
}

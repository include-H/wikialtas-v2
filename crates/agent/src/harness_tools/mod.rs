//! 通用 Harness 的工具面：读 + 控制 + 写。schema 与 dispatch 同处一个模块，
//! `policy` 再做动作层二次校验（D14 / D71）。

mod defs_read;
mod defs_write;
mod read_ops;
pub(crate) mod relation_ops;
mod write_ops;

use crate::runtime::RunContext;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

pub(crate) fn defs(ctx: &RunContext) -> Value {
    let mut out = Vec::new();
    push_all(&mut out, defs_read::defs());
    push_all(&mut out, defs_write::control_defs());
    if ctx.can_write() {
        push_all(&mut out, defs_write::defs());
        out.push(crate::tools::write_wiki_def());
    }
    Value::Array(out)
}

fn push_all(out: &mut Vec<Value>, value: Value) {
    if let Some(rows) = value.as_array() {
        out.extend(rows.iter().cloned());
    }
}

pub(crate) async fn dispatch(
    pool: &PgPool,
    target: Uuid,
    name: &str,
    args: &str,
    run_id: &str,
) -> Option<Value> {
    if is_base(name) {
        let call = crate::runtime::common::ToolCall {
            name: name.into(),
            call_id: String::new(),
            arguments: args.into(),
        };
        return Some(crate::read_tools::dispatch(pool, target, &call).await);
    }
    if let Some(v) = read_ops::dispatch(pool, name, args).await {
        return Some(v);
    }
    if let Some(v) = relation_ops::dispatch(pool, name, args, run_id).await {
        return Some(v);
    }
    write_ops::dispatch(pool, target, name, args, run_id).await
}

pub(crate) fn is_base(name: &str) -> bool {
    crate::read_tools::read_tool_names().contains(&name)
}

/// 动作层需要剥掉的写工具（含整篇写作）。
pub(crate) fn is_write(name: &str) -> bool {
    matches!(
        name,
        "write_wiki"
            | "upsert_work"
            | "write_content"
            | "patch_section"
            | "patch_sections"
            | "edit"
            | "upsert_relation"
            | "upsert_relations"
            | "detach_relation"
            | "attach_library_link"
            | "detach_link"
            | "relink"
            | "rebuild_doc"
            | "create_doc"
            | "archive_node"
    )
}

pub(crate) fn names() -> Vec<&'static str> {
    vec![
        "upsert_work",
        "write_content",
        "patch_section",
        "patch_sections",
        "edit",
        "upsert_relation",
        "upsert_relations",
        "detach_relation",
        "attach_library_link",
        "detach_link",
        "relink",
        "rebuild_doc",
        "create_doc",
        "archive_node",
        "todo_write",
        "ask_user",
        "list_library",
        "list_skills",
        "list_relation_types",
        "search_sessions",
        "search_web",
        "fetch_url",
        "read_session",
        "read_source",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 读模式真的没有写工具() {
        let ctx = RunContext { doc_mode: "read".into(), ..Default::default() };
        let names: Vec<String> = defs(&ctx)
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t.get("name").and_then(Value::as_str).map(str::to_string))
            .collect();
        assert!(names.contains(&"answer".to_string()));
        assert!(names.contains(&"todo_write".to_string()));
        assert!(!names.iter().any(|n| is_write(n)));
    }

    #[test]
    fn 校对模式保留写工具() {
        let ctx = RunContext {
            doc_mode: "proofread".into(),
            ..Default::default()
        };
        let names: Vec<String> = defs(&ctx)
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t.get("name").and_then(Value::as_str).map(str::to_string))
            .collect();
        assert!(names.iter().any(|n| n == "edit"));
        assert!(names.iter().any(|n| n == "patch_section"));
    }

    #[test]
    fn 编辑模式工具清单包含关键写工具() {
        let ctx = RunContext::default();
        let names: Vec<String> = defs(&ctx)
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t.get("name").and_then(Value::as_str).map(str::to_string))
            .collect();
        for n in ["upsert_work", "edit", "upsert_relations", "relink", "create_doc", "write_wiki"] {
            assert!(names.contains(&n.to_string()), "缺 {n}");
        }
    }

    #[test]
    fn 工具清单与已知实现一致且无重复() {
        let ctx = RunContext::default();
        let defs = defs(&ctx);
        let mut seen = std::collections::HashSet::new();
        let known = crate::tools::known_tool_names();
        for t in defs.as_array().unwrap() {
            let name = t.get("name").and_then(Value::as_str).unwrap();
            assert!(known.contains(&name), "工具 {name} 没有实现");
            assert!(seen.insert(name.to_string()), "工具 {name} 重复");
        }
        assert!(seen.contains("search_web"));
        assert!(seen.contains("fetch_url"));
    }
}

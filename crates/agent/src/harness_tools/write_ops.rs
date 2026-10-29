//! 通用 Harness 的节点、正文与媒体链写工具。

use crate::guard::{self, Verdict};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain as domain;
use wikiatlas_store::{content, library, nodes, relations, sync};

fn bad(reason: impl Into<String>) -> Value {
    json!({ "ok": false, "reason": reason.into() })
}

fn args<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T, Value> {
    guard::parse_args(raw).map_err(|v| bad(v.reason().unwrap_or("参数错误")))
}

pub async fn dispatch(
    pool: &PgPool,
    target: Uuid,
    name: &str,
    raw: &str,
    run_id: &str,
) -> Option<Value> {
    let out = match name {
        "upsert_work" => upsert_work(pool, raw).await,
        "write_content" => write_content(pool, target, raw, run_id).await,
        "patch_section" => patch_section(pool, target, raw, run_id).await,
        "patch_sections" => patch_sections(pool, target, raw, run_id).await,
        "edit" => edit(pool, target, raw, run_id).await,
        "create_doc" => create_doc(pool, raw).await,
        "archive_node" => archive_node(pool, raw).await,
        "attach_library_link" => attach_link(pool, raw).await,
        "detach_link" => detach_link(pool, raw).await,
        "relink" => relink(pool, raw).await,
        "rebuild_doc" => rebuild_doc(pool, raw).await,
        "todo_write" => todo(raw),
        _ => return None,
    };
    Some(out)
}

async fn upsert_work(pool: &PgPool, raw: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct A {
        #[serde(default)]
        id: Option<Uuid>,
        #[serde(default)]
        parent_id: Option<Uuid>,
        #[serde(default)]
        move_to_root: bool,
        #[serde(default)]
        kind: Option<String>,
        #[serde(default)]
        medium: Option<Option<String>>,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        aliases: Option<Vec<String>>,
        #[serde(default)]
        tags: Option<Vec<String>>,
        #[serde(default)]
        status: Option<String>,
        #[serde(default)]
        visibility: Option<String>,
    }
    let a: A = match args(raw) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let kind = match a.kind.as_deref().map(domain::NodeKind::parse).transpose() {
        Ok(v) => v,
        Err(e) => return bad(e.to_string()),
    };
    let medium = match a
        .medium
        .map(|v| v.map(|s| domain::Medium::parse(&s)).transpose())
        .transpose()
    {
        Ok(v) => v,
        Err(e) => return bad(e.to_string()),
    };
    let result = if let Some(id) = a.id {
        let patch = nodes::NodePatch {
            parent_id: if a.move_to_root {
                Some(None)
            } else {
                a.parent_id.map(Some)
            },
            kind,
            medium,
            title: a.title,
            aliases: a.aliases,
            tags: a.tags,
            status: match a.status.as_deref().map(domain::NodeStatus::parse).transpose() {
                Ok(v) => v,
                Err(e) => return bad(e.to_string()),
            },
            visibility: match a
                .visibility
                .as_deref()
                .map(domain::Visibility::parse)
                .transpose()
            {
                Ok(v) => v,
                Err(e) => return bad(e.to_string()),
            },
        };
        nodes::update(pool, id, &patch).await
    } else {
        let Some(title) = a.title else {
            return bad("新建节点必须给 title");
        };
        nodes::create(
            pool,
            &domain::NewNode {
                parent_id: a.parent_id,
                kind: kind.unwrap_or(domain::NodeKind::Work),
                medium: medium.unwrap_or(None),
                title,
                aliases: a.aliases.unwrap_or_default(),
                tags: a.tags.unwrap_or_default(),
            },
        )
        .await
    };
    match result {
        Ok(n) => json!({ "ok": true, "node": n }),
        Err(e) => bad(format!("节点没写成：{e}")),
    }
}

async fn write_content(pool: &PgPool, target: Uuid, raw: &str, run_id: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct A {
        #[serde(default)]
        node_id: Option<Uuid>,
        content_md: String,
        #[serde(default)]
        summary: String,
    }
    let a: A = match args(raw) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let id = match check_target(target, a.node_id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    save_whole(pool, id, &a.content_md, run_id, &a.summary).await
}

async fn patch_section(pool: &PgPool, target: Uuid, raw: &str, run_id: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct A {
        #[serde(default)]
        node_id: Option<Uuid>,
        anchor: String,
        new_markdown: String,
    }
    let a: A = match args(raw) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let id = match check_target(target, a.node_id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let (_, base_ver, old) = match crate::runtime::common::node_base(pool, id).await {
        Ok(v) => v,
        Err(e) => return bad(e.to_string()),
    };
    let body = match crate::splice::apply(&old, &a.anchor, &a.new_markdown) {
        Ok(v) => v,
        Err(e) => return bad(format!("章节替换失败：{e}")),
    };
    save_body(pool, id, &body, base_ver, run_id, "patch_section").await
}

async fn patch_sections(pool: &PgPool, target: Uuid, raw: &str, run_id: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct Item {
        anchor: String,
        new_markdown: String,
    }
    #[derive(serde::Deserialize)]
    struct A {
        #[serde(default)]
        node_id: Option<Uuid>,
        sections: Vec<Item>,
    }
    let a: A = match args(raw) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if a.sections.is_empty() || a.sections.len() > 20 {
        return bad("patch_sections 一次要 1–20 节");
    }
    let id = match check_target(target, a.node_id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let (_, base_ver, mut body) = match crate::runtime::common::node_base(pool, id).await {
        Ok(v) => v,
        Err(e) => return bad(e.to_string()),
    };
    for item in &a.sections {
        body = match crate::splice::apply(&body, &item.anchor, &item.new_markdown) {
            Ok(v) => v,
            Err(e) => return bad(format!("预检失败，整批未写入：{e}")),
        };
    }
    save_body(pool, id, &body, base_ver, run_id, "patch_sections").await
}

async fn edit(pool: &PgPool, target: Uuid, raw: &str, run_id: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct A {
        #[serde(default)]
        node_id: Option<Uuid>,
        old_string: String,
        new_string: String,
    }
    let a: A = match args(raw) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if a.old_string.is_empty() {
        return bad("old_string 不能为空");
    }
    let id = match check_target(target, a.node_id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let (_, base_ver, old) = match crate::runtime::common::node_base(pool, id).await {
        Ok(v) => v,
        Err(e) => return bad(e.to_string()),
    };
    let n = old.matches(&a.old_string).count();
    if n != 1 {
        return bad(format!("old_string 必须唯一命中，实际命中 {n} 次"));
    }
    let body = old.replacen(&a.old_string, &a.new_string, 1);
    save_body(pool, id, &body, base_ver, run_id, "edit").await
}

async fn save_whole(pool: &PgPool, id: Uuid, body: &str, run_id: &str, summary: &str) -> Value {
    let (node, base_ver, old) = match crate::runtime::common::node_base(pool, id).await {
        Ok(v) => v,
        Err(e) => return bad(e.to_string()),
    };
    if let Verdict::Reject(r) = guard::check_write(node.status, &old, body) {
        return bad(r);
    }
    save_body(pool, id, body, base_ver, run_id, summary).await
}

async fn save_body(
    pool: &PgPool,
    id: Uuid,
    body: &str,
    base_ver: i32,
    run_id: &str,
    old_or_summary: &str,
) -> Value {
    let node = match nodes::get(pool, id).await {
        Ok(v) => v,
        Err(e) => return bad(e.to_string()),
    };
    let (_, _, old) = crate::runtime::common::node_base(pool, id)
        .await
        .unwrap_or((node.clone(), 0, String::new()));
    if let Verdict::Reject(r) = guard::check_write(node.status, &old, body) {
        return bad(r);
    }
    match content::put_draft(pool, id, body, base_ver, &format!("llm:{run_id}")).await {
        Ok(d) => json!({
            "ok": true, "wrote": true, "node_id": id,
            "base_ver": d.base_ver, "chars": body.chars().count(),
            "summary": old_or_summary
        }),
        Err(e) => bad(format!("落草稿失败：{e}")),
    }
}

async fn create_doc(pool: &PgPool, raw: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct A {
        #[serde(default)]
        parent_id: Option<Uuid>,
        title: String,
        #[serde(default)]
        doc_type: Option<String>,
        #[serde(default)]
        work_ids: Vec<Uuid>,
        #[serde(default)]
        tags: Vec<String>,
    }
    let a: A = match args(raw) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Some(kind) = a.doc_type.as_deref() {
        if !a.work_ids.is_empty() {
            return match relations::create_synthesis(
                pool,
                a.parent_id,
                &a.title,
                kind,
                &a.work_ids,
            )
            .await
            {
                Ok(n) => json!({ "ok": true, "node": n }),
                Err(e) => bad(format!("综合页没建成：{e}")),
            };
        }
    }
    match nodes::create(
        pool,
        &domain::NewNode {
            parent_id: a.parent_id,
            kind: domain::NodeKind::Doc,
            medium: None,
            title: a.title,
            aliases: vec![],
            tags: a.tags,
        },
    )
    .await
    {
        Ok(n) => json!({ "ok": true, "node": n }),
        Err(e) => bad(format!("资料没建成：{e}")),
    }
}

async fn archive_node(pool: &PgPool, raw: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct A {
        node_id: Uuid,
    }
    let a: A = match args(raw) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match nodes::archive(pool, a.node_id).await {
        Ok(n) => json!({ "ok": true, "node": n }),
        Err(e) => bad(format!("归档失败：{e}")),
    }
}

async fn attach_link(pool: &PgPool, raw: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct A {
        node_id: Uuid,
        source: String,
        external_id: String,
        #[serde(default)]
        role: Option<String>,
    }
    let a: A = match args(raw) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let source = match domain::library::Source::parse(&a.source) {
        Ok(v) => v,
        Err(e) => return bad(e.to_string()),
    };
    let role = match a
        .role
        .as_deref()
        .map(domain::library::LinkRole::parse)
        .transpose()
    {
        Ok(v) => v.unwrap_or(domain::library::LinkRole::Work),
        Err(e) => return bad(e.to_string()),
    };
    match library::attach_existing(
        pool,
        a.node_id,
        source,
        &a.external_id,
        role,
        domain::library::LinkedBy::Manual,
    )
    .await
    {
        Ok(id) => json!({ "ok": true, "link_id": id }),
        Err(e) => bad(format!("挂链失败：{e}")),
    }
}

async fn detach_link(pool: &PgPool, raw: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct A {
        link_id: Uuid,
    }
    let a: A = match args(raw) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match library::unlink(pool, a.link_id).await {
        Ok(node_id) => json!({ "ok": true, "node_id": node_id }),
        Err(e) => bad(format!("解链失败：{e}")),
    }
}

async fn relink(pool: &PgPool, raw: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct A {
        link_id: Uuid,
        external_id: String,
    }
    let a: A = match args(raw) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match sync::relink(
        pool,
        a.link_id,
        &a.external_id,
        domain::library::LinkedBy::Manual,
    )
    .await
    {
        Ok(id) => json!({ "ok": true, "link_id": id }),
        Err(e) => bad(format!("重挂失败：{e}")),
    }
}

async fn rebuild_doc(pool: &PgPool, raw: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct A {
        source: String,
        external_id: String,
        #[serde(default)]
        role: Option<String>,
        #[serde(default)]
        parent_id: Option<Uuid>,
    }
    let a: A = match args(raw) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let source = match domain::library::Source::parse(&a.source) {
        Ok(v) => v,
        Err(e) => return bad(e.to_string()),
    };
    let role = match a
        .role
        .as_deref()
        .map(domain::library::LinkRole::parse)
        .transpose()
    {
        Ok(v) => v,
        Err(e) => return bad(e.to_string()),
    };
    match library::build_doc(
        pool,
        &library::BuildDoc {
            source,
            external_id: a.external_id,
            role,
            parent_id: a.parent_id,
            linked_by: domain::library::LinkedBy::Manual,
        },
    )
    .await
    {
        Ok(v) => json!({ "ok": true, "node": v.node, "link_id": v.link_id, "reused": v.reused }),
        Err(e) => bad(format!("重建档失败：{e}")),
    }
}

fn todo(raw: &str) -> Value {
    let a: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(e) => return bad(format!("参数不是合法 JSON：{e}")),
    };
    json!({ "ok": true, "todos": a.get("todos").cloned().unwrap_or_else(|| json!([])) })
}

fn check_target(current: Uuid, wanted: Option<Uuid>) -> Result<Uuid, Value> {
    match wanted {
        Some(id) if id != current => Err(bad("正文写工具只能写当前工单的目标节点")),
        _ => Ok(current),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "../store/migrations")]
    async fn 元数据编辑章节关系工具真跑(pool: PgPool) {
        let a = nodes::create(
            &pool,
            &domain::NewNode {
                parent_id: None,
                kind: domain::NodeKind::Work,
                medium: Some(domain::Medium::Game),
                title: "工具测试 A".into(),
                aliases: vec![],
                tags: vec![],
            },
        )
        .await
        .unwrap();
        let b = nodes::create(
            &pool,
            &domain::NewNode {
                parent_id: None,
                kind: domain::NodeKind::Work,
                medium: Some(domain::Medium::Game),
                title: "工具测试 B".into(),
                aliases: vec![],
                tags: vec![],
            },
        )
        .await
        .unwrap();
        content::commit(
            &pool,
            a.id,
            "# 工具测试 A\n\n## 1. 概览\n\n旧句子。",
            0,
            "",
            "human",
        )
        .await
        .unwrap();

        let edited = edit(
            &pool,
            a.id,
            r#"{"old_string":"旧句子。","new_string":"新句子。"}"#,
            "run:test",
        )
        .await;
        assert_eq!(edited["ok"], true);
        assert_eq!(
            content::read(&pool, a.id)
                .await
                .unwrap()
                .draft
                .unwrap()
                .body_md,
            "# 工具测试 A\n\n## 1. 概览\n\n新句子。\n"
        );

        let rel = crate::harness_tools::relation_ops::dispatch(
            &pool,
            "upsert_relation",
            &format!(
                r#"{{"from_id":"{}","to_id":"{}","type":"sequel_of"}}"#,
                b.id, a.id
            ),
            "run:test",
        )
        .await
        .unwrap();
        assert_eq!(rel["ok"], true);

        let renamed = upsert_work(
            &pool,
            &format!(r#"{{"id":"{}","title":"工具测试 A（改）"}}"#, a.id),
        )
        .await;
        assert_eq!(renamed["node"]["title"], "工具测试 A（改）");
    }
}

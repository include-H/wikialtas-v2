//! 系列 / 宇宙的宿主索引页（从 `relations.rs` 拆出，D41）。

use crate::{nodes, Error, Result};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain as domain;
use domain::{NewNode, Node, NodeKind};

#[derive(sqlx::FromRow)]
struct Child {
    id: Uuid,
    kind: String,
    medium: Option<String>,
    title: String,
    status: String,
    tags: Vec<String>,
}

pub async fn sync_index(pool: &PgPool, parent_id: Uuid) -> Result<Node> {
    let parent = nodes::get(pool, parent_id).await?;
    if !matches!(parent.kind, NodeKind::Series | NodeKind::Universe) {
        return Err(Error::Domain(domain::Error::Validation(
            "索引页只给系列或宇宙节点生成".into(),
        )));
    }
    let mut tx = pool.begin().await?;
    let existing: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM nodes WHERE parent_id = $1 AND kind = 'doc' \
           AND tags @> ARRAY['宿主索引']::text[] LIMIT 1",
    )
    .bind(parent_id)
    .fetch_optional(&mut *tx)
    .await?;
    let index_node = if let Some((id,)) = existing {
        nodes::get_in_tx(&mut tx, id).await?
    } else {
        nodes::create_in_tx(
            &mut tx,
            &NewNode {
                parent_id: Some(parent_id),
                kind: NodeKind::Doc,
                medium: None,
                title: "索引".into(),
                aliases: vec![],
                tags: vec!["宿主索引".into()],
            },
        )
        .await?
    };

    let children: Vec<Child> = sqlx::query_as(
        "SELECT id, kind, medium, title, status, tags FROM nodes \
         WHERE parent_id = $1 AND id <> $2 ORDER BY kind, sort_key",
    )
    .bind(parent_id)
    .bind(index_node.id)
    .fetch_all(&mut *tx)
    .await?;
    let body = render(&parent.title, &children);
    let ver: (i32,) = sqlx::query_as(
        "INSERT INTO contents (node_id, body_md, ver, updated_by, updated_at) \
         VALUES ($1, $2, 1, 'host', now()) \
         ON CONFLICT (node_id) DO UPDATE SET \
           body_md = EXCLUDED.body_md, ver = contents.ver + 1, \
           updated_by = 'host', updated_at = now() RETURNING ver",
    )
    .bind(index_node.id)
    .bind(&body)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO write_events (node_id, ver, source) VALUES ($1, $2, 'host')")
        .bind(index_node.id)
        .bind(ver.0)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM doc_refs WHERE doc_id = $1")
        .bind(index_node.id)
        .execute(&mut *tx)
        .await?;
    for child in children.iter().filter(|c| c.kind == "work") {
        sqlx::query("INSERT INTO doc_refs (doc_id, work_id) VALUES ($1, $2)")
            .bind(index_node.id)
            .bind(child.id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(index_node)
}

fn render(parent_title: &str, children: &[Child]) -> String {
    let mut out = format!(
        "# 《{}》索引\n\n> 说明：本文由 Atlas 宿主确定性维护，新增或移动子节点后重新生成。\n\n",
        parent_title
    );
    out.push_str("## 作品\n\n");
    let works: Vec<_> = children.iter().filter(|c| c.kind == "work").collect();
    if works.is_empty() {
        out.push_str("- （暂无作品）\n");
    } else {
        for w in works {
            let medium = w.medium.as_deref().unwrap_or("other");
            out.push_str(&format!("- **{}** — {} · {}\n", w.title, medium, w.status));
        }
    }
    out.push_str("\n## 综合页\n\n");
    let docs: Vec<_> = children
        .iter()
        .filter(|c| c.kind == "doc" && c.tags.iter().any(|t| t == "综合"))
        .collect();
    if docs.is_empty() {
        out.push_str("- （暂无综合页）\n");
    } else {
        for d in docs {
            out.push_str(&format!("- {}\n", d.title));
        }
    }
    out
}

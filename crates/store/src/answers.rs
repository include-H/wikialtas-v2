//! 把好回答存成资料（D19）：doc + 正文 + 引用，一个事务；不改目标作品正文。

use crate::{nodes, Error, Result};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain as domain;
use domain::{NewNode, Node, NodeKind, WriteSource};

pub async fn save_as_doc(
    pool: &PgPool,
    parent_id: Option<Uuid>,
    title: &str,
    body_md: &str,
    ref_ids: &[Uuid],
) -> Result<Node> {
    let title = title.trim();
    let body = domain::text::normalize_markdown(body_md);
    if title.is_empty() || body.trim().is_empty() {
        return Err(Error::Domain(domain::Error::Validation(
            "标题和回答正文都不能为空".into(),
        )));
    }
    let mut refs = ref_ids.to_vec();
    refs.sort();
    refs.dedup();
    let mut tx = pool.begin().await?;
    if !refs.is_empty() {
        let found: (i64,) = sqlx::query_as("SELECT count(*) FROM nodes WHERE id = ANY($1)")
            .bind(&refs)
            .fetch_one(&mut *tx)
            .await?;
        if found.0 as usize != refs.len() {
            return Err(Error::Domain(domain::Error::Validation(
                "ref_ids 里有不存在的节点".into(),
            )));
        }
    }
    let node = nodes::create_in_tx(
        &mut tx,
        &NewNode {
            parent_id,
            kind: NodeKind::Doc,
            medium: None,
            title: title.to_string(),
            aliases: vec![],
            tags: vec!["问答".into(), "资料".into()],
        },
    )
    .await?;
    sqlx::query(
        "INSERT INTO contents (node_id, body_md, ver, updated_by) \
         VALUES ($1, $2, 1, 'host:answer')",
    )
    .bind(node.id)
    .bind(&body)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO content_versions \
           (id, node_id, ver, body_md, source, summary) \
         VALUES ($1, $2, 1, $3, $4, '问答存为资料')",
    )
    .bind(Uuid::now_v7())
    .bind(node.id)
    .bind(&body)
    .bind(WriteSource::Host.as_str())
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO write_events (node_id, ver, source) VALUES ($1, 1, 'host')")
        .bind(node.id)
        .execute(&mut *tx)
        .await?;
    for ref_id in refs {
        sqlx::query("INSERT INTO doc_refs (doc_id, work_id) VALUES ($1, $2)")
            .bind(node.id)
            .bind(ref_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO relations (id, from_id, to_id, type, created_by) \
             VALUES ($1, $2, $3, 'references', 'host') ON CONFLICT DO NOTHING",
        )
        .bind(Uuid::now_v7())
        .bind(node.id)
        .bind(ref_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(node)
}

//! 事实账本的落库（`docs/design.md §2.3` · D56）。
//!
//! 账本**不是新表**：它就是作品节点下的一个资料节点（`kind='doc'`、`tags=['账本']`、
//! 标题「事实账本」），正文就是一篇 Markdown（断言表 + 来源表）。人能在同一个编辑器里
//! 看它、改它；宿主（研究段）按 `ledger_put` 增量把行写进来。
//!
//! 为什么放在 `contents`：这样它天然进「草稿 / 定稿 / 快照」的既有路径，
//! 前端的文档页不用为它单开一条路。

use crate::{Error, Result};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain as domain;
use domain::{NewNode, NodeKind};

/// 找作品节点下的账本资料（`tags @> ['账本']`）。
pub async fn find(pool: &PgPool, work_id: Uuid) -> Result<Option<Uuid>> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM nodes WHERE parent_id = $1 AND tags @> ARRAY['账本']::text[] \
         ORDER BY created_at LIMIT 1",
    )
    .bind(work_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id,)| id))
}

/// 读账本正文（没有账本节点就返回 None）。给研究段判断「已有账本就不重查」。
pub async fn load(pool: &PgPool, work_id: Uuid) -> Result<Option<String>> {
    let Some(node_id) = find(pool, work_id).await? else {
        return Ok(None);
    };
    let row: Option<(String,)> = sqlx::query_as("SELECT body_md FROM contents WHERE node_id = $1")
        .bind(node_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(b,)| b))
}

/// 写账本正文：**没有账本节点就建一个**，然后 upsert `contents`。
///
/// 三件事一个事务（建节点 + 写正文 + 更新版本）——`docs/database.md §0` 的纪律。
/// 账本由宿主维护（`updated_by='host'`），不产生草稿租约（它是资料，不是人在写的正文）。
pub async fn save(pool: &PgPool, work_id: Uuid, body_md: &str) -> Result<Uuid> {
    let body_md = domain::text::normalize_markdown(body_md);
    let mut tx = pool.begin().await?;

    // 作品节点必须在（否则外键错误会变成 500，而不是 404）
    let exists: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM nodes WHERE id = $1")
        .bind(work_id)
        .fetch_optional(&mut *tx)
        .await?;
    if exists.is_none() {
        return Err(Error::Domain(domain::Error::NotFound(format!("节点 {work_id}"))));
    }

    let node_id = match find_in_tx(&mut tx, work_id).await? {
        Some(id) => id,
        None => {
            let node = crate::nodes::create_in_tx(
                &mut tx,
                &NewNode {
                    parent_id: Some(work_id),
                    kind: NodeKind::Doc,
                    medium: None,
                    title: "事实账本".into(),
                    aliases: vec![],
                    tags: vec!["账本".into()],
                },
            )
            .await?;
            node.id
        }
    };

    sqlx::query(
        "INSERT INTO contents (node_id, body_md, ver, updated_by, updated_at) \
         VALUES ($1, $2, 1, 'host', now()) \
         ON CONFLICT (node_id) DO UPDATE SET \
           body_md = EXCLUDED.body_md, ver = contents.ver + 1, \
           updated_by = 'host', updated_at = now()",
    )
    .bind(node_id)
    .bind(&body_md)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(node_id)
}

async fn find_in_tx(tx: &mut sqlx::PgConnection, work_id: Uuid) -> Result<Option<Uuid>> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM nodes WHERE parent_id = $1 AND tags @> ARRAY['账本']::text[] \
         ORDER BY created_at LIMIT 1",
    )
    .bind(work_id)
    .fetch_optional(&mut *tx)
    .await?;
    Ok(row.map(|(id,)| id))
}

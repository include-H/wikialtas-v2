//! 反哺状态、重挂候选与重挂事务（`docs/library-sync.md` §5 §6）。
//!
//! 这里不联网：worker 从 `sync_jobs_*` 取到正文后调用源客户端，再回写状态。

use crate::{Error, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain as domain;
use domain::library::Source;
use domain::sync::{SyncJob, SyncStatus, SyncTarget};

mod relink;
pub use relink::{relink, relink_candidates};

#[derive(sqlx::FromRow)]
struct JobRow {
    link_id: Uuid,
    node_id: Uuid,
    source: String,
    external_id: String,
    role: String,
    ver: i32,
    body_md: String,
}

fn to_job(r: JobRow) -> Result<SyncJob> {
    let source = Source::parse(&r.source)?;
    if !matches!(source, Source::GameAtlas | Source::Komga) {
        return Err(Error::Domain(domain::Error::Validation(format!(
            "{} 是只读源，不能反哺",
            source
        ))));
    }
    Ok(SyncJob {
        link_id: r.link_id,
        node_id: r.node_id,
        source,
        external_id: r.external_id,
        role: r.role,
        ver: r.ver,
        body_md: r.body_md,
    })
}

/// 定稿后自动反哺：该节点所有 active 且可写的链，取**当前最新定稿**。
pub async fn sync_jobs_for_node(pool: &PgPool, node_id: Uuid) -> Result<Vec<SyncJob>> {
    let rows: Vec<JobRow> = sqlx::query_as(
        "SELECT l.id AS link_id, l.node_id, l.source, l.external_id, l.role, c.ver, c.body_md \
         FROM library_links l \
         JOIN contents c ON c.node_id = l.node_id \
         WHERE l.node_id = $1 AND l.state = 'active' \
           AND l.source IN ('gameatlas','komga') AND c.ver > 0 \
           AND length(trim(c.body_md)) > 0 \
         ORDER BY l.created_at, l.id",
    )
    .bind(node_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(to_job).collect()
}

/// 手动「重新推送」：只处理指定链，仍要求 active 且有定稿正文。
pub async fn sync_job_for_link(pool: &PgPool, link_id: Uuid) -> Result<SyncJob> {
    let row: Option<JobRow> = sqlx::query_as(
        "SELECT l.id AS link_id, l.node_id, l.source, l.external_id, l.role, c.ver, c.body_md \
         FROM library_links l JOIN contents c ON c.node_id = l.node_id \
         WHERE l.id = $1 AND l.state = 'active' \
           AND l.source IN ('gameatlas','komga') AND c.ver > 0 \
           AND length(trim(c.body_md)) > 0",
    )
    .bind(link_id)
    .fetch_optional(pool)
    .await?;
    row.map(to_job)
        .transpose()?
        .ok_or_else(|| Error::Domain(domain::Error::NotFound(
            "这条链不可反哺（不存在、已失效、只读源或还没有定稿正文）".into(),
        )))
}

/// 每个可写目标的状态；还没跑过时合成一行 `pending`，让 UI 有稳定形状。
pub async fn targets_for_link(pool: &PgPool, link_id: Uuid) -> Result<Vec<SyncTarget>> {
    let source: Option<(String,)> = sqlx::query_as("SELECT source FROM library_links WHERE id = $1")
        .bind(link_id)
        .fetch_optional(pool)
        .await?;
    let source = source
        .map(|(s,)| Source::parse(&s))
        .transpose()?
        .ok_or_else(|| Error::Domain(domain::Error::NotFound(format!("链 {link_id}"))))?;
    if !matches!(source, Source::GameAtlas | Source::Komga) {
        return Ok(vec![]);
    }

    #[derive(sqlx::FromRow)]
    struct Row {
        link_id: Uuid,
        target: String,
        synced_ver: Option<i32>,
        status: String,
        message: String,
        synced_at: Option<DateTime<Utc>>,
    }
    let row: Option<Row> = sqlx::query_as(
        "SELECT link_id, target, synced_ver, status, message, synced_at \
         FROM sync_targets WHERE link_id = $1 AND target = $2",
    )
    .bind(link_id)
    .bind(source.as_str())
    .fetch_optional(pool)
    .await?;
    if let Some(r) = row {
        return Ok(vec![SyncTarget {
            link_id: r.link_id,
            target: Source::parse(&r.target)?,
            synced_ver: r.synced_ver,
            status: SyncStatus::parse(&r.status)?,
            message: r.message,
            synced_at: r.synced_at,
        }]);
    }
    Ok(vec![SyncTarget {
        link_id,
        target: source,
        synced_ver: None,
        status: SyncStatus::Pending,
        message: String::new(),
        synced_at: None,
    }])
}

pub async fn should_auto_sync(
    pool: &PgPool,
    link_id: Uuid,
    target: Source,
    ver: i32,
) -> Result<bool> {
    let row: Option<(Option<i32>, String)> = sqlx::query_as(
        "SELECT synced_ver, status FROM sync_targets WHERE link_id = $1 AND target = $2",
    )
    .bind(link_id)
    .bind(target.as_str())
    .fetch_optional(pool)
    .await?;
    Ok(!matches!(row, Some((Some(v), ref s)) if v == ver && s == "ok"))
}

pub async fn mark_pending(pool: &PgPool, link_id: Uuid, target: Source) -> Result<()> {
    sqlx::query(
        "INSERT INTO sync_targets (link_id, target, status, message) VALUES ($1, $2, 'pending', '') \
         ON CONFLICT (link_id, target) DO UPDATE SET status = 'pending', message = ''",
    )
    .bind(link_id)
    .bind(target.as_str())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_ok(pool: &PgPool, link_id: Uuid, target: Source, ver: i32) -> Result<()> {
    sqlx::query(
        "INSERT INTO sync_targets (link_id, target, synced_ver, status, message, synced_at) \
         VALUES ($1, $2, $3, 'ok', '', now()) \
         ON CONFLICT (link_id, target) DO UPDATE SET \
           synced_ver = EXCLUDED.synced_ver, status = 'ok', message = '', synced_at = now()",
    )
    .bind(link_id)
    .bind(target.as_str())
    .bind(ver)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_failed(pool: &PgPool, link_id: Uuid, target: Source, message: &str) -> Result<()> {
    let message: String = message.chars().take(500).collect();
    sqlx::query(
        "INSERT INTO sync_targets (link_id, target, status, message) VALUES ($1, $2, 'failed', $3) \
         ON CONFLICT (link_id, target) DO UPDATE SET status = 'failed', message = EXCLUDED.message",
    )
    .bind(link_id)
    .bind(target.as_str())
    .bind(message)
    .execute(pool)
    .await?;
    Ok(())
}

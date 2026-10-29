//! 池子（库状态仪表盘）与扫描记账（`scan_runs`）。
//!
//! 清单本身的读写 / 过滤条件在 `store::manifest`（单文件 ≤300 行，D41）；建档在 `store::link`。
//! **这里 re-export 那两块**，所以调用方路径 `wikiatlas_store::library::*` 一直不变。

use crate::manifest::ManifestDb;
use crate::Result;
use chrono::{DateTime, Utc};
use domain::library::{ContainerKind, LinkRole, ManifestRow, PoolEntry, ScanRun, Source};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain as domain;

pub use crate::link::{
    add_ignore, attach_existing, build_doc, list_ignores, remove_ignore, unlink, BuildDoc,
    BuiltDoc,
};
pub use crate::manifest::{
    list_manifest, upsert_manifest, upsert_manifest_for_source, ManifestFilter, ManifestStats,
};

const ALL_COLS: &str = "m.source, m.external_id, m.title, m.subtitle, m.aliases, m.year, \
                        m.kind, m.format, m.cover_url, m.url, m.container_key, m.container_title, \
                        m.container_kind, m.extra, m.content_hash, m.seen_at, m.first_seen_at, \
                        m.missing_since, l.id AS link_id, l.node_id, n.title AS node_title, \
                        n.kind AS node_kind";

/// 池子一行 = 清单条目 + 挂链（左连接，可能没有）。
#[derive(sqlx::FromRow)]
struct PoolDb {
    #[sqlx(flatten)]
    entry: ManifestDb,
    link_id: Option<Uuid>,
    node_id: Option<Uuid>,
    node_title: Option<String>,
    node_kind: Option<String>,
}

/// 读池子：清单条目 + 挂链状态。池子 = 库状态仪表盘（`docs/features.md` §2）。
pub async fn list_pool(pool: &PgPool, f: &ManifestFilter) -> Result<Vec<PoolEntry>> {
    let sql = format!(
        "SELECT {ALL_COLS} FROM library_manifest m \
         LEFT JOIN library_links l ON l.source = m.source AND l.external_id = m.external_id \
              AND l.state <> 'replaced' \
         LEFT JOIN nodes n ON n.id = l.node_id \
         WHERE ($1::text IS NULL OR m.source = $1) \
           AND ($2::text IS NULL OR m.container_key = $2) \
           AND ($3::bool IS NULL OR ($3 = (l.id IS NOT NULL))) \
           AND ($4::bool OR m.missing_since IS NULL) \
           AND NOT EXISTS (SELECT 1 FROM library_ignores i \
                           WHERE i.source = m.source \
                             AND (i.external_id = m.external_id \
                                  OR (i.group_key IS NOT NULL AND i.group_key = m.container_key))) \
         ORDER BY m.source, m.container_key, m.title_norm LIMIT $5"
    );
    let rows: Vec<PoolDb> = sqlx::query_as(&sql)
        .bind(f.source.map(|s| s.as_str().to_string()))
        .bind(f.container_key.clone())
        .bind(f.has_doc)
        .bind(f.include_missing)
        .bind(f.limit())
        .fetch_all(pool)
        .await?;
    rows.into_iter()
        .map(|r| {
            let entry = ManifestRow::try_from(r.entry)?;
            // 条目固有角色：Emby 混杂库 = mix，其余 = work（`docs/features.md` §2）。
            let role = if entry.source == Source::Emby && entry.container_kind == ContainerKind::Mixed
            {
                LinkRole::Mix
            } else {
                LinkRole::Work
            };
            Ok(PoolEntry {
                entry,
                link_id: r.link_id,
                node_id: r.node_id,
                node_title: r.node_title,
                node_kind: r.node_kind,
                role,
            })
        })
        .collect()
}

/// 开一次扫描记账，返回 run id。
pub async fn start_scan_run(pool: &PgPool, source: Source) -> Result<i64> {
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO scan_runs (source) VALUES ($1) RETURNING id",
    )
    .bind(source.as_str())
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// 结束一次扫描记账。
pub async fn finish_scan_run(
    pool: &PgPool,
    id: i64,
    ok: bool,
    stats: &serde_json::Value,
    message: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE scan_runs SET finished_at = now(), ok = $2, stats = $3, message = $4 WHERE id = $1",
    )
    .bind(id)
    .bind(ok)
    .bind(stats)
    .bind(message)
    .execute(pool)
    .await?;
    Ok(())
}

/// 最近的扫描记账（最新在前）。
pub async fn list_scan_runs(pool: &PgPool, limit: i64) -> Result<Vec<ScanRun>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: i64,
        source: String,
        started_at: DateTime<Utc>,
        finished_at: Option<DateTime<Utc>>,
        ok: Option<bool>,
        stats: serde_json::Value,
        message: String,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, source, started_at, finished_at, ok, stats, message \
         FROM scan_runs ORDER BY started_at DESC LIMIT $1",
    )
    .bind(limit.clamp(1, 200))
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(ScanRun {
                id: r.id,
                source: Source::parse(&r.source)?,
                started_at: r.started_at,
                finished_at: r.finished_at,
                ok: r.ok,
                stats: r.stats,
                message: r.message,
            })
        })
        .collect()
}

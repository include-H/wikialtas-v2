//! 媒体库仓储：清单（`library_manifest`）· 池子 · 扫描记账（`scan_runs`）。
//!
//! 纪律：**一次扫描的清单写入 = 一个事务**（`docs/database.md` §0.3）——
//! 新增 / 更新 / 标记消失要么都落，要么都不落。零 LLM（`docs/library-sync.md`）。
//!
//! 增量缓存：`content_hash` 未变的行只刷新 `seen_at` 并清 `missing_since`，
//! 不重写字段（借 llm_wiki 的 `ingest-cache`，`docs/library-sync.md` §3）。

use crate::{Error, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain as domain;
use domain::library::{ContainerKind, LinkRole, ManifestEntry, ManifestRow, PoolEntry, ScanRun, Source};

/// 一次扫描的写入统计（落 `scan_runs.stats`）。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ManifestStats {
    pub seen: usize,
    pub inserted: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub missing: u64,
}

/// 清单 / 池子的过滤条件。None = 不过滤。
#[derive(Debug, Clone, Default)]
pub struct ManifestFilter {
    pub source: Option<Source>,
    pub container_key: Option<String>,
    /// Some(true) = 只看有文档；Some(false) = 只看没文档。
    pub has_doc: Option<bool>,
    /// 是否包含已消失（`missing_since IS NOT NULL`）的条目。默认不含。
    pub include_missing: bool,
    pub limit: i64,
}

impl ManifestFilter {
    fn limit(&self) -> i64 {
        if self.limit <= 0 {
            2000
        } else {
            self.limit.min(5000)
        }
    }
}

const COLS: &str = "m.source, m.external_id, m.title, m.subtitle, m.aliases, m.year, \
                    m.kind, m.format, m.cover_url, m.url, m.container_key, m.container_title, \
                    m.container_kind, m.extra, m.content_hash, m.seen_at, m.first_seen_at, \
                    m.missing_since";

const ALL_COLS: &str = "m.source, m.external_id, m.title, m.subtitle, m.aliases, m.year, \
                        m.kind, m.format, m.cover_url, m.url, m.container_key, m.container_title, \
                        m.container_kind, m.extra, m.content_hash, m.seen_at, m.first_seen_at, \
                        m.missing_since, l.id AS link_id, l.node_id, n.title AS node_title, \
                        n.kind AS node_kind";

#[derive(sqlx::FromRow)]
struct ManifestDb {
    source: String,
    external_id: String,
    title: String,
    subtitle: Option<String>,
    aliases: Vec<String>,
    year: Option<i32>,
    kind: String,
    format: Option<String>,
    cover_url: Option<String>,
    url: Option<String>,
    container_key: String,
    container_title: String,
    container_kind: String,
    extra: String,
    content_hash: String,
    seen_at: DateTime<Utc>,
    first_seen_at: DateTime<Utc>,
    missing_since: Option<DateTime<Utc>>,
}

impl TryFrom<ManifestDb> for ManifestRow {
    type Error = Error;
    fn try_from(r: ManifestDb) -> Result<Self> {
        Ok(ManifestRow {
            source: Source::parse(&r.source)?,
            external_id: r.external_id,
            title: r.title,
            subtitle: r.subtitle,
            aliases: r.aliases,
            year: r.year,
            kind: r.kind,
            format: r.format,
            cover_url: r.cover_url,
            url: r.url,
            container_key: r.container_key,
            container_title: r.container_title,
            container_kind: ContainerKind::parse(&r.container_kind)?,
            extra: r.extra,
            content_hash: r.content_hash,
            seen_at: r.seen_at,
            first_seen_at: r.first_seen_at,
            missing_since: r.missing_since,
        })
    }
}

#[derive(sqlx::FromRow)]
struct PoolDb {
    #[sqlx(flatten)]
    entry: ManifestDb,
    link_id: Option<Uuid>,
    node_id: Option<Uuid>,
    node_title: Option<String>,
    node_kind: Option<String>,
}

/// 写入一次全量扫描的清单：upsert 见到的条目，标记本源的消失条目。
pub async fn upsert_manifest(pool: &PgPool, entries: &[ManifestEntry]) -> Result<ManifestStats> {
    let mut stats = ManifestStats {
        seen: entries.len(),
        ..Default::default()
    };
    let mut tx = pool.begin().await?;

    // 先把「本次扫描前这些源已有的指纹」读出来，才能分辨 新增 / 变更 / 未变。
    let sources: Vec<String> = {
        let mut v: Vec<String> = entries.iter().map(|e| e.source.as_str().into()).collect();
        v.sort();
        v.dedup();
        v
    };
    let mut prior: std::collections::HashMap<(String, String), String> =
        std::collections::HashMap::new();
    for src in &sources {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT external_id, content_hash FROM library_manifest WHERE source = $1",
        )
        .bind(src)
        .fetch_all(&mut *tx)
        .await?;
        for (ext, hash) in rows {
            prior.insert((src.clone(), ext), hash);
        }
    }

    for e in entries {
        let hash = e.content_hash();
        match prior.get(&(e.source.as_str().to_string(), e.external_id.clone())) {
            None => stats.inserted += 1,
            Some(old) if *old != hash => stats.changed += 1,
            Some(_) => stats.unchanged += 1,
        }
        sqlx::query(
            "INSERT INTO library_manifest \
               (source, external_id, title, title_norm, subtitle, aliases, year, kind, format, \
                cover_url, url, container_key, container_title, container_kind, extra, raw, \
                content_hash, seen_at, first_seen_at, missing_since) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17, now(), now(), NULL) \
             ON CONFLICT (source, external_id) DO UPDATE SET \
               title = EXCLUDED.title, title_norm = EXCLUDED.title_norm, subtitle = EXCLUDED.subtitle, \
               aliases = EXCLUDED.aliases, year = EXCLUDED.year, kind = EXCLUDED.kind, \
               format = EXCLUDED.format, cover_url = EXCLUDED.cover_url, url = EXCLUDED.url, \
               container_key = EXCLUDED.container_key, container_title = EXCLUDED.container_title, \
               container_kind = EXCLUDED.container_kind, extra = EXCLUDED.extra, raw = EXCLUDED.raw, \
               content_hash = EXCLUDED.content_hash, seen_at = now(), missing_since = NULL",
        )
        .bind(e.source.as_str())
        .bind(&e.external_id)
        .bind(&e.title)
        .bind(e.title_norm())
        .bind(&e.subtitle)
        .bind(&e.aliases)
        .bind(e.year)
        .bind(&e.kind)
        .bind(&e.format)
        .bind(&e.cover_url)
        .bind(&e.url)
        .bind(&e.container_key)
        .bind(&e.container_title)
        .bind(e.container_kind.as_str())
        .bind(&e.extra)
        .bind(&e.raw)
        .bind(&hash)
        .execute(&mut *tx)
        .await?;
    }

    // 本源的「消失」：还在清单里但这次没扫到 → 打 missing_since（不删行）。
    for src in &sources {
        let seen: Vec<String> = entries
            .iter()
            .filter(|e| e.source.as_str() == src)
            .map(|e| e.external_id.clone())
            .collect();
        let res = sqlx::query(
            "UPDATE library_manifest SET missing_since = now() \
             WHERE source = $1 AND missing_since IS NULL AND NOT (external_id = ANY($2))",
        )
        .bind(src)
        .bind(&seen)
        .execute(&mut *tx)
        .await?;
        stats.missing += res.rows_affected();
    }

    tx.commit().await?;
    Ok(stats)
}

/// 读清单（不含已消失的，除非显式要求）。
pub async fn list_manifest(pool: &PgPool, f: &ManifestFilter) -> Result<Vec<ManifestRow>> {
    let sql = format!(
        "SELECT {COLS} FROM library_manifest m \
         LEFT JOIN library_links l ON l.source = m.source AND l.external_id = m.external_id \
              AND l.state <> 'replaced' \
         WHERE ($1::text IS NULL OR m.source = $1) \
           AND ($2::text IS NULL OR m.container_key = $2) \
           AND ($3::bool IS NULL OR ($3 = (l.id IS NOT NULL))) \
           AND ($4::bool OR m.missing_since IS NULL) \
         ORDER BY m.source, m.container_key, m.title_norm LIMIT $5"
    );
    let rows: Vec<ManifestDb> = sqlx::query_as(&sql)
        .bind(f.source.map(|s| s.as_str().to_string()))
        .bind(f.container_key.clone())
        .bind(f.has_doc)
        .bind(f.include_missing)
        .bind(f.limit())
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(ManifestRow::try_from).collect()
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

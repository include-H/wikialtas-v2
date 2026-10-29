//! 清单仓储（`library_manifest`）：扫描落库 + 读清单。
//!
//! 纪律：**一次扫描的清单写入 = 一个事务**（`docs/database.md` §0.3）——
//! 新增 / 更新 / 标记消失要么都落，要么都不落。零 LLM（`docs/library-sync.md`）。
//!
//! 增量缓存：`content_hash` 未变的行只刷新 `seen_at` 并清 `missing_since`，
//! 不重写字段（借 llm_wiki 的 `ingest-cache`，`docs/library-sync.md` §3）。

use crate::{Error, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use wikiatlas_domain as domain;
use domain::library::{ContainerKind, ManifestEntry, ManifestRow, Source};

// M2 的建档 / 解链 / 忽略在 `store::link`（单文件 ≤300 行的纪律，D41）。
// 这里 re-export，调用方路径 `library::build_doc` 不变。
pub use crate::link::{add_ignore, build_doc, list_ignores, remove_ignore, unlink, BuildDoc, BuiltDoc};

/// 一次扫描的写入统计（落 `scan_runs.stats`）。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ManifestStats {
    pub seen: usize,
    pub inserted: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub missing: u64,
    /// 本次扫描新标成 stale 的链数。
    pub stale_links: u64,
    /// 消失后又出现、自动恢复 active 的链数。
    pub restored_links: u64,
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
    pub(crate) fn limit(&self) -> i64 {
        if self.limit <= 0 {
            2000
        } else {
            self.limit.min(5000)
        }
    }
}

pub(crate) const COLS: &str = "m.source, m.external_id, m.title, m.subtitle, m.aliases, m.year, \
                    m.kind, m.format, m.cover_url, m.url, m.container_key, m.container_title, \
                    m.container_kind, m.extra, m.content_hash, m.seen_at, m.first_seen_at, \
                    m.missing_since";

#[derive(sqlx::FromRow)]
pub(crate) struct ManifestDb {
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

/// 写入一次全量扫描的清单：upsert 见到的条目，标记本源的消失条目。
pub async fn upsert_manifest(pool: &PgPool, entries: &[ManifestEntry]) -> Result<ManifestStats> {
    upsert_manifest_inner(pool, entries, None).await
}

/// 单源扫描入口。**空清单也必须知道是哪个源**，否则「源清空」不会标 missing / stale。
pub async fn upsert_manifest_for_source(
    pool: &PgPool,
    source: Source,
    entries: &[ManifestEntry],
) -> Result<ManifestStats> {
    upsert_manifest_inner(pool, entries, Some(source)).await
}

async fn upsert_manifest_inner(
    pool: &PgPool,
    entries: &[ManifestEntry],
    forced_source: Option<Source>,
) -> Result<ManifestStats> {
    let mut stats = ManifestStats {
        seen: entries.len(),
        ..Default::default()
    };
    let mut tx = pool.begin().await?;

    // 先把「本次扫描前这些源已有的指纹」读出来，才能分辨 新增 / 变更 / 未变。
    let sources: Vec<String> = {
        let mut v: Vec<String> = entries.iter().map(|e| e.source.as_str().into()).collect();
        if let Some(source) = forced_source {
            v.push(source.as_str().into());
        }
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

        // 清单消失 → 链 stale；再次出现 → 链自动 active。文档与链行都不删。
        let stale = sqlx::query(
            "UPDATE library_links l SET state = 'stale', \
                    missing_since = COALESCE(l.missing_since, m.missing_since, now()) \
             FROM library_manifest m \
             WHERE l.source = m.source AND l.external_id = m.external_id \
               AND m.source = $1 AND m.missing_since IS NOT NULL AND l.state = 'active'",
        )
        .bind(src)
        .execute(&mut *tx)
        .await?;
        stats.stale_links += stale.rows_affected();

        let restored = sqlx::query(
            "UPDATE library_links l SET state = 'active', missing_since = NULL \
             FROM library_manifest m \
             WHERE l.source = m.source AND l.external_id = m.external_id \
               AND m.source = $1 AND m.missing_since IS NULL AND l.state = 'stale'",
        )
        .bind(src)
        .execute(&mut *tx)
        .await?;
        stats.restored_links += restored.rows_affected();
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

// 池子（`list_pool`）与扫描记账（`scan_runs`）在 `store::library`；
// 建档 / 解链 / 忽略在 `store::link`（单文件 ≤300 行，D41）。

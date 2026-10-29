//! 失效链候选与重挂事务（从 `sync.rs` 拆出，D41）。

use crate::{Error, Result};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain as domain;
use domain::library::{LinkedBy, Source};
use domain::sync::RelinkCandidate;

pub async fn relink_candidates(
    pool: &PgPool,
    link_id: Uuid,
) -> Result<Vec<RelinkCandidate>> {
    #[derive(sqlx::FromRow)]
    struct Old {
        source: String,
        title: String,
        aliases: Vec<String>,
    }
    #[derive(sqlx::FromRow)]
    struct Row {
        external_id: String,
        title: String,
        subtitle: Option<String>,
        aliases: Vec<String>,
        year: Option<i32>,
        cover_url: Option<String>,
        url: Option<String>,
        container_title: String,
    }
    let old: Old = sqlx::query_as(
        "SELECT l.source, COALESCE(m.title, l.title_hint, '') AS title, \
                COALESCE(m.aliases, '{}') AS aliases \
         FROM library_links l \
         LEFT JOIN library_manifest m ON m.source = l.source AND m.external_id = l.external_id \
         WHERE l.id = $1 AND l.state = 'stale'",
    )
    .bind(link_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        Error::Domain(domain::Error::NotFound(
            "只有失效链需要重挂候选".into(),
        ))
    })?;
    let source = Source::parse(&old.source)?;
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT m.external_id, m.title, m.subtitle, m.aliases, m.year, m.cover_url, m.url, \
                m.container_title FROM library_manifest m \
         WHERE m.source = $1 AND m.missing_since IS NULL \
           AND NOT EXISTS (SELECT 1 FROM library_links l \
                           WHERE l.source = m.source AND l.external_id = m.external_id) \
         ORDER BY m.title_norm, m.year NULLS LAST LIMIT 5000",
    )
    .bind(source.as_str())
    .fetch_all(pool)
    .await?;

    let mut out: Vec<RelinkCandidate> = rows
        .into_iter()
        .filter_map(|r| {
            let mut score = domain::text::trigram_similarity(&old.title, &r.title);
            for alias in &old.aliases {
                score = score.max(domain::text::trigram_similarity(alias, &r.title));
            }
            for alias in &r.aliases {
                score = score.max(domain::text::trigram_similarity(&old.title, alias));
            }
            (score >= 0.08).then_some(RelinkCandidate {
                source,
                external_id: r.external_id,
                title: r.title,
                subtitle: r.subtitle,
                aliases: r.aliases,
                year: r.year,
                cover_url: r.cover_url,
                url: r.url,
                container_title: r.container_title,
                score,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.title.cmp(&b.title))
    });
    out.truncate(20);
    Ok(out)
}

/// 新链 active，旧链 replaced + replaced_by；节点和正文完全不动。
pub async fn relink(
    pool: &PgPool,
    link_id: Uuid,
    external_id: &str,
    linked_by: LinkedBy,
) -> Result<Uuid> {
    if external_id.trim().is_empty() {
        return Err(Error::Domain(domain::Error::Validation(
            "新条目 external_id 不能为空".into(),
        )));
    }
    let mut tx = pool.begin().await?;
    let old: Option<(Uuid, String, String)> = sqlx::query_as(
        "SELECT node_id, source, role FROM library_links \
         WHERE id = $1 AND state IN ('active','stale') FOR UPDATE",
    )
    .bind(link_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (node_id, source, role) = old.ok_or_else(|| {
        Error::Domain(domain::Error::NotFound(
            "这条链不存在或不是失效状态".into(),
        ))
    })?;
    let candidate: Option<(String, String)> = sqlx::query_as(
        "SELECT title, COALESCE(url,'') FROM library_manifest \
         WHERE source = $1 AND external_id = $2 AND missing_since IS NULL",
    )
    .bind(&source)
    .bind(external_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (title, url) = candidate.ok_or_else(|| {
        Error::Domain(domain::Error::NotFound(
            "候选条目不在当前清单里（先重新扫描媒体库）".into(),
        ))
    })?;
    let used: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM library_links WHERE source = $1 AND external_id = $2",
    )
    .bind(&source)
    .bind(external_id)
    .fetch_optional(&mut *tx)
    .await?;
    if used.is_some() {
        return Err(Error::Domain(domain::Error::Duplicated(format!(
            "「{title}」已经挂过文档，不能重复挂链"
        ))));
    }

    // 旧 stale work 仍占正片位；先以 replaced 插入新链让旧链让位，再激活。
    let new_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO library_links \
           (id, node_id, source, external_id, url, title_hint, role, state, linked_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'replaced', $8)",
    )
    .bind(new_id)
    .bind(node_id)
    .bind(&source)
    .bind(external_id)
    .bind(url)
    .bind(title)
    .bind(&role)
    .bind(linked_by.as_str())
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE library_links SET state = 'replaced', replaced_by = $2 WHERE id = $1")
        .bind(link_id)
        .bind(new_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE library_links SET state = 'active' WHERE id = $1")
        .bind(new_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(new_id)
}

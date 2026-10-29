//! 标题 / 别名 / 正文检索（M8，D36）：归一化 + `pg_trgm`，应用侧 bigram 兜底。

use crate::Result;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain as domain;

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub node_id: Uuid,
    pub title: String,
    pub kind: String,
    pub medium: Option<String>,
    pub status: String,
    pub score: f64,
    pub snippet: String,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    title: String,
    kind: String,
    medium: Option<String>,
    status: String,
    aliases: Vec<String>,
    body_md: String,
}

pub async fn search(pool: &PgPool, query: &str, limit: i64) -> Result<Vec<SearchHit>> {
    let raw = query.trim();
    let norm = domain::text::normalize_title(raw);
    if norm.is_empty() {
        return Ok(vec![]);
    }
    let limit = limit.clamp(1, 50);
    let like = format!("%{norm}%");
    let raw_like = format!("%{raw}%");
    let mut rows: Vec<Row> = sqlx::query_as(
        "SELECT n.id, n.title, n.kind, n.medium, n.status, n.aliases, \
                COALESCE(left(c.body_md, 20000), '') AS body_md \
         FROM nodes n LEFT JOIN contents c ON c.node_id = n.id \
         WHERE n.status <> 'archived' AND ( \
           n.title_norm LIKE $1 OR n.title ILIKE $2 OR $3 = ANY(n.aliases) \
           OR COALESCE(c.body_md,'') ILIKE $2 OR similarity(n.title_norm, $4) > 0.08 \
         ) ORDER BY similarity(n.title_norm, $4) DESC, n.updated_at DESC LIMIT 500",
    )
    .bind(&like)
    .bind(&raw_like)
    .bind(&norm)
    .bind(&norm)
    .fetch_all(pool)
    .await?;

    // SQL 没召回时再走 bigram（典型：中文别名/正文里的近似说法）。
    if rows.len() < limit as usize {
        let fallback: Vec<Row> = sqlx::query_as(
            "SELECT n.id, n.title, n.kind, n.medium, n.status, n.aliases, \
                    COALESCE(left(c.body_md, 20000), '') AS body_md \
             FROM nodes n LEFT JOIN contents c ON c.node_id = n.id \
             WHERE n.status <> 'archived' ORDER BY n.updated_at DESC LIMIT 1000",
        )
        .fetch_all(pool)
        .await?;
        let seen: std::collections::HashSet<Uuid> = rows.iter().map(|r| r.id).collect();
        rows.extend(fallback.into_iter().filter(|r| !seen.contains(&r.id)));
    }

    let mut hits: Vec<(f64, SearchHit)> = rows
        .into_iter()
        .filter_map(|r| {
            let title_norm = domain::text::normalize_title(&r.title);
            let alias_exact = r.aliases.iter().any(|a| {
                domain::text::normalize_title(a) == norm
            });
            let alias_hit = r.aliases.iter().any(|a| {
                domain::text::normalize_title(a).contains(&norm)
            });
            let title_exact = title_norm == norm;
            let title_hit = title_norm.contains(&norm);
            let body_hit = r.body_md.to_lowercase().contains(&raw.to_lowercase());
            let title_tri = domain::text::trigram_similarity(&norm, &r.title);
            let text_bigram = domain::text::bigram_similarity(
                &norm,
                &format!("{} {}", r.title, r.body_md.chars().take(4000).collect::<String>()),
            );
            let score = if title_exact {
                8.0
            } else if title_hit {
                6.0
            } else if alias_exact {
                5.5
            } else if alias_hit {
                4.0
            } else if body_hit {
                3.0 + title_tri
            } else {
                title_tri * 2.0 + text_bigram
            };
            if score < 0.45 {
                return None;
            }
            Some((
                score,
                SearchHit {
                    node_id: r.id,
                    title: r.title,
                    kind: r.kind,
                    medium: r.medium,
                    status: r.status,
                    score,
                    snippet: snippet(&r.body_md, raw),
                },
            ))
        })
        .collect();
    hits.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.title.cmp(&b.1.title)));
    hits.truncate(limit as usize);
    Ok(hits.into_iter().map(|(_, h)| h).collect())
}

fn snippet(body: &str, query: &str) -> String {
    let plain = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with(":::") && *l != "---")
        .collect::<Vec<_>>()
        .join(" ");
    let chars: Vec<char> = plain.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let lower = plain.to_lowercase();
    let q = query.to_lowercase();
    let at = lower.find(&q).map(|byte| plain[..byte].chars().count()).unwrap_or(0);
    let start = at.saturating_sub(45);
    let end = (start + 180).min(chars.len());
    let mut out = chars[start..end].iter().collect::<String>();
    if start > 0 {
        out.insert(0, '…');
    }
    if end < chars.len() {
        out.push('…');
    }
    out
}

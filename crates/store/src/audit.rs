//! 确定性体检（M6）：只做代码能判断的事，结果落 `health_findings`。

use crate::Result;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;
use wikiatlas_domain as domain;
use domain::review::{FindingSeverity, FindingState, HealthFinding};

const AUTO_KINDS: &[&str] = &[
    "stale_link",
    "sync_not_latest",
    "ready_without_content",
    "isolated_node",
    "duplicate_title",
    "stale_draft",
    "unlinked_library",
];

pub struct AuditSummary {
    pub findings: usize,
}

pub async fn run(pool: &PgPool) -> Result<AuditSummary> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE health_findings SET state = 'resolved', resolved_at = now() \
         WHERE state = 'open' AND kind = ANY($1)",
    )
    .bind(AUTO_KINDS)
    .execute(&mut *tx)
    .await?;

    let mut count = 0usize;

    let stale: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT id, node_id, COALESCE(title_hint,'') FROM library_links \
         WHERE state = 'stale' ORDER BY missing_since NULLS LAST LIMIT 200",
    )
    .fetch_all(&mut *tx)
    .await?;
    for (link_id, node_id, title) in stale {
        insert(
            &mut tx,
            "stale_link",
            Some(node_id),
            FindingSeverity::Warn,
            json!({ "link_id": link_id, "title": title, "fix": "重新挂载或解链" }),
        )
        .await?;
        count += 1;
    }

    let unsynced: Vec<(Uuid, Uuid, String, i32, Option<i32>, String)> = sqlx::query_as(
        "SELECT l.id, l.node_id, l.source, c.ver, s.synced_ver, COALESCE(s.status,'pending') \
         FROM library_links l \
         JOIN contents c ON c.node_id = l.node_id \
         LEFT JOIN sync_targets s ON s.link_id = l.id AND s.target = l.source \
         WHERE l.state = 'active' AND l.source IN ('gameatlas','komga') \
           AND c.ver > 0 AND (s.status = 'failed' OR s.synced_ver IS DISTINCT FROM c.ver) \
         ORDER BY l.created_at LIMIT 200",
    )
    .fetch_all(&mut *tx)
    .await?;
    for (link_id, node_id, source, ver, synced_ver, status) in unsynced {
        insert(
            &mut tx,
            "sync_not_latest",
            Some(node_id),
            if status == "failed" {
                FindingSeverity::Error
            } else {
                FindingSeverity::Warn
            },
            json!({
                "link_id": link_id, "source": source, "current_ver": ver,
                "synced_ver": synced_ver, "status": status, "fix": "重新推送"
            }),
        )
        .await?;
        count += 1;
    }

    let missing: Vec<Uuid> = sqlx::query_scalar(
        "SELECT n.id FROM nodes n LEFT JOIN contents c ON c.node_id = n.id \
         WHERE n.status = 'ready' AND (c.node_id IS NULL OR length(trim(c.body_md)) = 0) \
         ORDER BY n.updated_at DESC LIMIT 200",
    )
    .fetch_all(&mut *tx)
    .await?;
    for node_id in missing {
        insert(
            &mut tx,
            "ready_without_content",
            Some(node_id),
            FindingSeverity::Error,
            json!({ "fix": "补正文，或把节点退回草稿" }),
        )
        .await?;
        count += 1;
    }

    let isolated: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT n.id, n.kind, n.title FROM nodes n \
         WHERE n.status <> 'archived' AND n.kind IN ('work','doc') \
           AND NOT EXISTS (SELECT 1 FROM relations r WHERE r.from_id = n.id OR r.to_id = n.id) \
         ORDER BY n.updated_at DESC LIMIT 200",
    )
    .fetch_all(&mut *tx)
    .await?;
    for (node_id, kind, title) in isolated {
        insert(
            &mut tx,
            "isolated_node",
            Some(node_id),
            FindingSeverity::Info,
            json!({ "kind": kind, "title": title, "fix": "补关系或确认它确实独立" }),
        )
        .await?;
        count += 1;
    }

    let duplicates: Vec<(String, Vec<Uuid>, i64)> = sqlx::query_as(
        "SELECT title_norm, array_agg(id ORDER BY created_at), count(*) \
         FROM nodes WHERE status <> 'archived' \
         GROUP BY title_norm HAVING count(*) > 1 ORDER BY count(*) DESC LIMIT 100",
    )
    .fetch_all(&mut *tx)
    .await?;
    for (norm, ids, n) in duplicates {
        insert(
            &mut tx,
            "duplicate_title",
            None,
            FindingSeverity::Warn,
            json!({ "title_norm": norm, "node_ids": ids, "count": n, "fix": "合并或加介质后缀" }),
        )
        .await?;
        count += 1;
    }

    let drafts: Vec<(Uuid, String, DateTime<Utc>, i32)> = sqlx::query_as(
        "SELECT d.node_id, n.title, d.updated_at, d.base_ver FROM content_drafts d \
         JOIN nodes n ON n.id = d.node_id \
         WHERE d.updated_at < now() - interval '24 hours' \
         ORDER BY d.updated_at LIMIT 200",
    )
    .fetch_all(&mut *tx)
    .await?;
    for (node_id, title, updated_at, base_ver) in drafts {
        insert(
            &mut tx,
            "stale_draft",
            Some(node_id),
            FindingSeverity::Info,
            json!({ "title": title, "updated_at": updated_at, "base_ver": base_ver }),
        )
        .await?;
        count += 1;
    }

    let unlinked: Vec<(String, i64)> = sqlx::query_as(
        "SELECT m.source, count(*) FROM library_manifest m \
         WHERE m.missing_since IS NULL \
           AND NOT EXISTS (SELECT 1 FROM library_links l \
                           WHERE l.source = m.source AND l.external_id = m.external_id) \
         GROUP BY m.source HAVING count(*) > 0",
    )
    .fetch_all(&mut *tx)
    .await?;
    for (source, n) in unlinked {
        insert(
            &mut tx,
            "unlinked_library",
            None,
            FindingSeverity::Info,
            json!({ "source": source, "count": n, "fix": "去媒体库池子挑值得建档的条目" }),
        )
        .await?;
        count += 1;
    }

    tx.commit().await?;
    Ok(AuditSummary { findings: count })
}

async fn insert(
    tx: &mut PgConnection,
    kind: &str,
    node_id: Option<Uuid>,
    severity: FindingSeverity,
    detail: Value,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO health_findings (id, kind, node_id, severity, detail) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(kind)
    .bind(node_id)
    .bind(severity.as_str())
    .bind(detail)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct FindingRow {
    id: Uuid,
    kind: String,
    node_id: Option<Uuid>,
    severity: String,
    detail: Value,
    state: String,
    created_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
}

pub async fn list(
    pool: &PgPool,
    state: Option<FindingState>,
) -> Result<Vec<HealthFinding>> {
    let rows: Vec<FindingRow> = sqlx::query_as(
        "SELECT id, kind, node_id, severity, detail, state, created_at, resolved_at \
         FROM health_findings WHERE ($1::text IS NULL OR state = $1) \
         ORDER BY CASE severity WHEN 'error' THEN 0 WHEN 'warn' THEN 1 ELSE 2 END, \
                  created_at DESC LIMIT 1000",
    )
    .bind(state.map(|s| s.as_str()))
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(HealthFinding {
                id: r.id,
                kind: r.kind,
                node_id: r.node_id,
                severity: FindingSeverity::parse(&r.severity)?,
                detail: r.detail,
                state: FindingState::parse(&r.state)?,
                created_at: r.created_at,
                resolved_at: r.resolved_at,
            })
        })
        .collect()
}

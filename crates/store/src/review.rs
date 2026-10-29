//! Review 队列：建议的创建、列出与人工裁决。

use crate::{Error, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain as domain;
use domain::library::Source;
use domain::review::{Suggestion, SuggestionKind, SuggestionState};

mod actions;

#[derive(sqlx::FromRow)]
struct SuggestionRow {
    id: Uuid,
    kind: String,
    node_id: Option<Uuid>,
    source: Option<String>,
    external_id: Option<String>,
    payload: Value,
    state: String,
    created_by: String,
    created_at: DateTime<Utc>,
    decided_at: Option<DateTime<Utc>>,
}

impl TryFrom<SuggestionRow> for Suggestion {
    type Error = Error;
    fn try_from(r: SuggestionRow) -> Result<Self> {
        Ok(Suggestion {
            id: r.id,
            kind: SuggestionKind::parse(&r.kind)?,
            node_id: r.node_id,
            source: r.source.as_deref().map(Source::parse).transpose()?,
            external_id: r.external_id,
            payload: r.payload,
            state: SuggestionState::parse(&r.state)?,
            created_by: r.created_by,
            created_at: r.created_at,
            decided_at: r.decided_at,
        })
    }
}

const COLS: &str = "id, kind, node_id, source, external_id, payload, state, created_by, \
                    created_at, decided_at";

pub async fn create(
    pool: &PgPool,
    kind: SuggestionKind,
    node_id: Option<Uuid>,
    source: Option<Source>,
    external_id: Option<String>,
    payload: Value,
    created_by: &str,
) -> Result<Uuid> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO suggestions \
           (id, kind, node_id, source, external_id, payload, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(kind.as_str())
    .bind(node_id)
    .bind(source.map(|s| s.as_str()))
    .bind(external_id)
    .bind(payload)
    .bind(created_by)
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn list(pool: &PgPool, state: Option<SuggestionState>) -> Result<Vec<Suggestion>> {
    let sql = format!(
        "SELECT {COLS} FROM suggestions \
         WHERE ($1::text IS NULL OR state = $1) \
         ORDER BY created_at DESC LIMIT 500"
    );
    let rows: Vec<SuggestionRow> = sqlx::query_as(&sql)
        .bind(state.map(|s| s.as_str()))
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(Suggestion::try_from).collect()
}

pub struct Decision {
    pub suggestion: Suggestion,
    pub action: String,
}

/// 人工裁决。`accept=true` 时执行安全的联动动作；merge / split / research
/// 只标记已接受，具体结构改动仍需人明确执行（避免猜测式合并）。
pub async fn decide(
    pool: &PgPool,
    id: Uuid,
    accept: bool,
    reason: &str,
) -> Result<Decision> {
    let mut tx = pool.begin().await?;
    let row: Option<SuggestionRow> = sqlx::query_as(&format!(
        "SELECT {COLS} FROM suggestions WHERE id = $1 AND state = 'pending' FOR UPDATE"
    ))
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let row = row.ok_or_else(|| {
        Error::Domain(domain::Error::NotFound("建议不存在或已经裁决".into()))
    })?;

    let action = if accept {
        actions::execute(&mut tx, &row).await?
    } else {
        "rejected".into()
    };
    let state = if accept { "accepted" } else { "rejected" };
    let payload = if reason.trim().is_empty() {
        row.payload.clone()
    } else {
        let mut p = row.payload.clone();
        if let Some(obj) = p.as_object_mut() {
            obj.insert(
                "decision_reason".into(),
                Value::String(reason.trim().to_string()),
            );
        }
        p
    };
    let decided: SuggestionRow = sqlx::query_as(&format!(
        "UPDATE suggestions SET state = $2, payload = $3, decided_at = now() \
         WHERE id = $1 RETURNING {COLS}"
    ))
    .bind(id)
    .bind(state)
    .bind(&payload)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Decision {
        suggestion: decided.try_into()?,
        action,
    })
}

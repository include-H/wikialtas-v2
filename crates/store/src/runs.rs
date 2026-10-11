//! 工单与事件（`runs` / `run_events` / `sessions` / `session_messages`）。
//!
//! 形状沿用 v1（`docs/database.md §4`：runs / run_events / sessions / session_messages **原样搬**）。
//! **api 只入队，worker 执行**；事件是前端 SSE 的真相源（`docs/api.md §4`：
//! `response.*` 占 `sequence_number`，宿主旁路 `wikiatlas.*` 不占）。

use crate::{Error, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain as domain;

/// 新建工单。
#[derive(Debug, Clone)]
pub struct NewRun {
    pub id: String,
    pub workspace: String,
    pub intent: String,
    pub goal: String,
    pub model: String,
    /// 宿主塞的东西（当前只有 `{"node_id": "..."}`）。
    pub checkpoint: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct Run {
    pub id: String,
    pub workspace: String,
    pub intent: String,
    pub goal: String,
    pub status: String,
    pub model: String,
    pub checkpoint: serde_json::Value,
    pub result_json: Option<serde_json::Value>,
    pub error_json: Option<serde_json::Value>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct RunEvent {
    pub seq: i32,
    pub r#type: String,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct RunDb {
    id: String,
    workspace: String,
    intent: String,
    goal: String,
    status: String,
    model: String,
    checkpoint: serde_json::Value,
    result_json: Option<serde_json::Value>,
    error_json: Option<serde_json::Value>,
    started_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

impl From<RunDb> for Run {
    fn from(r: RunDb) -> Self {
        Run {
            id: r.id,
            workspace: r.workspace,
            intent: r.intent,
            goal: r.goal,
            status: r.status,
            model: r.model,
            checkpoint: r.checkpoint,
            result_json: r.result_json,
            error_json: r.error_json,
            started_at: r.started_at,
            completed_at: r.completed_at,
        }
    }
}

pub async fn create(pool: &PgPool, new: &NewRun) -> Result<()> {
    sqlx::query(
        "INSERT INTO runs (id, workspace, intent, goal, status, model, checkpoint, started_at, last_active) \
         VALUES ($1, $2, $3, $4, 'queued', $5, $6, now(), now())",
    )
    .bind(&new.id)
    .bind(&new.workspace)
    .bind(&new.intent)
    .bind(&new.goal)
    .bind(&new.model)
    .bind(&new.checkpoint)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get(pool: &PgPool, id: &str) -> Result<Run> {
    let row: Option<RunDb> = sqlx::query_as(
        "SELECT id, workspace, intent, goal, status, model, checkpoint, result_json, error_json, \
                started_at, completed_at FROM runs WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    row.map(Run::from)
        .ok_or_else(|| Error::Domain(domain::Error::NotFound(format!("工单 {id}"))))
}

/// 改状态；`result` / `error` 给了就一起写。
pub async fn set_status(
    pool: &PgPool,
    id: &str,
    status: &str,
    result: Option<&serde_json::Value>,
    error: Option<&serde_json::Value>,
) -> Result<()> {
    let done = matches!(status, "completed" | "failed" | "cancelled");
    sqlx::query(
        "UPDATE runs SET status = $2, result_json = COALESCE($3, result_json), \
           error_json = COALESCE($4, error_json), last_active = now(), \
           completed_at = CASE WHEN $5 THEN now() ELSE completed_at END \
         WHERE id = $1",
    )
    .bind(id)
    .bind(status)
    .bind(result)
    .bind(error)
    .bind(done)
    .execute(pool)
    .await?;
    Ok(())
}

/// 追加一条事件（seq 由 `run_events` 自己按 run 递增）。
pub async fn append_event(
    pool: &PgPool,
    run_id: &str,
    ty: &str,
    payload: &serde_json::Value,
) -> Result<i32> {
    let (seq,): (i32,) = sqlx::query_as(
        "INSERT INTO run_events (id, run_id, seq, type, payload, created_at) \
         VALUES ($1, $2, COALESCE((SELECT max(seq) + 1 FROM run_events WHERE run_id = $2), 0), $3, $4, now()) \
         RETURNING seq",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(run_id)
    .bind(ty)
    .bind(payload)
    .fetch_one(pool)
    .await?;
    Ok(seq)
}

/// 读 `seq > after` 的事件（SSE 续传的游标）。
pub async fn events_after(pool: &PgPool, run_id: &str, after: i32) -> Result<Vec<RunEvent>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        seq: i32,
        r#type: String,
        payload: serde_json::Value,
        created_at: DateTime<Utc>,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT seq, type, payload, created_at FROM run_events \
         WHERE run_id = $1 AND seq > $2 ORDER BY seq LIMIT 500",
    )
    .bind(run_id)
    .bind(after)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| RunEvent { seq: r.seq, r#type: r.r#type, payload: r.payload, created_at: r.created_at })
        .collect())
}

/// 会话日志追加一行（append-only；`docs/behavior-notes.md §1`）。
pub struct NewMessage {
    pub session_id: String,
    pub run_id: String,
    pub turn: i32,
    pub role: String,
    pub content: String,
    pub tool_calls_json: String,
    pub tool_call_id: String,
    pub tool_name: String,
}

pub async fn append_message(pool: &PgPool, m: &NewMessage) -> Result<i32> {
    let (seq,): (i32,) = sqlx::query_as(
        "INSERT INTO session_messages \
           (id, session_id, seq, run_id, turn, role, content, tool_calls_json, tool_call_id, tool_name, created_at) \
         VALUES ($1, $2, COALESCE((SELECT max(seq) + 1 FROM session_messages WHERE session_id = $2), 0), \
                 $3, $4, $5, $6, $7, $8, $9, now()) RETURNING seq",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&m.session_id)
    .bind(&m.run_id)
    .bind(m.turn)
    .bind(&m.role)
    .bind(&m.content)
    .bind(&m.tool_calls_json)
    .bind(&m.tool_call_id)
    .bind(&m.tool_name)
    .fetch_one(pool)
    .await?;
    Ok(seq)
}

/// 确保会话行存在（`work:<节点 id>` 这类自由字符串，沿用 v1 形状）。
pub async fn ensure_session(pool: &PgPool, id: &str, target: &str, title: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO sessions (id, target, title, created_at, updated_at) VALUES ($1, $2, $3, now(), now()) \
         ON CONFLICT (id) DO UPDATE SET updated_at = now()",
    )
    .bind(id)
    .bind(target)
    .bind(title)
    .execute(pool)
    .await?;
    Ok(())
}

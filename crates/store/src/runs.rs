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

/// 记阶段（`docs/design.md §2.1`）：三段是同一工单的三个 `checkpoint.phase`。
/// 合并写进 checkpoint，不动其他键。
pub async fn set_phase(pool: &PgPool, id: &str, phase: &str) -> Result<()> {
    sqlx::query(
        "UPDATE runs SET checkpoint = checkpoint || jsonb_build_object('phase', $2::text), \
           last_active = now() WHERE id = $1",
    )
    .bind(id)
    .bind(phase)
    .execute(pool)
    .await?;
    Ok(())
}

/// 往 checkpoint 合并一个键（阶段产物：骨架、覆盖率等）。
pub async fn set_checkpoint(pool: &PgPool, id: &str, key: &str, value: &serde_json::Value) -> Result<()> {
    sqlx::query(
        "UPDATE runs SET checkpoint = checkpoint || jsonb_build_object($2::text, $3::jsonb), \
           last_active = now() WHERE id = $1",
    )
    .bind(id)
    .bind(key)
    .bind(value)
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

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub target: String,
    pub title: String,
    pub message_count: i64,
    pub preview: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionMessageView {
    pub id: String,
    pub seq: i32,
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
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
    sqlx::query(
        "UPDATE sessions SET updated_at = now(), \
           title = CASE WHEN title = '新对话' AND $2 = 'user' \
                        THEN left(regexp_replace($3, '[[:space:]]+', ' ', 'g'), 36) ELSE title END \
         WHERE id = $1",
    )
        .bind(&m.session_id)
        .bind(&m.role)
        .bind(&m.content)
        .execute(pool)
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

/// 某节点的会话列表（最近更新在前）。
pub async fn sessions_for_node(pool: &PgPool, node_id: Uuid) -> Result<Vec<SessionInfo>> {
    let rows: Vec<(
        String,
        String,
        String,
        i64,
        String,
        DateTime<Utc>,
        DateTime<Utc>,
    )> = sqlx::query_as(
        "SELECT s.id, s.target, s.title, \
                count(m.id)::bigint AS message_count, \
                COALESCE((SELECT left(m2.content, 120) FROM session_messages m2 \
                          WHERE m2.session_id = s.id AND m2.content <> '' \
                          ORDER BY m2.seq DESC LIMIT 1), '') AS preview, \
                s.created_at, s.updated_at \
         FROM sessions s LEFT JOIN session_messages m ON m.session_id = s.id \
         WHERE s.target = $1 OR s.id = $2 OR s.id LIKE $3 \
         GROUP BY s.id ORDER BY s.updated_at DESC LIMIT 100",
    )
    .bind(node_id.to_string())
    .bind(format!("work:{node_id}"))
    .bind(format!("work:{node_id}:%"))
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| SessionInfo {
            id: r.0,
            target: r.1,
            title: r.2,
            message_count: r.3,
            preview: r.4,
            created_at: r.5,
            updated_at: r.6,
        })
        .collect())
}

/// 会话详情消息（正序，只取 user/assistant；工具日志留在库里给审计，不直接回灌成自然语言）。
pub async fn session_messages(
    pool: &PgPool,
    session_id: &str,
    limit: i64,
) -> Result<Vec<SessionMessageView>> {
    let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM sessions WHERE id = $1")
        .bind(session_id)
        .fetch_optional(pool)
        .await?;
    if exists.is_none() {
        return Err(Error::Domain(domain::Error::NotFound(format!(
            "会话 {session_id}"
        ))));
    }
    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        seq: i32,
        role: String,
        content: String,
        created_at: DateTime<Utc>,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, seq, role, content, created_at FROM ( \
           SELECT id, seq, role, content, created_at FROM session_messages \
           WHERE session_id = $1 AND role IN ('user','assistant') AND content <> '' \
           ORDER BY seq DESC LIMIT $2 \
         ) t ORDER BY seq",
    )
    .bind(session_id)
    .bind(limit.clamp(1, 200))
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| SessionMessageView {
            id: r.id,
            seq: r.seq,
            role: r.role,
            content: r.content,
            created_at: r.created_at,
        })
        .collect())
}

/// 运行时上下文：最近消息按字符预算裁剪，返回正序 (role, content)。
pub async fn conversation_context(
    pool: &PgPool,
    session_id: &str,
    char_budget: usize,
) -> Result<Vec<(String, String)>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT role, content FROM ( \
           SELECT role, content, seq FROM session_messages \
           WHERE session_id = $1 AND role IN ('user','assistant') AND content <> '' \
           ORDER BY seq DESC LIMIT 40 \
         ) t ORDER BY seq DESC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    let mut used = 0usize;
    let mut out = vec![];
    for (role, content) in rows {
        let n = content.chars().count();
        if used + n > char_budget && !out.is_empty() {
            break;
        }
        used += n;
        out.push((role, content));
    }
    out.reverse();
    Ok(out)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionHit {
    pub session_id: String,
    pub title: String,
    pub snippet: String,
    pub updated_at: DateTime<Utc>,
}

pub async fn search_sessions(
    pool: &PgPool,
    query: &str,
    limit: i64,
) -> Result<Vec<SessionHit>> {
    let like = format!("%{}%", query.trim());
    if query.trim().is_empty() {
        return Ok(vec![]);
    }
    #[derive(sqlx::FromRow)]
    struct Row {
        session_id: String,
        title: String,
        content: String,
        updated_at: DateTime<Utc>,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT m.session_id, COALESCE(s.title,'') AS title, \
                left(m.content, 500) AS content, COALESCE(s.updated_at,m.created_at) AS updated_at \
         FROM session_messages m LEFT JOIN sessions s ON s.id = m.session_id \
         WHERE m.content ILIKE $1 ORDER BY m.created_at DESC LIMIT $2",
    )
    .bind(like)
    .bind(limit.clamp(1, 50))
    .fetch_all(pool)
    .await?;
    let mut out: Vec<SessionHit> = vec![];
    for r in rows {
        if out.iter().any(|x| x.session_id == r.session_id) {
            continue;
        }
        out.push(SessionHit {
            session_id: r.session_id,
            title: r.title,
            snippet: r.content.chars().take(240).collect(),
            updated_at: r.updated_at,
        });
    }
    Ok(out)
}

pub async fn read_session(
    pool: &PgPool,
    id: &str,
    limit: i64,
) -> Result<Vec<NewMessage>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        session_id: String,
        run_id: String,
        turn: i32,
        role: String,
        content: String,
        tool_calls_json: String,
        tool_call_id: String,
        tool_name: String,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT session_id, run_id, turn, role, content, tool_calls_json, tool_call_id, tool_name \
         FROM session_messages WHERE session_id = $1 ORDER BY seq DESC LIMIT $2",
    )
    .bind(id)
    .bind(limit.clamp(1, 200))
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Err(Error::Domain(domain::Error::NotFound(format!(
            "会话 {id}"
        ))));
    }
    Ok(rows
        .into_iter()
        .rev()
        .map(|r| NewMessage {
            session_id: r.session_id,
            run_id: r.run_id,
            turn: r.turn,
            role: r.role,
            content: r.content,
            tool_calls_json: r.tool_calls_json,
            tool_call_id: r.tool_call_id,
            tool_name: r.tool_name,
        })
        .collect())
}

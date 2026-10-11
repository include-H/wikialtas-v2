//! 队列（`jobs`）：`FOR UPDATE SKIP LOCKED` + 租约（`docs/backend.md` §1）。
//!
//! api 只入队；worker 认领并跑。**替代 v1 的进程内队列与「重启全标 interrupted」**——
//! 状态在 PG 里，崩了租约到期自动回收（`docs/database.md` §2）。

use crate::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

/// 一个被认领的工单。
#[derive(Debug, Clone)]
pub struct Job {
    pub id: i64,
    pub kind: String,
    pub payload: serde_json::Value,
    pub attempts: i32,
}

/// 入队，返回 job id。
pub async fn enqueue(pool: &PgPool, kind: &str, payload: &serde_json::Value) -> Result<i64> {
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO jobs (kind, payload) VALUES ($1, $2) RETURNING id",
    )
    .bind(kind)
    .bind(payload)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// 认领一个待跑工单（跳锁 + 打租约）。没有可跑的返回 None。
pub async fn claim(pool: &PgPool, worker: &str, lease_secs: i64) -> Result<Option<Job>> {
    let row: Option<(i64, String, serde_json::Value, i32)> = sqlx::query_as(
        "UPDATE jobs SET state = 'running', locked_by = $1, \
           locked_until = now() + make_interval(secs => $2), attempts = attempts + 1 \
         WHERE id = ( \
           SELECT id FROM jobs \
           WHERE (state = 'pending' AND run_after <= now()) \
              OR (state = 'running' AND locked_until < now()) \
           ORDER BY run_after, id \
           FOR UPDATE SKIP LOCKED LIMIT 1) \
         RETURNING id, kind, payload, attempts",
    )
    .bind(worker)
    .bind(lease_secs as f64)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id, kind, payload, attempts)| Job {
        id,
        kind,
        payload,
        attempts,
    }))
}

/// 续租（长任务用）。
pub async fn renew(pool: &PgPool, id: i64, lease_secs: i64) -> Result<()> {
    sqlx::query("UPDATE jobs SET locked_until = now() + make_interval(secs => $2) WHERE id = $1")
        .bind(id)
        .bind(lease_secs as f64)
        .execute(pool)
        .await?;
    Ok(())
}

/// 完成 / 失败。失败不需要单独状态：`state='failed'` + message 即可。
pub async fn finish(pool: &PgPool, id: i64, ok: bool, message: &str) -> Result<()> {
    let state = if ok { "done" } else { "failed" };
    let payload = serde_json::json!({ "message": message });
    sqlx::query(
        "UPDATE jobs SET state = $2, locked_by = NULL, locked_until = NULL, \
           payload = payload || $3::jsonb WHERE id = $1",
    )
    .bind(id)
    .bind(state)
    .bind(payload)
    .execute(pool)
    .await?;
    Ok(())
}

/// 查一个工单状态（api 给前端轮询用）。
pub async fn get(pool: &PgPool, id: i64) -> Result<Option<(String, serde_json::Value, DateTime<Utc>)>> {
    let row: Option<(String, serde_json::Value, DateTime<Utc>)> = sqlx::query_as(
        "SELECT state, payload, created_at FROM jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 队列概况（池子页顶部的「有没有在扫」）。
pub async fn queue_depth(pool: &PgPool) -> Result<i64> {
    let (n,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM jobs WHERE state IN ('pending','running')",
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

//! worker 进程：消费 `jobs`（`FOR UPDATE SKIP LOCKED` + 租约）。
//!
//! 按 `docs/backend.md` §1：**api 只入队，跑活在 worker**。
//! 已实现 `scan`（M1）；`run` / `sync` / `audit` 分别在 M3 / M7 / M6 落地——
//! 现在收到就明确失败，不假装成功（**不许静默吞掉**）。

use std::time::Duration;
use wikiatlas_domain::library::Source;
use wikiatlas_store::jobs;

/// 认领租约：扫描可能拉几分钟，给 10 分钟。
const LEASE_SECS: i64 = 600;
const WORKER_ID: &str = "worker-1";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wikiatlas_worker=debug".into()),
        )
        .init();

    let url = std::env::var("WIKIATLAS_DB_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .map_err(|_| anyhow::anyhow!("缺少数据库连接串：设 WIKIATLAS_DB_URL"))?;
    let pool = wikiatlas_store::connect(&url).await?;
    wikiatlas_store::migrate(&pool).await?;
    tracing::info!("worker {WORKER_ID} 就绪，开始消费 jobs");

    loop {
        match jobs::claim(&pool, WORKER_ID, LEASE_SECS).await? {
            None => tokio::time::sleep(Duration::from_millis(1000)).await,
            Some(job) => {
                tracing::info!(id = job.id, kind = %job.kind, "认领工单");
                let (ok, msg) = dispatch(&pool, &job).await;
                if !ok {
                    tracing::warn!(id = job.id, "工单失败：{msg}");
                }
                jobs::finish(&pool, job.id, ok, &msg).await?;
            }
        }
    }
}

/// 一个工单的执行体。返回（是否成功, 说明）。
async fn dispatch(pool: &sqlx::PgPool, job: &jobs::Job) -> (bool, String) {
    match job.kind.as_str() {
        "scan" => run_scan(pool, &job.payload).await,
        other => (false, format!("工单类型 {other} 还没实现（见 docs/TRACKING.md 里程碑）")),
    }
}

async fn run_scan(pool: &sqlx::PgPool, payload: &serde_json::Value) -> (bool, String) {
    // payload: {"sources": ["gameatlas","emby","komga"]}（缺省 = 全部已配置的源）
    let only = payload.get("sources").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|x| x.as_str())
            .filter_map(|s| Source::parse(s).ok())
            .collect::<Vec<_>>()
    });
    match wikiatlas_sources::scan::scan(pool, only).await {
        Ok(outcome) => {
            let ok = outcome.results.iter().any(|r| r.ok);
            let summary = outcome
                .results
                .iter()
                .map(|r| {
                    format!(
                        "{}: {}（见 {} / 新 {} / 变 {} / 消失 {}）",
                        r.source.as_str(),
                        if r.ok { "ok" } else { "失败" },
                        r.seen,
                        r.inserted,
                        r.changed,
                        r.missing
                    )
                })
                .collect::<Vec<_>>()
                .join("；");
            if outcome.results.is_empty() {
                return (false, "没有已配置的源".into());
            }
            (ok, summary)
        }
        Err(e) => (false, e.to_string()),
    }
}

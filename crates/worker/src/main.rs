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
        "run" => run_agent(pool, &job.payload).await,
        "sync" => run_sync(pool, &job.payload).await,
        "audit" => run_audit(pool).await,
        other => (false, format!("工单类型 {other} 还没实现（见 docs/TRACKING.md 里程碑）")),
    }
}

/// `run` 工单：加载模型配置 → 跑执行器。
/// 没配模型（`secrets: llm.base_url / llm.api_key`）就明确失败，**不假装跑过**。
async fn run_agent(pool: &sqlx::PgPool, payload: &serde_json::Value) -> (bool, String) {
    let run_id = payload.get("run_id").and_then(|v| v.as_str()).unwrap_or("");
    if run_id.is_empty() {
        return (false, "run 工单没带 run_id".into());
    }
    let cfg = match wikiatlas_agent::client::LlmConfig::load(pool).await {
        Ok(Some(c)) => c,
        Ok(None) => return (false, "没配模型：secrets 里补 llm.base_url / llm.api_key".into()),
        Err(e) => return (false, format!("读模型配置失败：{e}")),
    };
    match wikiatlas_agent::runtime::execute(pool, &cfg, run_id).await {
        Ok(s) => (
            true,
            format!(
                "{}（草稿{}，{} 轮）",
                s.status,
                if s.draft_written { "已落" } else { "没落" },
                s.iterations
            ),
        ),
        Err(e) => (false, e.to_string()),
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

/// `sync` 工单：定稿后自动反哺，或手动重推一条链。
///
/// payload 二选一：
/// - `{ "node_id": "...", "force": false }`：该节点全部 active 可写链
/// - `{ "link_id": "...", "force": true }`：手动重推指定链
async fn run_sync(pool: &sqlx::PgPool, payload: &serde_json::Value) -> (bool, String) {
    let force = payload.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
    let jobs = if let Some(link_id) = payload
        .get("link_id")
        .and_then(|v| v.as_str())
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
    {
        match wikiatlas_store::sync::sync_job_for_link(pool, link_id).await {
            Ok(job) => vec![job],
            Err(e) => return (false, e.to_string()),
        }
    } else if let Some(node_id) = payload
        .get("node_id")
        .and_then(|v| v.as_str())
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
    {
        match wikiatlas_store::sync::sync_jobs_for_node(pool, node_id).await {
            Ok(jobs) => jobs,
            Err(e) => return (false, e.to_string()),
        }
    } else {
        return (false, "sync 工单没带 node_id 或 link_id".into());
    };

    if jobs.is_empty() {
        return (true, "没有可反哺的 active 链".into());
    }

    let cfg = match wikiatlas_sources::config::load(pool).await {
        Ok(cfg) => cfg,
        Err(e) => return (false, format!("读取媒体库配置失败：{e}")),
    };

    let mut ok_n = 0usize;
    let mut skipped_n = 0usize;
    let mut errors = Vec::new();
    for job in jobs {
        if !force
            && !wikiatlas_store::sync::should_auto_sync(
                pool,
                job.link_id,
                job.source,
                job.ver,
            )
            .await
            .unwrap_or(true)
        {
            skipped_n += 1;
            continue;
        }
        if let Err(e) =
            wikiatlas_store::sync::mark_pending(pool, job.link_id, job.source).await
        {
            errors.push(format!("{} 状态写入失败：{e}", job.source));
            continue;
        }

        let pushed = match job.source {
            Source::GameAtlas => push_gameatlas(&cfg, &job).await,
            Source::Komga => push_komga(&cfg, &job).await,
            Source::Emby => Err("Emby 是只读源".to_string()),
        };
        match pushed {
            Ok(()) => {
                if let Err(e) =
                    wikiatlas_store::sync::mark_ok(pool, job.link_id, job.source, job.ver).await
                {
                    errors.push(format!("{} 成功但状态没落：{e}", job.source));
                } else {
                    ok_n += 1;
                }
            }
            Err(e) => {
                let _ =
                    wikiatlas_store::sync::mark_failed(pool, job.link_id, job.source, &e).await;
                errors.push(format!("{}：{e}", job.source));
            }
        }
    }

    let mut msg = format!(
        "反哺 ok {ok_n}；跳过 {skipped_n}{}",
        if errors.is_empty() {
            String::new()
        } else {
            format!("；失败 {}：{}", errors.len(), errors.join("；"))
        }
    );
    if errors.is_empty() {
        if ok_n == 0 && skipped_n > 0 {
            msg.push_str("（同版本已反哺）");
        }
        (true, msg)
    } else {
        (false, msg)
    }
}

async fn push_gameatlas(
    cfg: &wikiatlas_sources::config::SourceConfig,
    job: &wikiatlas_domain::sync::SyncJob,
) -> std::result::Result<(), String> {
    let cfg = cfg
        .gameatlas
        .as_ref()
        .ok_or_else(|| "GameAtlas 未配置".to_string())?;
    let client = wikiatlas_sources::gameatlas::GameAtlasClient::connect(cfg)
        .await
        .map_err(|e| e.to_string())?;
    let summary = wikiatlas_domain::text::markdown_summary(&job.body_md, 200);
    client
        .push_wiki(&job.external_id, &job.body_md, summary.as_deref())
        .await
        .map_err(|e| e.to_string())
}

async fn push_komga(
    cfg: &wikiatlas_sources::config::SourceConfig,
    job: &wikiatlas_domain::sync::SyncJob,
) -> std::result::Result<(), String> {
    let cfg = cfg
        .komga
        .as_ref()
        .ok_or_else(|| "Komga 未配置".to_string())?;
    let summary = wikiatlas_domain::text::markdown_summary(&job.body_md, 500)
        .ok_or_else(|| "没提取到简介：正文先写一段普通文字".to_string())?;
    let client =
        wikiatlas_sources::komga::KomgaClient::connect(cfg).map_err(|e| e.to_string())?;
    client
        .push_summary(&job.external_id, &summary)
        .await
        .map_err(|e| e.to_string())
}

/// `audit` 工单：确定性体检；不调用模型。
async fn run_audit(pool: &sqlx::PgPool) -> (bool, String) {
    match wikiatlas_store::audit::run(pool).await {
        Ok(s) => (true, format!("体检完成，发现 {} 项", s.findings)),
        Err(e) => (false, e.to_string()),
    }
}

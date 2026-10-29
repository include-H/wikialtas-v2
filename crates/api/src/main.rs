//! api 进程：HTTP + 短事务（人的读写）。
//!
//! 按 `docs/backend.md` §1 的纪律：**这个进程不认识 LLM、不跑工单**——那是 worker 的事。
//! 它只做三件事：起 axum、连 PG（唯一真相）、把 store 的结果映射成 HTTP。

mod auth;
mod dto;
mod error;
mod routes;

use axum::routing::{delete, get, post, put};
use axum::Router;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wikiatlas_api=debug,tower_http=info".into()),
        )
        .init();

    let url = std::env::var("WIKIATLAS_DB_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .map_err(|_| {
            anyhow::anyhow!(
                "缺少数据库连接串：设 WIKIATLAS_DB_URL（或 DATABASE_URL），\
                 本地在 `.secrets/wikiatlas.pg.env` 里"
            )
        })?;

    let pool = wikiatlas_store::connect(&url).await?;
    wikiatlas_store::migrate(&pool).await?;
    tracing::info!("迁移已是最新");

    let app = Router::new()
        .route("/api/healthz", get(routes::healthz))
        .route("/api/auth", get(auth::status))
        .route("/api/login", post(auth::login))
        .route("/api/logout", post(auth::logout))
        .route("/api/tree", get(routes::tree))
        .route("/api/overview", get(routes::overview))
        .route("/api/nodes", post(routes::create_node))
        .route(
            "/api/nodes/{id}",
            get(routes::node_detail).patch(routes::update_node),
        )
        .route("/api/archived", get(routes::archived_nodes))
        .route("/api/nodes/{id}/archive", post(routes::archive_node))
        .route("/api/nodes/{id}/unarchive", post(routes::unarchive_node))
        .route(
            "/api/nodes/{id}/draft",
            put(routes::put_draft).delete(routes::discard_draft),
        )
        .route("/api/nodes/{id}/commit", post(routes::commit))
        .route("/api/nodes/{id}/versions", get(routes::versions))
        .route("/api/nodes/{id}/versions/{ver}", get(routes::version_body))
        .route("/api/nodes/{id}/links", get(routes::node_links))
        .route("/api/nodes/{id}/index", post(routes::sync_index))
        // 关系与综合（M5）
        .route("/api/relation-types", get(routes::relation_types))
        .route(
            "/api/relations",
            get(routes::list_relations).post(routes::create_relation),
        )
        .route("/api/relations/{id}", delete(routes::delete_relation))
        .route(
            "/api/synthesis",
            get(routes::list_synthesis).post(routes::create_synthesis),
        )
        // Review / 体检（M6）
        .route(
            "/api/suggestions",
            get(routes::list_suggestions).post(routes::create_suggestion),
        )
        .route(
            "/api/suggestions/{id}/accept",
            post(routes::accept_suggestion),
        )
        .route(
            "/api/suggestions/{id}/reject",
            post(routes::reject_suggestion),
        )
        .route("/api/health-findings", get(routes::health_findings))
        .route("/api/health-findings/run", post(routes::run_audit))
        // 检索 / 问答产物（M8）
        .route("/api/search", get(routes::search))
        .route("/api/answers", post(routes::save_answer))
        // 设置 / 用量 / 导出导入（M9）
        .route(
            "/api/settings",
            get(routes::get_settings).put(routes::put_settings),
        )
        .route("/api/secrets", put(routes::put_secrets))
        .route("/api/usage", get(routes::usage))
        .route("/api/export", get(routes::export_project))
        .route("/api/import", post(routes::import_project))
        // 媒体库（M1）：清单 / 池子 / 扫描入队 / 扫描记账
        .route("/api/library/pool", get(routes::library_pool))
        .route("/api/library/manifest", get(routes::library_manifest))
        .route("/api/library/scan", post(routes::library_scan))
        .route("/api/library/scan-runs", get(routes::library_scan_runs))
        .route("/api/library/status", get(routes::library_status))
        // 建档 / 解链 / 忽略（M2）
        .route("/api/library/links", post(routes::build_link))
        .route("/api/library/links/{id}", delete(routes::unlink))
        .route(
            "/api/library/links/{id}/relink-candidates",
            get(routes::relink_candidates),
        )
        .route("/api/library/links/{id}/relink", post(routes::relink))
        .route("/api/library/links/{id}/sync", post(routes::resync_link))
        .route(
            "/api/library/ignores",
            get(routes::list_ignores).post(routes::add_ignore),
        )
        .route("/api/library/ignores/{id}", delete(routes::remove_ignore))
        // 工单 / SSE（M3）
        .route(
            "/api/sessions",
            get(routes::list_sessions).post(routes::create_session),
        )
        .route("/api/sessions/{id}", get(routes::session_detail))
        .route("/api/skills", get(routes::list_skills))
        .route("/api/runs", post(routes::create_run))
        .route("/api/runs/{id}", get(routes::get_run))
        .route("/api/runs/{id}/cancel", post(routes::cancel_run))
        .route("/api/runs/{id}/events", get(routes::run_events))
        // 仅开发：放行本地前端（Vite）。上线前收紧成白名单。
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn_with_state(
            pool.clone(),
            auth::require,
        ))
        .with_state(pool);

    let addr: SocketAddr = std::env::var("WIKIATLAS_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8088".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("api 起在 http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

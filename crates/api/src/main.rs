//! api 进程：HTTP + 短事务（人的读写）。
//!
//! 按 `docs/backend.md` §1 的纪律：**这个进程不认识 LLM、不跑工单**——那是 worker 的事。
//! 它只做三件事：起 axum、连 PG（唯一真相）、把 store 的结果映射成 HTTP。

mod dto;
mod error;
mod routes;

use axum::routing::{get, post, put};
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
        .route("/api/tree", get(routes::tree))
        .route("/api/nodes", post(routes::create_node))
        .route("/api/nodes/{id}", get(routes::node_detail))
        .route("/api/nodes/{id}/draft", put(routes::put_draft))
        .route("/api/nodes/{id}/commit", post(routes::commit))
        .route("/api/nodes/{id}/versions", get(routes::versions))
        // 仅开发：放行本地前端（Vite）。上线前收紧成白名单。
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(pool);

    let addr: SocketAddr = std::env::var("WIKIATLAS_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8088".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("api 起在 http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

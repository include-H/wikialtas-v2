//! 路由处理器：只做**编排**（取参 → 调 store → 出 JSON）。
//! 业务规则不在这一层——v1 的教训就是规则塞进了 handler，于是没有测试基建。

use axum::extract::{Path, State};
use axum::Json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::dto::{CommitReq, CommitResp, CreateNodeReq, DraftReq, VersionResp};
use crate::error::ApiError;
use wikiatlas_domain as domain;

/// 还没有登录体系：人写一律记成这个身份（写者矩阵落地后由中间件给）。
const HUMAN: &str = "human:local";

pub async fn healthz() -> &'static str {
    "ok"
}

pub async fn tree(State(pool): State<PgPool>) -> Result<Json<Vec<domain::Node>>, ApiError> {
    Ok(Json(wikiatlas_store::nodes::tree(&pool).await?))
}

pub async fn create_node(
    State(pool): State<PgPool>,
    Json(req): Json<CreateNodeReq>,
) -> Result<Json<domain::Node>, ApiError> {
    let new = req.into_new()?;
    Ok(Json(wikiatlas_store::nodes::create(&pool, &new).await?))
}

pub async fn node_detail(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<domain::NodeContent>, ApiError> {
    Ok(Json(wikiatlas_store::content::read(&pool, id).await?))
}

pub async fn put_draft(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(req): Json<DraftReq>,
) -> Result<Json<domain::Draft>, ApiError> {
    let draft =
        wikiatlas_store::content::put_draft(&pool, id, &req.body_md, req.base_ver, HUMAN).await?;
    Ok(Json(draft))
}

pub async fn commit(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(req): Json<CommitReq>,
) -> Result<Json<CommitResp>, ApiError> {
    let out = wikiatlas_store::content::commit(
        &pool,
        id,
        &req.body_md,
        req.base_ver,
        &req.summary,
        HUMAN,
    )
    .await?;
    Ok(Json(CommitResp {
        ver: out.ver,
        updated_at: out.updated_at,
    }))
}

pub async fn versions(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<VersionResp>>, ApiError> {
    let rows = wikiatlas_store::content::versions(&pool, id, 50).await?;
    Ok(Json(
        rows.into_iter()
            .map(|(ver, created_at, summary)| VersionResp {
                ver,
                created_at,
                summary,
            })
            .collect(),
    ))
}

// ============================================================ 媒体库（M1）

use axum::extract::Query;
use serde_json::json;
use wikiatlas_store::{jobs, secrets};

use crate::dto::{LibraryQuery, ScanReq};

pub async fn library_pool(
    State(pool): State<PgPool>,
    Query(q): Query<LibraryQuery>,
) -> Result<Json<Vec<domain::library::PoolEntry>>, ApiError> {
    let f = q.into_filter()?;
    Ok(Json(wikiatlas_store::library::list_pool(&pool, &f).await?))
}

pub async fn library_manifest(
    State(pool): State<PgPool>,
    Query(q): Query<LibraryQuery>,
) -> Result<Json<Vec<domain::library::ManifestRow>>, ApiError> {
    let f = q.into_filter()?;
    Ok(Json(wikiatlas_store::library::list_manifest(&pool, &f).await?))
}

/// POST /api/library/scan：**只入队**（扫描在 worker 里跑，见 docs/backend.md §1）。
pub async fn library_scan(
    State(pool): State<PgPool>,
    body: Option<Json<ScanReq>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let sources = body.map(|Json(b)| b.sources).unwrap_or_default();
    // 空数组 = 扫全部已配置的源：payload 里干脆不放 sources 键（worker 按缺失处理）。
    let payload = if sources.is_empty() {
        json!({})
    } else {
        json!({ "sources": sources })
    };
    let id = jobs::enqueue(&pool, "scan", &payload).await?;
    Ok(Json(json!({ "jobId": id })))
}

pub async fn library_scan_runs(
    State(pool): State<PgPool>,
) -> Result<Json<Vec<domain::library::ScanRun>>, ApiError> {
    Ok(Json(wikiatlas_store::library::list_scan_runs(&pool, 50).await?))
}

/// 池子页顶部：哪些源配好了（**只有布尔，没有值**）+ 队列里还有没有活。
pub async fn library_status(
    State(pool): State<PgPool>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let keys = secrets::list_keys(&pool).await?;
    let has = |k: &str| keys.iter().any(|x| x == k);
    let configured = json!({
        "gameatlas": has("gameatlas.base_url") && has("gameatlas.password"),
        "emby": has("emby.base_url") && has("emby.token"),
        "komga": has("komga.base_url") && has("komga.api_key"),
    });
    let depth = jobs::queue_depth(&pool).await?;
    Ok(Json(json!({ "configured": configured, "queueDepth": depth })))
}

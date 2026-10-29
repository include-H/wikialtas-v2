//! 可选单密码访问控制（D5）：未设置密码时本地开放；设置后 API 需要 HttpOnly Cookie。

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

const COOKIE: &str = "wikiatlas_session";

async fn password(pool: &PgPool) -> Option<String> {
    if let Ok(Some(v)) = wikiatlas_store::secrets::get_secret(pool, "access.password").await {
        if !v.is_empty() {
            return Some(v);
        }
    }
    std::env::var("WIKIATLAS_ACCESS_PASSWORD")
        .ok()
        .filter(|v| !v.is_empty())
}

fn token(password: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"wikiatlas-session-v1\0");
    h.update(password.as_bytes());
    format!("{:x}", h.finalize())
}

fn cookie_value(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(k, v)| (k == COOKIE).then(|| v.to_string()))
}

fn same(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes()
        .iter()
        .zip(b.as_bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

pub async fn status(State(pool): State<PgPool>, headers: HeaderMap) -> Json<serde_json::Value> {
    let configured = password(&pool).await;
    let authed = match &configured {
        None => true,
        Some(p) => cookie_value(&headers).is_some_and(|v| same(&v, &token(p))),
    };
    Json(json!({ "configured": configured.is_some(), "authenticated": authed }))
}

pub async fn login(State(pool): State<PgPool>, Json(req): Json<crate::dto::LoginReq>) -> Response {
    let Some(password) = password(&pool).await else {
        return Json(json!({ "ok": true, "configured": false })).into_response();
    };
    if !same(&req.password, &password) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthorized", "message": "访问密码不正确" })),
        )
            .into_response();
    }
    let cookie = format!(
        "{COOKIE}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=604800",
        token(&password)
    );
    (
        [(header::SET_COOKIE, cookie)],
        Json(json!({ "ok": true, "configured": true })),
    )
        .into_response()
}

pub async fn logout() -> Response {
    (
        [(
            header::SET_COOKIE,
            format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"),
        )],
        Json(json!({ "ok": true })),
    )
        .into_response()
}

pub async fn require(State(pool): State<PgPool>, req: Request, next: Next) -> Response {
    let path = req.uri().path();
    if matches!(path, "/api/healthz" | "/api/login" | "/api/auth") {
        return next.run(req).await;
    }
    let Some(password) = password(&pool).await else {
        return next.run(req).await;
    };
    if cookie_value(req.headers()).is_some_and(|v| same(&v, &token(&password))) {
        next.run(req).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthorized", "message": "需要访问密码登录" })),
        )
            .into_response()
    }
}

//! M7 源客户端真 HTTP：GameAtlas 全文+摘要，Komga 系列/单册摘要。

use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::routing::{patch, post, put};
use axum::{Json, Router};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use wikiatlas_sources::config::{GameAtlasConfig, KomgaConfig};
use wikiatlas_sources::gameatlas::GameAtlasClient;
use wikiatlas_sources::komga::KomgaClient;

async fn serve(app: Router) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn gameatlas_反哺带全文摘要且沿用登录会话() {
    type Seen = Arc<Mutex<Option<Value>>>;
    let seen: Seen = Arc::default();

    async fn login() -> impl axum::response::IntoResponse {
        let mut h = HeaderMap::new();
        h.insert(
            header::SET_COOKIE,
            HeaderValue::from_static("gameatlas_admin=sess-1; Path=/"),
        );
        (
            h,
            Json(json!({ "success": true, "data": { "is_admin": true } })),
        )
    }
    async fn wiki(
        State(seen): State<Seen>,
        headers: HeaderMap,
        Path(id): Path<String>,
        Json(body): Json<Value>,
    ) -> impl axum::response::IntoResponse {
        let cookie = headers
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if !cookie.contains("gameatlas_admin=sess-1") {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "success": false, "error": "no session", "data": null })),
            );
        }
        *seen.lock().unwrap() = Some(json!({ "id": id, "body": body }));
        (
            StatusCode::OK,
            Json(json!({ "success": true, "data": { "game_id": 1 } })),
        )
    }

    let app = Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/games/{id}/wiki", put(wiki))
        .with_state(seen.clone());
    let base = serve(app).await;
    let client = GameAtlasClient::connect(&GameAtlasConfig {
        base_url: base,
        password: "ga-pw".into(),
    })
    .await
    .unwrap();
    client
        .push_wiki("abc", "# Wiki\n\n完整正文。", Some("一段摘要。"))
        .await
        .unwrap();

    let seen = seen.lock().unwrap().clone().unwrap();
    assert_eq!(seen["id"], "abc");
    assert_eq!(seen["body"]["content"], "# Wiki\n\n完整正文。");
    assert_eq!(seen["body"]["summary"], "一段摘要。");
    assert_eq!(seen["body"]["change_summary"], "反哺自 Atlas");
}

#[tokio::test]
async fn komga_先试系列_404后落单册摘要() {
    type Seen = Arc<Mutex<Vec<(String, Value)>>>;
    let seen: Seen = Arc::default();

    async fn series(
        Path(_id): Path<String>,
        Json(_body): Json<Value>,
    ) -> StatusCode {
        StatusCode::NOT_FOUND
    }
    async fn book(
        State(seen): State<Seen>,
        Path(id): Path<String>,
        Json(body): Json<Value>,
    ) -> StatusCode {
        seen.lock().unwrap().push((id, body));
        StatusCode::NO_CONTENT
    }

    let app = Router::new()
        .route("/api/v1/series/{id}/metadata", patch(series))
        .route("/api/v1/books/{id}/metadata", patch(book))
        .with_state(seen.clone());
    let base = serve(app).await;
    let client = KomgaClient::connect(&KomgaConfig {
        base_url: format!("{base}/api/v1"),
        api_key: "kk".into(),
    })
    .unwrap();
    client.push_summary("book-1", "漫画简介").await.unwrap();

    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, "book-1");
    assert_eq!(seen[0].1["summary"], "漫画简介");
}

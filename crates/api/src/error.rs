//! 把 store / domain 的错误映射成 HTTP。
//!
//! 规矩：**领域错误有明确的状态码**，非领域错误一律 500 且只记日志、不外泄细节。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

pub struct ApiError(pub wikiatlas_store::Error);

impl From<wikiatlas_store::Error> for ApiError {
    fn from(e: wikiatlas_store::Error) -> Self {
        ApiError(e)
    }
}

impl From<wikiatlas_domain::Error> for ApiError {
    fn from(e: wikiatlas_domain::Error) -> Self {
        ApiError(wikiatlas_store::Error::Domain(e))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        use wikiatlas_domain::Error as D;
        use wikiatlas_store::Error as E;

        let (status, body) = match &self.0 {
            E::Domain(D::NotFound(what)) => (
                StatusCode::NOT_FOUND,
                json!({ "error": "not_found", "message": what }),
            ),
            E::Domain(D::Duplicated(what)) => (
                StatusCode::CONFLICT,
                json!({ "error": "duplicated", "message": what }),
            ),
            E::Domain(D::Conflict { base_ver, current_ver }) => (
                StatusCode::CONFLICT,
                json!({
                    "error": "version_conflict",
                    "message": format!("正文已被别人改过：你的草稿基于 v{base_ver}，库里已是 v{current_ver}"),
                    "baseVer": base_ver,
                    "currentVer": current_ver,
                }),
            ),
            E::Domain(D::Validation(msg)) => (
                StatusCode::BAD_REQUEST,
                json!({ "error": "validation", "message": msg }),
            ),
            other => {
                tracing::error!(error = %other, "内部错误");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({ "error": "internal", "message": "服务器内部错误" }),
                )
            }
        };

        (status, Json(body)).into_response()
    }
}

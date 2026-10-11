//! 媒体库源客户端（`docs/library-sync.md` §1）。
//!
//! **零 LLM**：这一层只把三个库拉成 `ManifestEntry`，不做任何判断。
//! 增量缓存、比对、落库交给 `scan` + `store`（`docs/library-sync.md` §3）。
//!
//! 边界纪律（`docs/backend.md` §3）：`domain` 不碰 IO；这一层只碰 HTTP，不碰 SQL。

pub mod config;
pub mod emby;
pub mod gameatlas;
pub mod komga;
pub mod scan;

use thiserror::Error;

/// 源客户端统一错误：**不带密钥**（消息可能进 `scan_runs`）。
#[derive(Debug, Error)]
pub enum SourceError {
    #[error("未配置：{0}")]
    NotConfigured(String),
    #[error("连不上 {src}：{message}")]
    Http { src: &'static str, message: String },
    #[error("{src} 返回异常：{message}")]
    BadResponse { src: &'static str, message: String },
    #[error("存储错误：{0}")]
    Store(#[from] wikiatlas_store::Error),
    #[error("库里没有可用用户/条目（{0}）")]
    Empty(String),
}

pub type Result<T> = std::result::Result<T, SourceError>;

/// 统一的 HTTP 超时：库在局域网，20s 足够。
pub(crate) fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .cookie_store(true) // GameAtlas 靠登录 Cookie 带会话
        .build()
        .map_err(|e| SourceError::BadResponse {
            src: "http",
            message: e.to_string(),
        })
}

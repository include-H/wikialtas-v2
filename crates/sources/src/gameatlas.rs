//! GameAtlas 客户端（`docs/library-sync.md` §1）：管理员密码换会话 Cookie，双向源（M1 只读）。
//!
//! 与 v1 一致：`/api/auth/login` 换 Cookie（reqwest 的 cookie store 自动带），
//! `/api/games/all` 全量精简目录。M1 不做增量 `/games/recent`（清单落库后由 hash 比对）。

use crate::config::GameAtlasConfig;
use crate::{http_client, Result, SourceError};
use serde::Deserialize;
use wikiatlas_domain::library::{ContainerKind, ManifestEntry, Source};

const SRC: &str = "gameatlas";

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    success: bool,
    #[serde(default)]
    error: String,
    data: Option<T>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct Game {
    public_id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    title_alt: Option<String>,
    #[serde(default)]
    release_date: Option<String>,
    #[serde(default)]
    cover_image: Option<String>,
    #[serde(default)]
    series: Option<Series>,
    #[serde(default)]
    summary: Option<String>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct Series {
    #[serde(default)]
    id: Option<i64>,
    #[serde(default)]
    name: String,
}

pub struct GameAtlasClient {
    http: reqwest::Client,
    base_url: String,
}

impl GameAtlasClient {
    /// 建客户端并**立即登录**（失败即未就绪，扫描时跳过这个源）。
    pub async fn connect(cfg: &GameAtlasConfig) -> Result<Self> {
        let base = cfg.base_url.trim().trim_end_matches('/').to_string();
        if base.is_empty() || cfg.password.trim().is_empty() {
            return Err(SourceError::NotConfigured(SRC.into()));
        }
        let http = http_client()?;
        let c = Self { http, base_url: base };
        c.login(&cfg.password).await?;
        Ok(c)
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    async fn login(&self, password: &str) -> Result<()> {
        let resp = self
            .http
            .post(format!("{}/api/auth/login", self.base_url))
            .json(&serde_json::json!({ "password": password }))
            .send()
            .await
            .map_err(|e| SourceError::Http { src: SRC, message: e.to_string() })?;
        let status = resp.status();
        let env: Envelope<serde_json::Value> = resp
            .json()
            .await
            .map_err(|e| SourceError::BadResponse { src: SRC, message: e.to_string() })?;
        if !status.is_success() || !env.success {
            return Err(SourceError::BadResponse {
                src: SRC,
                message: if env.error.is_empty() {
                    format!("登录失败（HTTP {status}）")
                } else {
                    format!("登录失败：{}", env.error)
                },
            });
        }
        Ok(())
    }

    /// 全量精简目录 → 清单条目（容器 = GA 系列）。
    pub async fn fetch_manifest(&self) -> Result<Vec<ManifestEntry>> {
        let resp = self
            .http
            .get(format!("{}/api/games/all", self.base_url))
            .send()
            .await
            .map_err(|e| SourceError::Http { src: SRC, message: e.to_string() })?;
        let status = resp.status();
        let env: Envelope<Vec<Game>> = resp
            .json()
            .await
            .map_err(|e| SourceError::BadResponse { src: SRC, message: e.to_string() })?;
        if !status.is_success() || !env.success {
            return Err(SourceError::BadResponse {
                src: SRC,
                message: format!("目录拉取失败（HTTP {status}）：{}", env.error),
            });
        }
        let games = env.data.unwrap_or_default();
        Ok(games.into_iter().map(|g| self.to_entry(g)).collect())
    }

    fn to_entry(&self, g: Game) -> ManifestEntry {
        let raw = serde_json::to_value(&g).unwrap_or(serde_json::Value::Null);
        let (container_key, container_title, extra) = match &g.series {
            Some(s) if !s.name.trim().is_empty() => (
                format!("series:{}", s.id.map(|i| i.to_string()).unwrap_or_else(|| s.name.clone())),
                s.name.clone(),
                format!("GA系列：{}", s.name),
            ),
            _ => ("series:none".to_string(), "（无系列）".to_string(), String::new()),
        };
        let aliases = g.title_alt.iter().filter(|s| !s.trim().is_empty()).cloned().collect();
        ManifestEntry {
            source: Source::GameAtlas,
            external_id: g.public_id.clone(),
            title: g.title,
            subtitle: None,
            aliases,
            year: g.release_date.as_deref().and_then(|d| d.get(0..4)).and_then(|y| y.parse().ok()),
            kind: "game".into(),
            format: None,
            cover_url: g.cover_image.as_deref().map(|c| self.absolutize(c)),
            url: Some(format!("{}/games/{}", self.base_url, g.public_id)),
            container_key,
            container_title,
            container_kind: ContainerKind::Series,
            extra,
            raw,
        }
    }

    fn absolutize(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            path.to_string()
        } else {
            format!("{}/{}", self.base_url, path.trim_start_matches('/'))
        }
    }
}

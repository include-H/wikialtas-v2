//! 源配置：从 `secrets` 表读地址 / 密钥（`docs/library-sync.md` §1）。
//!
//! **M1 临时入口**：设置页还没做，所以允许环境变量兜底（便于本地/CI 真跑）。
//! 设置页落地后删掉环境变量那一半——密钥的唯一真相是 `secrets` 表。
//! **这里的值永远不许进 API DTO**（`docs/database.md` §4）。

use crate::Result;
use sqlx::PgPool;
use wikiatlas_store::secrets;

#[derive(Debug, Clone)]
pub struct GameAtlasConfig {
    pub base_url: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub struct EmbyConfig {
    pub base_url: String,
    pub token: String,
    /// 被标成「混杂」的库（view id 或 view 名）。空 = 全按正片处理。
    pub mixed_views: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct KomgaConfig {
    pub base_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Default)]
pub struct SourceConfig {
    pub gameatlas: Option<GameAtlasConfig>,
    pub emby: Option<EmbyConfig>,
    pub komga: Option<KomgaConfig>,
}

impl SourceConfig {
    /// 有哪些源配好了（池子页顶部提示用；只有布尔，没有值）。
    pub fn configured(&self) -> Vec<&'static str> {
        let mut out = vec![];
        if self.gameatlas.is_some() {
            out.push("gameatlas");
        }
        if self.emby.is_some() {
            out.push("emby");
        }
        if self.komga.is_some() {
            out.push("komga");
        }
        out
    }
}

/// 组合 DB 值 + 环境变量兜底。两个都没有 = 这个源未配置。
async fn value(pool: &PgPool, key: &str, env: &str) -> Result<Option<String>> {
    if let Some(v) = secrets::get_secret(pool, key).await? {
        if !v.trim().is_empty() {
            return Ok(Some(v));
        }
    }
    Ok(std::env::var(env).ok().filter(|v| !v.trim().is_empty()))
}

pub async fn load(pool: &PgPool) -> Result<SourceConfig> {
    let gameatlas = match (
        value(pool, "gameatlas.base_url", "WIKIATLAS_GA_URL").await?,
        value(pool, "gameatlas.password", "WIKIATLAS_GA_PASSWORD").await?,
    ) {
        (Some(base_url), Some(password)) => Some(GameAtlasConfig { base_url, password }),
        _ => None,
    };

    let emby = match (
        value(pool, "emby.base_url", "WIKIATLAS_EMBY_URL").await?,
        value(pool, "emby.token", "WIKIATLAS_EMBY_TOKEN").await?,
    ) {
        (Some(base_url), Some(token)) => {
            let mixed_views = secrets::get_setting(pool, "library.emby_mixed_views")
                .await?
                .and_then(|v| v.as_array().map(|a| {
                    a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()
                }))
                .unwrap_or_default();
            Some(EmbyConfig { base_url, token, mixed_views })
        }
        _ => None,
    };

    let komga = match (
        value(pool, "komga.base_url", "WIKIATLAS_KOMGA_URL").await?,
        value(pool, "komga.api_key", "WIKIATLAS_KOMGA_API_KEY").await?,
    ) {
        (Some(base_url), Some(api_key)) => Some(KomgaConfig { base_url, api_key }),
        _ => None,
    };

    Ok(SourceConfig { gameatlas, emby, komga })
}

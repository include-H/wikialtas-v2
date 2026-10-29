//! 配置与密钥（`app_settings` / `secrets`）。
//!
//! **纪律**：`secrets` 的**值绝不进任何 API DTO**（`docs/database.md` §4）。
//! 读值只给后端自己用（`sources` 建客户端）；对外只暴露「有没有配」。
//! 键名约定：`<source>.base_url` / `gameatlas.password` / `emby.token` / `komga.api_key`。

use crate::Result;
use sqlx::PgPool;

/// 读一个密钥原文。**只能在后端内部调**，别塞进返回给前端的结构。
pub async fn get_secret(pool: &PgPool, key: &str) -> Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM secrets WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(v,)| v))
}

/// 写一个密钥（覆盖）。
pub async fn put_secret(pool: &PgPool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO secrets (key, value, updated_at) VALUES ($1, $2, now()) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// 读一个配置（`app_settings`，非密钥）。返回 None = 没设过。
pub async fn get_setting(pool: &PgPool, key: &str) -> Result<Option<serde_json::Value>> {
    let row: Option<(serde_json::Value,)> =
        sqlx::query_as("SELECT value FROM app_settings WHERE key = $1")
            .bind(key)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(v,)| v))
}

/// 写一个配置（覆盖）。
pub async fn put_setting(pool: &PgPool, key: &str, value: &serde_json::Value) -> Result<()> {
    sqlx::query(
        "INSERT INTO app_settings (key, value, updated_at) VALUES ($1, $2, now()) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// 只列「哪些键配了」——给设置页判断显示，**不带值**。
pub async fn list_keys(pool: &PgPool) -> Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT key FROM secrets ORDER BY key")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(k,)| k).collect())
}

/// 列出全部非密钥配置（值可以回显给设置页）。
pub async fn list_settings(
    pool: &PgPool,
) -> Result<Vec<(String, serde_json::Value)>> {
    let rows: Vec<(String, serde_json::Value)> =
        sqlx::query_as("SELECT key, value FROM app_settings ORDER BY key")
            .fetch_all(pool)
            .await?;
    Ok(rows)
}

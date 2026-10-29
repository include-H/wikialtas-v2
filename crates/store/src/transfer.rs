//! 项目树导出 / 导入：只搬耐久内容，不搬密钥、运行日志与媒体库缓存。

use crate::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use wikiatlas_domain as domain;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectBundle {
    pub format_version: i32,
    pub exported_at: DateTime<Utc>,
    pub nodes: serde_json::Value,
    pub contents: serde_json::Value,
    pub versions: serde_json::Value,
    pub relations: serde_json::Value,
    pub doc_refs: serde_json::Value,
    pub settings: serde_json::Value,
}

pub async fn export(pool: &PgPool) -> Result<ProjectBundle> {
    let nodes: (serde_json::Value,) = sqlx::query_as(
        "WITH RECURSIVE ranked AS ( \
           SELECT n.*, 0 AS depth FROM nodes n WHERE parent_id IS NULL \
           UNION ALL \
           SELECT c.*, p.depth + 1 FROM nodes c JOIN ranked p ON c.parent_id = p.id \
         ) SELECT COALESCE(jsonb_agg(to_jsonb(ranked) - 'depth' ORDER BY depth, sort_key), '[]') \
           FROM ranked",
    )
    .fetch_one(pool)
    .await?;
    let contents: (serde_json::Value,) =
        sqlx::query_as("SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY node_id), '[]') FROM contents t")
            .fetch_one(pool)
            .await?;
    let versions: (serde_json::Value,) = sqlx::query_as(
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY node_id, ver), '[]') FROM content_versions t",
    )
    .fetch_one(pool)
    .await?;
    let relations: (serde_json::Value,) =
        sqlx::query_as("SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY created_at), '[]') FROM relations t")
            .fetch_one(pool)
            .await?;
    let doc_refs: (serde_json::Value,) =
        sqlx::query_as("SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY doc_id), '[]') FROM doc_refs t")
            .fetch_one(pool)
            .await?;
    let settings: (serde_json::Value,) = sqlx::query_as(
        "SELECT COALESCE(jsonb_object_agg(key, value), '{}') FROM app_settings",
    )
    .fetch_one(pool)
    .await?;
    Ok(ProjectBundle {
        format_version: 1,
        exported_at: Utc::now(),
        nodes: nodes.0,
        contents: contents.0,
        versions: versions.0,
        relations: relations.0,
        doc_refs: doc_refs.0,
        settings: settings.0,
    })
}

/// 导入到**空项目树**；全部在一个事务里。已有节点时拒绝，避免 ID 撞车与半合并。
pub async fn import(pool: &PgPool, bundle: &ProjectBundle) -> Result<usize> {
    if bundle.format_version != 1 {
        return Err(Error::Domain(domain::Error::Validation(format!(
            "不支持的导出格式版本：{}",
            bundle.format_version
        ))));
    }
    let mut tx = pool.begin().await?;
    let (existing,): (i64,) = sqlx::query_as("SELECT count(*) FROM nodes")
        .fetch_one(&mut *tx)
        .await?;
    if existing > 0 {
        return Err(Error::Domain(domain::Error::Validation(
            "导入只允许写进空树；当前已有节点，避免覆盖或半合并".into(),
        )));
    }
    sqlx::query(
        "INSERT INTO nodes SELECT * FROM jsonb_populate_recordset(NULL::nodes, $1)",
    )
    .bind(&bundle.nodes)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO contents SELECT * FROM jsonb_populate_recordset(NULL::contents, $1)",
    )
    .bind(&bundle.contents)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO content_versions \
           SELECT * FROM jsonb_populate_recordset(NULL::content_versions, $1)",
    )
    .bind(&bundle.versions)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO relations SELECT * FROM jsonb_populate_recordset(NULL::relations, $1)",
    )
    .bind(&bundle.relations)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO doc_refs SELECT * FROM jsonb_populate_recordset(NULL::doc_refs, $1)",
    )
    .bind(&bundle.doc_refs)
    .execute(&mut *tx)
    .await?;
    if let Some(settings) = bundle.settings.as_object() {
        for (key, value) in settings {
            sqlx::query(
                "INSERT INTO app_settings (key, value, updated_at) VALUES ($1, $2, now()) \
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
            )
            .bind(key)
            .bind(value)
            .execute(&mut *tx)
            .await?;
        }
    }
    let count = bundle.nodes.as_array().map(|a| a.len()).unwrap_or(0);
    tx.commit().await?;
    Ok(count)
}

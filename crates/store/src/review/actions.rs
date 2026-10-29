//! Review 接受时的安全联动动作（从 `review.rs` 拆出，D41）。

use super::SuggestionRow;
use crate::{Error, Result};
use serde_json::Value;
use uuid::Uuid;
use wikiatlas_domain as domain;
use domain::library::{LinkedBy, Source};
use domain::review::SuggestionKind;
use domain::{Medium, NewNode, NodeKind};

pub(super) async fn execute(
    tx: &mut sqlx::PgConnection,
    row: &SuggestionRow,
) -> Result<String> {
    match SuggestionKind::parse(&row.kind)? {
        SuggestionKind::Create => create(tx, row).await,
        SuggestionKind::Link => link(tx, row).await,
        SuggestionKind::Ignore => ignore(tx, row).await,
        SuggestionKind::Merge | SuggestionKind::Research | SuggestionKind::Split => {
            Ok("accepted_for_manual_followup".into())
        }
    }
}

async fn create(tx: &mut sqlx::PgConnection, row: &SuggestionRow) -> Result<String> {
    let title = row
        .payload
        .get("title")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            Error::Domain(domain::Error::Validation(
                "create 建议缺 payload.title".into(),
            ))
        })?;
    let node_kind = NodeKind::parse(
        row.payload
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("work"),
    )?;
    let medium = row
        .payload
        .get("medium")
        .and_then(Value::as_str)
        .map(Medium::parse)
        .transpose()?;
    let parent_id = row
        .payload
        .get("parent_id")
        .and_then(Value::as_str)
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|e| {
            Error::Domain(domain::Error::Validation(format!(
                "parent_id 不是 UUID：{e}"
            )))
        })?;
    let node = crate::nodes::create_in_tx(
        tx,
        &NewNode {
            parent_id,
            kind: node_kind,
            medium,
            title: title.to_string(),
            aliases: string_array(&row.payload, "aliases"),
            tags: string_array(&row.payload, "tags"),
        },
    )
    .await?;
    Ok(format!("created:{}", node.id))
}

async fn link(tx: &mut sqlx::PgConnection, row: &SuggestionRow) -> Result<String> {
    let node_id = row
        .payload
        .get("node_id")
        .and_then(Value::as_str)
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| {
            Error::Domain(domain::Error::Validation(
                "link.node_id 不是 UUID".into(),
            ))
        })?
        .or(row.node_id)
        .ok_or_else(|| {
            Error::Domain(domain::Error::Validation(
                "link 建议缺 node_id".into(),
            ))
        })?;
    let source = row
        .source
        .as_deref()
        .map(Source::parse)
        .transpose()?
        .ok_or_else(|| {
            Error::Domain(domain::Error::Validation(
                "link 建议缺 source".into(),
            ))
        })?;
    let external_id = row.external_id.as_deref().ok_or_else(|| {
        Error::Domain(domain::Error::Validation(
            "link 建议缺 external_id".into(),
        ))
    })?;
    let manifest: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT title, url FROM library_manifest WHERE source = $1 AND external_id = $2",
    )
    .bind(source.as_str())
    .bind(external_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (title, url) = manifest.ok_or_else(|| {
        Error::Domain(domain::Error::NotFound(
            "清单里没有这条，先扫描媒体库".into(),
        ))
    })?;
    let role = row
        .payload
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("work");
    sqlx::query(
        "INSERT INTO library_links \
           (id, node_id, source, external_id, url, title_hint, role, linked_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(Uuid::now_v7())
    .bind(node_id)
    .bind(source.as_str())
    .bind(external_id)
    .bind(url)
    .bind(title)
    .bind(role)
    .bind(LinkedBy::Manual.as_str())
    .execute(&mut *tx)
    .await
    .map_err(|e| crate::map_unique(e, "这个条目已经挂过链"))?;
    Ok("linked".into())
}

async fn ignore(tx: &mut sqlx::PgConnection, row: &SuggestionRow) -> Result<String> {
    let source = row
        .source
        .as_deref()
        .map(Source::parse)
        .transpose()?;
    sqlx::query(
        "INSERT INTO library_ignores \
           (id, source, external_id, group_key, reason, created_by) \
         VALUES ($1, $2, $3, $4, $5, 'human:review')",
    )
    .bind(Uuid::now_v7())
    .bind(source.map(|s| s.as_str()))
    .bind(&row.external_id)
    .bind(row.payload.get("group_key").and_then(Value::as_str))
    .bind(
        row.payload
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("Review 接受忽略"),
    )
    .execute(&mut *tx)
    .await?;
    Ok("ignored".into())
}

fn string_array(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

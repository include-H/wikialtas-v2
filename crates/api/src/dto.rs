//! 线上的请求 / 响应形状。
//!
//! 字段名就是 Rust 的 snake_case（serde 默认）——前端按这个写，别两边各起一套命名。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wikiatlas_domain as domain;

#[derive(Debug, Deserialize)]
pub struct CreateNodeReq {
    pub parent_id: Option<Uuid>,
    pub kind: String,
    pub medium: Option<String>,
    pub title: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl CreateNodeReq {
    pub fn into_new(self) -> Result<domain::NewNode, domain::Error> {
        Ok(domain::NewNode {
            parent_id: self.parent_id,
            kind: domain::NodeKind::parse(&self.kind)?,
            medium: self.medium.as_deref().map(domain::Medium::parse).transpose()?,
            title: self.title,
            aliases: self.aliases,
            tags: self.tags,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct DraftReq {
    pub body_md: String,
    /// 这份草稿基于哪一版定稿（前端手里那份的 ver；还没有定稿就是 0）
    pub base_ver: i32,
}

#[derive(Debug, Deserialize)]
pub struct CommitReq {
    pub body_md: String,
    pub base_ver: i32,
    #[serde(default)]
    pub summary: String,
}

#[derive(Debug, Serialize)]
pub struct CommitResp {
    pub ver: i32,
    pub updated_at: DateTime<Utc>,
}

/// 手动扫描的入参（缺省 = 扫全部已配置的源）。
#[derive(Debug, Default, Deserialize)]
pub struct ScanReq {
    #[serde(default)]
    pub sources: Vec<String>,
}

/// 清单 / 池子的查询参数。
#[derive(Debug, Deserialize)]
pub struct LibraryQuery {
    pub source: Option<String>,
    pub container: Option<String>,
    pub has_doc: Option<bool>,
    #[serde(default)]
    pub include_missing: bool,
}

impl LibraryQuery {
    pub fn into_filter(self) -> Result<wikiatlas_store::library::ManifestFilter, domain::Error> {
        Ok(wikiatlas_store::library::ManifestFilter {
            source: self.source.as_deref().map(domain::library::Source::parse).transpose()?,
            container_key: self.container,
            has_doc: self.has_doc,
            include_missing: self.include_missing,
            limit: 0,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct VersionResp {
    pub ver: i32,
    pub created_at: DateTime<Utc>,
    pub summary: String,
}

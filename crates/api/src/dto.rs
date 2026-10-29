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
            medium: self
                .medium
                .as_deref()
                .map(domain::Medium::parse)
                .transpose()?,
            title: self.title,
            aliases: self.aliases,
            tags: self.tags,
        })
    }
}

/// PATCH /api/nodes/{id}：只改人会在 UI 里改的元数据。
/// `move_to_root=true` 时移到根（因为 JSON 里 `parent_id: null` 与“没传”不可区分）。
#[derive(Debug, Deserialize)]
pub struct UpdateNodeReq {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub aliases: Option<Vec<String>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub parent_id: Option<Uuid>,
    #[serde(default)]
    pub move_to_root: bool,
}

impl UpdateNodeReq {
    pub fn into_patch(self) -> wikiatlas_store::nodes::NodePatch {
        let parent_id = if self.move_to_root {
            Some(None)
        } else {
            self.parent_id.map(Some)
        };
        wikiatlas_store::nodes::NodePatch {
            parent_id,
            kind: None,
            medium: None,
            title: self.title,
            aliases: self.aliases,
            tags: self.tags,
            status: None,
            visibility: None,
        }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_job_id: Option<i64>,
}

/// 手动扫描的入参（缺省 = 扫全部已配置的源）。
#[derive(Debug, Default, Deserialize)]
pub struct ScanReq {
    #[serde(default)]
    pub sources: Vec<String>,
}

/// 建档（池子 → 节点 + 链）。
#[derive(Debug, Deserialize)]
pub struct BuildLinkReq {
    pub source: String,
    pub external_id: String,
    /// work | mix，缺省按容器种类（Emby 混杂库 = mix）。
    #[serde(default)]
    pub role: Option<String>,
    /// 用户指定的父节点（不给就按落位规则：作品系列容器找/建系列，库容器进顶级）。
    #[serde(default)]
    pub parent_id: Option<Uuid>,
}

impl BuildLinkReq {
    pub fn into_build(self) -> Result<wikiatlas_store::library::BuildDoc, domain::Error> {
        Ok(wikiatlas_store::library::BuildDoc {
            source: domain::library::Source::parse(&self.source)?,
            external_id: self.external_id,
            role: self
                .role
                .as_deref()
                .map(domain::library::LinkRole::parse)
                .transpose()?,
            parent_id: self.parent_id,
            linked_by: domain::library::LinkedBy::Pool,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct BuiltDocResp {
    pub node: domain::Node,
    pub link_id: Uuid,
    /// 复用了同一容器下已有节点（还是新建的）——前端据此提示。
    pub reused: bool,
}

/// 加一条「不要」（单条 / 整组二选一）。
#[derive(Debug, Deserialize)]
pub struct IgnoreReq {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub group_key: Option<String>,
    #[serde(default)]
    pub reason: String,
}

/// 重挂：把失效链换成同源另一条当前清单条目。
#[derive(Debug, Deserialize)]
pub struct RelinkReq {
    pub external_id: String,
}

/// 某节点的关系查询。
#[derive(Debug, Deserialize)]
pub struct RelationQuery {
    pub node_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct CreateRelationReq {
    pub from_id: Uuid,
    pub to_id: Uuid,
    pub r#type: String,
}

#[derive(Debug, Deserialize)]
pub struct SynthesisReq {
    #[serde(default)]
    pub parent_id: Option<Uuid>,
    pub title: String,
    pub doc_type: String,
    #[serde(default)]
    pub work_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct ReviewQuery {
    pub state: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSuggestionReq {
    pub kind: String,
    #[serde(default)]
    pub node_id: Option<Uuid>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Default, Deserialize)]
pub struct DecisionReq {
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct FindingQuery {
    pub state: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SaveAnswerReq {
    #[serde(default)]
    pub parent_id: Option<Uuid>,
    pub title: String,
    pub body_md: String,
    #[serde(default)]
    pub ref_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct LoginReq {
    pub password: String,
}

/// 起一条工单（M3：只做「照这段话说，去写这个节点的正文」）。
#[derive(Debug, Deserialize)]
pub struct CreateRunReq {
    pub node_id: Uuid,
    pub prompt: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub context: RunContextReq,
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionReq {
    pub node_id: Uuid,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SessionQuery {
    pub node_id: Uuid,
}

#[derive(Debug, Default, Deserialize)]
pub struct SessionMessageQuery {
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RunContextReq {
    #[serde(default)]
    pub doc_mode: Option<String>,
    #[serde(default)]
    pub one_shot: bool,
    #[serde(default)]
    pub skills: Vec<String>,
}

/// SSE 续传的游标。
#[derive(Debug, Deserialize)]
pub struct AfterQuery {
    #[serde(default)]
    pub after: i32,
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
            source: self
                .source
                .as_deref()
                .map(domain::library::Source::parse)
                .transpose()?,
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

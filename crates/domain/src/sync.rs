//! 反哺与重挂的读侧契约（`docs/library-sync.md` §5 §6）。
//!
//! 这一层只有 DTO 与枚举；状态推进和 HTTP 写回分别在 store / sources。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::library::Source;

crate::str_enum!(SyncStatus {
    Pending => "pending",
    Ok => "ok",
    Failed => "failed",
    Skipped => "skipped",
});

/// 一条链在某个可写目标上的反哺状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncTarget {
    pub link_id: Uuid,
    pub target: Source,
    pub synced_ver: Option<i32>,
    pub status: SyncStatus,
    pub message: String,
    pub synced_at: Option<DateTime<Utc>>,
}

/// 失效链的重挂候选：同一来源、当前仍在清单里且尚未挂链的条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelinkCandidate {
    pub source: Source,
    pub external_id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub aliases: Vec<String>,
    pub year: Option<i32>,
    pub cover_url: Option<String>,
    pub url: Option<String>,
    pub container_title: String,
    pub score: f64,
}

/// worker 执行一次反哺所需的最小事实。
#[derive(Debug, Clone)]
pub struct SyncJob {
    pub link_id: Uuid,
    pub node_id: Uuid,
    pub source: Source,
    pub external_id: String,
    pub role: String,
    pub ver: i32,
    pub body_md: String,
}

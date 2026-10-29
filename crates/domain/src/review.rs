//! Review 队列与体检结果的读侧契约（`docs/ai.md §9`）。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::library::Source;

crate::str_enum!(SuggestionKind {
    Create => "create",
    Link => "link",
    Merge => "merge",
    Research => "research",
    Ignore => "ignore",
    Split => "split",
});

crate::str_enum!(SuggestionState {
    Pending => "pending",
    Accepted => "accepted",
    Rejected => "rejected",
    Ignored => "ignored",
});

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub id: Uuid,
    pub kind: SuggestionKind,
    pub node_id: Option<Uuid>,
    pub source: Option<Source>,
    pub external_id: Option<String>,
    pub payload: serde_json::Value,
    pub state: SuggestionState,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
}

crate::str_enum!(FindingSeverity {
    Info => "info",
    Warn => "warn",
    Error => "error",
});

crate::str_enum!(FindingState {
    Open => "open",
    Resolved => "resolved",
    Ignored => "ignored",
});

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthFinding {
    pub id: Uuid,
    pub kind: String,
    pub node_id: Option<Uuid>,
    pub severity: FindingSeverity,
    pub detail: serde_json::Value,
    pub state: FindingState,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

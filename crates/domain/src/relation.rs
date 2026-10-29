//! 类型化关系与综合页（`docs/features.md §6.5`）。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

crate::str_enum!(SynthesisType {
    Role => "role",
    Worldbuilding => "worldbuilding",
    Timeline => "timeline",
    Theme => "theme",
    Creator => "creator",
});

impl SynthesisType {
    pub fn label(self) -> &'static str {
        match self {
            SynthesisType::Role => "角色",
            SynthesisType::Worldbuilding => "世界观",
            SynthesisType::Timeline => "时间线",
            SynthesisType::Theme => "主题",
            SynthesisType::Creator => "创作者",
        }
    }
}

/// 关系类型契约行（方向标签的唯一来源是数据库 `relation_types`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationType {
    pub r#type: String,
    pub from_label: String,
    pub to_label: String,
    pub note: String,
}

/// 关系视图里的另一端节点摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationNode {
    pub id: Uuid,
    pub title: String,
    pub kind: String,
    pub medium: Option<String>,
}

/// `out` = 本节点是 from；`in` = 本节点是 to。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationView {
    pub id: Uuid,
    pub r#type: String,
    pub direction: String,
    /// 从本节点视角读的方向标签（例：本节点是续作 → “前作”指另一端）。
    pub label: String,
    pub other: RelationNode,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

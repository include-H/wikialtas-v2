//! WikiAtlas 的**纯**领域层：类型 + 规则，零 IO。
//!
//! 纪律（`docs/backend.md` §3）：这一层不依赖 sqlx / axum / tokio；
//! 规则写在类型与函数里、配单测——v1 的教训正是「规则写在 httpapi 里，没有测试基建」。

pub mod library;
pub mod sortkey;
pub mod text;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 把「枚举 ↔ 数据库里的小写字符串」的样板收进一处，免得四处漏写法漂移。
#[macro_export]
macro_rules! str_enum {
    ($name:ident { $($variant:ident => $s:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum $name { $(#[serde(rename = $s)] $variant),+ }

        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $($name::$variant => $s),+ }
            }
            pub fn parse(s: &str) -> std::result::Result<Self, $crate::Error> {
                match s {
                    $($s => Ok($name::$variant),)+
                    other => Err($crate::Error::Validation(format!("未知取值：{other}"))),
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = $crate::Error;
            fn from_str(s: &str) -> std::result::Result<Self, Self::Err> { Self::parse(s) }
        }
    };
}

str_enum!(NodeKind {
    Universe => "universe",
    Series => "series",
    Work => "work",
    Doc => "doc",
});

str_enum!(Medium {
    Game => "game",
    Movie => "movie",
    Tv => "tv",
    Manga => "manga",
    Book => "book",
    Other => "other",
});

str_enum!(NodeStatus {
    Draft => "draft",
    Ready => "ready",
    Archived => "archived",
});

str_enum!(Visibility {
    Private => "private",
    Public => "public",
});

// 谁写的（写入事实与快照都记这个）。
str_enum!(WriteSource {
    Llm => "llm",
    Human => "human",
    Host => "host",
    Import => "import",
});

/// 介质与 kind 的搭配规则。落位规则里最容易写错的一档，
/// 所以**两处都落**：这里（带单测）+ 数据库的 CHECK（`docs/database.md` §7）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediumRule {
    Required,
    Optional,
    Forbidden,
}

impl NodeKind {
    pub fn medium_rule(self) -> MediumRule {
        match self {
            NodeKind::Work => MediumRule::Required,
            NodeKind::Doc => MediumRule::Forbidden,
            NodeKind::Universe | NodeKind::Series => MediumRule::Optional,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("找不到：{0}")]
    NotFound(String),
    #[error("已存在：{0}")]
    Duplicated(String),
    #[error("版本冲突：草稿基于 v{base_ver}，库里已是 v{current_ver}")]
    Conflict { base_ver: i32, current_ver: i32 },
    #[error("校验失败：{0}")]
    Validation(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub kind: NodeKind,
    pub medium: Option<Medium>,
    pub title: String,
    pub title_norm: String,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    pub status: NodeStatus,
    pub visibility: Visibility,
    pub sort_key: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 定稿正文。一节点一行，`ver` 是乐观并发的那个版本号。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content {
    pub node_id: Uuid,
    pub body_md: String,
    pub ver: i32,
    pub updated_by: String,
    pub updated_at: DateTime<Utc>,
}

/// 草稿：一行、覆盖写、不产生快照。`base_ver` 记它基于哪一版定稿。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    pub node_id: Uuid,
    pub body_md: String,
    pub base_ver: i32,
    pub by: String,
    pub lease_until: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 一个节点的编辑视角：节点 + 定稿（可能还没有）+ 草稿（可能没有）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeContent {
    pub node: Node,
    pub content: Option<Content>,
    pub draft: Option<Draft>,
}

/// 建节点的入参。校验放这里，store 与 api 都不重复写一遍规则。
#[derive(Debug, Clone)]
pub struct NewNode {
    pub parent_id: Option<Uuid>,
    pub kind: NodeKind,
    pub medium: Option<Medium>,
    pub title: String,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
}

impl NewNode {
    pub fn validate(&self) -> Result<()> {
        if self.title.trim().is_empty() {
            return Err(Error::Validation("标题不能为空".into()));
        }
        match (self.kind.medium_rule(), self.medium) {
            (MediumRule::Required, None) => {
                Err(Error::Validation(format!("{} 必须带介质", self.kind)))
            }
            (MediumRule::Forbidden, Some(m)) => Err(Error::Validation(format!(
                "{} 不应带介质（给了 {m}）",
                self.kind
            ))),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(kind: NodeKind, medium: Option<Medium>) -> NewNode {
        NewNode {
            parent_id: None,
            kind,
            medium,
            title: "  某 作  ".into(),
            aliases: vec![],
            tags: vec![],
        }
    }

    #[test]
    fn 标题不能为空() {
        let mut n = node(NodeKind::Work, Some(Medium::Game));
        n.title = "   ".into();
        assert!(matches!(n.validate(), Err(Error::Validation(_))));
    }

    #[test]
    fn work必须带介质_doc不带介质() {
        assert!(node(NodeKind::Work, None).validate().is_err());
        assert!(node(NodeKind::Work, Some(Medium::Game)).validate().is_ok());
        assert!(node(NodeKind::Doc, Some(Medium::Game)).validate().is_err());
        assert!(node(NodeKind::Doc, None).validate().is_ok());
    }

    #[test]
    fn series与universe的介质可选() {
        assert!(node(NodeKind::Series, None).validate().is_ok());
        assert!(node(NodeKind::Series, Some(Medium::Game)).validate().is_ok());
    }

    #[test]
    fn 枚举与字符串来回() {
        for s in ["universe", "series", "work", "doc"] {
            assert_eq!(NodeKind::parse(s).unwrap().as_str(), s);
        }
        for s in ["game", "movie", "tv", "manga", "book", "other"] {
            assert_eq!(Medium::parse(s).unwrap().as_str(), s);
        }
        assert!(NodeKind::parse("misc").is_err());
    }
}

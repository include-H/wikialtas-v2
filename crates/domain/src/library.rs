//! 媒体库：三源身份、清单条目、链的属性枚举（`docs/library-sync.md` §1–§3）。
//!
//! **零 IO**：这里只有类型与纯规则。拉清单在 `sources` crate，落库在 `store`。
//! 身份 = `source + external_id`（`docs/library-sync.md` §2.2）——标题会变会重名，
//! 所以清单的锚点永远不是标题。

use serde::{Deserialize, Serialize};

crate::str_enum!(Source {
    GameAtlas => "gameatlas",
    Emby => "emby",
    Komga => "komga",
});

// 容器语义（`docs/features.md` §1）：作品系列可当父节点；库（Emby）只当分组。
crate::str_enum!(ContainerKind {
    Series => "series",
    Library => "library",
    // Emby 里被标成「混杂」的库（OST / 预告 / 解析），条目落链时 role=mix。
    Mixed => "mixed",
});

crate::str_enum!(LinkRole {
    Work => "work",
    Mix => "mix",
});

crate::str_enum!(LinkState {
    Active => "active",
    Stale => "stale",
    Replaced => "replaced",
});

crate::str_enum!(LinkedBy {
    Manual => "manual",
    Pool => "pool",
    Archive => "archive",
    Scan => "scan",
});

/// 清单条目：`library_manifest` 一行的领域形态（`docs/library-sync.md` §2.1）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub source: Source,
    pub external_id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub aliases: Vec<String>,
    pub year: Option<i32>,
    /// 库侧类型（movie / tv / game / book / series …），原样保留。
    pub kind: String,
    /// comic | novel（只有 Komga 这类书源才填）。
    pub format: Option<String>,
    pub cover_url: Option<String>,
    pub url: Option<String>,
    pub container_key: String,
    pub container_title: String,
    pub container_kind: ContainerKind,
    pub extra: String,
    /// 源侧原始载荷（GA 的 series 成员、Emby 的 Path 等），别读它的形状做业务判断。
    pub raw: serde_json::Value,
}

impl ManifestEntry {
    /// 匹配的唯一口径（归一化标题）。
    pub fn title_norm(&self) -> String {
        crate::text::normalize_title(&self.title)
    }

    /// 增量缓存指纹（借 llm_wiki 的 `ingest-cache`）：源条目内容没变就跳过重写。
    /// 用 FNV-1a 自己算——**不引哈希依赖**，只要确定、够散即可（不是安全用途）。
    pub fn content_hash(&self) -> String {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |s: &str| {
            for b in s.as_bytes() {
                h ^= *b as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
            h ^= 0xff;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        };
        eat(self.source.as_str());
        eat(&self.external_id);
        eat(&self.title);
        eat(self.subtitle.as_deref().unwrap_or(""));
        eat(&self.aliases.join("\u{1f}"));
        eat(&self.year.map(|y| y.to_string()).unwrap_or_default());
        eat(&self.kind);
        eat(self.format.as_deref().unwrap_or(""));
        eat(self.cover_url.as_deref().unwrap_or(""));
        eat(self.url.as_deref().unwrap_or(""));
        eat(&self.container_key);
        eat(&self.container_title);
        eat(self.container_kind.as_str());
        eat(&self.extra);
        format!("{h:016x}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry() -> ManifestEntry {
        ManifestEntry {
            source: Source::GameAtlas,
            external_id: "abc".into(),
            title: "使命召唤：黑色行动".into(),
            subtitle: None,
            aliases: vec!["Call of Duty: Black Ops".into()],
            year: Some(2010),
            kind: "game".into(),
            format: None,
            cover_url: None,
            url: Some("http://ga/games/abc".into()),
            container_key: "series:cod".into(),
            container_title: "使命召唤".into(),
            container_kind: ContainerKind::Series,
            extra: String::new(),
            raw: json!({}),
        }
    }

    #[test]
    fn 枚举与字符串来回() {
        for s in ["gameatlas", "emby", "komga"] {
            assert_eq!(Source::parse(s).unwrap().as_str(), s);
        }
        for s in ["series", "library", "mixed"] {
            assert_eq!(ContainerKind::parse(s).unwrap().as_str(), s);
        }
        for s in ["active", "stale", "replaced"] {
            assert_eq!(LinkState::parse(s).unwrap().as_str(), s);
        }
    }

    #[test]
    fn serde_与数据库字符串是同一个口径() {
        // D? 教训：`#[serde(rename_all)]` 会把 GameAtlas 写成 "game_atlas"，
        // 而数据库/查询串是 "gameatlas"——两边漂移就是线上事故。这里钉死。
        for s in [Source::GameAtlas, Source::Emby, Source::Komga] {
            let json = serde_json::to_string(&s).unwrap();
            assert_eq!(json, format!("\"{}\"", s.as_str()), "{s:?} 的 JSON 与 as_str 不一致");
            assert_eq!(serde_json::from_str::<Source>(&json).unwrap(), s);
        }
        assert_eq!(serde_json::to_string(&ContainerKind::Series).unwrap(), "\"series\"");
        assert_eq!(serde_json::to_string(&LinkRole::Mix).unwrap(), "\"mix\"");
        assert_eq!(serde_json::to_string(&LinkState::Stale).unwrap(), "\"stale\"");
    }

    #[test]
    fn 指纹随任一契约字段变化() {
        let base = entry().content_hash();
        assert_eq!(base, entry().content_hash(), "同样内容必须同指纹");

        let mut t = entry();
        t.title = "使命召唤：黑色行动 2".into();
        assert_ne!(base, t.content_hash());
    }

    #[test]
    fn 归一化标题与空白无关() {
        let mut a = entry();
        a.title = "  上古 卷轴 ".into();
        let mut b = entry();
        b.title = "上古  卷轴".into();
        assert_eq!(a.title_norm(), b.title_norm());
    }
}

/// 清单里的一行（读侧形态，带生命周期时间戳）。字段对应 `library_manifest`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestRow {
    pub source: Source,
    pub external_id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub aliases: Vec<String>,
    pub year: Option<i32>,
    pub kind: String,
    pub format: Option<String>,
    pub cover_url: Option<String>,
    pub url: Option<String>,
    pub container_key: String,
    pub container_title: String,
    pub container_kind: ContainerKind,
    pub extra: String,
    pub content_hash: String,
    pub seen_at: chrono::DateTime<chrono::Utc>,
    pub first_seen_at: chrono::DateTime<chrono::Utc>,
    pub missing_since: Option<chrono::DateTime<chrono::Utc>>,
}

impl ManifestRow {
    pub fn title_norm(&self) -> String {
        crate::text::normalize_title(&self.title)
    }
}

/// 池子的一行：清单条目 + 它的挂链状态（`has_doc` 就是 link 是否存在）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolEntry {
    pub entry: ManifestRow,
    pub link_id: Option<uuid::Uuid>,
    pub node_id: Option<uuid::Uuid>,
    pub node_title: Option<String>,
    pub node_kind: Option<String>,
    /// 落链时该用的角色（Emby 混杂库 = mix，其余 work）。
    pub role: LinkRole,
}

impl PoolEntry {
    pub fn has_doc(&self) -> bool {
        self.link_id.is_some()
    }
}

/// 一条「不要」（`library_ignores`，`docs/library-sync.md` §4）。
/// 两种范围：忽略**单条**（`external_id`）/ 忽略**整组**（`group_key` = 容器键）。
/// （第三种「别向某个节点提起这个条目」随建议系统走 M5。）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ignore {
    pub id: uuid::Uuid,
    pub source: Option<Source>,
    pub external_id: Option<String>,
    pub group_key: Option<String>,
    pub reason: String,
    pub created_by: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 一次源扫描的记账行（`scan_runs`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRun {
    pub id: i64,
    pub source: Source,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ok: Option<bool>,
    pub stats: serde_json::Value,
    pub message: String,
}

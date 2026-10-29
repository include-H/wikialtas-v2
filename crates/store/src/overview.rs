//! 首页概览 / 节点来源链的读查询（`docs/design.md §6.1 §6.3`）。
//!
//! 从 `nodes` / `library` 里读几组数——**只读、零 LLM、确定性**（D9 的精神：聚合面由宿主维护）。

use crate::{Error, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain as domain;

/// 首页「最近更新」一行。
#[derive(Debug, Clone, Serialize)]
pub struct RecentNode {
    pub id: Uuid,
    pub title: String,
    pub kind: String,
    pub medium: Option<String>,
    pub status: String,
    /// 有没有定稿正文（决定显示「定稿 vN」还是「草稿」）。
    pub has_content: bool,
    /// 定稿版本号（0 = 还没定稿）。
    pub ver: i32,
    pub updated_at: DateTime<Utc>,
}

/// 首页统计卡 + 最近更新（`design.md §6.1`）。
#[derive(Debug, Clone, Serialize)]
pub struct HomeStats {
    pub works: i64,
    pub committed: i64,
    pub drafting: i64,
    pub unbuilt: i64,
    pub recent: Vec<RecentNode>,
}

pub async fn home(pool: &PgPool) -> Result<HomeStats> {
    let (works,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM nodes WHERE kind = 'work' AND status <> 'archived'",
    )
    .fetch_one(pool)
    .await
    .map_err(db)?;
    let (committed,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM nodes n JOIN contents c ON c.node_id = n.id \
         WHERE n.kind = 'work' AND n.status <> 'archived' AND c.ver > 0",
    )
    .fetch_one(pool)
    .await
    .map_err(db)?;
    let (drafting,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM nodes n JOIN content_drafts d ON d.node_id = n.id \
         WHERE n.kind = 'work' AND n.status <> 'archived'",
    )
    .fetch_one(pool)
    .await
    .map_err(db)?;
    // 「未建档」= 清单里有、没有活链、也没被忽略的条目（对齐池子口径）
    let (unbuilt,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM library_manifest m \
         WHERE m.missing_since IS NULL \
           AND NOT EXISTS (SELECT 1 FROM library_links l \
                           WHERE l.source = m.source AND l.external_id = m.external_id \
                             AND l.state <> 'replaced') \
           AND NOT EXISTS (SELECT 1 FROM library_ignores ig \
                           WHERE (ig.source IS NULL OR ig.source = m.source) \
                             AND (ig.external_id = m.external_id \
                                  OR (ig.group_key IS NOT NULL AND ig.group_key = m.container_key)))",
    )
    .fetch_one(pool)
    .await
    .map_err(db)?;

    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        title: String,
        kind: String,
        medium: Option<String>,
        status: String,
        ver: Option<i32>,
        updated_at: DateTime<Utc>,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT n.id, n.title, n.kind, n.medium, n.status, c.ver, n.updated_at \
         FROM nodes n LEFT JOIN contents c ON c.node_id = n.id \
         WHERE n.status <> 'archived' \
           AND NOT (n.kind = 'doc' AND n.tags @> ARRAY['账本']::text[]) \
         ORDER BY n.updated_at DESC LIMIT 12",
    )
    .fetch_all(pool)
    .await
    .map_err(db)?;
    let recent = rows
        .into_iter()
        .map(|r| RecentNode {
            id: r.id,
            title: r.title,
            kind: r.kind,
            medium: r.medium,
            status: r.status,
            has_content: r.ver.unwrap_or(0) > 0,
            ver: r.ver.unwrap_or(0),
            updated_at: r.updated_at,
        })
        .collect();

    Ok(HomeStats { works, committed, drafting, unbuilt, recent })
}

fn db(e: sqlx::Error) -> Error {
    Error::Db(e)
}

/// 节点的来源链（`design.md §6.3` 的来源条）：只给活链用于展示。
#[derive(Debug, Clone, Serialize)]
pub struct NodeLink {
    pub id: Uuid,
    pub source: String,
    pub external_id: String,
    pub title_hint: String,
    pub url: Option<String>,
    pub role: String,
    pub state: String,
    pub missing_since: Option<DateTime<Utc>>,
    pub replaced_by: Option<Uuid>,
    pub sync_targets: Vec<domain::sync::SyncTarget>,
}

impl NodeLink {
    fn from_row(
        r: (
            Uuid,
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            Option<DateTime<Utc>>,
            Option<Uuid>,
        ),
        sync_targets: Vec<domain::sync::SyncTarget>,
    ) -> Result<Self> {
        Ok(NodeLink {
            id: r.0,
            source: domain::library::Source::parse(&r.1)?.as_str().to_string(),
            external_id: r.2,
            title_hint: r.3,
            url: r.4,
            role: r.5,
            state: r.6,
            missing_since: r.7,
            replaced_by: r.8,
            sync_targets,
        })
    }
}

/// 某节点上未删除的链（`replaced` 的也带上，用于「来源已失效」提示）。
pub async fn links_of_node(pool: &PgPool, node_id: Uuid) -> Result<Vec<NodeLink>> {
    let rows: Vec<(
        Uuid,
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        Option<DateTime<Utc>>,
        Option<Uuid>,
    )> = sqlx::query_as(
        "SELECT id, source, external_id, COALESCE(title_hint,''), url, role, state, \
                missing_since, replaced_by \
         FROM library_links WHERE node_id = $1 ORDER BY created_at",
    )
    .bind(node_id)
    .fetch_all(pool)
    .await
    .map_err(db)?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let targets = crate::sync::targets_for_link(pool, r.0).await?;
        out.push(NodeLink::from_row(r, targets)?);
    }
    Ok(out)
}

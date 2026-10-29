//! 内容仓储：定稿（contents）、草稿（content_drafts）、快照（content_versions）、
//! 写入事实（write_events）。
//!
//! **定稿是那个事务**：contents 版本 +1、写一条快照、写一条写入事实、清掉草稿——
//! 四件事要么都成，要么都不成（`docs/database.md` §0.3）。

use crate::{Error, Result};
use chrono::{DateTime, Utc};
use domain::{Content, Draft, NodeContent, WriteSource};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain as domain;

/// 草稿租约：到期即视为没人正在写（人写与模型写共用这一条）。
const DRAFT_LEASE_SECS: i32 = 300;

#[derive(sqlx::FromRow)]
struct ContentRow {
    node_id: Uuid,
    body_md: String,
    ver: i32,
    updated_by: String,
    updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct DraftRow {
    node_id: Uuid,
    body_md: String,
    base_ver: i32,
    by: String,
    lease_until: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<ContentRow> for Content {
    fn from(r: ContentRow) -> Self {
        Content {
            node_id: r.node_id,
            body_md: r.body_md,
            ver: r.ver,
            updated_by: r.updated_by,
            updated_at: r.updated_at,
        }
    }
}

impl From<DraftRow> for Draft {
    fn from(r: DraftRow) -> Self {
        Draft {
            node_id: r.node_id,
            body_md: r.body_md,
            base_ver: r.base_ver,
            by: r.by,
            lease_until: r.lease_until,
            updated_at: r.updated_at,
        }
    }
}

/// 一个节点的编辑视角。前端打开文档时拉这一个就够。
pub async fn read(pool: &PgPool, node_id: Uuid) -> Result<NodeContent> {
    let node = crate::nodes::get(pool, node_id).await?;

    let content: Option<ContentRow> = sqlx::query_as(
        "SELECT node_id, body_md, ver, updated_by, updated_at FROM contents WHERE node_id = $1",
    )
    .bind(node_id)
    .fetch_optional(pool)
    .await?;

    let draft: Option<DraftRow> = sqlx::query_as(
        "SELECT node_id, body_md, base_ver, by, lease_until, updated_at \
         FROM content_drafts WHERE node_id = $1",
    )
    .bind(node_id)
    .fetch_optional(pool)
    .await?;

    Ok(NodeContent {
        node,
        content: content.map(Into::into),
        draft: draft.map(Into::into),
    })
}

/// 丢弃草稿（v2 的「拒绝修订」：回到最近一次定稿，不产生版本）。
/// 没有草稿时返回 `false`，调用方不必当成错误。
pub async fn discard_draft(pool: &PgPool, node_id: Uuid) -> Result<bool> {
    crate::nodes::get(pool, node_id).await?;
    let res = sqlx::query("DELETE FROM content_drafts WHERE node_id = $1")
        .bind(node_id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

/// 存草稿（覆盖写，不产生快照）。前端防抖调它。
pub async fn put_draft(
    pool: &PgPool,
    node_id: Uuid,
    body_md: &str,
    base_ver: i32,
    by: &str,
) -> Result<Draft> {
    // 规范化在写路径的入口做一次；编辑器序列化出来的空段落 `<br />` 在这里被清掉
    let body_md = domain::text::normalize_markdown(body_md);
    if base_ver < 0 {
        return Err(Error::Domain(domain::Error::Validation(
            "base_ver 不能为负".into(),
        )));
    }
    // 节点必须在（顺带拿到当前定稿版本，用于挡「来自未来的草稿」）
    let current: Option<(i32,)> = sqlx::query_as("SELECT ver FROM contents WHERE node_id = $1")
        .bind(node_id)
        .fetch_optional(pool)
        .await?;
    let exists = crate::nodes::get(pool, node_id).await.is_ok();
    if !exists {
        return Err(Error::Domain(domain::Error::NotFound(format!(
            "节点 {node_id}"
        ))));
    }
    let current_ver = current.map(|(v,)| v).unwrap_or(0);
    if base_ver > current_ver {
        return Err(Error::Domain(domain::Error::Validation(format!(
            "草稿基于 v{base_ver}，但库里只有 v{current_ver}"
        ))));
    }

    let row: DraftRow = sqlx::query_as(
        "INSERT INTO content_drafts (node_id, body_md, base_ver, by, lease_until, updated_at) \
         VALUES ($1, $2, $3, $4, now() + make_interval(secs => $5), now()) \
         ON CONFLICT (node_id) DO UPDATE SET \
           body_md = EXCLUDED.body_md, base_ver = EXCLUDED.base_ver, by = EXCLUDED.by, \
           lease_until = EXCLUDED.lease_until, updated_at = now() \
         RETURNING node_id, body_md, base_ver, by, lease_until, updated_at",
    )
    .bind(node_id)
    .bind(body_md)
    .bind(base_ver)
    .bind(by)
    .bind(DRAFT_LEASE_SECS)
    .fetch_one(pool)
    .await?;

    Ok(row.into())
}

#[derive(Debug, Clone)]
pub struct CommitResult {
    pub ver: i32,
    pub updated_at: DateTime<Utc>,
    /// 定稿事务里同时入队的反哺工单；没有可写链时为 None。
    pub sync_job_id: Option<i64>,
}

/// 定稿：**乐观并发**。`base_ver` 与库里不符 → 明确冲突（而不是覆盖别人的改动）。
///
/// 并发靠一条语句扛：`ON CONFLICT ... DO UPDATE ... WHERE contents.ver = $base`。
/// 版本不对 → 0 行受影响 → 不更新任何东西（不需要显式 `FOR UPDATE`）。
pub async fn commit(
    pool: &PgPool,
    node_id: Uuid,
    body_md: &str,
    base_ver: i32,
    summary: &str,
    by: &str,
) -> Result<CommitResult> {
    if base_ver < 0 {
        return Err(Error::Domain(domain::Error::Validation(
            "base_ver 不能为负".into(),
        )));
    }

    let body_md = domain::text::normalize_markdown(body_md);
    let mut tx = pool.begin().await?;

    // 节点必须在（否则下面的外键错误会变成 500，而不是 404）
    let exists: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM nodes WHERE id = $1")
        .bind(node_id)
        .fetch_optional(&mut *tx)
        .await?;
    if exists.is_none() {
        return Err(Error::Domain(domain::Error::NotFound(format!(
            "节点 {node_id}"
        ))));
    }

    let next_ver = base_ver + 1;
    let updated: Option<(i32, DateTime<Utc>)> = sqlx::query_as(
        "INSERT INTO contents (node_id, body_md, ver, updated_by, updated_at) \
         VALUES ($1, $2, $3, $4, now()) \
         ON CONFLICT (node_id) DO UPDATE SET \
           body_md = EXCLUDED.body_md, ver = EXCLUDED.ver, \
           updated_by = EXCLUDED.updated_by, updated_at = now() \
         WHERE contents.ver = $5 \
         RETURNING ver, updated_at",
    )
    .bind(node_id)
    .bind(&body_md)
    .bind(next_ver)
    .bind(by)
    .bind(base_ver)
    .fetch_optional(&mut *tx)
    .await?;

    let Some((ver, updated_at)) = updated else {
        // 版本不符：读回当前版本给前端做冲突提示
        let current: Option<(i32,)> = sqlx::query_as("SELECT ver FROM contents WHERE node_id = $1")
            .bind(node_id)
            .fetch_optional(&mut *tx)
            .await?;
        return Err(Error::Domain(domain::Error::Conflict {
            base_ver,
            current_ver: current.map(|(v,)| v).unwrap_or(0),
        }));
    };

    sqlx::query(
        "INSERT INTO content_versions (id, node_id, ver, body_md, source, summary) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(Uuid::now_v7())
    .bind(node_id)
    .bind(ver)
    .bind(body_md)
    .bind(WriteSource::Human.as_str())
    .bind(summary)
    .execute(&mut *tx)
    .await?;

    sqlx::query("INSERT INTO write_events (node_id, ver, source) VALUES ($1, $2, $3)")
        .bind(node_id)
        .bind(ver)
        .bind(WriteSource::Human.as_str())
        .execute(&mut *tx)
        .await?;

    // 草稿使命完成：定稿已经是更新的真相了
    sqlx::query("DELETE FROM content_drafts WHERE node_id = $1")
        .bind(node_id)
        .execute(&mut *tx)
        .await?;

    // 定稿 → 反哺工单与正文在同一个事务里入队。worker 仍在外侧跑 HTTP（D39）；
    // 这里只保证「定稿成功就一定留下了 sync job」，不会出现中间崩溃丢推送。
    let sync_job_id: Option<(i64,)> = sqlx::query_as(
        "INSERT INTO jobs (kind, payload) \
         SELECT 'sync', jsonb_build_object('node_id', $1::text, 'ver', $2, 'force', false) \
         WHERE EXISTS ( \
           SELECT 1 FROM library_links \
           WHERE node_id = $1 AND state = 'active' AND source IN ('gameatlas','komga') \
             AND role IN ('work','mix')) \
         RETURNING id",
    )
    .bind(node_id)
    .bind(ver)
    .fetch_optional(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(CommitResult {
        ver,
        updated_at,
        sync_job_id: sync_job_id.map(|(id,)| id),
    })
}

/// 某节点的快照列表（倒序）。写正文的「历史版本」面板就用它。
pub async fn versions(
    pool: &PgPool,
    node_id: Uuid,
    limit: i64,
) -> Result<Vec<(i32, DateTime<Utc>, String)>> {
    let rows: Vec<(i32, DateTime<Utc>, String)> = sqlx::query_as(
        "SELECT ver, created_at, summary FROM content_versions \
         WHERE node_id = $1 ORDER BY ver DESC LIMIT $2",
    )
    .bind(node_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 某版快照的正文（版本历史抽屉的预览，`design.md §6.3`）。
pub async fn version_body(pool: &PgPool, node_id: Uuid, ver: i32) -> Result<String> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT body_md FROM content_versions WHERE node_id = $1 AND ver = $2")
            .bind(node_id)
            .bind(ver)
            .fetch_optional(pool)
            .await?;
    row.map(|(b,)| b)
        .ok_or_else(|| Error::Domain(domain::Error::NotFound(format!("节点 {node_id} 的 v{ver}"))))
}

//! 节点仓储：树的读、建，以及 `sort_key` 的分配。

use crate::{map_unique, Error, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain as domain;
use domain::{Medium, NewNode, Node, NodeKind, NodeStatus, Visibility};

#[derive(sqlx::FromRow)]
struct NodeRow {
    id: Uuid,
    parent_id: Option<Uuid>,
    kind: String,
    medium: Option<String>,
    title: String,
    title_norm: String,
    aliases: Vec<String>,
    tags: Vec<String>,
    status: String,
    visibility: String,
    sort_key: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<NodeRow> for Node {
    type Error = Error;
    fn try_from(r: NodeRow) -> Result<Self> {
        Ok(Node {
            id: r.id,
            parent_id: r.parent_id,
            kind: NodeKind::parse(&r.kind)?,
            medium: r.medium.as_deref().map(Medium::parse).transpose()?,
            title: r.title,
            title_norm: r.title_norm,
            aliases: r.aliases,
            tags: r.tags,
            status: NodeStatus::parse(&r.status)?,
            visibility: Visibility::parse(&r.visibility)?,
            sort_key: r.sort_key,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
    }
}

const COLS: &str = "id, parent_id, kind, medium, title, title_norm, aliases, tags, \
                    status, visibility, sort_key, created_at, updated_at";

/// 整棵树（扁平返回，前端自己拼；排序：父在前、同级按 sort_key）。
pub async fn tree(pool: &PgPool) -> Result<Vec<Node>> {
    let sql = format!("SELECT {COLS} FROM nodes ORDER BY parent_id NULLS FIRST, sort_key");
    let rows: Vec<NodeRow> = sqlx::query_as(&sql).fetch_all(pool).await?;
    rows.into_iter().map(Node::try_from).collect()
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<Node> {
    let sql = format!("SELECT {COLS} FROM nodes WHERE id = $1");
    let row: Option<NodeRow> = sqlx::query_as(&sql).bind(id).fetch_optional(pool).await?;
    row.ok_or_else(|| Error::Domain(domain::Error::NotFound(format!("节点 {id}"))))?
        .try_into()
}

/// 读一个节点（**调用方的事务里**）：`library::build_doc` 要在一个事务里读回刚复用/新建的节点。
pub async fn get_in_tx(tx: &mut sqlx::PgConnection, id: Uuid) -> Result<Node> {
    let sql = format!("SELECT {COLS} FROM nodes WHERE id = $1");
    let row: Option<NodeRow> = sqlx::query_as(&sql).bind(id).fetch_optional(&mut *tx).await?;
    row.ok_or_else(|| Error::Domain(domain::Error::NotFound(format!("节点 {id}"))))?
        .try_into()
}

/// 建节点。`sort_key` 由 store 分配（放在同级末尾），所以**必须在事务里**读末尾再写。
pub async fn create(pool: &PgPool, new: &NewNode) -> Result<Node> {
    let mut tx = pool.begin().await?;
    let node = create_in_tx(&mut tx, new).await?;
    tx.commit().await?;
    Ok(node)
}

/// 建节点（**调用方的事务里**）。`library::build_doc` 要「建容器系列 + 建文档 + 挂链」
/// 三件事同一个事务，所以把这段抽出来共用——避免两条建节点路径各写一遍规则。
pub async fn create_in_tx(tx: &mut sqlx::PgConnection, new: &NewNode) -> Result<Node> {
    new.validate()?;

    // 父节点必须在；顺便同一把咨询锁把「同一父下的并发建节点」串起来。
    // 用咨询锁而不是行锁，是因为根节点（parent_id IS NULL）没有行可锁。
    if let Some(pid) = new.parent_id {
        let exists: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM nodes WHERE id = $1")
            .bind(pid)
            .fetch_optional(&mut *tx)
            .await?;
        if exists.is_none() {
            return Err(Error::Domain(domain::Error::NotFound(format!("父节点 {pid}"))));
        }
    }
    let lock_key = new.parent_id.map(|p| p.to_string()).unwrap_or_else(|| "root".into());
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&lock_key)
        .execute(&mut *tx)
        .await?;

    let last: Option<(String,)> = sqlx::query_as(
        "SELECT sort_key FROM nodes WHERE parent_id IS NOT DISTINCT FROM $1 \
         ORDER BY sort_key DESC LIMIT 1",
    )
    .bind(new.parent_id)
    .fetch_optional(&mut *tx)
    .await?;
    let sort_key = domain::sortkey::after(last.as_ref().map(|(k,)| k.as_str()));

    let sql = format!(
        "INSERT INTO nodes (id, parent_id, kind, medium, title, title_norm, aliases, tags, sort_key) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING {COLS}"
    );
    let row: NodeRow = sqlx::query_as(&sql)
        .bind(Uuid::now_v7())
        .bind(new.parent_id)
        .bind(new.kind.as_str())
        .bind(new.medium.map(|m| m.as_str()))
        .bind(new.title.trim())
        .bind(domain::text::normalize_title(&new.title))
        .bind(&new.aliases)
        .bind(&new.tags)
        .bind(&sort_key)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_unique(e, "同级排序键已存在"))?;

    row.try_into()
}

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

/// 部分更新；`None` = 不改，`Some(None)` 用于把可空字段清空。
#[derive(Debug, Clone, Default)]
pub struct NodePatch {
    pub parent_id: Option<Option<Uuid>>,
    pub kind: Option<NodeKind>,
    pub medium: Option<Option<Medium>>,
    pub title: Option<String>,
    pub aliases: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub status: Option<NodeStatus>,
    pub visibility: Option<Visibility>,
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
    let sql = format!(
        "SELECT {COLS} FROM nodes WHERE status <> 'archived' \
         ORDER BY parent_id NULLS FIRST, sort_key"
    );
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

/// 从知识库移除一个节点及其整棵子树。
///
/// 删除策略是归档而不是物理删除：正文、版本、关系和审计记录都保留，树与检索默认不再
/// 展示。递归归档避免出现「父节点不见了、子节点还挂在它下面」的半删除状态。
pub async fn archive(pool: &PgPool, id: Uuid) -> Result<Node> {
    let sql = format!(
        "WITH RECURSIVE subtree(id) AS ( \
           SELECT id FROM nodes WHERE id = $1 \
           UNION ALL \
           SELECT n.id FROM nodes n JOIN subtree s ON n.parent_id = s.id \
         ) \
         UPDATE nodes SET status = 'archived', updated_at = now() \
         WHERE id IN (SELECT id FROM subtree) \
         RETURNING {COLS}"
    );
    let rows: Vec<NodeRow> = sqlx::query_as(&sql).bind(id).fetch_all(pool).await?;
    rows.into_iter()
        .find(|row| row.id == id)
        .ok_or_else(|| Error::Domain(domain::Error::NotFound(format!("节点 {id}"))))?
        .try_into()
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

/// 更新节点元数据。父节点变更会校验「不能移进自己的子树」，并分配新的同级 sort_key。
pub async fn update(pool: &PgPool, id: Uuid, patch: &NodePatch) -> Result<Node> {
    let mut tx = pool.begin().await?;
    let current = get_in_tx(&mut tx, id).await?;
    let parent_id = patch.parent_id.unwrap_or(current.parent_id);
    let kind = patch.kind.unwrap_or(current.kind);
    let medium = patch.medium.unwrap_or(current.medium);
    let title = patch.title.as_deref().unwrap_or(&current.title).trim();
    NewNode {
        parent_id,
        kind,
        medium,
        title: title.to_string(),
        aliases: vec![],
        tags: vec![],
    }
    .validate()?;

    if let Some(Some(pid)) = patch.parent_id {
        let cycle: (bool,) = sqlx::query_as(
            "WITH RECURSIVE descendants(id) AS ( \
               SELECT id FROM nodes WHERE parent_id = $1 \
               UNION ALL SELECT n.id FROM nodes n JOIN descendants d ON n.parent_id = d.id \
             ) SELECT EXISTS (SELECT 1 FROM descendants WHERE id = $2) OR $1 = $2",
        )
        .bind(pid)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if cycle.0 {
            return Err(Error::Domain(domain::Error::Validation(
                "不能把节点移动到它自己或它的子树里".into(),
            )));
        }
    }
    let sort_key = if parent_id != current.parent_id {
        let lock_key = parent_id.map(|p| p.to_string()).unwrap_or_else(|| "root".into());
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(&lock_key)
            .execute(&mut *tx)
            .await?;
        let last: Option<(String,)> = sqlx::query_as(
            "SELECT sort_key FROM nodes WHERE parent_id IS NOT DISTINCT FROM $1 \
             ORDER BY sort_key DESC LIMIT 1",
        )
        .bind(parent_id)
        .fetch_optional(&mut *tx)
        .await?;
        domain::sortkey::after(last.as_ref().map(|(k,)| k.as_str()))
    } else {
        current.sort_key.clone()
    };
    let row: NodeRow = sqlx::query_as(&format!(
        "UPDATE nodes SET parent_id = $2, kind = $3, medium = $4, title = $5, \
           title_norm = $6, aliases = $7, tags = $8, status = $9, visibility = $10, \
           sort_key = $11, updated_at = now() WHERE id = $1 RETURNING {COLS}"
    ))
    .bind(id)
    .bind(parent_id)
    .bind(kind.as_str())
    .bind(medium.map(|m| m.as_str()))
    .bind(title)
    .bind(domain::text::normalize_title(title))
    .bind(patch.aliases.as_ref().unwrap_or(&current.aliases))
    .bind(patch.tags.as_ref().unwrap_or(&current.tags))
    .bind(patch.status.unwrap_or(current.status).as_str())
    .bind(patch.visibility.unwrap_or(current.visibility).as_str())
    .bind(sort_key)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| map_unique(e, "同级排序键已存在"))?;
    tx.commit().await?;
    row.try_into()
}

/// 列作品（`work` 节点，可按介质 / 状态过滤）。给读工具 `list_works` 用。
pub async fn list_works(
    pool: &PgPool,
    medium: Option<Medium>,
    status: Option<NodeStatus>,
    limit: i64,
) -> Result<Vec<Node>> {
    let sql = format!(
        "SELECT {COLS} FROM nodes WHERE kind = 'work' \
           AND ($1::text IS NULL OR medium = $1) \
           AND ($2::text IS NULL OR status = $2) \
         ORDER BY updated_at DESC LIMIT $3"
    );
    let rows: Vec<NodeRow> = sqlx::query_as(&sql)
        .bind(medium.map(|m| m.as_str()))
        .bind(status.map(|s| s.as_str()))
        .bind(limit.clamp(1, 100))
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(Node::try_from).collect()
}

/// 标题 / 别名检索（`database.md §3`）：归一化精确 → 子串 → trigram 模糊。
/// 头一刀够用；M8 的完整检索（bigram + 检索页）在它之上做。
pub async fn search(pool: &PgPool, query: &str, limit: i64) -> Result<Vec<Node>> {
    let norm = domain::text::normalize_title(query);
    if norm.is_empty() {
        return Ok(vec![]);
    }
    let like = format!("%{norm}%");
    let sql = format!(
        "SELECT {COLS} FROM nodes \
         WHERE status <> 'archived' \
           AND (title_norm LIKE $1 OR title ILIKE $2 OR $3 = ANY(aliases) \
                OR similarity(title_norm, $4) > 0.25) \
         ORDER BY (title_norm = $4) DESC, similarity(title_norm, $4) DESC, updated_at DESC \
         LIMIT $5"
    );
    let rows: Vec<NodeRow> = sqlx::query_as(&sql)
        .bind(&like)
        .bind(&like)
        .bind(&norm)
        .bind(&norm)
        .bind(limit.clamp(1, 50))
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(Node::try_from).collect()
}

//! 链（`library_links`）与「不要」（`library_ignores`）：建档 / 解链 / 忽略。
//!
//! 从 `library.rs` 拆出来（单文件 ≤300 行，D41）。建档的三件事——**建容器系列 + 建文档节点 +
//! 挂链**——在同一个事务里（`docs/library-sync.md` §2.3 的原子性）。

use crate::manifest::{ManifestDb, COLS};
use crate::{map_unique, nodes, Error, Result};
use chrono::{DateTime, Utc};
use domain::library::{ContainerKind, Ignore, LinkRole, LinkedBy, ManifestRow, Source};
use domain::{NewNode, Node, NodeKind};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain as domain;

/// 「建档」的入参：把一条清单条目变成一个文档节点 + 一条链。
#[derive(Debug, Clone)]
pub struct BuildDoc {
    pub source: Source,
    pub external_id: String,
    /// 显式指定正片位（默认按容器种类：Emby 混杂库 = mix，其余 work）。
    pub role: Option<LinkRole>,
    /// 用户指定的父节点（给了就跳过「按容器找/建系列」）。
    pub parent_id: Option<Uuid>,
    pub linked_by: LinkedBy,
}

#[derive(Debug, Clone)]
pub struct BuiltDoc {
    pub node: Node,
    pub link_id: Uuid,
    /// 这一次是新建了节点，还是复用了同一容器下已有节点（v1 的 `reuseChild`）。
    pub reused: bool,
}

/// 建档：**建节点 + 挂链在同一个事务里**（`docs/library-sync.md` §2.3 的原子性）。
///
/// 落位规则（`docs/library-sync.md` §2.3，纯规则在 `domain::place`）：
/// 容器是**作品系列** → 系列不存在就先建一个同名 series；
/// 容器是**库**（Emby view）→ 建在顶级；子标题默认 = 条目名，撞名/与父同名才加介质后缀。
pub async fn build_doc(pool: &PgPool, req: &BuildDoc) -> Result<BuiltDoc> {
    let mut tx = pool.begin().await?;

    // ① 条目必须在清单里（身份 = source + external_id，不靠标题）
    let entry: Option<ManifestDb> = sqlx::query_as(&format!(
        "SELECT {COLS} FROM library_manifest m WHERE m.source = $1 AND m.external_id = $2"
    ))
    .bind(req.source.as_str())
    .bind(&req.external_id)
    .fetch_optional(&mut *tx)
    .await?;
    let entry = entry.ok_or_else(|| {
        Error::Domain(domain::Error::NotFound(format!(
            "清单里没有 {}/{}（先扫一次媒体库）",
            req.source.as_str(),
            req.external_id
        )))
    })?;
    let entry = ManifestRow::try_from(entry)?;

    // ② 一个库条目只能挂一篇文档（DB 上也有 UNIQUE 兜底）
    let already: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM library_links WHERE source = $1 AND external_id = $2")
            .bind(req.source.as_str())
            .bind(&req.external_id)
            .fetch_optional(&mut *tx)
            .await?;
    if already.is_some() {
        return Err(Error::Domain(domain::Error::Duplicated(format!(
            "「{}」已经挂过文档了",
            entry.title
        ))));
    }

    let medium = domain::place::medium_of(entry.source, &entry.kind, entry.format.as_deref());

    // ③ 父节点：用户指定 > 作品系列容器（找/建）> 顶级
    let (parent_id, parent_title) = if let Some(pid) = req.parent_id {
        let row: Option<(String,)> = sqlx::query_as("SELECT title FROM nodes WHERE id = $1")
            .bind(pid)
            .fetch_optional(&mut *tx)
            .await?;
        let title = row.ok_or_else(|| {
            Error::Domain(domain::Error::NotFound(format!("父节点 {pid}")))
        })?;
        (Some(pid), Some(title.0))
    } else if domain::place::container_makes_parent(entry.container_kind, &entry.container_key) {
        ensure_series(&mut tx, &entry.container_title).await?
    } else {
        (None, None)
    };

    // ④ 先看同一父下能不能**复用**（重跑同一个条目 / 同作品同介质都该复用同一个节点）
    let siblings: Vec<(Uuid, String, Option<String>, Vec<String>)> = sqlx::query_as(
        "SELECT id, title, medium, aliases FROM nodes WHERE parent_id IS NOT DISTINCT FROM $1",
    )
    .bind(parent_id)
    .fetch_all(&mut *tx)
    .await?;
    let reuse = siblings.iter().find(|(_, t, m, aliases)| {
        let node_medium = m.as_deref().and_then(|s| domain::Medium::parse(s).ok());
        domain::place::reuses_existing(t, node_medium, aliases, &entry.title, medium)
    });

    let (node, reused) = match reuse {
        Some((id, _, _, _)) => {
            let n = nodes::get_in_tx(&mut tx, *id).await?;
            (n, true)
        }
        None => {
            let sibling_titles: Vec<String> = siblings.iter().map(|(_, t, _, _)| t.clone()).collect();
            let title = domain::place::child_title(
                &entry.title,
                medium,
                parent_title.as_deref(),
                &sibling_titles,
            );
            let n = nodes::create_in_tx(
                &mut tx,
                &NewNode {
                    parent_id,
                    kind: NodeKind::Work,
                    medium: Some(medium),
                    title,
                    aliases: entry.aliases.clone(),
                    tags: vec![],
                },
            )
            .await?;
            (n, false)
        }
    };

    // ⑤ 挂链（正片位唯一索引会给重复 work 位兜底）
    let role = req.role.unwrap_or(if entry.source == Source::Emby
        && entry.container_kind == ContainerKind::Mixed
    {
        LinkRole::Mix
    } else {
        LinkRole::Work
    });
    let link_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO library_links \
           (id, node_id, source, external_id, url, title_hint, role, state, linked_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'active', $8)",
    )
    .bind(link_id)
    .bind(node.id)
    .bind(entry.source.as_str())
    .bind(&entry.external_id)
    .bind(&entry.url)
    .bind(&entry.title)
    .bind(role.as_str())
    .bind(req.linked_by.as_str())
    .execute(&mut *tx)
    .await
    .map_err(|e| map_unique(e, "这个节点在该源上的正片位已经有链了"))?;

    tx.commit().await?;
    Ok(BuiltDoc { node, link_id, reused })
}

/// 把清单条目挂到**已有节点**（不新建文档）。身份唯一由 DB 兜底。
pub async fn attach_existing(
    pool: &PgPool,
    node_id: Uuid,
    source: Source,
    external_id: &str,
    role: LinkRole,
    linked_by: LinkedBy,
) -> Result<Uuid> {
    let mut tx = pool.begin().await?;
    nodes::get_in_tx(&mut tx, node_id).await?;
    let entry: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT title, url FROM library_manifest WHERE source = $1 AND external_id = $2",
    )
    .bind(source.as_str())
    .bind(external_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (title, url) = entry.ok_or_else(|| {
        Error::Domain(domain::Error::NotFound(
            "清单里没有这个条目（先扫描媒体库）".into(),
        ))
    })?;
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO library_links \
           (id, node_id, source, external_id, url, title_hint, role, linked_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(node_id)
    .bind(source.as_str())
    .bind(external_id)
    .bind(url)
    .bind(title)
    .bind(role.as_str())
    .bind(linked_by.as_str())
    .execute(&mut *tx)
    .await
    .map_err(|e| map_unique(e, "这个条目已经挂过链"))?;
    tx.commit().await?;
    Ok(id)
}

/// 找同名系列节点；没有就建一个（顶级、不带介质）。
async fn ensure_series(
    tx: &mut sqlx::PgConnection,
    title: &str,
) -> Result<(Option<Uuid>, Option<String>)> {
    let norm = domain::text::normalize_title(title);
    let hit: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, title FROM nodes WHERE kind = 'series' AND title_norm = $1 \
         ORDER BY (parent_id IS NULL) DESC, sort_key LIMIT 1",
    )
    .bind(&norm)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((id, t)) = hit {
        return Ok((Some(id), Some(t)));
    }
    let series = nodes::create_in_tx(
        tx,
        &NewNode {
            parent_id: None,
            kind: NodeKind::Series,
            medium: None,
            title: title.trim().to_string(),
            aliases: vec![],
            tags: vec![],
        },
    )
    .await?;
    Ok((Some(series.id), Some(series.title)))
}

/// 解链：**只删链，保留文档**（`docs/features.md` §2 的撤销）。
/// 返回被解开的节点 id，让前端能提示「文档还在」。
pub async fn unlink(pool: &PgPool, link_id: Uuid) -> Result<Uuid> {
    let mut tx = pool.begin().await?;
    let row: Option<(Uuid,)> = sqlx::query_as("DELETE FROM library_links WHERE id = $1 RETURNING node_id")
        .bind(link_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            // 被 replaced_by 指着的链不能删（会断掉重挂的可追溯）——明确说清楚。
            if let sqlx::Error::Database(ref db) = e {
                if db.code().as_deref() == Some("23503") {
                    return Error::Domain(domain::Error::Validation(
                        "这条链被别的链指为「被谁取代」，不能直接解链".into(),
                    ));
                }
            }
            Error::Db(e)
        })?;
    let node_id = row.ok_or_else(|| {
        Error::Domain(domain::Error::NotFound(format!("链 {link_id}")))
    })?;
    tx.commit().await?;
    Ok(node_id.0)
}

/// 加一条「不要」：给 `external_id`（单条）或 `group_key`（整组）二者之一。
pub async fn add_ignore(
    pool: &PgPool,
    source: Option<Source>,
    external_id: Option<String>,
    group_key: Option<String>,
    reason: &str,
) -> Result<Uuid> {
    if external_id.is_none() && group_key.is_none() {
        return Err(Error::Domain(domain::Error::Validation(
            "忽略要么给条目，要么给整组".into(),
        )));
    }
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO library_ignores (id, source, external_id, group_key, reason, created_by) \
         VALUES ($1, $2, $3, $4, $5, 'human:local')",
    )
    .bind(id)
    .bind(source.map(|s| s.as_str()))
    .bind(&external_id)
    .bind(&group_key)
    .bind(reason)
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn remove_ignore(pool: &PgPool, id: Uuid) -> Result<()> {
    let res = sqlx::query("DELETE FROM library_ignores WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(Error::Domain(domain::Error::NotFound(format!("忽略 {id}"))));
    }
    Ok(())
}

pub async fn list_ignores(pool: &PgPool) -> Result<Vec<Ignore>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        source: Option<String>,
        external_id: Option<String>,
        group_key: Option<String>,
        reason: String,
        created_by: String,
        created_at: DateTime<Utc>,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, source, external_id, group_key, reason, created_by, created_at \
         FROM library_ignores ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(Ignore {
                id: r.id,
                source: r.source.as_deref().map(Source::parse).transpose()?,
                external_id: r.external_id,
                group_key: r.group_key,
                reason: r.reason,
                created_by: r.created_by,
                created_at: r.created_at,
            })
        })
        .collect()
}

//! 关系 CRUD、综合页与宿主索引（M5）。

use crate::{map_unique, nodes, Error, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain as domain;
use domain::relation::{
    RelationNode, RelationType, RelationView, SynthesisType,
};
use domain::{NewNode, Node, NodeKind};

mod index;
mod batch;
pub use index::sync_index;
pub use batch::create_many;

#[derive(sqlx::FromRow)]
struct RelationRow {
    id: Uuid,
    r#type: String,
    direction: String,
    label: String,
    other_id: Uuid,
    other_title: String,
    other_kind: String,
    other_medium: Option<String>,
    created_by: String,
    created_at: DateTime<Utc>,
}

pub async fn list_types(pool: &PgPool) -> Result<Vec<RelationType>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        r#type: String,
        from_label: String,
        to_label: String,
        note: String,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT type, from_label, to_label, note FROM relation_types ORDER BY type",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| RelationType {
            r#type: r.r#type,
            from_label: r.from_label,
            to_label: r.to_label,
            note: r.note,
        })
        .collect())
}

/// 某节点的出边 + 入边；标签由 `relation_types` 给出，不在前端手抄。
pub async fn list_for_node(pool: &PgPool, node_id: Uuid) -> Result<Vec<RelationView>> {
    nodes::get(pool, node_id).await?;
    let rows: Vec<RelationRow> = sqlx::query_as(
        "SELECT r.id, r.type, 'out' AS direction, rt.from_label AS label, \
                n.id AS other_id, n.title AS other_title, n.kind AS other_kind, n.medium AS other_medium, \
                r.created_by, r.created_at \
         FROM relations r \
         JOIN relation_types rt ON rt.type = r.type \
         JOIN nodes n ON n.id = r.to_id \
         WHERE r.from_id = $1 \
         UNION ALL \
         SELECT r.id, r.type, 'in' AS direction, rt.to_label AS label, \
                n.id AS other_id, n.title AS other_title, n.kind AS other_kind, n.medium AS other_medium, \
                r.created_by, r.created_at \
         FROM relations r \
         JOIN relation_types rt ON rt.type = r.type \
         JOIN nodes n ON n.id = r.from_id \
         WHERE r.to_id = $1 \
         ORDER BY created_at DESC",
    )
    .bind(node_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| RelationView {
            id: r.id,
            r#type: r.r#type,
            direction: r.direction,
            label: r.label,
            other: RelationNode {
                id: r.other_id,
                title: r.other_title,
                kind: r.other_kind,
                medium: r.other_medium,
            },
            created_by: r.created_by,
            created_at: r.created_at,
        })
        .collect())
}

/// 建关系。同类型的环在应用层拒绝；跨类型环可能是合法语义，不拦。
pub async fn create(
    pool: &PgPool,
    from_id: Uuid,
    to_id: Uuid,
    relation_type: &str,
    created_by: &str,
) -> Result<Uuid> {
    if from_id == to_id {
        return Err(Error::Domain(domain::Error::Validation(
            "关系两端不能是同一个节点".into(),
        )));
    }
    let mut tx = pool.begin().await?;
    for id in [from_id, to_id] {
        let exists: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM nodes WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        if exists.is_none() {
            return Err(Error::Domain(domain::Error::NotFound(format!("节点 {id}"))));
        }
    }
    let known: Option<(String,)> =
        sqlx::query_as("SELECT type FROM relation_types WHERE type = $1")
            .bind(relation_type)
            .fetch_optional(&mut *tx)
            .await?;
    if known.is_none() {
        return Err(Error::Domain(domain::Error::Validation(format!(
            "关系类型不在冻结词典里：{relation_type}"
        ))));
    }
    let cycle: (bool,) = sqlx::query_as(
        "WITH RECURSIVE reach(id) AS ( \
           SELECT to_id FROM relations WHERE from_id = $1 AND type = $3 \
           UNION \
           SELECT r.to_id FROM relations r JOIN reach ON r.from_id = reach.id \
           WHERE r.type = $3 \
         ) SELECT EXISTS (SELECT 1 FROM reach WHERE id = $2)",
    )
    .bind(to_id)
    .bind(from_id)
    .bind(relation_type)
    .fetch_one(&mut *tx)
    .await?;
    if cycle.0 {
        return Err(Error::Domain(domain::Error::Validation(
            "这条关系会在同类型边上形成环".into(),
        )));
    }

    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO relations (id, from_id, to_id, type, created_by) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(from_id)
    .bind(to_id)
    .bind(relation_type)
    .bind(created_by)
    .execute(&mut *tx)
    .await
    .map_err(|e| map_unique(e, "这条关系已经存在"))?;
    tx.commit().await?;
    Ok(id)
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
    let res = sqlx::query("DELETE FROM relations WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(Error::Domain(domain::Error::NotFound(format!("关系 {id}"))));
    }
    Ok(())
}

/// 建综合页：doc 节点 + `doc_refs` + 指向各作品的 `references` 边，一个事务。
pub async fn create_synthesis(
    pool: &PgPool,
    parent_id: Option<Uuid>,
    title: &str,
    synthesis_type: &str,
    work_ids: &[Uuid],
) -> Result<Node> {
    let kind = SynthesisType::parse(synthesis_type)?;
    let mut works: Vec<Uuid> = work_ids.to_vec();
    works.sort();
    works.dedup();
    if works.is_empty() {
        return Err(Error::Domain(domain::Error::Validation(
            "综合页至少引用一部作品".into(),
        )));
    }

    let mut tx = pool.begin().await?;
    let found: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM nodes WHERE kind = 'work' AND id = ANY($1)",
    )
    .bind(&works)
    .fetch_one(&mut *tx)
    .await?;
    if found.0 as usize != works.len() {
        return Err(Error::Domain(domain::Error::Validation(
            "work_ids 里有不存在或不是作品的节点".into(),
        )));
    }
    let node = nodes::create_in_tx(
        &mut tx,
        &NewNode {
            parent_id,
            kind: NodeKind::Doc,
            medium: None,
            title: title.trim().to_string(),
            aliases: vec![],
            tags: vec!["综合".into(), kind.label().into()],
        },
    )
    .await?;
    for work_id in works {
        sqlx::query("INSERT INTO doc_refs (doc_id, work_id) VALUES ($1, $2)")
            .bind(node.id)
            .bind(work_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO relations (id, from_id, to_id, type, created_by) \
             VALUES ($1, $2, $3, 'references', 'host') ON CONFLICT DO NOTHING",
        )
        .bind(Uuid::now_v7())
        .bind(node.id)
        .bind(work_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(node)
}

pub async fn list_synthesis(pool: &PgPool) -> Result<Vec<Node>> {
    #[derive(sqlx::FromRow)]
    struct IdRow {
        id: Uuid,
    }
    let rows: Vec<IdRow> = sqlx::query_as(
        "SELECT id FROM nodes WHERE kind = 'doc' AND tags @> ARRAY['综合']::text[] \
         ORDER BY updated_at DESC",
    )
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(nodes::get(pool, row.id).await?);
    }
    Ok(out)
}

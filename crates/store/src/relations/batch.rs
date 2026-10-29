//! 批量关系：预检全在前，一个事务全成或全不成。

use crate::{map_unique, Error, Result};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain as domain;

pub async fn create_many(
    pool: &PgPool,
    items: &[(Uuid, Uuid, String)],
    created_by: &str,
) -> Result<Vec<Uuid>> {
    if items.is_empty() || items.len() > 50 {
        return Err(Error::Domain(domain::Error::Validation(
            "批量关系一次要 1–50 条".into(),
        )));
    }
    let mut tx = pool.begin().await?;
    let mut ids = Vec::with_capacity(items.len());
    for (from_id, to_id, relation_type) in items {
        if from_id == to_id {
            return Err(Error::Domain(domain::Error::Validation(
                "关系两端不能是同一个节点".into(),
            )));
        }
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
               UNION SELECT r.to_id FROM relations r JOIN reach ON r.from_id = reach.id \
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
                "批量关系会在同类型边上形成环".into(),
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
        .map_err(|e| map_unique(e, "批量里有一条关系已经存在"))?;
        ids.push(id);
    }
    tx.commit().await?;
    Ok(ids)
}

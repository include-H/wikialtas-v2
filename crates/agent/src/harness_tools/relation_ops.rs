//! 通用 Harness 的关系工具。

use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_store::relations;

fn bad(reason: impl Into<String>) -> Value {
    json!({ "ok": false, "reason": reason.into() })
}

pub async fn dispatch(
    pool: &PgPool,
    name: &str,
    raw: &str,
    run_id: &str,
) -> Option<Value> {
    let out = match name {
        "upsert_relation" => one(pool, raw, run_id).await,
        "upsert_relations" => many(pool, raw, run_id).await,
        "detach_relation" => detach(pool, raw).await,
        "list_relation_types" => list_types(pool).await,
        _ => return None,
    };
    Some(out)
}

async fn one(pool: &PgPool, raw: &str, run_id: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct A {
        from_id: Uuid,
        to_id: Uuid,
        r#type: String,
    }
    let a: A = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(e) => return bad(format!("参数不是合法 JSON：{e}")),
    };
    match relations::create(
        pool,
        a.from_id,
        a.to_id,
        &a.r#type,
        &format!("llm:{run_id}"),
    )
    .await
    {
        Ok(id) => json!({ "ok": true, "relation_id": id }),
        Err(e) => bad(format!("关系没建成：{e}")),
    }
}

async fn many(pool: &PgPool, raw: &str, run_id: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct Item {
        from_id: Uuid,
        to_id: Uuid,
        r#type: String,
    }
    #[derive(serde::Deserialize)]
    struct A {
        relations: Vec<Item>,
    }
    let a: A = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(e) => return bad(format!("参数不是合法 JSON：{e}")),
    };
    let items: Vec<(Uuid, Uuid, String)> = a
        .relations
        .into_iter()
        .map(|r| (r.from_id, r.to_id, r.r#type))
        .collect();
    match relations::create_many(pool, &items, &format!("llm:{run_id}")).await {
        Ok(ids) => json!({ "ok": true, "count": ids.len(), "relation_ids": ids }),
        Err(e) => bad(format!("批量关系整批拒绝：{e}")),
    }
}

async fn detach(pool: &PgPool, raw: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct A {
        relation_id: Uuid,
    }
    let a: A = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(e) => return bad(format!("参数不是合法 JSON：{e}")),
    };
    match relations::delete(pool, a.relation_id).await {
        Ok(()) => json!({ "ok": true }),
        Err(e) => bad(format!("删关系失败：{e}")),
    }
}

async fn list_types(pool: &PgPool) -> Value {
    match relations::list_types(pool).await {
        Ok(rows) => json!({ "ok": true, "types": rows }),
        Err(e) => bad(format!("读关系类型失败：{e}")),
    }
}

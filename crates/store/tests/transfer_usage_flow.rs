//! M9：项目树导出/导入、密钥不导出、用量与成本汇总。

use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain as domain;
use wikiatlas_store::{content, nodes, relations, secrets, transfer, usage};

#[sqlx::test(migrations = "./migrations")]
async fn 项目树导出导入往返_密钥不跟着走(pool: PgPool) {
    let a = nodes::create(
        &pool,
        &domain::NewNode {
            parent_id: None,
            kind: domain::NodeKind::Work,
            medium: Some(domain::Medium::Game),
            title: "导出测试 A".into(),
            aliases: vec![],
            tags: vec![],
        },
    )
    .await
    .unwrap();
    let b = nodes::create(
        &pool,
        &domain::NewNode {
            parent_id: None,
            kind: domain::NodeKind::Work,
            medium: Some(domain::Medium::Game),
            title: "导出测试 B".into(),
            aliases: vec![],
            tags: vec![],
        },
    )
    .await
    .unwrap();
    content::commit(&pool, a.id, "可导出的正文", 0, "首版", "human")
        .await
        .unwrap();
    relations::create(&pool, b.id, a.id, "sequel_of", "human")
        .await
        .unwrap();
    secrets::put_setting(&pool, "llm.model", &json!("test-model"))
        .await
        .unwrap();
    secrets::put_secret(&pool, "llm.api_key", "must-not-export")
        .await
        .unwrap();

    let bundle = transfer::export(&pool).await.unwrap();
    let raw = serde_json::to_string(&bundle).unwrap();
    assert!(!raw.contains("must-not-export"));
    assert_eq!(bundle.nodes.as_array().unwrap().len(), 2);
    assert!(bundle.contents.as_array().unwrap().iter().any(|c| {
        c.get("node_id").and_then(|v| v.as_str()) == Some(&a.id.to_string())
    }));

    sqlx::query("DELETE FROM nodes").execute(&pool).await.unwrap();
    let imported = transfer::import(&pool, &bundle).await.unwrap();
    assert_eq!(imported, 2);
    let tree = nodes::tree(&pool).await.unwrap();
    assert_eq!(tree.len(), 2);
    assert_eq!(content::read(&pool, a.id).await.unwrap().content.unwrap().body_md, "可导出的正文\n");
    assert_eq!(relations::list_for_node(&pool, b.id).await.unwrap().len(), 1);
    assert_eq!(
        secrets::get_setting(&pool, "llm.model").await.unwrap(),
        Some(json!("test-model"))
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn 用量按模型汇总_配置单价后给估算成本(pool: PgPool) {
    let run_id = "run:usage";
    sqlx::query(
        "INSERT INTO runs (id, intent, goal, status, model, started_at, last_active) \
         VALUES ($1, '问答', 'test', 'completed', 'model-a', now(), now())",
    )
    .bind(run_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO run_events (id, run_id, seq, type, payload, created_at) \
         VALUES ($1, $2, 0, 'response.completed', \
                 '{\"usage\":{\"input_tokens\":1000000,\"output_tokens\":500000}}', now())",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(run_id)
    .execute(&pool)
    .await
    .unwrap();

    let plain = usage::summary(&pool).await.unwrap();
    assert_eq!(plain.total_tokens, 1_500_000);
    assert_eq!(plain.runs, 1);
    assert!(plain.estimated_cost.is_none());

    secrets::put_setting(&pool, "llm.price_input_per_million", &json!(2.0))
        .await
        .unwrap();
    secrets::put_setting(&pool, "llm.price_output_per_million", &json!(4.0))
        .await
        .unwrap();
    let priced = usage::summary(&pool).await.unwrap();
    assert!(priced.pricing_configured);
    assert_eq!(priced.estimated_cost, Some(4.0));
}

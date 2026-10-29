//! M6：Review 裁决与确定性体检。

use serde_json::json;
use sqlx::PgPool;
use wikiatlas_domain as domain;
use domain::library::{ContainerKind, LinkedBy, ManifestEntry, Source};
use domain::review::{FindingState, SuggestionKind, SuggestionState};
use wikiatlas_store::{audit, library, nodes, review};

fn new_work(title: &str) -> domain::NewNode {
    domain::NewNode {
        parent_id: None,
        kind: domain::NodeKind::Work,
        medium: Some(domain::Medium::Game),
        title: title.into(),
        aliases: vec![],
        tags: vec![],
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn create建议接受后真建节点(pool: PgPool) {
    let id = review::create(
        &pool,
        SuggestionKind::Create,
        None,
        None,
        None,
        json!({ "title": "被接受的节点", "kind": "work", "medium": "game" }),
        "llm:test",
    )
    .await
    .unwrap();
    let out = review::decide(&pool, id, true, "").await.unwrap();
    assert_eq!(out.suggestion.state, SuggestionState::Accepted);
    assert!(out.action.starts_with("created:"));
    let rows = nodes::tree(&pool).await.unwrap();
    assert!(rows.iter().any(|n| n.title == "被接受的节点"));
    assert_eq!(
        review::list(&pool, Some(SuggestionState::Pending))
            .await
            .unwrap()
            .len(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn link建议接受后挂到已有节点且拒绝理由保留(pool: PgPool) {
    let node = nodes::create(&pool, &new_work("既有作品")).await.unwrap();
    library::upsert_manifest(
        &pool,
        &[ManifestEntry {
            source: Source::GameAtlas,
            external_id: "ga-1".into(),
            title: "库里的作品".into(),
            subtitle: None,
            aliases: vec![],
            year: None,
            kind: "game".into(),
            format: None,
            cover_url: None,
            url: None,
            container_key: "series:x".into(),
            container_title: "系列".into(),
            container_kind: ContainerKind::Series,
            extra: String::new(),
            raw: json!({}),
        }],
    )
    .await
    .unwrap();
    let id = review::create(
        &pool,
        SuggestionKind::Link,
        Some(node.id),
        Some(Source::GameAtlas),
        Some("ga-1".into()),
        json!({ "role": "work" }),
        "llm:test",
    )
    .await
    .unwrap();
    review::decide(&pool, id, true, "").await.unwrap();
    let (link_n,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM library_links WHERE node_id = $1 AND external_id = 'ga-1'",
    )
    .bind(node.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(link_n, 1);

    let rejected = review::create(
        &pool,
        SuggestionKind::Research,
        Some(node.id),
        None,
        None,
        json!({ "query": "续作销量" }),
        "llm:test",
    )
    .await
    .unwrap();
    let out = review::decide(&pool, rejected, false, "来源不够硬").await.unwrap();
    assert_eq!(out.suggestion.state, SuggestionState::Rejected);
    assert_eq!(out.suggestion.payload["decision_reason"], "来源不够硬");
}

#[sqlx::test(migrations = "./migrations")]
async fn 体检找出缺正文孤立节点与失效链_重跑不会重复堆积(pool: PgPool) {
    let ready = nodes::create(&pool, &new_work("定稿却没有正文")).await.unwrap();
    sqlx::query("UPDATE nodes SET status = 'ready' WHERE id = $1")
        .bind(ready.id)
        .execute(&pool)
        .await
        .unwrap();
    let lonely = nodes::create(&pool, &new_work("孤立节点")).await.unwrap();

    library::upsert_manifest(
        &pool,
        &[ManifestEntry {
            source: Source::GameAtlas,
            external_id: "stale-1".into(),
            title: "会消失的作品".into(),
            subtitle: None,
            aliases: vec![],
            year: None,
            kind: "game".into(),
            format: None,
            cover_url: None,
            url: None,
            container_key: "series:s".into(),
            container_title: "系列".into(),
            container_kind: ContainerKind::Series,
            extra: String::new(),
            raw: json!({}),
        }],
    )
    .await
    .unwrap();
    let built = library::build_doc(
        &pool,
        &library::BuildDoc {
            source: Source::GameAtlas,
            external_id: "stale-1".into(),
            role: None,
            parent_id: Some(lonely.id),
            linked_by: LinkedBy::Pool,
        },
    )
    .await
    .unwrap();
    library::upsert_manifest_for_source(&pool, Source::GameAtlas, &[])
        .await
        .unwrap();

    let first = audit::run(&pool).await.unwrap();
    assert!(first.findings >= 3);
    let open = audit::list(&pool, Some(FindingState::Open)).await.unwrap();
    let kinds: Vec<_> = open.iter().map(|f| f.kind.as_str()).collect();
    assert!(kinds.contains(&"ready_without_content"));
    assert!(kinds.contains(&"isolated_node"));
    assert!(kinds.contains(&"stale_link"));

    audit::run(&pool).await.unwrap();
    let open2 = audit::list(&pool, Some(FindingState::Open)).await.unwrap();
    assert_eq!(open.len(), open2.len(), "重跑应先解决上一轮 open，不重复堆积");
    assert!(built.node.id != lonely.id);
}

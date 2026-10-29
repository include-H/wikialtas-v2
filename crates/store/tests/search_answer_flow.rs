//! M8：正文检索 + bigram 兜底 + 好答案存资料。

use sqlx::PgPool;
use wikiatlas_domain as domain;
use wikiatlas_store::{answers, content, nodes, search};

async fn work(pool: &PgPool, title: &str) -> domain::Node {
    nodes::create(
        pool,
        &domain::NewNode {
            parent_id: None,
            kind: domain::NodeKind::Work,
            medium: Some(domain::Medium::Game),
            title: title.into(),
            aliases: vec!["Trails in the Sky SC".into()],
            tags: vec![],
        },
    )
    .await
    .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn 正文里的独有事实能被检索到并给片段(pool: PgPool) {
    let node = work(&pool, "空之轨迹 SC").await;
    content::commit(
        &pool,
        node.id,
        "# 空之轨迹 SC\n\n青羽测试是只在正文里出现的事实。\n\n## 1. 作品概览\n\n正文。",
        0,
        "",
        "human",
    )
    .await
    .unwrap();

    let hits = search::search(&pool, "青羽测试", 10).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].node_id, node.id);
    assert!(hits[0].snippet.contains("青羽测试"));
    assert!(hits[0].score >= 3.0);
}

#[sqlx::test(migrations = "./migrations")]
async fn 别名精确命中与bigram近似标题进结果(pool: PgPool) {
    let node = work(&pool, "空之轨迹 SC").await;
    let alias = search::search(&pool, "Trails in the Sky SC", 10).await.unwrap();
    assert_eq!(alias[0].node_id, node.id);

    let near = search::search(&pool, "空之轨迹SC", 10).await.unwrap();
    assert!(near.iter().any(|h| h.node_id == node.id));
    assert!(search::search(&pool, "   ", 10).await.unwrap().is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn 好回答存成资料_带引用与快照_不改作品正文(pool: PgPool) {
    let node = work(&pool, "空之轨迹 SC").await;
    content::commit(&pool, node.id, "作品正文", 0, "", "human")
        .await
        .unwrap();
    let before = content::read(&pool, node.id).await.unwrap().content.unwrap();

    let doc = answers::save_as_doc(
        &pool,
        node.parent_id,
        "问答：SC 与前作的关系",
        "SC 是 FC 的直接续作。",
        &[node.id],
    )
    .await
    .unwrap();
    assert_eq!(doc.kind, domain::NodeKind::Doc);
    assert!(doc.tags.iter().any(|t| t == "问答"));

    let saved = content::read(&pool, doc.id).await.unwrap().content.unwrap();
    assert_eq!(saved.body_md, "SC 是 FC 的直接续作。\n");
    let (snaps, writes, refs, edges): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM content_versions WHERE node_id = $1), \
                (SELECT count(*) FROM write_events WHERE node_id = $1), \
                (SELECT count(*) FROM doc_refs WHERE doc_id = $1), \
                (SELECT count(*) FROM relations WHERE from_id = $1 AND type = 'references')",
    )
    .bind(doc.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((snaps, writes, refs, edges), (1, 1, 1, 1));
    let after = content::read(&pool, node.id).await.unwrap().content.unwrap();
    assert_eq!(after.body_md, before.body_md);
    assert_eq!(after.ver, before.ver);
}

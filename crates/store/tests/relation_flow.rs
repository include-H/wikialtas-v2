//! M5：类型化关系、综合页与宿主索引。

use sqlx::PgPool;
use wikiatlas_domain as domain;
use wikiatlas_store::{nodes, relations};

async fn node(pool: &PgPool, kind: domain::NodeKind, title: &str) -> domain::Node {
    nodes::create(
        pool,
        &domain::NewNode {
            parent_id: None,
            kind,
            medium: (kind == domain::NodeKind::Work).then_some(domain::Medium::Game),
            title: title.into(),
            aliases: vec![],
            tags: vec![],
        },
    )
    .await
    .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn 关系有出边入边且标签来自契约表(pool: PgPool) {
    let sequel = node(&pool, domain::NodeKind::Work, "空之轨迹 SC").await;
    let prequel = node(&pool, domain::NodeKind::Work, "空之轨迹 FC").await;
    relations::create(&pool, sequel.id, prequel.id, "sequel_of", "human")
        .await
        .unwrap();

    let out = relations::list_for_node(&pool, sequel.id).await.unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].direction, "out");
    assert_eq!(out[0].label, "续作");
    assert_eq!(out[0].other.title, "空之轨迹 FC");

    let incoming = relations::list_for_node(&pool, prequel.id).await.unwrap();
    assert_eq!(incoming.len(), 1);
    assert_eq!(incoming[0].direction, "in");
    assert_eq!(incoming[0].label, "前作");
    assert!(!relations::list_types(&pool).await.unwrap().is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn 同类型关系成环被拒(pool: PgPool) {
    let a = node(&pool, domain::NodeKind::Work, "A").await;
    let b = node(&pool, domain::NodeKind::Work, "B").await;
    relations::create(&pool, a.id, b.id, "sequel_of", "human")
        .await
        .unwrap();
    let err = relations::create(&pool, b.id, a.id, "sequel_of", "human")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("形成环"), "{err}");
}

#[sqlx::test(migrations = "./migrations")]
async fn 综合页引用多部作品并自动落references边(pool: PgPool) {
    let a = node(&pool, domain::NodeKind::Work, "仙剑奇侠传").await;
    let b = node(&pool, domain::NodeKind::Work, "仙剑奇侠传二").await;
    let doc = relations::create_synthesis(
        &pool,
        None,
        "李逍遥",
        "role",
        &[a.id, b.id],
    )
    .await
    .unwrap();
    assert_eq!(doc.kind, domain::NodeKind::Doc);
    assert!(doc.tags.iter().any(|t| t == "综合"));

    let (refs, edges): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM doc_refs WHERE doc_id = $1), \
                (SELECT count(*) FROM relations WHERE from_id = $1 AND type = 'references')",
    )
    .bind(doc.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((refs, edges), (2, 2));
    assert_eq!(relations::list_synthesis(&pool).await.unwrap().len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn 宿主索引随子节点重算且不把自己列进去(pool: PgPool) {
    let series = node(&pool, domain::NodeKind::Series, "轨迹系列").await;
    let mut work = domain::NewNode {
        parent_id: Some(series.id),
        kind: domain::NodeKind::Work,
        medium: Some(domain::Medium::Game),
        title: "空之轨迹 SC".into(),
        aliases: vec![],
        tags: vec![],
    };
    nodes::create(&pool, &work).await.unwrap();

    let index = relations::sync_index(&pool, series.id).await.unwrap();
    assert_eq!(index.tags, vec!["宿主索引"]);
    let first = wikiatlas_store::content::read(&pool, index.id)
        .await
        .unwrap()
        .content
        .unwrap();
    assert!(first.body_md.contains("空之轨迹 SC"));
    assert!(!first.body_md.contains("- 索引"));
    assert_eq!(first.updated_by, "host");

    work.title = "空之轨迹 the 3rd".into();
    nodes::create(&pool, &work).await.unwrap();
    let same = relations::sync_index(&pool, series.id).await.unwrap();
    assert_eq!(same.id, index.id, "索引页应复用，不重复建");
    let second = wikiatlas_store::content::read(&pool, index.id)
        .await
        .unwrap()
        .content
        .unwrap();
    assert_eq!(second.ver, first.ver + 1);
    assert!(second.body_md.contains("空之轨迹 the 3rd"));
}

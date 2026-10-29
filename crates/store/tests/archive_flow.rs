//! 删除文章：默认归档整棵子树，树不再展示，数据仍可读取。

use sqlx::PgPool;
use wikiatlas_domain as domain;
use wikiatlas_store::nodes;

#[sqlx::test(migrations = "./migrations")]
async fn 删除节点会归档整棵子树并从树中移除(pool: PgPool) {
    let series = nodes::create(
        &pool,
        &domain::NewNode {
            parent_id: None,
            kind: domain::NodeKind::Series,
            medium: None,
            title: "测试系列".into(),
            aliases: vec![],
            tags: vec![],
        },
    )
    .await
    .unwrap();
    let work = nodes::create(
        &pool,
        &domain::NewNode {
            parent_id: Some(series.id),
            kind: domain::NodeKind::Work,
            medium: Some(domain::Medium::Game),
            title: "测试作品".into(),
            aliases: vec![],
            tags: vec![],
        },
    )
    .await
    .unwrap();

    let archived = nodes::archive(&pool, series.id).await.unwrap();
    assert_eq!(archived.status, domain::NodeStatus::Archived);
    assert_eq!(
        nodes::get(&pool, work.id).await.unwrap().status,
        domain::NodeStatus::Archived
    );
    assert!(nodes::tree(&pool).await.unwrap().is_empty());
    let home = wikiatlas_store::overview::home(&pool).await.unwrap();
    assert_eq!(home.works, 0);
    assert!(home.recent.is_empty());
}

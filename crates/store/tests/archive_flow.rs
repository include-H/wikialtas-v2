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
    let archived_list = nodes::archived(&pool, 50).await.unwrap();
    assert_eq!(archived_list.len(), 1, "只列归档根，不列子树里的重复项");
    assert_eq!(archived_list[0].id, series.id);
    let home = wikiatlas_store::overview::home(&pool).await.unwrap();
    assert_eq!(home.works, 0);
    assert!(home.recent.is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn 取消归档恢复整棵子树_有定稿的回到ready(pool: PgPool) {
    let series = nodes::create(
        &pool,
        &domain::NewNode {
            parent_id: None,
            kind: domain::NodeKind::Series,
            medium: None,
            title: "恢复系列".into(),
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
            title: "恢复作品".into(),
            aliases: vec![],
            tags: vec![],
        },
    )
    .await
    .unwrap();
    wikiatlas_store::content::commit(&pool, work.id, "定稿正文", 0, "首版", "human:local")
        .await
        .unwrap();

    nodes::archive(&pool, series.id).await.unwrap();
    assert!(nodes::tree(&pool).await.unwrap().is_empty());

    let restored = nodes::unarchive(&pool, series.id).await.unwrap();
    assert_eq!(
        restored.status,
        domain::NodeStatus::Draft,
        "系列没有正文，回 draft"
    );
    assert_eq!(
        nodes::get(&pool, work.id).await.unwrap().status,
        domain::NodeStatus::Ready,
        "有定稿的子树节点回 ready"
    );
    assert_eq!(nodes::tree(&pool).await.unwrap().len(), 2);
}

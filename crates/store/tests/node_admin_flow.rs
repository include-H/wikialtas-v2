//! 节点管理：改名 / 别名 / 标签 / 移动父节点，以及「不能移进自己子树」的环校验。

use sqlx::PgPool;
use wikiatlas_domain as domain;
use wikiatlas_store::nodes::{self, NodePatch};

fn new_node(title: &str, parent_id: Option<uuid::Uuid>) -> domain::NewNode {
    domain::NewNode {
        parent_id,
        kind: domain::NodeKind::Work,
        medium: Some(domain::Medium::Game),
        title: title.into(),
        aliases: vec![],
        tags: vec![],
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn 改名别名标签_同父下保留排序键(pool: PgPool) {
    let a = nodes::create(&pool, &new_node("旧名", None)).await.unwrap();
    let updated = nodes::update(
        &pool,
        a.id,
        &NodePatch {
            parent_id: None,
            kind: None,
            medium: None,
            title: Some("新名".into()),
            aliases: Some(vec!["别名一".into()]),
            tags: Some(vec!["测试".into()]),
            status: None,
            visibility: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.title, "新名");
    assert_eq!(updated.title_norm, "新名");
    assert_eq!(updated.aliases, vec!["别名一"]);
    assert_eq!(updated.tags, vec!["测试"]);
    assert_eq!(updated.sort_key, a.sort_key, "不换父就不动排序键");
}

#[sqlx::test(migrations = "./migrations")]
async fn 移动父节点_拒绝移进自己的子树(pool: PgPool) {
    let root = nodes::create(&pool, &new_node("根", None)).await.unwrap();
    let child = nodes::create(&pool, &new_node("子", Some(root.id)))
        .await
        .unwrap();
    let other = nodes::create(&pool, &new_node("另一个根", None))
        .await
        .unwrap();

    let moved = nodes::update(
        &pool,
        child.id,
        &NodePatch {
            parent_id: Some(Some(other.id)),
            kind: None,
            medium: None,
            title: None,
            aliases: None,
            tags: None,
            status: None,
            visibility: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(moved.parent_id, Some(other.id));

    let err = nodes::update(
        &pool,
        root.id,
        &NodePatch {
            parent_id: Some(Some(root.id)),
            kind: None,
            medium: None,
            title: None,
            aliases: None,
            tags: None,
            status: None,
            visibility: None,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("子树"), "应拒绝环：{err}");
}

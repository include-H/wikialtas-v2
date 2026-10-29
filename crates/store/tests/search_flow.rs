//! 读工具背后的检索 / 列作品（D69 的 `search_works` / `list_works`）。
//!
//! 真跑（`#[sqlx::test]` 每个用例一个临时库 + 跑迁移）。

use sqlx::PgPool;
use wikiatlas_domain as domain;
use wikiatlas_store::nodes;

fn work(title: &str, medium: domain::Medium) -> domain::NewNode {
    domain::NewNode {
        parent_id: None,
        kind: domain::NodeKind::Work,
        medium: Some(medium),
        title: title.into(),
        aliases: vec![],
        tags: vec![],
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn 检索命中子串且归一化精确优先(pool: PgPool) {
    let a = nodes::create(&pool, &work("空之轨迹 SC", domain::Medium::Game))
        .await
        .unwrap();
    let b = nodes::create(&pool, &work("零之轨迹", domain::Medium::Game))
        .await
        .unwrap();

    let hits = nodes::search(&pool, "轨迹", 10).await.unwrap();
    assert_eq!(hits.len(), 2, "两条都该命中「轨迹」");

    // 归一化精确（大小写 / 空白）应排第一
    let exact = nodes::search(&pool, "  空之轨迹 sc  ", 10).await.unwrap();
    assert_eq!(exact[0].id, a.id);
    let _ = b;
}

#[sqlx::test(migrations = "./migrations")]
async fn 检索不返回明显无关的节点(pool: PgPool) {
    let a = nodes::create(&pool, &work("仙剑奇侠传", domain::Medium::Game))
        .await
        .unwrap();
    let hits = nodes::search(&pool, "完全不相干的词zzz", 10).await.unwrap();
    assert!(hits.iter().all(|n| n.id != a.id), "不该误命中");
}

#[sqlx::test(migrations = "./migrations")]
async fn 列作品按介质过滤(pool: PgPool) {
    nodes::create(&pool, &work("塞尔达传说", domain::Medium::Game)).await.unwrap();
    nodes::create(&pool, &work("千与千寻", domain::Medium::Movie)).await.unwrap();

    let games = nodes::list_works(&pool, Some(domain::Medium::Game), None, 30).await.unwrap();
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].title, "塞尔达传说");

    let all = nodes::list_works(&pool, None, None, 30).await.unwrap();
    assert_eq!(all.len(), 2);
}

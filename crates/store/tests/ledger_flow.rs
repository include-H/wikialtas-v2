//! 事实账本落库真跑（`#[sqlx::test]`）。
//!
//! 盯的是：账本**复用 nodes(kind='doc') + contents**（不加新表，D56），
//! 并且**同一作品只有一个账本节点**（find 幂等）——不然每次研究都会多挂一篇「事实账本」。

use sqlx::PgPool;
use wikiatlas_domain as domain;
use wikiatlas_store::{ledger, nodes};

async fn work(pool: &PgPool, title: &str) -> domain::Node {
    nodes::create(
        pool,
        &domain::NewNode {
            parent_id: None,
            kind: domain::NodeKind::Work,
            medium: Some(domain::Medium::Game),
            title: title.into(),
            aliases: vec![],
            tags: vec![],
        },
    )
    .await
    .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn 首次保存建账本资料节点并落正文(pool: PgPool) {
    let w = work(&pool, "空之轨迹 FC").await;
    assert!(ledger::load(&pool, w.id).await.unwrap().is_none(), "一开始没有账本");

    let body = "## 断言\n\n| # | 字段 | 断言 | 来源 | 状态 |\n|---|---|---|---|---|\n| 1 | 首发日期 | 2006-03-09，Windows | S1 | 已核实 |\n\n## 来源\n\n- [S1] Falcom — https://x — 抓取 2026-10-12 — 「原文」\n";
    let ledger_id = ledger::save(&pool, w.id, body).await.unwrap();

    // 是一个 doc 资料节点、挂在作品下、带「账本」标签、不带介质
    let node = nodes::get(&pool, ledger_id).await.unwrap();
    assert_eq!(node.kind, domain::NodeKind::Doc);
    assert_eq!(node.parent_id, Some(w.id));
    assert_eq!(node.medium, None);
    assert!(node.tags.contains(&"账本".to_string()));
    assert_eq!(node.title, "事实账本");

    // 正文可读回（规范化后以换行结尾）
    let md = ledger::load(&pool, w.id).await.unwrap().unwrap();
    assert!(md.contains("首发日期"));
    assert!(md.ends_with('\n'));
}

#[sqlx::test(migrations = "./migrations")]
async fn 二次保存复用同一节点且版本递增(pool: PgPool) {
    let w = work(&pool, "仙剑奇侠传").await;
    let id1 = ledger::save(&pool, w.id, "## 断言\n\n| # | 字段 | 断言 | 来源 | 状态 |\n|---|---|---|---|---|\n| 1 | 首发日期 | 1995 | S1 | 已核实 |\n").await.unwrap();
    let id2 = ledger::save(&pool, w.id, "## 断言\n\n| # | 字段 | 断言 | 来源 | 状态 |\n|---|---|---|---|---|\n| 1 | 首发日期 | 1995-07-07 | S1 | 已核实 |\n").await.unwrap();
    assert_eq!(id1, id2, "同一个作品只应有一个账本节点");

    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM nodes WHERE parent_id = $1 AND tags @> ARRAY['账本']::text[]")
        .bind(w.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);

    // 内容是第二次那份，版本递增到 2
    let md = ledger::load(&pool, w.id).await.unwrap().unwrap();
    assert!(md.contains("1995-07-07"));
    let (ver,): (i32,) = sqlx::query_as("SELECT ver FROM contents WHERE node_id = $1")
        .bind(id1)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(ver, 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn 给不存在的作品记账本_报找不到(pool: PgPool) {
    let err = ledger::save(&pool, uuid::Uuid::now_v7(), "## 断言\n").await.unwrap_err();
    assert!(matches!(err, wikiatlas_store::Error::Domain(domain::Error::NotFound(_))), "{err:?}");
}

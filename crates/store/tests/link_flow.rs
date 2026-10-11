//! 建档 / 解链 / 忽略的存储层真跑（M2）。
//!
//! 盯三件事：**建档 = 建容器系列 + 建文档 + 挂链，一个事务** · **落位规则（撞名加后缀）** ·
//! **解链只删链、文档不动**。从 `library_flow.rs` 拆出来（单文件 ≤300 行）。

use serde_json::json;
use sqlx::PgPool;
use wikiatlas_domain as domain;
use domain::library::{ContainerKind, ManifestEntry, Source};
use wikiatlas_store::library::{self, BuildDoc, BuiltDoc, ManifestFilter};

use domain::library::LinkedBy;

fn entry_in(
    id: &str,
    title: &str,
    container_key: &str,
    container_title: &str,
    source: Source,
    kind: &str,
    format: Option<&str>,
) -> ManifestEntry {
    ManifestEntry {
        source,
        external_id: id.into(),
        title: title.into(),
        subtitle: None,
        aliases: vec![],
        year: None,
        kind: kind.into(),
        format: format.map(str::to_string),
        cover_url: None,
        url: None,
        container_key: container_key.into(),
        container_title: container_title.into(),
        container_kind: ContainerKind::Series,
        extra: String::new(),
        raw: json!({}),
    }
}

async fn build(pool: &PgPool, source: Source, id: &str) -> BuiltDoc {
    library::build_doc(
        pool,
        &BuildDoc {
            source,
            external_id: id.into(),
            role: None,
            parent_id: None,
            linked_by: LinkedBy::Pool,
        },
    )
    .await
    .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn 建档_系列容器先建系列_文档挂在它下面并挂链(pool: PgPool) {
    let e = entry_in("g1", "空之轨迹", "series:trails", "轨迹系列", Source::GameAtlas, "game", None);
    library::upsert_manifest(&pool, &[e]).await.unwrap();

    let built = build(&pool, Source::GameAtlas, "g1").await;
    assert!(!built.reused);
    assert_eq!(built.node.kind, domain::NodeKind::Work);
    assert_eq!(built.node.medium, Some(domain::Medium::Game));
    assert_eq!(built.node.title, "空之轨迹");

    // 系列节点被建出来了，文档挂在它下面
    let series: (String, String) = sqlx::query_as("SELECT id::text, title FROM nodes WHERE kind='series'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(series.1, "轨迹系列");
    assert_eq!(built.node.parent_id.unwrap().to_string(), series.0);

    // 链是 active / work / linked_by=pool
    let link: (String, String, String) = sqlx::query_as(
        "SELECT state, role, linked_by FROM library_links WHERE id = $1",
    )
    .bind(built.link_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(link, ("active".into(), "work".into(), "pool".into()));

    // 池子里这条已经是「有文档」
    let pool_rows = library::list_pool(&pool, &ManifestFilter { has_doc: Some(true), ..Default::default() })
        .await
        .unwrap();
    assert_eq!(pool_rows.len(), 1);
    assert_eq!(pool_rows[0].node_title.as_deref(), Some("空之轨迹"));
}

#[sqlx::test(migrations = "./migrations")]
async fn 同一个系列下再建一条_复用系列节点不重复建(pool: PgPool) {
    let a = entry_in("g1", "空之轨迹", "series:trails", "轨迹系列", Source::GameAtlas, "game", None);
    let b = entry_in("g2", "空之轨迹 SC", "series:trails", "轨迹系列", Source::GameAtlas, "game", None);
    library::upsert_manifest(&pool, &[a, b]).await.unwrap();

    build(&pool, Source::GameAtlas, "g1").await;
    build(&pool, Source::GameAtlas, "g2").await;

    let (series_n,): (i64,) = sqlx::query_as("SELECT count(*) FROM nodes WHERE kind='series'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(series_n, 1, "同一个容器只该建一个系列节点");
    let (work_n,): (i64,) = sqlx::query_as("SELECT count(*) FROM nodes WHERE kind='work'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(work_n, 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn 与父同名的条目加介质后缀(pool: PgPool) {
    // Komga 的常态：库/系列名 = 作品名 → 子节点必然与父同名
    let e = entry_in("k1", "百变小樱魔术卡", "series:ccs", "百变小樱魔术卡", Source::Komga, "book", Some("comic"));
    library::upsert_manifest(&pool, &[e]).await.unwrap();
    let built = build(&pool, Source::Komga, "k1").await;

    assert_eq!(built.node.title, "百变小樱魔术卡（漫画）");
    assert_eq!(built.node.medium, Some(domain::Medium::Manga));
}

#[sqlx::test(migrations = "./migrations")]
async fn 同作品不同介质_各建一个节点且带后缀(pool: PgPool) {
    let tv = entry_in("e1", "16bit的感动", "series:16bit", "16bit的感动", Source::Emby, "tv", None);
    let comic = entry_in("k1", "16bit的感动", "series:16bit", "16bit的感动", Source::Komga, "book", Some("comic"));
    library::upsert_manifest(&pool, &[tv, comic]).await.unwrap();

    let a = build(&pool, Source::Emby, "e1").await;
    let b = build(&pool, Source::Komga, "k1").await;

    assert_eq!(a.node.title, "16bit的感动（剧集）");
    assert_eq!(b.node.title, "16bit的感动（漫画）");
    assert_ne!(a.node.id, b.node.id, "不同介质不能挤在同一个节点上");
    let (same_parent,): (bool,) = sqlx::query_as("SELECT $1::uuid = $2::uuid")
        .bind(a.node.parent_id)
        .bind(b.node.parent_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(same_parent, "两条都该挂在同一个系列下");
}

#[sqlx::test(migrations = "./migrations")]
async fn 重跑同一个条目_复用节点不造第二个(pool: PgPool) {
    let e = entry_in("g1", "空之轨迹", "series:trails", "轨迹系列", Source::GameAtlas, "game", None);
    library::upsert_manifest(&pool, &[e]).await.unwrap();
    let first = build(&pool, Source::GameAtlas, "g1").await;

    // 解链（保留文档）后再建一次：该复用原来那个节点
    library::unlink(&pool, first.link_id).await.unwrap();
    let second = build(&pool, Source::GameAtlas, "g1").await;

    assert!(second.reused, "重跑同一个条目该复用节点");
    assert_eq!(second.node.id, first.node.id);
    let (work_n,): (i64,) = sqlx::query_as("SELECT count(*) FROM nodes WHERE kind='work'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(work_n, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn 解链只删链_文档还在(pool: PgPool) {
    let e = entry_in("g1", "空之轨迹", "series:trails", "轨迹系列", Source::GameAtlas, "game", None);
    library::upsert_manifest(&pool, &[e]).await.unwrap();
    let built = build(&pool, Source::GameAtlas, "g1").await;

    let node_id = library::unlink(&pool, built.link_id).await.unwrap();
    assert_eq!(node_id, built.node.id);
    let (nodes_n, links_n): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM nodes WHERE id = $1), (SELECT count(*) FROM library_links)",
    )
    .bind(built.node.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((nodes_n, links_n), (1, 0), "解链只该删链");
    // 解链后池子里它又回到「无文档」
    let rows = library::list_pool(&pool, &ManifestFilter { has_doc: Some(false), ..Default::default() })
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn 重复建档明确报重复(pool: PgPool) {
    let e = entry_in("g1", "空之轨迹", "series:trails", "轨迹系列", Source::GameAtlas, "game", None);
    library::upsert_manifest(&pool, &[e]).await.unwrap();
    build(&pool, Source::GameAtlas, "g1").await;

    let err = library::build_doc(
        &pool,
        &BuildDoc {
            source: Source::GameAtlas,
            external_id: "g1".into(),
            role: None,
            parent_id: None,
            linked_by: LinkedBy::Pool,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("已经挂过"), "得到：{err}");
}

#[sqlx::test(migrations = "./migrations")]
async fn 忽略的条目不出现在池子(pool: PgPool) {
    let a = entry_in("g1", "空之轨迹", "series:trails", "轨迹系列", Source::GameAtlas, "game", None);
    let b = entry_in("g2", "伊苏", "series:trails", "轨迹系列", Source::GameAtlas, "game", None);
    let c = entry_in("k1", "另一本", "series:other", "别的系列", Source::Komga, "book", Some("comic"));
    library::upsert_manifest(&pool, &[a, b, c]).await.unwrap();

    // 单条忽略 g1；整组忽略 轨迹系列 这个容器
    library::add_ignore(&pool, Some(Source::GameAtlas), Some("g1".into()), None, "没价值").await.unwrap();
    library::add_ignore(&pool, Some(Source::GameAtlas), None, Some("series:trails".into()), "整组不要").await.unwrap();

    let rows = library::list_pool(&pool, &ManifestFilter::default()).await.unwrap();
    assert_eq!(rows.len(), 1, "被忽略的单条与整组都不该出现");
    assert_eq!(rows[0].entry.external_id, "k1");

    let ignores = library::list_ignores(&pool).await.unwrap();
    assert_eq!(ignores.len(), 2);
    library::remove_ignore(&pool, ignores[0].id).await.unwrap();
    assert_eq!(library::list_ignores(&pool).await.unwrap().len(), 1);
}

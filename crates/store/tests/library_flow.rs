//! 清单 / 池子的存储层真跑（`#[sqlx::test]`）。
//!
//! 盯三件事：**一次扫描一个事务** · **指纹未变不重写** · **没扫到 = missing 而不是删除**。

use serde_json::json;
use sqlx::PgPool;
use wikiatlas_domain as domain;
use domain::library::{ContainerKind, LinkRole, ManifestEntry, Source};
use wikiatlas_store::library::{self, ManifestFilter};

fn entry(id: &str, title: &str) -> ManifestEntry {
    ManifestEntry {
        source: Source::GameAtlas,
        external_id: id.into(),
        title: title.into(),
        subtitle: None,
        aliases: vec![],
        year: Some(2010),
        kind: "game".into(),
        format: None,
        cover_url: None,
        url: None,
        container_key: "series:cod".into(),
        container_title: "使命召唤".into(),
        container_kind: ContainerKind::Series,
        extra: String::new(),
        raw: json!({ "public_id": id }),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn 扫描写入分新增变更未变_消失标missing不删行(pool: PgPool) {
    // 首次：两条都是新增
    let s = library::upsert_manifest(&pool, &[entry("a", "黑色行动"), entry("b", "现代战争")])
        .await
        .unwrap();
    assert_eq!((s.seen, s.inserted, s.changed, s.missing), (2, 2, 0, 0));

    // 原样重扫：指纹未变 → unchanged
    let s = library::upsert_manifest(&pool, &[entry("a", "黑色行动"), entry("b", "现代战争")])
        .await
        .unwrap();
    assert_eq!((s.inserted, s.changed, s.unchanged, s.missing), (0, 0, 2, 0));

    // 改标题 → changed；b 这次没扫到 → missing
    let s = library::upsert_manifest(&pool, &[entry("a", "黑色行动 2")]).await.unwrap();
    assert_eq!((s.inserted, s.changed, s.unchanged, s.missing), (0, 1, 0, 1));

    // 默认读不到消失的；显式 include_missing 才看得到，且行还在（不是删了）
    let f = ManifestFilter::default();
    let live = library::list_manifest(&pool, &f).await.unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].external_id, "a");

    let f_all = ManifestFilter { include_missing: true, ..Default::default() };
    let all = library::list_manifest(&pool, &f_all).await.unwrap();
    assert_eq!(all.len(), 2);
    let b = all.iter().find(|r| r.external_id == "b").unwrap();
    assert!(b.missing_since.is_some(), "消失的条目该带 missing_since");

    // b 再次出现 → 自动恢复（missing_since 清空，不是人工点两遍）
    let s = library::upsert_manifest(&pool, &[entry("a", "黑色行动 2"), entry("b", "现代战争")])
        .await
        .unwrap();
    assert_eq!(s.missing, 0);
    let live = library::list_manifest(&pool, &f).await.unwrap();
    assert_eq!(live.len(), 2);

    // 扫描记账
    let run = library::start_scan_run(&pool, Source::GameAtlas).await.unwrap();
    library::finish_scan_run(&pool, run, true, &json!({ "inserted": 2 }), "").await.unwrap();
    let runs = library::list_scan_runs(&pool, 10).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].ok, Some(true));
}

#[sqlx::test(migrations = "./migrations")]
async fn 池子按有无文档过滤_没挂链的就是无文档(pool: PgPool) {
    library::upsert_manifest(&pool, &[entry("a", "黑色行动"), entry("b", "现代战争")])
        .await
        .unwrap();

    let f = ManifestFilter::default();
    let pool_rows = library::list_pool(&pool, &f).await.unwrap();
    assert_eq!(pool_rows.len(), 2);
    assert!(pool_rows.iter().all(|p| !p.has_doc()));
    assert!(pool_rows.iter().all(|p| p.role == LinkRole::Work));

    // 手挂一条链（M2 才有正式建档 API，这里直接插一行验池子的 join）
    let node = wikiatlas_store::nodes::create(
        &pool,
        &domain::NewNode {
            parent_id: None,
            kind: domain::NodeKind::Work,
            medium: Some(domain::Medium::Game),
            title: "使命召唤：黑色行动".into(),
            aliases: vec![],
            tags: vec![],
        },
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO library_links (id, node_id, source, external_id, role, linked_by) \
         VALUES ($1, $2, 'gameatlas', 'a', 'work', 'pool')",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(node.id)
    .execute(&pool)
    .await
    .unwrap();

    let only_new = library::list_pool(&pool, &ManifestFilter { has_doc: Some(false), ..Default::default() })
        .await
        .unwrap();
    assert_eq!(only_new.len(), 1);
    assert_eq!(only_new[0].entry.external_id, "b");

    let linked = library::list_pool(&pool, &ManifestFilter { has_doc: Some(true), ..Default::default() })
        .await
        .unwrap();
    assert_eq!(linked.len(), 1);
    assert_eq!(linked[0].node_title.as_deref(), Some("使命召唤：黑色行动"));
}

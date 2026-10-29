//! M7：定稿入队、失效/恢复、重挂候选与替换链（`docs/library-sync.md` §5 §6）。

use serde_json::json;
use sqlx::PgPool;
use wikiatlas_domain as domain;
use domain::library::{ContainerKind, LinkedBy, ManifestEntry, Source};
use wikiatlas_store::library::{self, BuildDoc, ManifestFilter};

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
        url: Some(format!("http://ga/games/{id}")),
        container_key: "series:trails".into(),
        container_title: "轨迹系列".into(),
        container_kind: ContainerKind::Series,
        extra: String::new(),
        raw: json!({}),
    }
}

async fn build(pool: &PgPool, external_id: &str) -> uuid::Uuid {
    library::build_doc(
        pool,
        &BuildDoc {
            source: Source::GameAtlas,
            external_id: external_id.into(),
            role: None,
            parent_id: None,
            linked_by: LinkedBy::Pool,
        },
    )
    .await
    .unwrap()
    .link_id
}

#[sqlx::test(migrations = "./migrations")]
async fn 扫描消失标stale_再次出现自动恢复(pool: PgPool) {
    library::upsert_manifest(&pool, &[entry("g1", "空之轨迹")])
        .await
        .unwrap();
    let link_id = build(&pool, "g1").await;

    let s = library::upsert_manifest_for_source(&pool, Source::GameAtlas, &[])
        .await
        .unwrap();
    assert_eq!(s.stale_links, 1);
    let (state, missing): (String, bool) =
        sqlx::query_as("SELECT state, missing_since IS NOT NULL FROM library_links WHERE id = $1")
            .bind(link_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((state.as_str(), missing), ("stale", true));

    let s = library::upsert_manifest(&pool, &[entry("g1", "空之轨迹")])
        .await
        .unwrap();
    assert_eq!(s.restored_links, 1);
    let (state, missing): (String, bool) =
        sqlx::query_as("SELECT state, missing_since IS NOT NULL FROM library_links WHERE id = $1")
            .bind(link_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((state.as_str(), missing), ("active", false));
}

#[sqlx::test(migrations = "./migrations")]
async fn 定稿事务给可写链入队sync_job(pool: PgPool) {
    library::upsert_manifest(&pool, &[entry("g1", "空之轨迹")])
        .await
        .unwrap();
    let built = library::build_doc(
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
    .unwrap();

    let out = wikiatlas_store::content::commit(
        &pool,
        built.node.id,
        "# 空之轨迹\n\n这是简介。\n\n## 1. 作品概览\n\n正文。",
        0,
        "首版",
        "human:local",
    )
    .await
    .unwrap();
    assert!(out.sync_job_id.is_some(), "定稿事务应同时留下 sync job");

    let (kind, payload): (String, serde_json::Value) =
        sqlx::query_as("SELECT kind, payload FROM jobs WHERE id = $1")
            .bind(out.sync_job_id.unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(kind, "sync");
    assert_eq!(payload["node_id"], built.node.id.to_string());

    let jobs = wikiatlas_store::sync::sync_jobs_for_node(&pool, built.node.id)
        .await
        .unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].source, Source::GameAtlas);
    assert_eq!(jobs[0].ver, 1);
    assert!(jobs[0].body_md.contains("这是简介"));
}

#[sqlx::test(migrations = "./migrations")]
async fn 失效链给候选并重挂_旧链保留可追溯且文档不动(pool: PgPool) {
    library::upsert_manifest(
        &pool,
        &[entry("g1", "空之轨迹 SC"), entry("g2", "空之轨迹 SC 重制版")],
    )
    .await
    .unwrap();
    let old_link = build(&pool, "g1").await;
    let node_id: (uuid::Uuid,) =
        sqlx::query_as("SELECT node_id FROM library_links WHERE id = $1")
            .bind(old_link)
            .fetch_one(&pool)
            .await
            .unwrap();

    // g1 消失；g2 仍在。旧链 stale，候选应为 g2。
    library::upsert_manifest(&pool, &[entry("g2", "空之轨迹 SC 重制版")])
        .await
        .unwrap();
    let candidates = wikiatlas_store::sync::relink_candidates(&pool, old_link)
        .await
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].external_id, "g2");
    assert!(candidates[0].score > 0.0);

    let new_link = wikiatlas_store::sync::relink(
        &pool,
        old_link,
        "g2",
        LinkedBy::Manual,
    )
    .await
    .unwrap();

    let old: (String, uuid::Uuid) =
        sqlx::query_as("SELECT state, replaced_by FROM library_links WHERE id = $1")
            .bind(old_link)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(old, ("replaced".into(), new_link));
    let new: (String, uuid::Uuid, String) = sqlx::query_as(
        "SELECT state, node_id, external_id FROM library_links WHERE id = $1",
    )
    .bind(new_link)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(new, ("active".into(), node_id.0, "g2".into()));

    // 文档与原节点没被重挂影响。
    assert!(wikiatlas_store::nodes::get(&pool, node_id.0).await.is_ok());
    let pool_rows =
        library::list_pool(&pool, &ManifestFilter { has_doc: Some(false), ..Default::default() })
            .await
            .unwrap();
    assert!(pool_rows.is_empty(), "新条目已经挂上，不该再出现在无文档池子");
}

#[sqlx::test(migrations = "./migrations")]
async fn 自动反哺幂等_同版本ok跳过_失败可重试(pool: PgPool) {
    library::upsert_manifest(&pool, &[entry("g1", "空之轨迹")])
        .await
        .unwrap();
    let link_id = build(&pool, "g1").await;
    assert!(
        wikiatlas_store::sync::should_auto_sync(&pool, link_id, Source::GameAtlas, 3)
            .await
            .unwrap()
    );
    wikiatlas_store::sync::mark_ok(&pool, link_id, Source::GameAtlas, 3)
        .await
        .unwrap();
    assert!(
        !wikiatlas_store::sync::should_auto_sync(&pool, link_id, Source::GameAtlas, 3)
            .await
            .unwrap()
    );
    assert!(
        wikiatlas_store::sync::should_auto_sync(&pool, link_id, Source::GameAtlas, 4)
            .await
            .unwrap()
    );
}

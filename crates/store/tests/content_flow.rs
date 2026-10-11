//! 存储层真跑（`#[sqlx::test]` 每个用例一个临时库 + 跑迁移）。
//!
//! 盯的是这条纪律：**定稿是一个事务**（contents 版本 +1、快照、写入事实、清草稿），
//! 以及**乐观并发不覆盖别人的改动**。

use sqlx::PgPool;
use wikiatlas_domain as domain;
use wikiatlas_store::{content, nodes};

fn new_node(title: &str) -> domain::NewNode {
    domain::NewNode {
        parent_id: None,
        kind: domain::NodeKind::Work,
        medium: Some(domain::Medium::Game),
        title: title.into(),
        aliases: vec![],
        tags: vec![],
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn 定稿是一个事务_定稿加快照加写入事实加清草稿(pool: PgPool) {
    let node = nodes::create(&pool, &new_node("空之轨迹SC")).await.unwrap();

    // 先存草稿（前端防抖那一步）
    let draft = content::put_draft(&pool, node.id, "草稿正文", 0, "human:local").await.unwrap();
    assert_eq!(draft.base_ver, 0);
    // 规范化后的正文总以换行结尾（唯一规范形）
    assert_eq!(draft.body_md, "草稿正文\n");

    // 定稿
    let out = content::commit(&pool, node.id, "定稿正文", 0, "首版", "human:local").await.unwrap();
    assert_eq!(out.ver, 1);

    let detail = content::read(&pool, node.id).await.unwrap();
    assert_eq!(detail.content.as_ref().unwrap().body_md, "定稿正文\n");
    assert_eq!(detail.content.as_ref().unwrap().ver, 1);
    assert_eq!(detail.content.as_ref().unwrap().updated_by, "human:local");
    assert!(detail.draft.is_none(), "定稿后草稿该被清掉");

    let (snapshots,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM content_versions WHERE node_id = $1")
            .bind(node.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(snapshots, 1);

    let (writes,): (i64,) = sqlx::query_as("SELECT count(*) FROM write_events WHERE node_id = $1")
        .bind(node.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(writes, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn 乐观并发_版本不符时明确冲突且不动正文(pool: PgPool) {
    let node = nodes::create(&pool, &new_node("仙剑奇侠传2")).await.unwrap();

    content::commit(&pool, node.id, "甲写的", 0, "首版", "human:local").await.unwrap();

    // 乙手里还是 base_ver=0（基于空库），提交必须被挡，而不是把甲的覆盖掉
    let err = content::commit(&pool, node.id, "乙写的", 0, "", "human:local").await.unwrap_err();
    match err {
        wikiatlas_store::Error::Domain(domain::Error::Conflict { base_ver, current_ver }) => {
            assert_eq!((base_ver, current_ver), (0, 1));
        }
        other => panic!("期望版本冲突，得到 {other:?}"),
    }

    // 正文与版本都没被改
    let detail = content::read(&pool, node.id).await.unwrap();
    assert_eq!(detail.content.as_ref().unwrap().body_md, "甲写的\n");
    assert_eq!(detail.content.as_ref().unwrap().ver, 1);

    // 拿对版本再提交 → 通过，版本 +1
    let out = content::commit(&pool, node.id, "乙写的（已看最新）", 1, "", "human:local").await.unwrap();
    assert_eq!(out.ver, 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn 草稿不能基于未来的版本(pool: PgPool) {
    let node = nodes::create(&pool, &new_node("博物馆奇妙夜")).await.unwrap();
    content::commit(&pool, node.id, "正文", 0, "", "human:local").await.unwrap();

    let err = content::put_draft(&pool, node.id, "草稿", 5, "human:local").await.unwrap_err();
    assert!(matches!(err, wikiatlas_store::Error::Domain(domain::Error::Validation(_))), "{err:?}");

    // 基于当前版本则是允许的
    assert!(content::put_draft(&pool, node.id, "草稿", 1, "human:local").await.is_ok());
}

#[sqlx::test(migrations = "./migrations")]
async fn 同一父下依次建节点_排序键单调且能插中间(pool: PgPool) {
    let series = nodes::create(
        &pool,
        &domain::NewNode {
            parent_id: None,
            kind: domain::NodeKind::Series,
            medium: Some(domain::Medium::Game),
            title: "轨迹系列".into(),
            aliases: vec![],
            tags: vec![],
        },
    )
    .await
    .unwrap();

    let mut keys = vec![];
    for t in ["FC", "SC", "the 3rd"] {
        let n = nodes::create(
            &pool,
            &domain::NewNode {
                parent_id: Some(series.id),
                kind: domain::NodeKind::Work,
                medium: Some(domain::Medium::Game),
                title: t.into(),
                aliases: vec![],
                tags: vec![],
            },
        )
        .await
        .unwrap();
        keys.push(n.sort_key);
    }
    assert!(keys[0] < keys[1] && keys[1] < keys[2], "排序键应单调：{keys:?}");

    // 树按 sort_key 出来就是 FC → SC → the 3rd
    let all = nodes::tree(&pool).await.unwrap();
    let children: Vec<_> = all.iter().filter(|n| n.parent_id == Some(series.id)).collect();
    assert_eq!(children.len(), 3);
    assert_eq!(children[0].title, "FC");
    assert_eq!(children[2].title, "the 3rd");
}

#[sqlx::test(migrations = "./migrations")]
async fn 建节点走领域规则(pool: PgPool) {
    // work 缺介质
    let bad = domain::NewNode {
        parent_id: None,
        kind: domain::NodeKind::Work,
        medium: None,
        title: "没有介质的作品".into(),
        aliases: vec![],
        tags: vec![],
    };
    assert!(nodes::create(&pool, &bad).await.is_err());

    // doc 带介质
    let bad_doc = domain::NewNode {
        parent_id: None,
        kind: domain::NodeKind::Doc,
        medium: Some(domain::Medium::Game),
        title: "资料".into(),
        aliases: vec![],
        tags: vec![],
    };
    assert!(nodes::create(&pool, &bad_doc).await.is_err());

    // 标题归一化进 title_norm（全角/大小写/空白都折掉）
    let ok = nodes::create(&pool, &new_node("  ＣＬＡＮＮＡＤ  ")).await.unwrap();
    assert_eq!(ok.title, "ＣＬＡＮＮＡＤ");
    assert_eq!(ok.title_norm, "clannad");
}

#[sqlx::test(migrations = "./migrations")]
async fn 建到不存在的父节点上_报找不到(pool: PgPool) {
    let orphan = domain::NewNode {
        parent_id: Some(uuid::Uuid::now_v7()),
        kind: domain::NodeKind::Work,
        medium: Some(domain::Medium::Game),
        title: "孤儿".into(),
        aliases: vec![],
        tags: vec![],
    };
    let err = nodes::create(&pool, &orphan).await.unwrap_err();
    assert!(matches!(err, wikiatlas_store::Error::Domain(domain::Error::NotFound(_))), "{err:?}");
}

#[sqlx::test(migrations = "./migrations")]
async fn 写路径清掉编辑器塞进来的空段落_br_(pool: PgPool) {
    // 富文本编辑器会把「空段落」序列化成一行 <br />（Milkdown 就这德行）；
    // 上层不该为这个操心——规范化在 store 入口做。
    let node = nodes::create(&pool, &new_node("斗罗大陆")).await.unwrap();
    let out = content::commit(
        &pool,
        node.id,
        "# 斗罗大陆\n\n<br />\n\n唐三，唐门弟子。\n\n\n\n后记。",
        0,
        "",
        "human:local",
    )
    .await
    .unwrap();
    assert_eq!(out.ver, 1);

    let detail = content::read(&pool, node.id).await.unwrap();
    assert_eq!(
        detail.content.as_ref().unwrap().body_md,
        "# 斗罗大陆\n\n唐三，唐门弟子。\n\n后记。\n"
    );

    // 快照里存的也必须是清干净的那份（不然"版本历史"会把垃圾还魂）
    let (snap,): (String,) =
        sqlx::query_as("SELECT body_md FROM content_versions WHERE node_id = $1 AND ver = 1")
            .bind(node.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(snap, "# 斗罗大陆\n\n唐三，唐门弟子。\n\n后记。\n");
}

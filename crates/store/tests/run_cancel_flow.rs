//! 取消运行：排队中直接收尾；运行中只置标志（worker 在工具边界检查）。

use sqlx::PgPool;
use wikiatlas_store::runs;

fn new_run(id: &str) -> runs::NewRun {
    runs::NewRun {
        id: id.into(),
        workspace: "default".into(),
        intent: "对话".into(),
        goal: "测试取消".into(),
        model: "mock".into(),
        checkpoint: serde_json::json!({ "node_id": uuid::Uuid::now_v7().to_string() }),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn 排队中取消_直接收尾且job标done(pool: PgPool) {
    let id = format!("run:{}", uuid::Uuid::now_v7());
    runs::create(&pool, &new_run(&id)).await.unwrap();
    let (jid,): (i64,) = sqlx::query_as(
        "INSERT INTO jobs (kind, payload) VALUES ('run', jsonb_build_object('run_id', $1)) RETURNING id",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!runs::is_cancel_requested(&pool, &id).await.unwrap());

    assert!(runs::request_cancel(&pool, &id).await.unwrap());
    let run = runs::get(&pool, &id).await.unwrap();
    assert!(run.cancel_requested);
    assert_eq!(run.status, "cancelled", "排队中取消不必等 worker");
    let (state,): (String,) = sqlx::query_as("SELECT state FROM jobs WHERE id = $1")
        .bind(jid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "done");
    // 重复点取消：幂等 no-op
    assert!(!runs::request_cancel(&pool, &id).await.unwrap());
}

#[sqlx::test(migrations = "./migrations")]
async fn 运行中取消_只置标志(pool: PgPool) {
    let id = format!("run:{}", uuid::Uuid::now_v7());
    runs::create(&pool, &new_run(&id)).await.unwrap();
    runs::set_status(&pool, &id, "running", None, None)
        .await
        .unwrap();

    assert!(runs::request_cancel(&pool, &id).await.unwrap());
    let run = runs::get(&pool, &id).await.unwrap();
    assert!(run.cancel_requested);
    assert_eq!(run.status, "running", "运行中由 worker 收尾");

    runs::set_status(&pool, &id, "cancelled", None, None)
        .await
        .unwrap();
    assert!(!runs::request_cancel(&pool, &id).await.unwrap());
}

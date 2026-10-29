//! 多轮会话：append-only 日志、列表、详情与上下文重建。

use sqlx::PgPool;
use wikiatlas_domain as domain;
use wikiatlas_store::{nodes, runs};

fn msg(session: &str, role: &str, content: &str) -> runs::NewMessage {
    runs::NewMessage {
        session_id: session.into(),
        run_id: format!("run:{session}:{role}:{content}"),
        turn: 1,
        role: role.into(),
        content: content.into(),
        tool_calls_json: String::new(),
        tool_call_id: String::new(),
        tool_name: String::new(),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn 会话日志能重建多轮上下文并列出预览(pool: PgPool) {
    let node = nodes::create(
        &pool,
        &domain::NewNode {
            parent_id: None,
            kind: domain::NodeKind::Work,
            medium: Some(domain::Medium::Game),
            title: "会话测试".into(),
            aliases: vec![],
            tags: vec![],
        },
    )
    .await
    .unwrap();
    let sid = format!("work:{}:test", node.id);
    runs::ensure_session(&pool, &sid, &node.id.to_string(), "新对话")
        .await
        .unwrap();
    runs::append_message(&pool, &msg(&sid, "user", "第一问"))
        .await
        .unwrap();
    runs::append_message(&pool, &msg(&sid, "assistant", "第一答"))
        .await
        .unwrap();
    runs::append_message(&pool, &msg(&sid, "user", "第二问"))
        .await
        .unwrap();

    let ctx = runs::conversation_context(&pool, &sid, 10_000)
        .await
        .unwrap();
    assert_eq!(
        ctx,
        vec![
            ("user".into(), "第一问".into()),
            ("assistant".into(), "第一答".into()),
            ("user".into(), "第二问".into())
        ]
    );
    let list = runs::sessions_for_node(&pool, node.id).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].message_count, 3);
    assert_eq!(list[0].title, "第一问");
    assert_eq!(list[0].preview, "第二问");

    let detail = runs::session_messages(&pool, &sid, 20).await.unwrap();
    assert_eq!(detail.len(), 3);
    assert_eq!(detail[0].role, "user");
    assert_eq!(detail[2].content, "第二问");
}

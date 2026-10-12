//! run 执行器：把一条工单跑成「模型流 → 事件 → 草稿落库」。
//!
//! `docs/backend.md §5` 的落地：**正文从写工具的流式参数里出来**，边流边落草稿；
//! SSE（`run_events`）只是加速通道，**草稿行才是真相**。
//! LLM 调用在事务外（`backend.md §1`），每一步落库都是短事务。
//!
//! 这一版只装**一个写工具**（`write_content`）——M3 验收要的是把这条链打通，不是工具齐全。

use crate::client::{LlmConfig, ResponsesClient};
use crate::sse::StreamEvent;
use crate::guard::{self, DupCallGuard};
use crate::stream::{ArgProjector, Throttle};
use crate::tools::{call_read_skill, call_write, tool_defs};
use crate::{Error, Result};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;
use wikiatlas_store::{content, nodes, runs};

/// 单次工单的模型轮次上限（防绕圈，`behavior-notes.md §3`）。
const MAX_ITERS: u32 = 12;

#[derive(Debug, Clone)]
pub struct RunSummary {
    pub status: String,
    pub text: String,
    pub draft_written: bool,
    pub iterations: u32,
}

/// 跑一条工单。返回摘要；失败时把错误写进 `runs.error_json` 再抛。
pub async fn execute(pool: &PgPool, cfg: &LlmConfig, run_id: &str) -> Result<RunSummary> {
    let run = runs::get(pool, run_id).await?;
    let node_id: Uuid = run
        .checkpoint
        .get("node_id")
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| Error::RunState(format!("工单 {run_id} 没带 node_id")))?;

    runs::set_status(pool, run_id, "running", None, None).await?;
    let out = run_loop(pool, cfg, run_id, node_id, &run.goal).await;
    match &out {
        Ok(sum) => {
            runs::set_status(pool, run_id, "completed", Some(&json!({
                "draft_written": sum.draft_written, "iterations": sum.iterations
            })), None)
            .await?;
        }
        Err(e) => {
            runs::set_status(pool, run_id, "failed", None, Some(&json!({ "message": e.to_string() })))
                .await?;
        }
    }
    out
}

async fn run_loop(
    pool: &PgPool,
    cfg: &LlmConfig,
    run_id: &str,
    node_id: Uuid,
    goal: &str,
) -> Result<RunSummary> {
    let client = ResponsesClient::new(cfg.clone())?;
    let node = nodes::get(pool, node_id).await?;
    let detail = content::read(pool, node_id).await?;
    let base_ver = detail.content.as_ref().map(|c| c.ver).unwrap_or(0);
    let old_body = detail
        .draft
        .as_ref()
        .map(|d| d.body_md.clone())
        .or_else(|| detail.content.as_ref().map(|c| c.body_md.clone()))
        .unwrap_or_default();

    // 会话日志（append-only）：这一轮的用户意图先记下来。
    let session = format!("work:{node_id}");
    runs::ensure_session(pool, &session, &node.title, &node.title).await?;
    runs::append_message(pool, &runs::NewMessage {
        session_id: session.clone(),
        run_id: run_id.into(),
        turn: 1,
        role: "user".into(),
        content: goal.to_string(),
        tool_calls_json: String::new(),
        tool_call_id: String::new(),
        tool_name: String::new(),
    })
    .await?;

    emit(pool, run_id, "response.created", &json!({ "model": cfg.model })).await?;
    emit(pool, run_id, "wikiatlas.notice", &json!({ "text": "开始生成", "node": node.title })).await?;

    // 系统提示：骨架 + 技能目录（正文按需 read_skill，`prompts.md §4`）
    let root = crate::prompt::skills_root();
    let skills = crate::prompt::load(&root);
    let sys = crate::prompt::writing_system_prompt(&skills, node.medium.unwrap_or(wikiatlas_domain::Medium::Other));
    let medium = node.medium.unwrap_or(wikiatlas_domain::Medium::Other);

    // 目标节点的 id 必须**由宿主给出**：模型不该猜 id（猜错就写到别处/被护栏拦）。
    let user_text = format!("目标节点 id：{node_id}（《{}》）\n\n{goal}", node.title);
    let mut input = json!([
        { "role": "system", "content": [{ "type": "input_text", "text": sys }] },
        { "role": "user", "content": [{ "type": "input_text", "text": user_text }] }
    ]);

    let mut text_all = String::new();
    let mut draft_written = false;
    // 质量门纠错只给一次（`prompts.md §3.1`）
    let mut corrected = false;
    let mut dup = DupCallGuard::new();

    for iter in 1..=MAX_ITERS {
        // 第一轮强制调工具（M3 的意图就是"写"）；之后交给模型自己决定要不要继续。
        let choice = if iter == 1 { "required" } else { "auto" };
        let mut rx = client.stream(input.clone(), tool_defs(), choice)?;
        let mut turn_text = String::new();
        let mut projectors: HashMap<String, ArgProjector> = HashMap::new();
        let mut names: HashMap<String, String> = HashMap::new();
        let mut call_ids: HashMap<String, String> = HashMap::new();
        let mut args_done: HashMap<String, String> = HashMap::new();
        let mut throttle = Throttle::new(Duration::from_millis(200));
        let mut staging = false;

        while let Some(ev) = rx.recv().await {
            match ev? {
                StreamEvent::TextDelta(d) => {
                    turn_text.push_str(&d);
                    emit(pool, run_id, "response.output_text.delta", &json!({ "delta": d })).await?;
                }
                // 思考摘要：只用于展示，**不进上下文**（behavior-notes §1）
                StreamEvent::ReasoningDelta(_) => {}
                StreamEvent::FunctionCallAdded { item_id, name, call_id } => {
                    names.insert(item_id.clone(), name.clone());
                    call_ids.insert(item_id.clone(), call_id.clone());
                    if name == crate::tools::WRITE_TOOL {
                        projectors.insert(item_id.clone(), ArgProjector::new("content_md"));
                    }
                }
                StreamEvent::ArgsDelta { item_id, delta } => {
                    if let Some(p) = projectors.get_mut(&item_id) {
                        let cur = p.push(&delta).to_string();
                        if !staging {
                            staging = true;
                            emit(pool, run_id, "wikiatlas.content.staging",
                                 &json!({ "node_id": node_id })).await?;
                        }
                        if throttle.ready(Instant::now())
                            && !cur.is_empty()
                            && guard::check_write(node.status, &old_body, &cur).is_allow()
                        {
                            let by = format!("llm:{run_id}");
                            if content::put_draft(pool, node_id, &cur, base_ver, &by).await.is_ok() {
                                draft_written = true;
                                emit(pool, run_id, "wikiatlas.content.delta",
                                     &json!({ "body_md": cur })).await?;
                            }
                        }
                    }
                }
                StreamEvent::ArgsDone { item_id, arguments, .. } => {
                    args_done.insert(item_id, arguments);
                }
                StreamEvent::Completed { output, usage } => {
                    emit(pool, run_id, "response.completed",
                         &json!({ "output": output, "usage": usage })).await?;
                }
                StreamEvent::Other => {}
            }
        }

        text_all.push_str(&turn_text);
        if args_done.is_empty() {
            // 这轮没有工具调用 → 收工
            runs::append_message(pool, &runs::NewMessage {
                session_id: session.clone(), run_id: run_id.into(), turn: iter as i32,
                role: "assistant".into(), content: turn_text,
                tool_calls_json: String::new(), tool_call_id: String::new(), tool_name: String::new(),
            }).await?;
            emit(pool, run_id, "wikiatlas.run.end",
                 &json!({ "status": "completed", "draft_written": draft_written, "iterations": iter })).await?;
            return Ok(RunSummary { status: "completed".into(), text: text_all, draft_written, iterations: iter });
        }

        // 执行工具（波次：一次一个，全部有结果才进下一轮）
        for (item_id, arguments) in &args_done {
            let name = names.get(item_id).cloned().unwrap_or_default();
            let call_id = call_ids.get(item_id).cloned().unwrap_or_default();
            let out = if name == crate::tools::WRITE_TOOL {
                call_write(pool, node_id, base_ver, arguments, &node.status, &old_body, run_id, &mut dup).await
            } else if name == crate::tools::READ_SKILL_TOOL {
                call_read_skill(arguments)
            } else if name == crate::tools::WEB_SEARCH_TOOL {
                crate::tools::call_web_search(arguments).await
            } else {
                json!({ "ok": false, "reason": format!("没有这个工具：{name}") })
            };
            if name == crate::tools::WRITE_TOOL && out.get("ok").and_then(Value::as_bool) == Some(true) {
                draft_written = true;
            }
            emit(pool, run_id, "wikiatlas.tool.result",
                 &json!({ "name": name, "ok": out.get("ok"), "reason": out.get("reason") })).await?;
            // 追加到下一轮 input（Responses 的形状）
            push_item(&mut input, json!({
                "type": "function_call", "call_id": call_id, "name": name, "arguments": arguments
            }));
            push_item(&mut input, json!({
                "type": "function_call_output", "call_id": call_id, "output": out.to_string()
            }));
        }

        // 写完就跑**代码质量门**（`prompts.md §3`）：不过 → 回给模型一次纠错机会。
        if draft_written {
            let body = content::read(pool, node_id)
                .await
                .ok()
                .and_then(|d| d.draft.map(|x| x.body_md))
                .unwrap_or_default();
            let report = crate::quality::check(medium, &body, crate::quality::MIN_CHARS_FULL);
            emit(pool, run_id, "wikiatlas.quality", &serde_json::to_value(&report).unwrap_or(Value::Null)).await?;
            if report.passed() {
                emit(pool, run_id, "wikiatlas.run.end",
                     &json!({ "status": "completed", "draft_written": true, "iterations": iter,
                              "chars": report.chars, "chapters": report.chapters, "quality": "passed" })).await?;
                return Ok(RunSummary { status: "completed".into(), text: text_all, draft_written, iterations: iter });
            }
            let msgs = report.hard_messages();
            if corrected {
                // 纠错一次还不过 → 留草稿待人工（`prompts.md §3.1` 第 3 条）
                emit(pool, run_id, "wikiatlas.notice",
                     &json!({ "text": format!("质量门仍不过，草稿已留待人工：{}", msgs.join("；")) })).await?;
                emit(pool, run_id, "wikiatlas.run.end",
                     &json!({ "status": "completed", "draft_written": true, "iterations": iter,
                              "quality": "failed", "issues": msgs })).await?;
                return Ok(RunSummary { status: "completed".into(), text: text_all, draft_written, iterations: iter });
            }
            corrected = true;
            push_item(&mut input, json!({
                "role": "user",
                "content": [{ "type": "input_text",
                    "text": format!("宿主质量门没过，请用 write_content 重写整篇，逐条改掉：\n- {}", msgs.join("\n- ")) }]
            }));
        }
    }
    Err(Error::RunState(format!("超过 {MAX_ITERS} 轮还没收工")))
}

fn push_item(input: &mut Value, item: Value) {
    if let Some(arr) = input.as_array_mut() {
        arr.push(item);
    }
}

async fn emit(pool: &PgPool, run_id: &str, ty: &str, payload: &Value) -> Result<()> {
    runs::append_event(pool, run_id, ty, payload).await?;
    Ok(())
}

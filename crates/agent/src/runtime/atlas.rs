//! 通用 Atlas harness（D71）：**一个循环 + 一整套工具面**。
//!
//! 用户可能只是问，也可能要你写——**是答是写由模型按用户的话判断**，宿主只负责三件事：
//! 把工具面给全、把危险动作在动作层拦住（`policy`）、把每一步落日志。
//! 对齐 dsh `agent-loop` / Codex / OpenCode：模式只引导行为，不切换能力面。
//!
//! 三段式写作（研究 → 骨架 → 生成）收成了 `write_wiki` 工具（`design.md §2` D55 的结构修复保留），
//! 由模型**按需调用**——不再有"按 intent 走的第二条隐藏路径"。

use crate::client::ResponsesClient;
use crate::guard::{DupCallGuard, Verdict};
use crate::phases::{self, ATLAS_MAX_ITERS};
use crate::read_tools::{self, ANSWER_TOOL, FINISH_TOOL};
use crate::runtime::common::{emit, push_call, run_turn};
use crate::runtime::RunContext;
use crate::tools::{self, WRITE_WIKI_TOOL};
use crate::{policy, prompt, Result};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_store::runs;

/// 跑完一条工单的结果。
pub struct AtlasOutcome {
    pub answer: String,
    pub iterations: u32,
    pub wrote: bool,
}

/// 跑一条 Atlas 工单：建会话 → 一个工具循环 → 收尾。
pub async fn run(
    pool: &PgPool,
    client: &ResponsesClient,
    run_id: &str,
    node_id: Uuid,
    goal: &str,
    ctx: &RunContext,
) -> Result<AtlasOutcome> {
    let node = wikiatlas_store::nodes::get(pool, node_id).await?;

    let run = runs::get(pool, run_id).await?;
    let session = run
        .checkpoint
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("work:{node_id}"));
    runs::ensure_session(pool, &session, &node_id.to_string(), &node.title).await?;
    let history = runs::conversation_context(pool, &session, 30_000).await?;
    runs::append_message(
        pool,
        &runs::NewMessage {
            session_id: session.clone(),
            run_id: run_id.into(),
            turn: 1,
            role: "user".into(),
            content: goal.to_string(),
            tool_calls_json: String::new(),
            tool_call_id: String::new(),
            tool_name: String::new(),
        },
    )
    .await?;

    emit(pool, run_id, "response.created", &json!({ "model": client.config().model.clone() })).await?;
    let skills = prompt::load(&prompt::skills_root());
    let mut sys = phases::atlas_prompt(&skills, &node.title, &ctx.doc_mode, ctx.can_write());
    let loaded = prompt::loaded_skill_prompt(&skills, &ctx.skills);
    if !loaded.is_empty() {
        sys.push_str("\n\n");
        sys.push_str(&loaded);
        emit(
            pool,
            run_id,
            "wikiatlas.skill.loaded",
            &json!({ "skills": ctx.skills }),
        )
        .await?;
    }
    let mut input = vec![json!({
        "role": "system",
        "content": [{ "type": "input_text", "text": sys }]
    })];
    for (role, content) in history {
        input.push(json!({
            "role": role,
            "content": [{ "type": "input_text", "text": content }]
        }));
    }
    input.push(json!({
        "role": "user",
        "content": [{ "type": "input_text", "text": format!(
            "当前作品节点 id：{node_id}（《{}》）。\n用户说：{goal}",
            node.title
        ) }]
    }));
    let mut input = Value::Array(input);

    let mut dup = DupCallGuard::new();
    let mut visible = String::new();
    let mut answer = String::new();
    let mut refs: Vec<Value> = vec![];
    let mut wrote = false;
    let mut iterations = 0u32;

    for iter in 1..=ATLAS_MAX_ITERS {
        iterations = iter;
        let turn = run_turn(
            pool,
            run_id,
            client,
            input.clone(),
            atlas_tool_defs(ctx),
            "auto",
            &mut None,
        )
        .await?;
        if !turn.text.trim().is_empty() {
            visible.push_str(&turn.text);
        }
        if turn.calls.is_empty() {
            break; // 模型没调工具 = 它就是在回话
        }

        let mut done = false;
        for call in &turn.calls {
            let out = if let Verdict::Reject(r) = dup.check(&call.name, &call.arguments) {
                json!({ "ok": false, "reason": r })
            } else if let policy::Decision::Deny(r) =
                policy::check(&call.name, &node, wrote, ctx)
            {
                json!({ "ok": false, "reason": r })
            } else {
                if call.name == "ask_user" {
                    let q = serde_json::from_str::<Value>(&call.arguments)
                        .ok()
                        .and_then(|v| v.get("question").and_then(Value::as_str).map(str::to_string))
                        .unwrap_or_else(|| "需要你补充一条信息后才能继续。".into());
                    answer = q;
                    json!({ "ok": true, "waiting_for_user": true })
                } else {
                    dispatch(pool, client, run_id, node_id, ctx, call, &mut wrote).await
                }
            };
            emit(
                pool,
                run_id,
                "wikiatlas.tool.result",
                &json!({ "name": call.name, "ok": out.get("ok"), "reason": out.get("reason"),
                         "summary": tool_summary(&call.name, &out) }),
            )
            .await?;
            push_call(&mut input, call, &out);

            if call.name == ANSWER_TOOL && out.get("ok").and_then(Value::as_bool) == Some(true) {
                answer = out.get("text").and_then(Value::as_str).unwrap_or("").to_string();
                refs = out.get("refs").and_then(Value::as_array).cloned().unwrap_or_default();
                done = true;
            }
            if call.name == FINISH_TOOL {
                done = true;
            }
            if call.name == "ask_user" {
                done = true;
            }
        }
        if done {
            break;
        }
    }

    // 模型只回话、没走 `answer` 时，用它的正文兜底当回答
    if answer.trim().is_empty() {
        answer = visible.trim().to_string();
    }
    if answer.is_empty() {
        answer = "这一步我没能给出可交付的内容。".into();
    }

    emit(pool, run_id, "wikiatlas.answer", &json!({ "text": answer, "refs": refs })).await?;
    let _ = runs::append_message(
        pool,
        &runs::NewMessage {
            session_id: session,
            run_id: run_id.into(),
            turn: iterations as i32,
            role: "assistant".into(),
            content: answer.clone(),
            tool_calls_json: String::new(),
            tool_call_id: String::new(),
            tool_name: String::new(),
        },
    )
    .await;

    Ok(AtlasOutcome { answer, iterations, wrote })
}

/// 完整工具面：读 + 控制 + 写作。
fn atlas_tool_defs(ctx: &RunContext) -> Value {
    crate::harness_tools::defs(ctx)
}

/// 派发一个工具调用。写动作走 `write_wiki`（内部是研究→骨架→生成）。
async fn dispatch(
    pool: &PgPool,
    client: &ResponsesClient,
    run_id: &str,
    node_id: Uuid,
    ctx: &RunContext,
    call: &crate::runtime::common::ToolCall,
    wrote: &mut bool,
) -> Value {
    if call.name != WRITE_WIKI_TOOL {
        let out = crate::harness_tools::dispatch(pool, node_id, &call.name, &call.arguments, run_id)
            .await
            .unwrap_or_else(|| json!({ "ok": false, "reason": format!("没有这个工具：{}", call.name) }));
        if out.get("wrote").and_then(Value::as_bool) == Some(true) {
            *wrote = true;
        }
        return out;
    }
    // 校验 node_id：只允许写当前工单的目标节点
    #[derive(serde::Deserialize)]
    struct Args {
        #[serde(default)]
        node_id: String,
    }
    if let Ok(a) = serde_json::from_str::<Args>(&call.arguments) {
        if !a.node_id.trim().is_empty() && a.node_id.trim() != node_id.to_string() {
            return json!({ "ok": false, "reason": "只能写当前工单的目标节点" });
        }
    }
    match super::pipeline(pool, client, run_id, node_id, &ctx.skills).await {
        Ok(w) => {
            if w.draft_written {
                *wrote = true;
            }
            json!({
                "ok": true,
                "wrote": w.draft_written,
                "chapters": w.chapters,
                "chars": w.chars,
                "quality": if w.quality_passed { "passed" } else { "failed" },
                "citations": if w.citations_passed { "passed" } else { "failed" },
                "ledger_coverage": w.ledger_coverage,
            })
        }
        Err(e) => json!({ "ok": false, "reason": format!("撰写失败：{e}") }),
    }
}

/// 给右栏工具折叠条的一句人话摘要（不摊 JSON）。
fn tool_summary(name: &str, out: &Value) -> String {
    match name {
        read_tools::SEARCH_WORKS_TOOL => {
            format!("命中 {} 个节点", out.get("count").and_then(Value::as_u64).unwrap_or(0))
        }
        read_tools::LIST_WORKS_TOOL => {
            format!("列出 {} 部作品", out.get("count").and_then(Value::as_u64).unwrap_or(0))
        }
        read_tools::GET_TREE_TOOL => {
            format!("取到 {} 个节点", out.get("count").and_then(Value::as_u64).unwrap_or(0))
        }
        read_tools::READ_NODE_TOOL | read_tools::READ_DOC_TOOL => {
            let t = out.get("node").and_then(|n| n.get("title")).and_then(Value::as_str).unwrap_or("");
            format!("读了《{t}》")
        }
        tools::READ_LEDGER_TOOL => "读了事实账本".into(),
        tools::READ_SKILL_TOOL => {
            format!("读了技能 {}", out.get("name").and_then(Value::as_str).unwrap_or(""))
        }
        WRITE_WIKI_TOOL => {
            if out.get("ok").and_then(Value::as_bool) == Some(true) {
                format!(
                    "写了一篇（{} 字 · 账本覆盖 {}%）",
                    out.get("chars").and_then(Value::as_u64).unwrap_or(0),
                    out.get("ledger_coverage").and_then(Value::as_u64).unwrap_or(0),
                )
            } else {
                "撰写没成功".into()
            }
        }
        "list_library" => format!(
            "媒体库 {} 条",
            out.get("count").and_then(Value::as_u64).unwrap_or(0)
        ),
        "search_sessions" => format!(
            "找到 {} 个会话",
            out.get("count").and_then(Value::as_u64).unwrap_or(0)
        ),
        "list_skills" => format!(
            "列出 {} 个技能",
            out.get("count").and_then(Value::as_u64).unwrap_or(0)
        ),
        "upsert_work" => "更新了节点元数据".into(),
        "write_content" => "覆盖了正文草稿".into(),
        "patch_section" | "patch_sections" => "更新了正文草稿".into(),
        "edit" => "局部修改了草稿".into(),
        "upsert_relation" | "upsert_relations" => "更新了关系".into(),
        "detach_relation" => "删除了关系".into(),
        "attach_library_link" | "relink" | "rebuild_doc" => "更新了媒体链".into(),
        "detach_link" => "解除了媒体链".into(),
        "create_doc" => "创建了资料".into(),
        "archive_node" => "归档了节点".into(),
        "todo_write" => "更新了任务清单".into(),
        "ask_user" => "等待用户补充信息".into(),
        other => other.to_string(),
    }
}

//! 研究段（`docs/design.md §2.2`）：查资料 → 记事实账本，**不写正文**。
//!
//! 收工条件由**代码按必填字段覆盖率**算（每轮把「还缺什么」注入下一轮），
//! 预算兜底：检索 ≤ `SEARCH_BUDGET`、抓取 ≤ `FETCH_BUDGET`、轮次 ≤ `RESEARCH_MAX_ITERS`。
//! 预算用尽仍没覆盖 → 缺口记 `待核实`，照样进下一段（宁可带缺口写完，不许卡死）。

use crate::client::ResponsesClient;
use crate::guard::DupCallGuard;
use crate::ledger::Ledger;
use crate::phases::{self, Phase, FETCH_BUDGET, RESEARCH_MAX_ITERS, SEARCH_BUDGET};
use crate::runtime::common::{emit, push_call, push_item, run_turn, ToolCall};
use crate::{prompt, tools, Result};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain::Medium;

/// 研究段执行器的抽象（`docs/design.md §2.6` · D61）。
///
/// 默认用内置实现；只有当 A/B 评测里账本覆盖率持续 <80% 时，才接外部 harness
/// （经 MCP 暴露 `ledger_put` / `read_node`）。接口先留着，**默认不换**。
#[allow(async_fn_in_trait)]
pub trait ResearchRunner {
    async fn research(
        &self,
        pool: &PgPool,
        client: &ResponsesClient,
        run_id: &str,
        work_id: Uuid,
        medium: Medium,
        node_title: &str,
        ledger: &mut Ledger,
        loaded_skills: &[String],
    ) -> Result<Option<Uuid>>;
}

/// 内置研究段（当前唯一实现）。
pub struct BuiltinResearch;

impl ResearchRunner for BuiltinResearch {
    async fn research(
        &self,
        pool: &PgPool,
        client: &ResponsesClient,
        run_id: &str,
        work_id: Uuid,
        medium: Medium,
        node_title: &str,
        ledger: &mut Ledger,
        loaded_skills: &[String],
    ) -> Result<Option<Uuid>> {
        run(
            pool,
            client,
            run_id,
            work_id,
            medium,
            node_title,
            ledger,
            loaded_skills,
        )
        .await
    }
}

/// 跑研究段，更新 `ledger` 并把账本落库。返回账本节点 id（没有则 None）。
pub async fn run(
    pool: &PgPool,
    client: &ResponsesClient,
    run_id: &str,
    work_id: Uuid,
    medium: Medium,
    node_title: &str,
    ledger: &mut Ledger,
    loaded_skills: &[String],
) -> Result<Option<Uuid>> {
    let skills = prompt::load(&prompt::skills_root());
    let search_budget = budget(pool, "research.max_search_calls", SEARCH_BUDGET).await;
    let fetch_budget = budget(pool, "research.max_fetch_calls", FETCH_BUDGET).await;
    let max_iters = budget(pool, "research.max_iterations", RESEARCH_MAX_ITERS).await;
    let mut sys = phases::research_prompt(
        &skills,
        medium,
        node_title,
        search_budget,
        fetch_budget,
    );
    let loaded = prompt::loaded_skill_prompt(&skills, loaded_skills);
    if !loaded.is_empty() {
        sys.push_str("\n\n");
        sys.push_str(&loaded);
    }
    let mut input = json!([
        { "role": "system", "content": [{ "type": "input_text", "text": sys }] },
        { "role": "user", "content": [{ "type": "input_text",
            "text": format!("目标节点 id：{work_id}（《{node_title}》）。开始研究并把事实记进账本。") }] }
    ]);

    let mut searches = 0u32;
    let mut fetches = 0u32;
    let mut empty_turns = 0u32;
    let mut dup = DupCallGuard::new();

    for iter in 1..=max_iters {
        crate::runtime::common::ensure_not_cancelled(pool, run_id).await?;
        // 每轮开头把「还缺什么」用代码算出来注入（模型不用自己记账）
        if iter > 1 {
            let cov = ledger.coverage(medium);
            if !cov.missing.is_empty() {
                push_item(
                    &mut input,
                    json!({ "role": "user", "content": [{ "type": "input_text", "text": format!(
                        "宿主用代码算的覆盖率：还缺这些字段——{}。查到就用 ledger_put 补上；查不到就记「待核实」。\
                         检索已用 {searches}/{search_budget}，抓取已用 {fetches}/{fetch_budget}。",
                        cov.missing.join("、")
                    ) }] }),
                );
            }
        }
        let choice = if iter == 1 { "required" } else { "auto" };
        let turn = run_turn(pool, run_id, client, input.clone(), tools::research_tool_defs(), choice, &mut None).await?;

        if turn.calls.is_empty() {
            empty_turns += 1;
            if ledger.coverage(medium).missing.is_empty() || empty_turns >= 2 {
                break;
            }
            continue;
        }
        empty_turns = 0;

        let mut results: Vec<(ToolCall, Value)> = vec![];
        for call in &turn.calls {
            crate::runtime::common::ensure_not_cancelled(pool, run_id).await?;
            let out = dispatch(
                pool,
                work_id,
                ledger,
                medium,
                call,
                &mut searches,
                &mut fetches,
                search_budget,
                fetch_budget,
                &mut dup,
            )
            .await;
            results.push((call.clone(), out));
        }
        for (call, out) in results {
            emit(
                pool,
                run_id,
                "wikiatlas.tool.result",
                &json!({ "name": call.name, "ok": out.get("ok"), "reason": out.get("reason") }),
            )
            .await?;
            push_call(&mut input, &call, &out);
        }

        // 收工：覆盖满 or 预算用尽
        let cov = ledger.coverage(medium);
        if cov.missing.is_empty() || (searches >= search_budget && fetches >= fetch_budget) {
            break;
        }
    }

    // 研究收工：把账本正文落库，并回报覆盖率
    let node_id = wikiatlas_store::ledger::save(pool, work_id, &ledger.render()).await?;
    let gate = ledger.gate(medium);
    emit(
        pool,
        run_id,
        "wikiatlas.ledger",
        &json!({
            "node_id": node_id,
            "coverage": (gate.ratio * 100.0).round() as u32,
            "missing": gate.missing,
            "unique_urls": gate.unique_urls,
            "phase": Phase::Research.as_str(),
        }),
    )
    .await?;
    Ok(Some(node_id))
}

/// 执行一个研究段工具。
async fn dispatch(
    pool: &PgPool,
    work_id: Uuid,
    ledger: &mut Ledger,
    medium: Medium,
    call: &ToolCall,
    searches: &mut u32,
    fetches: &mut u32,
    search_budget: u32,
    fetch_budget: u32,
    dup: &mut DupCallGuard,
) -> Value {
    // 重复调用分级（同名同参 >3 拦；`behavior-notes.md §3`）
    if let crate::guard::Verdict::Reject(reason) = dup.check(&call.name, &call.arguments) {
        return json!({ "ok": false, "reason": reason });
    }
    match call.name.as_str() {
        tools::READ_SKILL_TOOL => tools::call_read_skill(&call.arguments),
        tools::WEB_SEARCH_TOOL => {
            if *searches >= search_budget {
                json!({ "ok": false, "reason": format!("检索预算用尽（{search_budget} 次）——把已有的记完就收工") })
            } else {
                *searches += 1;
                tools::call_web_search(&call.arguments).await
            }
        }
        tools::WEB_FETCH_TOOL => {
            if *fetches >= fetch_budget {
                json!({ "ok": false, "reason": format!("抓取预算用尽（{fetch_budget} 页）") })
            } else {
                *fetches += 1;
                tools::call_web_fetch(&call.arguments).await
            }
        }
        tools::LEDGER_PUT_TOOL => {
            let before = ledger.clone();
            let out = tools::call_ledger_put(&call.arguments, medium, ledger);
            if out.get("ok").and_then(Value::as_bool) == Some(true) {
                // 增量落库：崩了也不丢已查到的事实
                if let Err(e) = wikiatlas_store::ledger::save(pool, work_id, &ledger.render()).await {
                    tracing::warn!("账本增量落库失败：{e}");
                }
            } else {
                *ledger = before; // 没记成就别改内存
            }
            out
        }
        other => json!({ "ok": false, "reason": format!("研究段没有这个工具：{other}") }),
    }
}

async fn budget(pool: &PgPool, key: &str, default: u32) -> u32 {
    wikiatlas_store::secrets::get_setting(pool, key)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_u64())
        .map(|v| v.clamp(1, 200) as u32)
        .unwrap_or(default)
}

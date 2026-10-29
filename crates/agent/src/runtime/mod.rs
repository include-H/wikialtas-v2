//! run 执行器（D71）：跑**一个通用 Atlas harness**（`atlas`），把工具循环里的每一步落库。
//!
//! 三段式写作（研究 → 骨架 → 生成）现在是 `write_wiki` 工具的**实现**（`pipeline`），
//! 由模型按需调用——不再是按 `intent` 走的第二条路径。
//!
//! 边界（`backend.md §1 §3`）：LLM 调用在事务外，每一步落库都是短事务；
//! 这一层只编排，机械判据在 `agent::{ledger, skeleton, quality, citations}` 里。
//!
//! D41 例外：本模块按子模块拆分（`common` / `research` / `outline` / `generation` / `atlas`），
//! 每个文件各自 ≤300 行左右；`mod.rs` 只做编排与写作管线。

mod atlas;
pub(crate) mod common;
mod generation;
mod outline;
mod research;

use crate::client::{LlmConfig, ResponsesClient};
use crate::ledger::Ledger;
use crate::phases::Phase;
use crate::runtime::research::ResearchRunner;
use crate::{Error, Result};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain::{Medium, NodeStatus};
use wikiatlas_store::runs;

/// 用户显式选择的运行上下文。模式决定是否剥写工具；Skill 是用户显式加载项。
#[derive(Debug, Clone, Default)]
pub struct RunContext {
    pub doc_mode: String,
    pub one_shot: bool,
    /// 用户从 UI 显式加载的技能名。正文在每次模型调用前从 `skills/` 现读。
    pub skills: Vec<String>,
}

impl RunContext {
    pub fn from_checkpoint(v: &Value) -> Self {
        let c = v.get("context").cloned().unwrap_or_else(|| json!({}));
        Self {
            doc_mode: c
                .get("doc_mode")
                .and_then(Value::as_str)
                .unwrap_or("edit")
                .into(),
            one_shot: c.get("one_shot").and_then(Value::as_bool).unwrap_or(false),
            skills: c
                .get("skills")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .take(8)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    pub fn can_write(&self) -> bool {
        self.doc_mode != "read" && !self.one_shot
    }
}

/// 一个工单跑完的摘要。
#[derive(Debug, Clone)]
pub struct RunSummary {
    pub status: String,
    pub text: String,
    pub draft_written: bool,
    pub iterations: u32,
}

/// `write_wiki` 跑完的摘要（三段式写作）。
#[derive(Debug, Clone)]
pub(crate) struct WriteOutcome {
    pub draft_written: bool,
    pub quality_passed: bool,
    pub citations_passed: bool,
    pub ledger_coverage: u32,
    pub chars: usize,
    pub chapters: usize,
}

/// 跑一条工单。失败时把错误写进 `runs.error_json` 再抛。
pub async fn execute(pool: &PgPool, cfg: &LlmConfig, run_id: &str) -> Result<RunSummary> {
    let run = runs::get(pool, run_id).await?;
    let node_id = run
        .checkpoint
        .get("node_id")
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| Error::RunState(format!("工单 {run_id} 没带 node_id")))?;

    // 排队期间就被取消：不要再起模型调用，直接收尾。
    if run.cancel_requested || run.status == "cancelled" {
        common::emit(
            pool,
            run_id,
            "wikiatlas.run.end",
            &json!({ "status": "cancelled", "draft_written": false }),
        )
        .await?;
        runs::set_status(pool, run_id, "cancelled", None, None).await?;
        return Ok(RunSummary {
            status: "cancelled".into(),
            text: "已取消".into(),
            draft_written: false,
            iterations: 0,
        });
    }

    runs::set_status(pool, run_id, "running", None, None).await?;
    let client = ResponsesClient::new(cfg.clone())?;
    let context = RunContext::from_checkpoint(&run.checkpoint);

    match atlas::run(pool, &client, run_id, node_id, &run.goal, &context).await {
        Ok(o) => {
            common::emit(
                pool,
                run_id,
                "wikiatlas.run.end",
                &json!({ "status": "completed", "draft_written": o.wrote, "iterations": o.iterations }),
            )
            .await?;
            runs::set_status(
                pool,
                run_id,
                "completed",
                Some(&json!({ "draft_written": o.wrote, "iterations": o.iterations })),
                None,
            )
            .await?;
            Ok(RunSummary {
                status: "completed".into(),
                text: o.answer,
                draft_written: o.wrote,
                iterations: o.iterations,
            })
        }
        Err(Error::Cancelled) => {
            common::emit(
                pool,
                run_id,
                "wikiatlas.run.end",
                &json!({ "status": "cancelled", "draft_written": false }),
            )
            .await?;
            // 取消是正常收尾，不是失败：草稿保留，用户可继续看 / 定稿。
            runs::set_status(pool, run_id, "cancelled", None, None).await?;
            Ok(RunSummary {
                status: "cancelled".into(),
                text: "已取消".into(),
                draft_written: false,
                iterations: 0,
            })
        }
        Err(e) => {
            runs::set_status(pool, run_id, "failed", None, Some(&json!({ "message": e.to_string() })))
                .await?;
            Err(e)
        }
    }
}

/// 三段式写作管线（`write_wiki` 工具的实现）：研究 → 骨架 → 生成。
pub(crate) async fn pipeline(
    pool: &PgPool,
    client: &ResponsesClient,
    run_id: &str,
    node_id: Uuid,
    loaded_skills: &[String],
) -> Result<WriteOutcome> {
    let (node, base_ver, old_body) = common::node_base(pool, node_id).await?;
    if node.status == NodeStatus::Archived {
        return Err(Error::RunState("这个节点已归档，写工具一律拒绝（先取消归档）".into()));
    }
    let medium = node.medium.unwrap_or(Medium::Other);

    common::ensure_not_cancelled(pool, run_id).await?;
    common::emit(
        pool,
        run_id,
        "wikiatlas.notice",
        &json!({ "text": "开始撰写：研究 → 骨架 → 生成", "node": node.title }),
    )
    .await?;

    // ---- 研究段 ----
    let mut ledger = match wikiatlas_store::ledger::load(pool, node_id).await? {
        Some(md) => Ledger::parse(&md),
        None => Ledger::default(),
    };
    if ledger.is_empty() {
        phase(pool, run_id, Phase::Research).await?;
        research::BuiltinResearch
            .research(
                pool,
                client,
                run_id,
                node_id,
                medium,
                &node.title,
                &mut ledger,
                loaded_skills,
            )
            .await?;
    } else {
        common::emit(pool, run_id, "wikiatlas.notice", &json!({ "text": "已有事实账本，跳过研究段" }))
            .await?;
    }
    let gate = ledger.gate(medium);

    // ---- 骨架段 ----
    common::ensure_not_cancelled(pool, run_id).await?;
    phase(pool, run_id, Phase::Outline).await?;
    let outline = outline::run(
        pool,
        client,
        run_id,
        medium,
        &node.title,
        &ledger,
        loaded_skills,
    )
    .await?;
    if let Ok(v) = serde_json::to_value(&outline) {
        runs::set_checkpoint(pool, run_id, "outline", &v).await?;
    }

    // ---- 生成段 ----
    common::ensure_not_cancelled(pool, run_id).await?;
    phase(pool, run_id, Phase::Generation).await?;
    let gen = generation::run(
        pool,
        client,
        run_id,
        node_id,
        medium,
        &node.title,
        &outline,
        &ledger,
        old_body,
        base_ver,
        loaded_skills,
    )
    .await?;

    let passed = gen.quality.passed() && gen.citations.passed();
    common::emit(
        pool,
        run_id,
        "wikiatlas.write.end",
        &json!({
            "node_id": node_id,
            "draft_written": gen.wrote,
            "quality": if gen.quality.passed() { "passed" } else { "failed" },
            "citations": if gen.citations.passed() { "passed" } else { "failed" },
            "ledger_coverage": (gate.ratio * 100.0).round() as u32,
            "ledger_missing": gate.missing,
            "chars": gen.body.chars().count(),
            "chapters": gen.quality.chapters,
        }),
    )
    .await?;

    if !passed {
        let mut issues = gen.quality.hard_messages();
        issues.extend(gen.citations.issues.clone());
        common::emit(
            pool,
            run_id,
            "wikiatlas.notice",
            &json!({ "text": format!("质量门/引用门未全过，草稿已留待人工：{}", issues.join("；")) }),
        )
        .await?;
    }

    Ok(WriteOutcome {
        draft_written: gen.wrote,
        quality_passed: gen.quality.passed(),
        citations_passed: gen.citations.passed(),
        ledger_coverage: (gate.ratio * 100.0).round() as u32,
        chars: gen.body.chars().count(),
        chapters: gen.quality.chapters.len(),
    })
}

/// 切阶段：落 `checkpoint.phase` 并广播。
async fn phase(pool: &PgPool, run_id: &str, p: Phase) -> Result<()> {
    runs::set_phase(pool, run_id, p.as_str()).await?;
    common::emit(pool, run_id, "wikiatlas.phase", &json!({ "phase": p.as_str(), "label": p.label() })).await?;
    Ok(())
}

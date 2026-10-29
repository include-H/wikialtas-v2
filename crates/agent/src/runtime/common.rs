//! 工单执行的公共件：事件、一轮模型往返、草稿投影。
//!
//! 从 `runtime.rs` 拆出来（`docs/backend.md §3` D41：`agent::runtime` 允许更高上限，
//! 但必须按子模块拆分——这里放三段都要用的那部分，阶段逻辑各回各的文件）。

use crate::client::ResponsesClient;
use crate::sse::StreamEvent;
use crate::stream::{extract_string_field, Throttle};
use crate::Result;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;
use wikiatlas_store::{content, nodes, runs};

use crate::tools::PATCH_SECTION_TOOL;

/// 一次工具调用（收齐参数后）。
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    pub call_id: String,
    pub arguments: String,
}

/// 一轮模型往返的结果。
#[derive(Debug, Default)]
pub struct TurnOutcome {
    pub text: String,
    pub calls: Vec<ToolCall>,
}

/// 记一条事件（SSE 的真相源，`docs/api.md §4`）。
pub async fn emit(pool: &PgPool, run_id: &str, ty: &str, payload: &Value) -> Result<()> {
    runs::append_event(pool, run_id, ty, payload).await?;
    Ok(())
}

/// 往下一轮的 input 追加一项（Responses 的形状）。
pub fn push_item(input: &mut Value, item: Value) {
    if let Some(arr) = input.as_array_mut() {
        arr.push(item);
    }
}

/// 拼 `function_call` + `function_call_output` 这对。
pub fn push_call(input: &mut Value, call: &ToolCall, out: &Value) {
    push_item(
        input,
        json!({ "type": "function_call", "call_id": call.call_id, "name": call.name, "arguments": call.arguments }),
    );
    push_item(
        input,
        json!({ "type": "function_call_output", "call_id": call.call_id, "output": out.to_string() }),
    );
}

/// 目标节点的基线：定稿版本 + 现有正文（草稿优先，因为它更新）。
pub async fn node_base(pool: &PgPool, node_id: Uuid) -> Result<(wikiatlas_domain::Node, i32, String)> {
    let node = nodes::get(pool, node_id).await?;
    let detail = content::read(pool, node_id).await?;
    let base_ver = detail.content.as_ref().map(|c| c.ver).unwrap_or(0);
    let old_body = detail
        .draft
        .as_ref()
        .map(|d| d.body_md.clone())
        .or_else(|| detail.content.as_ref().map(|c| c.body_md.clone()))
        .unwrap_or_default();
    Ok((node, base_ver, old_body))
}

/// 草稿投影：把 `patch_section.new_markdown` 的**增量**拼进 `base_body` 落草稿。
///
/// 这是「一章一章长出来」的来源（`behavior-notes.md §5`）。**草稿行才是真相**，
/// `content.delta` 只是加速通道；收尾时会用完整参数再拼一次（以权威结果为准）。
pub struct DraftProjector {
    pub pool: PgPool,
    pub node_id: Uuid,
    pub base_ver: i32,
    pub by: String,
    /// 这一章写之前的正文（固定的基准）。
    pub base_body: String,
    buf: String,
    last: String,
    throttle: Throttle,
    staged: bool,
    pub wrote: bool,
}

impl DraftProjector {
    pub fn new(pool: &PgPool, node_id: Uuid, base_ver: i32, by: String, base_body: String) -> Self {
        Self {
            pool: pool.clone(),
            node_id,
            base_ver,
            by,
            base_body,
            buf: String::new(),
            last: String::new(),
            throttle: Throttle::new(Duration::from_millis(200)),
            staged: false,
            wrote: false,
        }
    }

    /// 追加一段参数增量，尝试把当前能确定的正文落草稿。
    pub async fn on_delta(&mut self, run_id: &str, delta: &str) -> Result<()> {
        self.buf.push_str(delta);
        let anchor = extract_string_field(&self.buf, "anchor").unwrap_or_default();
        let md = extract_string_field(&self.buf, "new_markdown").unwrap_or_default();
        if anchor.trim().is_empty() || md.is_empty() {
            return Ok(());
        }
        // 半截内容拼不上就先不落（别把半截贴进草稿）
        let Ok(body) = crate::splice::apply(&self.base_body, &anchor, &md) else {
            return Ok(());
        };
        if body == self.last {
            return Ok(());
        }
        if !self.throttle.ready(Instant::now()) {
            return Ok(());
        }
        self.last = body.clone();
        if !self.staged {
            self.staged = true;
            emit(&self.pool, run_id, "wikiatlas.content.staging", &json!({ "node_id": self.node_id })).await?;
        }
        if content::put_draft(&self.pool, self.node_id, &body, self.base_ver, &self.by)
            .await
            .is_ok()
        {
            self.wrote = true;
            emit(&self.pool, run_id, "wikiatlas.content.delta", &json!({ "body_md": body })).await?;
        }
        Ok(())
    }
}

/// 跑一轮模型往返：收文本 + 工具调用，顺带把草稿投影（`draft` 给了才投）。
pub async fn run_turn(
    pool: &PgPool,
    run_id: &str,
    client: &ResponsesClient,
    input: Value,
    tools: Value,
    choice: &str,
    draft: &mut Option<DraftProjector>,
) -> Result<TurnOutcome> {
    let mut rx = client.stream(input, tools, choice)?;
    let mut out = TurnOutcome::default();
    let mut names: HashMap<String, String> = HashMap::new();
    let mut call_ids: HashMap<String, String> = HashMap::new();
    let mut done: HashMap<String, String> = HashMap::new();
    let mut order: Vec<String> = vec![];

    while let Some(ev) = rx.recv().await {
        match ev? {
            StreamEvent::TextDelta(d) => {
                out.text.push_str(&d);
                emit(pool, run_id, "response.output_text.delta", &json!({ "delta": d })).await?;
            }
            // 思考摘要只用于展示，**不进后续上下文**（behavior-notes §1）。
            StreamEvent::ReasoningDelta(d) => {
                emit(
                    pool,
                    run_id,
                    "response.reasoning_summary_text.delta",
                    &json!({ "delta": d }),
                )
                .await?;
            }
            StreamEvent::FunctionCallAdded { item_id, name, call_id } => {
                names.insert(item_id.clone(), name.clone());
                call_ids.insert(item_id.clone(), call_id);
                order.push(item_id);
            }
            StreamEvent::ArgsDelta { item_id, delta } => {
                let is_patch = draft.is_some()
                    && names.get(&item_id).map(|s| s.as_str()) == Some(PATCH_SECTION_TOOL);
                if is_patch {
                    if let Some(d) = draft.as_mut() {
                        d.on_delta(run_id, &delta).await?;
                    }
                }
            }
            StreamEvent::ArgsDone { item_id, name, arguments } => {
                if !name.is_empty() {
                    names.insert(item_id.clone(), name);
                }
                if !order.contains(&item_id) {
                    order.push(item_id.clone());
                }
                done.insert(item_id, arguments);
            }
            StreamEvent::Completed { output, usage } => {
                emit(pool, run_id, "response.completed", &json!({ "output": output, "usage": usage })).await?;
            }
            StreamEvent::Other => {}
        }
    }

    for item_id in order {
        if let Some(args) = done.remove(&item_id) {
            out.calls.push(ToolCall {
                name: names.get(&item_id).cloned().unwrap_or_default(),
                call_id: call_ids.get(&item_id).cloned().unwrap_or_default(),
                arguments: args,
            });
        }
    }
    Ok(out)
}

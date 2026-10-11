//! Responses API 客户端（自写，不套 SDK）。
//!
//! `docs/backend.md` §2：我们的后端事件本来就用 Responses API 形状，**自写比套 SDK 更可控**。
//! 这里只做两件事：把请求发出去（`stream:true`），把 SSE 拆成事件。
//! 事件名的实测形状见 `docs/behavior-notes.md §5`（正文从 `function_call_arguments.delta` 出来）。

use crate::sse::{parse_stream_event, SseReader, StreamEvent};
use crate::Result;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::time::Duration;
use tokio::sync::mpsc;

/// 模型配置。**窗口只有这一个来源**（`behavior-notes.md §2`：别再造一个拍出来的字节预算）。
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub context_window: u32,
    /// 思考档位：`none|minimal|low|medium|high|xhigh|max`（**没有 off**）。
    pub effort: String,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            api_key: String::new(),
            model: "Qwen-Uncensored".into(),
            context_window: 262_144,
            effort: "low".into(),
        }
    }
}

impl LlmConfig {
    /// 从 `secrets` 表读（设置页落地前的口径，见 D53）。没配 key 就返回 None
    /// （`behavior-notes.md §7`：没配 Key 必须退回 mock/echo，**不能直接打网络**）。
    pub async fn load(pool: &PgPool) -> Result<Option<Self>> {
        let base_url = secret_or_env(pool, "llm.base_url", "WIKIATLAS_LLM_BASE_URL")
            .await?
            .unwrap_or_default();
        let api_key = secret_or_env(pool, "llm.api_key", "WIKIATLAS_LLM_API_KEY")
            .await?
            .unwrap_or_default();
        if base_url.trim().is_empty() || api_key.trim().is_empty() {
            return Ok(None);
        }
        let model = secret_or_env(pool, "llm.model", "WIKIATLAS_LLM_MODEL")
            .await?
            .unwrap_or_else(|| "Qwen-Uncensored".into());
        let effort = secret_or_env(pool, "llm.effort", "WIKIATLAS_LLM_EFFORT")
            .await?
            .unwrap_or_else(|| "low".into());
        let context_window = secret_or_env(pool, "llm.context_window", "WIKIATLAS_LLM_CONTEXT_WINDOW")
            .await?
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(262_144);
        Ok(Some(Self { base_url, api_key, model, context_window, effort }))
    }
}

/// secrets 表优先，环境变量兜底（设置页落地前，见 D53）。
async fn secret_or_env(pool: &PgPool, key: &str, env: &str) -> Result<Option<String>> {
    if let Some(v) = wikiatlas_store::secrets::get_secret(pool, key).await? {
        if !v.trim().is_empty() {
            return Ok(Some(v));
        }
    }
    Ok(std::env::var(env).ok().filter(|v| !v.trim().is_empty()))
}

pub struct ResponsesClient {
    http: reqwest::Client,
    cfg: LlmConfig,
}

impl ResponsesClient {
    pub fn new(cfg: LlmConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| crate::Error::Llm(format!("建 HTTP 客户端失败：{e}")))?;
        Ok(Self { http, cfg })
    }

    pub fn config(&self) -> &LlmConfig {
        &self.cfg
    }

    /// 发一轮请求，返回事件流（后台任务读 SSE 并转发）。
    ///
    /// `tool_choice`：`auto` = 让模型自己决定；`required` = **这一轮必须调工具**。
    /// 写正文的第一轮用 `required`——实测模型很容易"把正文写在回复里"而不调工具。
    pub fn stream(
        &self,
        input: Value,
        tools: Value,
        tool_choice: &str,
    ) -> Result<mpsc::Receiver<Result<StreamEvent>>> {
        let body = json!({
            "model": self.cfg.model,
            "input": input,
            "stream": true,
            "reasoning": { "effort": self.cfg.effort },
            "tools": tools,
            "tool_choice": tool_choice,
            "store": false,
        });
        let url = format!("{}/responses", self.cfg.base_url.trim_end_matches('/'));
        let (tx, rx) = mpsc::channel::<Result<StreamEvent>>(256);
        let http = self.http.clone();
        let key = self.cfg.api_key.clone();
        tokio::spawn(async move {
            let resp = match http.post(&url).bearer_auth(&key).json(&body).send().await {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(Err(crate::Error::Llm(format!("连模型失败：{e}")))).await;
                    return;
                }
            };
            if !resp.status().is_success() {
                let code = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let msg: String = body.chars().take(200).collect();
                let _ = tx.send(Err(crate::Error::Llm(format!("模型 HTTP {code}：{msg}")))).await;
                return;
            }
            let mut resp = resp;
            let mut sse = SseReader::default();
            while let Some(chunk) = match resp.chunk().await {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Err(crate::Error::Llm(format!("读模型流失败：{e}")))).await;
                    return;
                }
            } {
                let text = String::from_utf8_lossy(&chunk);
                for frame in sse.push(&text) {
                    if frame.data == "[DONE]" {
                        continue;
                    }
                    let ev = parse_stream_event(&frame.data);
                    if tx.send(Ok(ev)).await.is_err() {
                        return; // 消费者走了（比如 run 被取消）
                    }
                }
            }
        });
        Ok(rx)
    }
}

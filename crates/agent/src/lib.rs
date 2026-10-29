//! Agent 层：run 执行器、流式投影、工具循环、质量门。
//!
//! 边界（`docs/backend.md` §3）：这一层**只做编排**——LLM 调用在事务外，
//! 规则该在 `domain` 的别搬进来。写正文落草稿走 `store::content` 的既有路径。
//!
//! **动这一层之前先读 `docs/behavior-notes.md`**（头号风险：v1 调出来的行为知识会丢）。

pub mod client;
pub mod citations;
pub mod guard;
mod harness_tools;
pub mod ledger;
pub mod phases;
pub mod policy;
pub mod prompt;
pub mod quality;
pub mod read_tools;
pub mod runtime;
pub mod skeleton;
pub mod sse;
pub mod splice;
pub mod tools;
pub mod websearch;
pub mod stream;

use thiserror::Error;

/// agent 层的错误。**消息可能进 `runs.error_json`，所以不许带密钥。**
#[derive(Debug, Error)]
pub enum Error {
    #[error("模型调用失败：{0}")]
    Llm(String),
    #[error("工单状态不对：{0}")]
    RunState(String),
    #[error("存储错误：{0}")]
    Store(#[from] wikiatlas_store::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

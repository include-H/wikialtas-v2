//! Agent 层：run 执行器、流式投影、工具循环、质量门。
//!
//! 边界（`docs/backend.md` §3）：这一层**只做编排**——LLM 调用在事务外，
//! 规则该在 `domain` 的别搬进来。写正文落草稿走 `store::content` 的既有路径。
//!
//! **动这一层之前先读 `docs/behavior-notes.md`**（头号风险：v1 调出来的行为知识会丢）。

pub mod guard;
pub mod stream;

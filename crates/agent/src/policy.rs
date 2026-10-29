//! 动作层许可（`docs/tools.md §3` · D71）：**能不能做由动作决定，不由"模式 / 意图"决定**。
//!
//! 通用 harness 只在一个循环里跑、工具面是完整的——把写工具从菜单里拿掉不是我们的做法
//! （`D14`：只按用户显式选择剥写工具）。危险动作在**这里**拦：这是 dsh
//! `tools/pre-execute`（allow / deny / ask）与 Codex / OpenCode 权限策略的对位物。
//!
//! 写工具永远只落草稿或走可回滚结构操作；归档节点拒写、整篇写一轮一次。

use crate::runtime::RunContext;
use crate::tools::WRITE_WIKI_TOOL;
use wikiatlas_domain::{Node, NodeStatus};

pub enum Decision {
    Allow,
    Deny(String),
}

/// 对一个工具调用做许可判定。`node` 是当前目标节点；`already_wrote` 表示本轮已经整篇写过一次。
pub fn check(tool: &str, node: &Node, already_wrote: bool, ctx: &RunContext) -> Decision {
    if crate::harness_tools::is_write(tool) && !ctx.can_write() {
        return Decision::Deny(
            "本轮是只读 / one-shot 模式，所有写工具都已被运行时剥除并拒绝".into(),
        );
    }
    if !crate::harness_tools::is_write(tool) {
        return Decision::Allow;
    }
    if node.status == NodeStatus::Archived {
        return Decision::Deny(format!(
            "《{}》已归档，写工具一律拒绝（先取消归档）",
            node.title
        ));
    }
    if tool == WRITE_WIKI_TOOL && already_wrote {
        // 实跑教训：模型看到 write_wiki 返回 quality=failed 会立刻整篇重来，烧额度。
        return Decision::Deny(
            "这一轮已经整篇写过一次了（草稿已落库）。要改具体某处请让用户指明章节 / 句子，别整篇重写。"
                .into(),
        );
    }
    Decision::Allow
}

#[cfg(test)]
mod tests {
    use super::*;
    use wikiatlas_domain::{Medium, NodeKind};

    fn node(status: NodeStatus) -> Node {
        Node {
            id: uuid::Uuid::nil(),
            parent_id: None,
            kind: NodeKind::Work,
            medium: Some(Medium::Game),
            title: "某作".into(),
            title_norm: "某作".into(),
            aliases: vec![],
            tags: vec![],
            status,
            visibility: wikiatlas_domain::Visibility::Private,
            sort_key: "a".into(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn 归档节点拒写_读工具放行() {
        assert!(matches!(check("read_node", &node(NodeStatus::Archived), false, &RunContext::default()), Decision::Allow));
        assert!(matches!(check(WRITE_WIKI_TOOL, &node(NodeStatus::Archived), false, &RunContext::default()), Decision::Deny(_)));
        assert!(matches!(check(WRITE_WIKI_TOOL, &node(NodeStatus::Draft), false, &RunContext::default()), Decision::Allow));
    }

    #[test]
    fn 一轮只整篇写一次() {
        // 已经写过 → 再调 write_wiki 拒（防模型看到 quality=failed 就整篇重来）
        assert!(matches!(check(WRITE_WIKI_TOOL, &node(NodeStatus::Draft), true, &RunContext::default()), Decision::Deny(_)));
        // 但读工具不受影响
        assert!(matches!(check("read_node", &node(NodeStatus::Draft), true, &RunContext::default()), Decision::Allow));
    }

    #[test]
    fn 读模式动作层拒绝写工具() {
        let ctx = RunContext {
            doc_mode: "read".into(),
            ..Default::default()
        };
        assert!(matches!(
            check("edit", &node(NodeStatus::Draft), false, &ctx),
            Decision::Deny(_)
        ));
        assert!(matches!(
            check("read_node", &node(NodeStatus::Draft), false, &ctx),
            Decision::Allow
        ));
    }
}

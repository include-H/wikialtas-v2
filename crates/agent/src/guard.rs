//! 写工具的护栏（`docs/behavior-notes.md §3` · `docs/tools.md §2.1`）。
//!
//! v1 验证过的"事实发生"级硬护栏，**一条都不能省**：
//! 空写拒绝 · 截断守卫 · 归档拒写 · 参数 JSON 错就报错 · 重复调用分级。
//! 放在**动作层**（不是提示里）——提示能劝，护栏能拦。

use std::collections::HashMap;
use wikiatlas_domain::NodeStatus;

/// 护栏结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Reject(String),
}

impl Verdict {
    pub fn is_allow(&self) -> bool {
        matches!(self, Verdict::Allow)
    }
    pub fn reason(&self) -> Option<&str> {
        match self {
            Verdict::Reject(r) => Some(r),
            Verdict::Allow => None,
        }
    }
}

/// 空白字符数（用字符数，不是字节——中文一个字节算一个，别按字节判长度）。
fn len(s: &str) -> usize {
    s.chars().count()
}

/// 写正文前的三查：空写 / 截断 / 归档（`docs/tools.md §2.1`）。
///
/// `old_body` = 目标节点当前的定稿或草稿正文（没有就传空串）。
pub fn check_write(status: NodeStatus, old_body: &str, new_body: &str) -> Verdict {
    if status == NodeStatus::Archived {
        return Verdict::Reject("这个节点已归档，写工具一律拒绝（先取消归档）".into());
    }
    if new_body.trim().is_empty() {
        return Verdict::Reject("正文是空的，拒绝写入（防把整章写没了）".into());
    }
    // 截断守卫：原文够长、新文突然变得极短 → 几乎一定是模型把内容写没了
    if len(old_body) > 200 && len(new_body) < 50 {
        return Verdict::Reject(format!(
            "新正文只有 {} 字，而原文有 {} 字——像是被截断了，拒绝写入（要真删请在人这边改）",
            len(new_body),
            len(old_body)
        ));
    }
    Verdict::Allow
}

/// 工具参数 JSON 解析失败 → 报错返回，**不猜、不补全**（`behavior-notes.md §3`）。
pub fn parse_args<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T, Verdict> {
    serde_json::from_str(raw).map_err(|e| {
        Verdict::Reject(format!("工具参数不是合法 JSON，请重新调用：{e}"))
    })
}

/// 重复调用分级：同名同参 > 3 次直接回 `ok:false`（`behavior-notes.md §3`）。
///
/// `todo_write` 例外——任务清单本来就要反复整份提交，**别把这条护栏套上去**。
#[derive(Debug, Default)]
pub struct DupCallGuard {
    seen: HashMap<(String, String), u32>,
}

/// 反复整份提交是正常行为的工具（不参与去重）。
const DUP_EXEMPT: &[&str] = &["todo_write"];

impl DupCallGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记一次调用；返回是否该拒绝。
    pub fn check(&mut self, name: &str, args: &str) -> Verdict {
        if DUP_EXEMPT.contains(&name) {
            return Verdict::Allow;
        }
        let key = (name.to_string(), args.to_string());
        let n = self.seen.entry(key).or_insert(0);
        *n += 1;
        if *n > 3 {
            Verdict::Reject(format!(
                "同一个工具同样参数已经调用 {n} 次了（>{limit} 次）——换个做法，别原地绕圈",
                limit = 3
            ))
        } else {
            Verdict::Allow
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 空写拒绝() {
        let v = check_write(NodeStatus::Draft, "", "   \n  ");
        assert!(!v.is_allow());
        assert!(v.reason().unwrap().contains("空"));
    }

    #[test]
    fn 截断守卫_老长新短就拦() {
        let old = "字".repeat(300);
        let new = "字".repeat(49);
        assert!(!check_write(NodeStatus::Draft, &old, &new).is_allow());
        // 边界：正好 200 / 50 不拦（护栏别误伤正常精简）
        let old = "字".repeat(200);
        let new = "字".repeat(50);
        assert!(check_write(NodeStatus::Draft, &old, &new).is_allow());
        // 原文本来就短 → 不适用截断守卫
        assert!(check_write(NodeStatus::Draft, "短", "更").is_allow());
    }

    #[test]
    fn 归档节点拒写() {
        let v = check_write(NodeStatus::Archived, "", "写点什么");
        assert!(!v.is_allow());
        assert!(v.reason().unwrap().contains("归档"));
    }

    #[test]
    fn 正常写放行() {
        assert!(check_write(NodeStatus::Draft, "# 旧\n\n正文", "# 新\n\n正文更长一点").is_allow());
        assert!(check_write(NodeStatus::Ready, "", "# 第一版").is_allow());
    }

    #[test]
    fn 参数不是合法json_直接报错不猜() {
        let bad: Result<serde_json::Value, _> = parse_args("{节点: 没引号}");
        assert!(bad.is_err());
        let ok: Result<serde_json::Value, _> = parse_args(r#"{"content_md":"正文"}"#);
        assert!(ok.is_ok());
    }

    #[test]
    fn 重复调用第四次才拦() {
        let mut g = DupCallGuard::new();
        for i in 1..=3 {
            assert!(g.check("write_content", r#"{"content_md":"同一段"}"#).is_allow(), "第 {i} 次");
        }
        let v = g.check("write_content", r#"{"content_md":"同一段"}"#);
        assert!(!v.is_allow(), "第 4 次该拦");
        // 参数不同 → 各自计数
        assert!(g.check("write_content", r#"{"content_md":"换一段"}"#).is_allow());
    }

    #[test]
    fn todo_write_不参与去重() {
        let mut g = DupCallGuard::new();
        for _ in 0..10 {
            assert!(g.check("todo_write", r#"{"todos":[]}"#).is_allow());
        }
    }
}

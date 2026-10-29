//! 章节拼接（`docs/design.md §2.5`）：按 `## N.` 锚点替换一节，锚点不存在就追加。
//!
//! 生成段是**按章写**的：每章一次 `patch_section` 调用，从空文档一章一章长出来。
//! 所以这里既要支持「改已有章节」（替换），也要支持「从零长出新章节」（追加）——
//! 两者用同一条规则，模型不必知道当前文档到哪了。
//!
//! 纯逻辑，零 IO（`tools.md §2.2` 说锚点必须唯一命中；这里多一个"没命中就追加"的分支，
//! 因为生成段本来就在造还不存在的章节）。

/// 把 `new_md` 按锚点拼进 `body`，返回新正文。
///
/// - 锚点**唯一命中** → 替换该节（从锚点行到下一个 `## ` 标题，或正文末尾）；
/// - 锚点**不存在** → 追加；此时要求 `new_md` 以锚点开头（否则明显是模型写岔了）；
/// - 锚点**命中多处** → 报错（别猜改哪一个）。
pub fn apply(body: &str, anchor: &str, new_md: &str) -> Result<String, String> {
    let anchor = anchor.trim();
    if anchor.is_empty() {
        return Err("锚点不能为空".into());
    }
    let new_md = new_md.trim_end();
    if new_md.trim().is_empty() {
        return Err("新内容为空，拒绝拼接".into());
    }

    let lines: Vec<&str> = body.lines().collect();
    let hits: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim() == anchor)
        .map(|(i, _)| i)
        .collect();

    match hits.len() {
        0 => {
            // 追加：新内容必须以锚点开头（防止把别的东西塞进来）
            if !new_md.trim_start().starts_with(anchor) {
                return Err(format!("锚点「{anchor}」在正文里没出现，且新内容也不是以它开头"));
            }
            let head = body.trim_end();
            let mut out = String::new();
            if !head.is_empty() {
                out.push_str(head);
                out.push_str("\n\n");
            }
            out.push_str(new_md.trim_start());
            out.push('\n');
            Ok(out)
        }
        1 => {
            let at = hits[0];
            let end = lines[at + 1..]
                .iter()
                .position(|l| l.starts_with("## "))
                .map(|i| i + at + 1)
                .unwrap_or(lines.len());
            let mut out = String::new();
            if at > 0 {
                out.push_str(lines[..at].join("\n").trim_end());
                out.push_str("\n\n");
            }
            out.push_str(new_md.trim_start());
            out.push('\n');
            if end < lines.len() {
                out.push('\n');
                out.push_str(lines[end..].join("\n").trim_end());
                out.push('\n');
            }
            Ok(out)
        }
        n => Err(format!("锚点「{anchor}」在正文里命中了 {n} 处，不知道该改哪一个")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 从零追加头与章() {
        let b = apply("", "# 作品 Wiki", "# 作品 Wiki\n\n> 说明：x").unwrap();
        assert!(b.starts_with("# 作品 Wiki"));
        let b = apply(&b, "## 1. 概览", "## 1. 概览\n\n正文").unwrap();
        assert!(b.contains("# 作品 Wiki"));
        assert!(b.contains("## 1. 概览\n\n正文"));
    }

    #[test]
    fn 替换已有章节() {
        let body = "# T\n\n> 说明\n\n## 1. 概览\n\n旧的\n\n## 2. 速览\n\n第二章\n";
        let out = apply(body, "## 1. 概览", "## 1. 概览\n\n新的内容").unwrap();
        assert!(out.contains("新的内容"));
        assert!(!out.contains("旧的"));
        assert!(out.contains("## 2. 速览\n\n第二章"), "后面的章要保住：{out}");
    }

    #[test]
    fn 替换末章() {
        let body = "# T\n\n## 1. 概览\n\n一\n\n## 2. 速览\n\n二\n";
        let out = apply(body, "## 2. 速览", "## 2. 速览\n\n二改").unwrap();
        assert!(out.contains("二改"));
        assert!(!out.contains("二\n"), "{out}");
    }

    #[test]
    fn 没命中又不对齐锚点_报错() {
        let err = apply("# T\n\n## 1. 概览\n\n一", "## 3. 剧情", "随便一段").unwrap_err();
        assert!(err.contains("没出现"));
    }

    #[test]
    fn 锚点多处命中_报错() {
        let body = "# T\n\n## 1. 概览\n\n一\n\n## 1. 概览\n\n二\n";
        assert!(apply(body, "## 1. 概览", "## 1. 概览\n\nx").is_err());
    }

    #[test]
    fn 空内容拒绝() {
        assert!(apply("# T", "# T", "   ").is_err());
    }

    #[test]
    fn 替换头时保住后面的章() {
        let body = "# 旧标题\n\n> 旧说明\n\n:::epigraph\n旧\n:::\n\n## 1. 概览\n\n一\n";
        let out = apply(body, "# 旧标题", "# 新标题\n\n> 新说明\n\n:::epigraph\n新\n:::").unwrap();
        assert!(out.contains("# 新标题"));
        assert!(!out.contains("旧说明"));
        assert!(out.contains("## 1. 概览\n\n一"));
    }
}

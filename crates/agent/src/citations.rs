//! 引用门（`docs/design.md §2.5 §3.2` · D59）：**参考资料**章的每一条必须对应账本里的某个 `[Sn]`。
//!
//! 纯逻辑，零 IO。硬项只查「对应关系」（点得着、且来源存在）；
//! 「正文里的数字 / 日期抽样比对账本」按设计只做**提醒**，不硬拦。

use crate::ledger::Ledger;

/// 引用门结论。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CitationGate {
    /// 参考资料章里合格的条数。
    pub references: usize,
    pub issues: Vec<String>,
    pub reminders: Vec<String>,
}

impl CitationGate {
    pub fn passed(&self) -> bool {
        self.issues.is_empty()
    }
}

/// 跑引用门：`body` 是整篇正文，`ledger` 是同一篇的事实账本。
pub fn check(body: &str, ledger: &Ledger) -> CitationGate {
    let ids: Vec<&str> = ledger.sources.iter().map(|s| s.id.as_str()).collect();
    let lines: Vec<&str> = body.lines().collect();

    // 定位「参考资料」章
    let start = lines
        .iter()
        .position(|l| l.starts_with("## ") && l.contains("参考资料"));
    let mut references = 0usize;
    let mut issues = vec![];

    match start {
        None => issues.push("没有「参考资料」章".into()),
        Some(s) => {
            let end = lines[s + 1..]
                .iter()
                .position(|l| l.starts_with("## "))
                .map(|i| i + s + 1)
                .unwrap_or(lines.len());
            for l in &lines[s + 1..end] {
                let t = l.trim();
                if !t.starts_with("- ") && !t.starts_with("* ") {
                    continue;
                }
                match bracket_ids(t) {
                    None => issues.push(format!("参考资料这条没标来源 `[Sn]`：{t}")),
                    Some(found) => {
                        if found.iter().any(|id| ids.contains(&id.as_str())) {
                            references += 1;
                        } else {
                            issues.push(format!("参考资料这条指向账本里没有的来源：{t}"));
                        }
                    }
                }
            }
            if references == 0 && issues.is_empty() {
                issues.push("「参考资料」章一条有效来源都没有".into());
            }
        }
    }

    // 软提醒：正文里的 4 位年份，账本里完全没出现过
    let ledger_text = ledger.render();
    let mut years: Vec<String> = extract_years(body);
    years.sort_unstable();
    years.dedup();
    let missing_years: Vec<String> = years
        .into_iter()
        .filter(|y| !ledger_text.contains(y.as_str()))
        .collect();
    let mut reminders = vec![];
    if !missing_years.is_empty() {
        reminders.push(format!(
            "正文里的年份 {} 没在账本里出现，抽样比对一下（不硬拦）",
            missing_years.join("、")
        ));
    }

    CitationGate { references, issues, reminders }
}

/// 抽出 `- [S1]` 这类方括号里的来源编号；没有方括号返回 None。
fn bracket_ids(line: &str) -> Option<Vec<String>> {
    let mut out = vec![];
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find(']') else { break };
        let id = after[..close].trim();
        // 只认 `S<数字>` 形状（别把 `[1]` 之类的脚注当来源）
        if id.len() >= 2 && id.starts_with('S') && id[1..].chars().all(|c| c.is_ascii_digit()) {
            out.push(id.to_string());
        }
        rest = &after[close + 1..];
    }
    if out.is_empty() && !line.contains('[') {
        None
    } else {
        Some(out)
    }
}

/// 抽 4 位数字（年份）。
fn extract_years(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i - start == 4 {
                out.push(chars[start..i].iter().collect());
            }
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::Source;

    fn ledger() -> Ledger {
        let mut l = Ledger::default();
        l.put_source(Source { id: "S1".into(), title: "官方".into(), url: "https://a".into(), fetched: "".into(), quote: "".into() });
        l.put_source(Source { id: "S2".into(), title: "媒体".into(), url: "https://b".into(), fetched: "".into(), quote: "".into() });
        l
    }

    #[test]
    fn 每条参考资料对应到账本来源才过() {
        let body = "# T\n\n## 9. 参考资料\n\n- [S1] 官方页 — https://a\n- [S2] 媒体 — https://b\n";
        let g = check(body, &ledger());
        assert!(g.passed(), "{:?}", g.issues);
        assert_eq!(g.references, 2);
    }

    #[test]
    fn 参考资料缺Sn被拦() {
        let body = "# T\n\n## 9. 参考资料\n\n- 官方页 https://a\n";
        let g = check(body, &ledger());
        assert!(!g.passed());
        assert!(g.issues.iter().any(|i| i.contains("没标来源")));
    }

    #[test]
    fn 参考资料指向不存在来源被拦() {
        let body = "# T\n\n## 9. 参考资料\n\n- [S9] 某处\n";
        let g = check(body, &ledger());
        assert!(!g.passed());
        assert!(g.issues.iter().any(|i| i.contains("没有的来源")));
    }

    #[test]
    fn 缺参考资料章被拦() {
        let body = "# T\n\n## 1. 概览\n\n正文。\n";
        let g = check(body, &ledger());
        assert!(!g.passed());
    }

    #[test]
    fn 年份不在账本只是提醒() {
        let body = "# T\n\n## 9. 参考资料\n\n- [S1] 官方页\n\n发布于 1999 年。\n";
        let g = check(body, &ledger());
        assert!(g.passed(), "年份只是提醒，不该硬拦：{:?}", g.issues);
        assert!(g.reminders.iter().any(|r| r.contains("1999")));
    }
}

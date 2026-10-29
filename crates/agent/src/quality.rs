//! 质量门（**机械项一律由代码查**，`docs/prompts.md §3`）。
//!
//! 为什么在代码里：技能 / 提示词怎么改都不该影响这些判据——这是"把规则从提示词搬到代码"的收益。
//! 硬项不过 → 回给模型一次纠错机会；仍不过 → **留草稿待人工**（不硬失败，`prompts.md §3.1`）。
//!
//! 只做**能机械判**的：数得出、比得到、正则匹配得上的。判不了的一律降成"提醒"（`soft`），
//! 别把需要阅读理解的东西硬编成正则（第 12 条就是这么栽的）。

use serde::Serialize;
use wikiatlas_domain::Medium;

use crate::skeleton::chapter5_name;

/// 一条检查结论。
#[derive(Debug, Clone, Serialize)]
pub struct Issue {
    pub code: &'static str,
    pub message: String,
    /// true = 必须回给模型改；false = 只提醒（不挡）
    pub hard: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub issues: Vec<Issue>,
    /// 正文汉字/字符数（用于长度判据，也回报给用户）
    pub chars: usize,
    /// 认出来的 `## N.` 章节编号
    pub chapters: Vec<u32>,
}

impl Report {
    /// 没有硬项 = 过门。
    pub fn passed(&self) -> bool {
        !self.issues.iter().any(|i| i.hard)
    }

    /// 硬项说明（回给模型纠错用）。
    pub fn hard_messages(&self) -> Vec<String> {
        self.issues.iter().filter(|i| i.hard).map(|i| i.message.clone()).collect()
    }
}

/// 完整建档的字数下限（`prompts.md §2.3`：游戏侧基准约 7000–12000 汉字）。
pub const MIN_CHARS_FULL: usize = 6000;

/// 套话黑名单（`prompts.md §3` 第 8/13 条）。**只列"糊弄"的**，不列正常评价词。
const BANNED: &[&str] = &["据说", "广受好评", "好评如潮", "神作", "天花板", "不容错过"];

/// 跑一遍质量门。
pub fn check(medium: Medium, body: &str, min_chars: usize) -> Report {
    let mut issues = vec![];
    let lines: Vec<&str> = body.lines().collect();
    // 1. 标题：以 Wiki 结尾
    let title = lines.iter().find(|l| l.starts_with("# ")).copied().unwrap_or("");
    if title.is_empty() {
        push_hard(&mut issues, "title", "缺主标题行（`# 作品名（介质）Wiki`）".into());
    } else if !title.trim_end().ends_with("Wiki") {
        push_hard(&mut issues, "title", format!("主标题要以 `Wiki` 结尾，现在是：{title}"));
    }

    // 6. 说明行：题记之前要有一条 `> 说明：…`
    let note_at = lines.iter().position(|l| l.trim_start().starts_with("> "));
    match note_at {
        None => push_hard(&mut issues, "note", "缺 `> 说明：…` 行（整理范围 / 来源 / 剧透 / 检索时间）".into()),
        Some(i) if !lines[i].contains("说明") => {
            push_soft(&mut issues, "note", "开头那条 `>` 不是「说明」行？确认一下".into())
        }
        _ => {}
    }

    // 2. 题记：有且仅有一个围栏块；块内不许有空行
    let fences: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim() == ":::epigraph")
        .map(|(i, _)| i)
        .collect();
    match fences.len() {
        0 => push_hard(&mut issues, "epigraph", "缺 `:::epigraph` 题记块".into()),
        1 => {
            let start = fences[0];
            let end = lines[start + 1..].iter().position(|l| l.trim() == ":::").map(|i| i + start + 1);
            match end {
                None => push_hard(&mut issues, "epigraph", "题记块没有收尾的 `:::`".into()),
                Some(e) => {
                    if lines[start + 1..e].iter().any(|l| l.trim().is_empty()) {
                        push_hard(&mut issues, "epigraph", "题记块内有空行——题记要逐行交替，不能按段落排".into());
                    }
                }
            }
        }
        n => push_hard(&mut issues, "epigraph", format!("题记块有 {n} 个，只能有一个")),
    }

    // 3. 章节：`## 1.`–`## 7.` 齐全、编号连续、无「引言」
    let mut chapters: Vec<u32> = vec![];
    let mut chapter_titles: Vec<(u32, String)> = vec![];
    for l in &lines {
        if let Some(rest) = l.strip_prefix("## ") {
            let (num, name) = match rest.split_once('.') {
                Some((n, name)) => (n.trim().parse::<u32>().ok(), name.trim().to_string()),
                None => (None, rest.trim().to_string()),
            };
            if let Some(n) = num {
                chapters.push(n);
                chapter_titles.push((n, name));
            } else if rest.contains("引言") {
                push_hard(&mut issues, "chapters", "不该有「## 引言」——这个概念在任何一篇基准里都没出现过".into());
            }
        }
    }
    for want in 1..=7u32 {
        if !chapters.contains(&want) {
            push_hard(&mut issues, "chapters", format!("缺第 {want} 章（`## {want}. …`）"));
        }
    }
    let mut sorted = chapters.clone();
    sorted.sort_unstable();
    sorted.dedup();
    for w in sorted.windows(2) {
        if w[1] != w[0] + 1 {
            push_hard(&mut issues, "chapters", format!("章号跳号：{} → {}", w[0], w[1]));
        }
    }

    // 4. 章名按介质替换（只看第 5 章）
    if let Some((_, name)) = chapter_titles.iter().find(|(n, _)| *n == 5) {
        let want = chapter5_name(medium);
        if !name.contains(want) {
            push_hard(&mut issues, "chapter5", format!("第 5 章章名该是「{want}」，现在是「{name}」"));
        }
    }

    // 8/13. 套话黑名单
    for b in BANNED {
        if body.contains(b) {
            push_hard(&mut issues, "banned", format!("出现套话「{b}」——查不到就写 `待核实`，别糊"));
        }
    }

    // 10. 字数
    let chars = body.chars().count();
    if chars < min_chars {
        push_hard(&mut issues, "length", format!("全篇 {chars} 字，低于下限 {min_chars} 字"));
    }

    // 12. 名场面/名台词章里的 `；` → **只提醒**（可能是两句挤一行，也可能本来就是一场戏）
    if let Some(i) = lines.iter().position(|l| l.starts_with("## ") && l.contains("名台词")) {
        let seg = &lines[i..];
        let end = seg
            .iter()
            .skip(1)
            .position(|l| l.starts_with("## "))
            .map(|j| j + 1)
            .unwrap_or(seg.len());
        let n = seg[..end].iter().filter(|l| l.trim_start().starts_with("- ") && l.contains('；')).count();
        if n > 0 {
            push_soft(&mut issues, "scenes", format!("名台词/名场面里有 {n} 条含 `；`——看一眼是不是两件事挤成了一行"));
        }
    }

    Report { issues, chars, chapters }
}

/// 记一条硬项。
fn push_hard(issues: &mut Vec<Issue>, code: &'static str, message: String) {
    issues.push(Issue { code, message, hard: true });
}

/// 记一条提醒（不挡门）。
fn push_soft(issues: &mut Vec<Issue>, code: &'static str, message: String) {
    issues.push(Issue { code, message, hard: false });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good_body() -> String {
        let filler = "这是一句落到具体对象的正文。".repeat(3);
        let mut s = String::from("# 空之轨迹 FC（游戏）Wiki\n\n> 说明：本文依据官方资料整理，含剧透。检索时间 2026-10。\n\n");
        s.push_str(":::epigraph\n「台词一。」\n「台词的译文。」\n——某角色\n:::\n\n");
        for (i, name) in [
            "1. 作品概览",
            "2. 基础信息速览",
            "3. 世界观与背景设定",
            "4. 剧情概要",
            "5. 玩法与结构特色",
            "6. 主要角色与势力",
            "7. 评价与历史定位",
        ]
        .iter()
        .enumerate()
        {
            s.push_str(&format!("## {name}\n\n{filler}\n\n"));
            let _ = i;
        }
        s.push_str("## 8. 一句话总结\n\n一段话。\n\n## 9. 参考资料\n\n- 官方站点。\n");
        // 堆到字数下限以上
        while s.chars().count() < MIN_CHARS_FULL + 10 {
            s.push_str(&filler);
        }
        s
    }

    #[test]
    fn 好正文过门() {
        let r = check(Medium::Game, &good_body(), MIN_CHARS_FULL);
        assert!(r.passed(), "不该有硬项：{:?}", r.hard_messages());
        assert_eq!(r.chapters, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn 缺章与跳号都拦() {
        let body = good_body().replace("## 4. 剧情概要", "## 5. 剧情概要");
        let r = check(Medium::Game, &body, 100);
        assert!(!r.passed());
        assert!(r.hard_messages().iter().any(|m| m.contains("第 4 章") || m.contains("跳号")));
    }

    #[test]
    fn 章名按介质替换() {
        let mut body = good_body();
        body = body.replace("5. 玩法与结构特色", "5. 创作特色");
        let r = check(Medium::Game, &body, 100);
        assert!(r.hard_messages().iter().any(|m| m.contains("玩法与结构特色")));
        // 同一篇换成漫画介质就对了
        let r2 = check(Medium::Manga, &body, 100);
        assert!(r2.passed(), "{:?}", r2.hard_messages());
    }

    #[test]
    fn 题记块内空行拦() {
        let body = good_body().replace("「台词一。」\n「台词的译文。」", "「台词一。」\n\n「台词的译文。」");
        let r = check(Medium::Game, &body, 100);
        assert!(r.hard_messages().iter().any(|m| m.contains("题记")));
    }

    #[test]
    fn 题记两个拦() {
        let mut body = good_body();
        body.push_str("\n:::epigraph\n又一段\n:::\n");
        let r = check(Medium::Game, &body, 100);
        assert!(r.hard_messages().iter().any(|m| m.contains("只能有一个")));
    }

    #[test]
    fn 套话与字数不够拦() {
        let mut body = good_body();
        body = body.replace("这是一句落到具体对象的正文。", "据说这是神作。");
        let r = check(Medium::Game, &body, 100_000);
        let msgs = r.hard_messages().join(" | ");
        assert!(msgs.contains("套话"), "{msgs}");
        assert!(msgs.contains("低于下限"), "{msgs}");
    }

    #[test]
    fn 标题不以wiki结尾拦() {
        let body = good_body().replace("（游戏）Wiki", "（游戏）");
        let r = check(Medium::Game, &body, 100);
        assert!(r.hard_messages().iter().any(|m| m.contains("Wiki")));
    }

    #[test]
    fn 引言章拦() {
        let mut body = good_body();
        body.push_str("\n## 引言\n\n不该有。\n");
        let r = check(Medium::Game, &body, 100);
        assert!(r.hard_messages().iter().any(|m| m.contains("引言")));
    }

    #[test]
    fn 名场面里的分号只是提醒不是硬项() {
        let mut body = good_body();
        body.push_str("\n## 10. 名台词与名场面\n\n- 「第一句」；「第二句」\n");
        let r = check(Medium::Game, &body, 100);
        assert!(r.passed(), "不该硬拦：{:?}", r.hard_messages());
        assert!(r.issues.iter().any(|i| i.code == "scenes" && !i.hard));
    }
}

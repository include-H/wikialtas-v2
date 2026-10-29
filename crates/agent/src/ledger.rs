//! 事实账本：格式、解析、覆盖率与账本门（`docs/design.md §2.2 §2.3 §3.1` · D56 D59）。
//!
//! **纯逻辑，零 IO**：账本正文的解析 / 渲染 / 覆盖率计算都不碰数据库，
//! 这样它能在单测里钉死（研究段收工条件就靠它，不能靠模型自觉）。
//!
//! 固定格式（代码解析，人也能在编辑器里改）：
//!
//! ```markdown
//! ## 断言
//! | # | 字段 | 断言 | 来源 | 状态 |
//! |---|---|---|---|---|
//! | 1 | 首发日期 | 2006-03-09，Windows | S1,S3 | 已核实 |
//!
//! ## 来源
//! - [S1] 日本 Falcom 官方商品页 — https://… — 抓取 2026-10-12 — 「……原文摘录……」
//! ```

use serde::{Deserialize, Serialize};
use wikiatlas_domain::Medium;

/// 断言状态只有三种（`design.md §2.3`：不许有别的）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Verified,
    Conflict,
    Pending,
}

impl Status {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "已核实" => Some(Status::Verified),
            "冲突" => Some(Status::Conflict),
            "待核实" => Some(Status::Pending),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Verified => "已核实",
            Status::Conflict => "冲突",
            Status::Pending => "待核实",
        }
    }
    /// 算不算「覆盖」到必填字段：待核实 = 没覆盖；冲突 = 覆盖（正文要写出分歧）。
    pub fn covers(self) -> bool {
        !matches!(self, Status::Pending)
    }
}

/// 一条断言。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub no: u32,
    pub field: String,
    pub claim: String,
    /// 来源编号（`S1`, `S2`…，不带中括号）。
    pub sources: Vec<String>,
    pub status: Status,
}

/// 一条来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    /// 标题 / 站点名。
    pub title: String,
    pub url: String,
    pub fetched: String,
    /// 原文摘录（≤300 字）。
    pub quote: String,
}

/// 账本。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    pub entries: Vec<Entry>,
    pub sources: Vec<Source>,
}

impl Ledger {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.sources.is_empty()
    }

    /// 下一条断言编号。
    pub fn next_no(&self) -> u32 {
        self.entries.iter().map(|e| e.no).max().unwrap_or(0) + 1
    }

    /// 追加 / 覆盖一条断言：**按字段名合并**（同字段已有行就替换，没有就追加）。
    /// 这就是 `ledger_put`「只追加/改行，不整篇重写」的实现（`design.md §2.3`）。
    pub fn put(&mut self, field: &str, claim: &str, sources: Vec<String>, status: Status) -> u32 {
        let field = field.trim();
        if let Some(e) = self.entries.iter_mut().find(|e| e.field == field) {
            e.claim = claim.trim().to_string();
            e.sources = sources;
            e.status = status;
            return e.no;
        }
        let no = self.next_no();
        self.entries.push(Entry {
            no,
            field: field.to_string(),
            claim: claim.trim().to_string(),
            sources,
            status,
        });
        no
    }

    /// 追加一条来源（同 id 已有就替换）。
    pub fn put_source(&mut self, s: Source) {
        if let Some(old) = self.sources.iter_mut().find(|x| x.id == s.id) {
            *old = s;
        } else {
            self.sources.push(s);
        }
    }

    /// 去重后的来源 URL 数（账本门用）。
    pub fn unique_urls(&self) -> usize {
        let mut v: Vec<&str> = self
            .sources
            .iter()
            .map(|s| s.url.trim())
            .filter(|u| !u.is_empty())
            .collect();
        v.sort_unstable();
        v.dedup();
        v.len()
    }

    /// 渲染成固定格式的 Markdown。
    pub fn render(&self) -> String {
        let mut out = String::from("## 断言\n\n| # | 字段 | 断言 | 来源 | 状态 |\n|---|---|---|---|---|\n");
        for e in &self.entries {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                e.no,
                cell(&e.field),
                cell(&e.claim),
                e.sources.join(","),
                e.status.as_str()
            ));
        }
        out.push_str("\n## 来源\n\n");
        let mut srcs = self.sources.clone();
        srcs.sort_by(|a, b| a.id.cmp(&b.id));
        for s in &srcs {
            out.push_str(&format!(
                "- [{}] {} — {} — 抓取 {} — 「{}」\n",
                s.id,
                s.title.trim(),
                s.url.trim(),
                s.fetched.trim(),
                truncate_chars(&s.quote, 300)
            ));
        }
        out
    }

    /// 解析固定格式（缺哪块就当空）。容错：模型可能多写空格 / 少写表头分隔行。
    pub fn parse(md: &str) -> Self {
        let mut ledger = Ledger::default();
        let mut section = "";
        for line in md.lines() {
            let t = line.trim();
            if t.starts_with("## ") {
                section = if t.contains("断言") {
                    "claims"
                } else if t.contains("来源") {
                    "sources"
                } else {
                    ""
                };
                continue;
            }
            match section {
                "claims" => {
                    if let Some(cells) = table_row(t) {
                        if let Some(e) = parse_entry(&cells) {
                            ledger.entries.push(e);
                        }
                    }
                }
                "sources" => {
                    if let Some(s) = parse_source(t) {
                        ledger.put_source(s);
                    }
                }
                _ => {}
            }
        }
        ledger
    }

    /// 必填字段覆盖率（`design.md §2.2`）。
    pub fn coverage(&self, medium: Medium) -> Coverage {
        let reqs = requirements(medium);
        let mut missing = vec![];
        for r in &reqs {
            let hit = self
                .entries
                .iter()
                .filter(|e| e.status.covers() && r.matches(&e.field))
                .count();
            if hit < r.min {
                missing.push(r.label.to_string());
            }
        }
        Coverage {
            total: reqs.len(),
            missing,
        }
    }

    /// 账本门（`design.md §3.1`）：覆盖率 ≥ 80% 且来源 URL 去重 ≥ 5。
    /// **不足不拦**（亮黄条），但结论要能回给前端与工单结果。
    pub fn gate(&self, medium: Medium) -> LedgerGate {
        let cov = self.coverage(medium);
        let urls = self.unique_urls();
        let mut issues = vec![];
        if cov.ratio() < 0.8 {
            issues.push(format!(
                "账本缺 {} 项（覆盖率 {}%，要求 ≥80%）：{}",
                cov.missing.len(),
                (cov.ratio() * 100.0).round() as u32,
                cov.missing.join("、")
            ));
        }
        if urls < 5 {
            issues.push(format!("去重来源 URL 只有 {urls} 个（要求 ≥5）"));
        }
        LedgerGate {
            ratio: cov.ratio(),
            total: cov.total,
            missing: cov.missing,
            unique_urls: urls,
            issues,
        }
    }
}

/// 覆盖率结论。
#[derive(Debug, Clone, PartialEq)]
pub struct Coverage {
    pub total: usize,
    pub missing: Vec<String>,
}

impl Coverage {
    pub fn covered(&self) -> usize {
        self.total.saturating_sub(self.missing.len())
    }
    pub fn ratio(&self) -> f64 {
        if self.total == 0 {
            1.0
        } else {
            self.covered() as f64 / self.total as f64
        }
    }
}

/// 账本门结论。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LedgerGate {
    pub ratio: f64,
    pub total: usize,
    pub missing: Vec<String>,
    pub unique_urls: usize,
    pub issues: Vec<String>,
}

impl LedgerGate {
    pub fn passed(&self) -> bool {
        self.issues.is_empty()
    }
}

/// 一个必填字段要求：`label` 是给人看的名字，`keys` 是账本字段名的匹配词，`min` 是条数下限。
pub struct Requirement {
    label: &'static str,
    keys: &'static [&'static str],
    min: usize,
}

impl Requirement {
    fn matches(&self, field: &str) -> bool {
        self.keys.iter().any(|k| field.contains(k))
    }
    /// 给人看的字段名。
    pub fn label(&self) -> &'static str {
        self.label
    }
    /// 条数下限（角色 ≥4 这类）。
    pub fn min(&self) -> usize {
        self.min
    }
}

/// 按介质的必填字段清单（`design.md §2.2`）。
///
/// `核心玩法机制 ≥3`、`主要角色 ≥4`、`评价 ≥1` 这种**计数**要求用 `min` 表达，
/// 由代码数，不靠模型自觉。
pub fn requirements(medium: Medium) -> Vec<Requirement> {
    match medium {
        Medium::Game => vec![
            r("原名", &["原名", "名称"], 1),
            r("开发", &["开发"], 1),
            r("发行", &["发行"], 1),
            r("首发日期", &["首发日期", "发售日期", "发行日期", "首映"], 1),
            r("首发平台", &["首发平台", "发售平台", "平台"], 1),
            r("版本关系", &["版本", "资料片", "DLC"], 1),
            r("世界观", &["世界观", "背景设定", "设定"], 1),
            r("剧情（含结局）", &["剧情", "故事", "结局"], 1),
            r("核心玩法机制", &["玩法", "机制"], 3),
            r("主要角色", &["角色", "主角"], 4),
            r("评价", &["评价", "评分", "销量", "奖项"], 1),
            r("题记素材", &["题记", "台词", "名句"], 1),
        ],
        Medium::Movie | Medium::Tv => vec![
            r("原名", &["原名", "名称"], 1),
            r("导演", &["导演"], 1),
            r("编剧", &["编剧"], 1),
            r("主演", &["主演", "声优", "演出"], 1),
            r("首映/首播", &["首映", "首播", "上映"], 1),
            r("集数或片长", &["集数", "片长", "时长"], 1),
            r("剧情（含结局）", &["剧情", "故事", "结局"], 1),
            r("主要角色", &["角色", "主演"], 4),
            r("评价", &["评价", "评分", "票房", "奖项"], 1),
            r("题记素材", &["题记", "台词", "名句"], 1),
        ],
        Medium::Manga | Medium::Book => vec![
            r("原名", &["原名", "名称"], 1),
            r("作者", &["作者", "原作"], 1),
            r("出版社/连载志", &["出版社", "连载", "出版"], 1),
            r("起止时间", &["起止", "连载期间", "出版期间"], 1),
            r("卷数", &["卷数", "卷"], 1),
            r("剧情（含结局）", &["剧情", "故事", "结局"], 1),
            r("主要角色", &["角色", "主角"], 4),
            r("评价", &["评价", "评分", "销量", "奖项"], 1),
            r("题记素材", &["题记", "台词", "名句"], 1),
        ],
        Medium::Other => vec![
            r("原名", &["原名", "名称"], 1),
            r("剧情（含结局）", &["剧情", "故事", "结局"], 1),
            r("评价", &["评价", "评分", "奖项"], 1),
        ],
    }
}

fn r(label: &'static str, keys: &'static [&'static str], min: usize) -> Requirement {
    Requirement { label, keys, min }
}

/// 表格行 → 单元格（`| a | b |`）。非表格行返回 None。
fn table_row(line: &str) -> Option<Vec<String>> {
    if !line.starts_with('|') || !line.ends_with('|') {
        return None;
    }
    let cells: Vec<String> = line
        .trim_matches('|')
        .split('|')
        .map(|c| c.trim().replace("\\|", "|"))
        .collect();
    if cells.is_empty() {
        return None;
    }
    // 分隔行（`---|---|---`）跳过
    if cells.iter().all(|c| c.is_empty() || c.chars().all(|ch| ch == '-' || ch == ':')) {
        return None;
    }
    Some(cells)
}

fn parse_entry(cells: &[String]) -> Option<Entry> {
    if cells.len() < 5 {
        return None;
    }
    // 表头行：第一格是 `#`
    if cells[0] == "#" {
        return None;
    }
    let no: u32 = cells[0].parse().ok()?;
    let sources: Vec<String> = cells[3]
        .split([',', '，'])
        .map(|s| s.trim().trim_matches(['[', ']']).to_string())
        .filter(|s| !s.is_empty() && s != "—" && s != "-")
        .collect();
    Some(Entry {
        no,
        field: cells[1].clone(),
        claim: cells[2].clone(),
        sources,
        status: Status::parse(&cells[4]).unwrap_or(Status::Pending),
    })
}

/// `- [S1] 标题 — url — 抓取 日期 — 「摘录」`
fn parse_source(line: &str) -> Option<Source> {
    let rest = line.strip_prefix("- ")?.trim();
    let rest = rest.strip_prefix('[')?;
    let close = rest.find(']')?;
    let id = rest[..close].trim().to_string();
    let rest = rest[close + 1..].trim();
    // 后续用 ` — ` 分隔；缺段就留空（容错）
    let parts: Vec<&str> = rest.split(" — ").map(|s| s.trim()).collect();
    let title = parts.first().copied().unwrap_or("").to_string();
    let url = parts.get(1).copied().unwrap_or("").to_string();
    let fetched = parts
        .get(2)
        .copied()
        .unwrap_or("")
        .trim_start_matches("抓取")
        .trim()
        .to_string();
    let quote = parts
        .get(3)
        .copied()
        .unwrap_or("")
        .trim_matches(['「', '」'])
        .to_string();
    Some(Source { id, title, url, fetched, quote })
}

/// 表格单元格转义（竖线与换行）。
fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ").trim().to_string()
}

fn truncate_chars(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Ledger {
        let mut l = Ledger::default();
        l.put("首发日期", "2006-03-09，Windows", vec!["S1".into()], Status::Verified);
        l.put("港台译名", "—", vec![], Status::Pending);
        l.put("销量", "日本首周 4.2 万", vec!["S2".into(), "S4".into()], Status::Conflict);
        l.put_source(Source {
            id: "S1".into(),
            title: "Falcom 官方".into(),
            url: "https://falcom.example/sc".into(),
            fetched: "2026-10-12".into(),
            quote: "原文".into(),
        });
        l
    }

    #[test]
    fn 解析与渲染往返一致() {
        let l = sample();
        let md = l.render();
        let back = Ledger::parse(&md);
        assert_eq!(back.entries.len(), 3);
        assert_eq!(back.entries[0].field, "首发日期");
        assert_eq!(back.entries[1].status, Status::Pending);
        assert_eq!(back.entries[2].sources, vec!["S2", "S4"]);
        assert_eq!(back.sources.len(), 1);
        assert_eq!(back.sources[0].url, "https://falcom.example/sc");
    }

    #[test]
    fn put_同字段覆盖不新增行() {
        let mut l = sample();
        let n = l.entries.len();
        l.put("首发日期", "2006-03-09（已订正）", vec!["S1".into()], Status::Verified);
        assert_eq!(l.entries.len(), n, "同字段应覆盖而不是新增");
        assert!(l.entries[0].claim.contains("订正"));
    }

    #[test]
    fn 状态只认三种() {
        assert_eq!(Status::parse("已核实"), Some(Status::Verified));
        assert_eq!(Status::parse("冲突"), Some(Status::Conflict));
        assert_eq!(Status::parse("待核实"), Some(Status::Pending));
        assert_eq!(Status::parse("可能吧"), None);
        assert!(Status::Pending.covers() == false);
        assert!(Status::Conflict.covers());
    }

    #[test]
    fn 覆盖率_待核实不算覆盖() {
        let l = sample();
        // 游戏要求 12 项，这里只命中「首发日期」与「销量(评价)」
        let cov = l.coverage(Medium::Game);
        assert!(cov.missing.contains(&"首发日期".to_string()) == false);
        assert!(cov.missing.contains(&"世界观".to_string()));
        assert!(cov.ratio() < 0.5);
    }

    #[test]
    fn 计数要求_角色不足4人不算覆盖() {
        let mut l = Ledger::default();
        for i in 0..4 {
            l.put(&format!("角色{i}"), "某人", vec!["S1".into()], Status::Verified);
        }
        let cov = l.coverage(Medium::Game);
        assert!(!cov.missing.contains(&"主要角色".to_string()), "4 个角色应满足 ≥4");
        let mut l2 = l.clone();
        l2.entries.pop();
        let cov2 = l2.coverage(Medium::Game);
        assert!(cov2.missing.contains(&"主要角色".to_string()), "3 个角色不满足 ≥4");
    }

    #[test]
    fn 账本门_覆盖率与来源数都查() {
        let l = sample();
        let g = l.gate(Medium::Game);
        assert!(!g.passed());
        assert!(g.issues.iter().any(|i| i.contains("覆盖率") || i.contains("缺")));
        assert!(g.issues.iter().any(|i| i.contains("URL")));
    }

    #[test]
    fn 来源URL去重() {
        let mut l = Ledger::default();
        l.put_source(Source { id: "S1".into(), title: "a".into(), url: "https://x".into(), fetched: "".into(), quote: "".into() });
        l.put_source(Source { id: "S2".into(), title: "b".into(), url: "https://x".into(), fetched: "".into(), quote: "".into() });
        l.put_source(Source { id: "S3".into(), title: "c".into(), url: "https://y".into(), fetched: "".into(), quote: "".into() });
        assert_eq!(l.unique_urls(), 2);
    }

    #[test]
    fn 空账本比例视作1() {
        let cov = Ledger::default().coverage(Medium::Other);
        // Other 有 3 项必填，空账本 missing 3 项
        assert_eq!(cov.total, 3);
        assert_eq!(cov.missing.len(), 3);
    }

    #[test]
    fn 摘录超300字截断且说话() {
        let mut l = Ledger::default();
        l.put_source(Source {
            id: "S1".into(),
            title: "t".into(),
            url: "u".into(),
            fetched: "".into(),
            quote: "字".repeat(400),
        });
        let md = l.render();
        assert!(md.contains('…'), "超长摘录要加省略号（截断必须说话）");
    }
}

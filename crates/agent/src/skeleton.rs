//! 骨架段：结构化产出与骨架门（`docs/design.md §2.4 §3` · D55 D58）。
//!
//! 骨架是「一次结构化输出」：`{ title, note, epigraph, chapters[] }`。
//! **没有事实支撑的小节不许出现**——每个小节至少挂 1 条账本行，这条由**代码**查，
//! 不靠模型自觉（`design.md §2.4`）。

use serde::{Deserialize, Serialize};
use wikiatlas_domain::Medium;

use crate::ledger::Ledger;

/// 题记写法（`skills/atlas/references/writing/epigraph.md`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EpigraphKind {
    /// 引用式：有可引原文，须指向账本里的来源。
    Quote,
    /// 场景式：拿不到原文时的默认。
    Scene,
    /// 诗句式：古装 / 武侠 / 仙侠类。
    Poem,
}

fn default_kind() -> EpigraphKind {
    EpigraphKind::Scene
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Epigraph {
    #[serde(default = "default_kind")]
    pub kind: EpigraphKind,
    /// 引用式时指向账本来源编号（`S1`）；场景式 / 诗句式可空。
    #[serde(default)]
    pub source_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    pub name: String,
    /// 这条小节支撑它的账本断言编号。
    #[serde(default)]
    pub ledger_rows: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chapter {
    pub no: u32,
    pub name: String,
    #[serde(default)]
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outline {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub epigraph: Option<Epigraph>,
    #[serde(default)]
    pub chapters: Vec<Chapter>,
}

impl Outline {
    /// 从模型输出里解析骨架：容错掉 ```json 围栏与前后闲聊。
    pub fn parse(raw: &str) -> Result<Self, String> {
        let text = strip_fences(raw);
        let start = text.find('{').ok_or("没找到 JSON 对象")?;
        let end = text.rfind('}').ok_or("JSON 对象没有收尾")?;
        if end <= start {
            return Err("JSON 对象不完整".into());
        }
        serde_json::from_str(&text[start..=end]).map_err(|e| format!("骨架 JSON 解析失败：{e}"))
    }

    /// 骨架门（`design.md §2.4 §3`，代码）。
    pub fn gate(&self, medium: Medium, ledger: &Ledger) -> OutlineGate {
        let mut issues: Vec<String> = vec![];
        let ids: Vec<u32> = ledger.entries.iter().map(|e| e.no).collect();

        if self.chapters.is_empty() {
            issues.push("骨架里一章都没有".into());
        }
        // 章号 1..=N 连续
        let mut nos: Vec<u32> = self.chapters.iter().map(|c| c.no).collect();
        nos.sort_unstable();
        for (i, n) in nos.iter().enumerate() {
            if *n != (i as u32) + 1 {
                issues.push(format!("章号不连续：期望第 {} 章，实际第 {} 章", i + 1, n));
                break;
            }
        }
        // 每节至少挂 1 条存在的账本行
        for c in &self.chapters {
            for s in &c.sections {
                if s.ledger_rows.is_empty() {
                    issues.push(format!("第 {} 章「{}」的小节「{}」没有挂账本行", c.no, c.name, s.name));
                    continue;
                }
                for r in &s.ledger_rows {
                    if !ids.contains(r) {
                        issues.push(format!(
                            "第 {} 章小节「{}」挂了不存在的账本行 #{}",
                            c.no, s.name, r
                        ));
                    }
                }
            }
        }
        // 第 5 章章名按介质三档
        if let Some(c5) = self.chapters.iter().find(|c| c.no == 5) {
            let want = chapter5_name(medium);
            if !c5.name.contains(want) {
                issues.push(format!("第 5 章章名该是「{want}」，现在是「{}」", c5.name));
            }
        }
        // 引用式题记必须指向账本里存在的来源
        if let Some(ep) = &self.epigraph {
            if ep.kind == EpigraphKind::Quote && !ep.source_id.trim().is_empty() {
                let sid = &ep.source_id;
                if !ledger.sources.iter().any(|s| &s.id == sid) {
                    issues.push(format!("引用式题记指向了不存在的来源 {sid}"));
                }
            }
        }

        OutlineGate {
            chapters: self.chapters.iter().map(|c| c.no).collect(),
            issues,
        }
    }
}

/// 骨架门结论。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutlineGate {
    pub chapters: Vec<u32>,
    pub issues: Vec<String>,
}

impl OutlineGate {
    pub fn passed(&self) -> bool {
        self.issues.is_empty()
    }
}

/// **介质对应的标准章名**（1–7 + 一句话总结 + 参考资料）——**章名的唯一来源**。
///
/// 骨架门（`gate`）、正文门（`quality`）、骨架段提示词、生成段提示词都认它；
/// 别再各写一份（`behavior-notes.md §0`：一个事实只有一个 owner）。
pub fn canonical_chapters(medium: Medium) -> [&'static str; 9] {
    let (c3, c5, c6) = match medium {
        Medium::Game => ("世界观与背景设定", "玩法与结构特色", "主要角色与势力"),
        Medium::Manga | Medium::Book => ("作品背景与设定", "创作特色", "主要角色与创作团队"),
        _ => ("故事背景与设定", "叙事与影像特色", "主要角色与创作团队"),
    };
    [
        "作品概览",
        "基础信息速览",
        c3,
        "剧情概要",
        c5,
        c6,
        "评价与历史定位",
        "一句话总结",
        "参考资料",
    ]
}

/// 第 5 章的章名（按介质三档；两个门都查它）。
pub fn chapter5_name(medium: Medium) -> &'static str {
    canonical_chapters(medium)[4]
}

/// 去掉 ``` 围栏（模型爱包一层）。
fn strip_fences(raw: &str) -> String {
    raw.replace("```json", "").replace("```", "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::{Ledger, Status};

    fn ledger() -> Ledger {
        let mut l = Ledger::default();
        for i in 1..=5 {
            l.put(&format!("字段{i}"), "值", vec!["S1".into()], Status::Verified);
        }
        l
    }

    fn good_json() -> String {
        r#"{
          "title": "空之轨迹 FC（游戏）Wiki",
          "note": "依据官方资料，含剧透。检索时间 2026-10。",
          "epigraph": {"kind": "quote", "source_id": "S1"},
          "chapters": [
            {"no": 1, "name": "作品概览", "sections": [{"name": "是什么", "ledger_rows": [1]}]},
            {"no": 2, "name": "基础信息速览", "sections": [{"name": "基本资料", "ledger_rows": [1,2]}]},
            {"no": 3, "name": "世界观与背景设定", "sections": [{"name": "世界规则", "ledger_rows": [3]}]},
            {"no": 4, "name": "剧情概要", "sections": [{"name": "主线", "ledger_rows": [4]}]},
            {"no": 5, "name": "玩法与结构特色", "sections": [{"name": "战斗", "ledger_rows": [5]}]}
          ]
        }"#
        .to_string()
    }

    #[test]
    fn 解析带围栏的骨架() {
        let raw = format!("好的，这是骨架：\n```json\n{}\n```\n以上。", good_json());
        let o = Outline::parse(&raw).unwrap();
        assert_eq!(o.chapters.len(), 5);
        assert_eq!(o.chapters[4].name, "玩法与结构特色");
        assert_eq!(o.epigraph.unwrap().kind, EpigraphKind::Quote);
    }

    #[test]
    fn 好骨架过门() {
        let o = Outline::parse(&good_json()).unwrap();
        let l = ledger();
        // 题记指向 S1，但账本里没有 S1 来源 → 会给一条问题；先补上
        let mut l = l;
        l.put_source(crate::ledger::Source {
            id: "S1".into(), title: "官方".into(), url: "https://x".into(),
            fetched: "2026-10".into(), quote: "原文".into(),
        });
        let g = o.gate(Medium::Game, &l);
        assert!(g.passed(), "不该有问题：{:?}", g.issues);
    }

    #[test]
    fn 小节没挂账本行拦() {
        let raw = good_json().replace(r#"{"name": "主线", "ledger_rows": [4]}"#, r#"{"name": "主线", "ledger_rows": []}"#);
        let o = Outline::parse(&raw).unwrap();
        let mut l = ledger();
        l.sources.push(crate::ledger::Source { id: "S1".into(), title: String::new(), url: String::new(), fetched: String::new(), quote: String::new() });
        let g = o.gate(Medium::Game, &l);
        assert!(!g.passed());
        assert!(g.issues.iter().any(|i| i.contains("没有挂账本行")));
    }

    #[test]
    fn 章号跳号拦() {
        let raw = good_json().replace(r#""no": 4"#, r#""no": 6"#);
        let o = Outline::parse(&raw).unwrap();
        let l = ledger();
        let g = o.gate(Medium::Game, &l);
        assert!(g.issues.iter().any(|i| i.contains("不连续")));
    }

    #[test]
    fn 第5章章名按介质查() {
        let raw = good_json().replace("玩法与结构特色", "创作特色");
        let o = Outline::parse(&raw).unwrap();
        let l = ledger();
        let g = o.gate(Medium::Game, &l);
        assert!(g.issues.iter().any(|i| i.contains("玩法与结构特色")));
        // 换漫画介质就对了（但还缺 S1 来源，只看这一条不出现即可）
        let g2 = o.gate(Medium::Manga, &l);
        assert!(!g2.issues.iter().any(|i| i.contains("第 5 章")));
    }

    #[test]
    fn 引用式题记指向不存在来源拦() {
        let raw = good_json().replace(r#""source_id": "S1""#, r#""source_id": "S9""#);
        let o = Outline::parse(&raw).unwrap();
        let l = ledger();
        let g = o.gate(Medium::Game, &l);
        assert!(g.issues.iter().any(|i| i.contains("S9")));
    }
}

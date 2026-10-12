//! 技能加载与系统提示组装（`docs/prompts.md §1 §4` · `docs/skills.md`）。
//!
//! 两条纪律：
//! 1. **技能段只渲染 L0 目录**（name + 一句话 description），正文靠模型自己 `read_skill`——
//!    这是 v2 相对 v1 的改法（`behavior-notes.md §6`）。
//! 2. **数字与状态从代码注入**（`behavior-notes.md §0`）：骨架、质量门判据都从代码出一份，
//!    不手抄进技能文本。

use std::path::{Path, PathBuf};
use wikiatlas_domain::Medium;

/// 一个技能：`name` + `description`（目录用）+ 正文（`read_skill` 用）。
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub body: String,
}

/// 技能根。默认就是仓库里的 `skills/`（D3 的规则层基准）。
pub fn skills_root() -> PathBuf {
    std::env::var("WIKIATLAS_SKILLS_DIR")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from("/root/WikiAltas/skills"))
}

/// 扫 `*/SKILL.md`，解析 front matter（`---` 之间的 `key: value`）。
pub fn load(root: &Path) -> Vec<Skill> {
    let mut out = vec![];
    let Ok(dir) = std::fs::read_dir(root) else { return out };
    for e in dir.flatten() {
        let p = e.path().join("SKILL.md");
        if !p.is_file() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        let (meta, body) = split_front_matter(&text);
        let name = meta
            .iter()
            .find(|(k, _)| k == "name")
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| e.file_name().to_string_lossy().to_string());
        let description = meta
            .iter()
            .find(|(k, _)| k == "description")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        // 触发纪律也进目录：模型得知道"何时用 / 何时不用"（`skills.md §3`）
        let when = meta.iter().find(|(k, _)| k == "when").map(|(_, v)| v.clone());
        let when_not = meta.iter().find(|(k, _)| k == "when_not").map(|(_, v)| v.clone());
        let mut desc = description;
        if let Some(w) = when {
            desc.push_str(&format!("｜何时用：{w}"));
        }
        if let Some(w) = when_not {
            desc.push_str(&format!("｜何时不用：{w}"));
        }
        out.push(Skill { name, description: desc, body: body.trim().to_string() });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn split_front_matter(text: &str) -> (Vec<(String, String)>, String) {
    let mut meta = vec![];
    let mut rest = text;
    if let Some(stripped) = text.strip_prefix("---") {
        if let Some(end) = stripped.find("\n---") {
            let head = &stripped[..end];
            rest = &stripped[end + 4..];
            let mut key = String::new();
            let mut buf = String::new();
            for line in head.lines() {
                if let Some((k, v)) = line.split_once(':') {
                    if !key.is_empty() {
                        meta.push((key.clone(), buf.trim().to_string()));
                    }
                    key = k.trim().to_string();
                    buf = v.trim().to_string();
                } else if !key.is_empty() {
                    buf.push(' ');
                    buf.push_str(line.trim());
                }
            }
            if !key.is_empty() {
                meta.push((key, buf.trim().to_string()));
            }
        }
    }
    (meta, rest.to_string())
}

/// L0 目录：一行一个技能。
pub fn l0_catalog(skills: &[Skill]) -> String {
    skills
        .iter()
        .map(|s| format!("- {}：{}", s.name, s.description))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 读技能正文（`read_skill` 工具的实现）。`section` 给了就只回那一节（`## 标题` 起）。
pub fn read_skill(root: &Path, name: &str, section: Option<&str>) -> Option<String> {
    // 防目录穿越：只认 `skills/<name>/SKILL.md`
    if name.contains('/') || name.contains("..") {
        return None;
    }
    let text = std::fs::read_to_string(root.join(name).join("SKILL.md")).ok()?;
    let (_, body) = split_front_matter(&text);
    let body = body.trim().to_string();
    match section {
        None => Some(body),
        Some(sec) => {
            let mut out = String::new();
            let mut on = false;
            for line in body.lines() {
                if line.starts_with("## ") {
                    on = line.contains(sec);
                }
                if on {
                    out.push_str(line);
                    out.push('\n');
                }
            }
            if out.is_empty() {
                Some(body) // 找不到那一节就给全文，别让模型空手
            } else {
                Some(out)
            }
        }
    }
}

/// 第三章 / 第五章的章名（`wiki-skeleton §3` 的三档）。
fn chapter3_name(medium: Medium) -> &'static str {
    match medium {
        Medium::Game => "世界观与背景设定",
        Medium::Manga | Medium::Book => "作品背景与设定",
        _ => "故事背景与设定",
    }
}

/// 写作工单的系统提示：身份 + 交付物 + **骨架（代码出的那份）** + 质量门判据 + 技能目录。
///
/// 质量门判据写在这里**不是**"把机械规则写进提示词"——门还是代码在查；
/// 这里只是把**靶子**告诉模型（不告诉它靶子，它没法一次写对）。
pub fn writing_system_prompt(skills: &[Skill], medium: Medium) -> String {
    let catalog = l0_catalog(skills);
    let c3 = chapter3_name(medium);
    let c5 = match medium {
        Medium::Game => "玩法与结构特色",
        Medium::Manga | Medium::Book => "创作特色",
        _ => "叙事与影像特色",
    };
    format!(
        "你是 WikiAtlas 的写作助手，为中文 ACG 资料库写条目。\n\
         写正文只有一个渠道：调用 write_content 工具（正文放进 content_md，markdown）。\n\n\
         ## 交付物\n\
         `# 作品名（介质）Wiki` → `> 说明：…`（整理范围/依据/剧透/检索时间）→ `:::epigraph` 题记 → 编号章节。\n\
         章节骨架（缺内容的小节删掉、同级重排，**不留空号**）：\n\
         ## 1. 作品概览\n## 2. 基础信息速览（2.1 基本资料 / 2.2 版本关系 / 2.3 核心卖点 / 2.4 关键词）\n\
         ## 3. {c3}\n## 4. 剧情概要（完整剧透到结局）\n## 5. {c5}\n## 6. 主要角色与势力（末节写关系简述）\n\
         ## 7. 评价与历史定位\n## 8. 一句话总结\n## 9. 参考资料\n\
         （有可指出处的名台词/名场面时才插「名台词与名场面」，后面章号顺延）\n\n\
         ## 硬规则\n\
         中文第三人称；每句落到具体对象；事实/推断/评价分开；查不到写 `待核实`（只这三个字），不编造。\n\
         禁套话（据说 / 广受好评 / 神作 / 天花板 / 不容错过）。长度服从信息量，按基准密度写（约 7000–12000 汉字）。\n\
         题记块**内不许有空行**（逐行交替）。\n\n\
         ## 工作方式\n\
         1) 先 read_skill 读规范（至少 wiki-write / wiki-skeleton / 该介质那份），按需读 epigraph / fact-check / prose-style。\n\
         2) 再调 write_content 落整篇。\n\
         3) 宿主会用代码质量门查你的产出；不过会把问题回给你一次，照着改。\n\n\
         ## 技能目录（read_skill 取正文）\n{catalog}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 真技能根能加载出目录() {
        let skills = load(&skills_root());
        assert!(skills.len() >= 10, "技能太少了：{}", skills.len());
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"wiki-write"));
        assert!(names.contains(&"wiki-skeleton"));
        assert!(names.contains(&"media-game"));
        // 目录里要有"何时用/何时不用"
        assert!(skills[0].description.contains("何时用"));
    }

    #[test]
    fn read_skill_读得到正文_也能取单节() {
        let root = skills_root();
        let body = read_skill(&root, "wiki-skeleton", None).unwrap();
        assert!(body.contains("骨架"));
        let sec = read_skill(&root, "wiki-skeleton", Some("骨架")).unwrap();
        assert!(sec.contains("## 1. 骨架"));
        assert!(!sec.contains("## 7. 失败路径"));
    }

    #[test]
    fn read_skill_挡目录穿越() {
        let root = skills_root();
        assert!(read_skill(&root, "../../etc", None).is_none());
        assert!(read_skill(&root, "不存在的技能", None).is_none());
    }

    #[test]
    fn 系统提示带上骨架与技能目录() {
        let skills = load(&skills_root());
        let p = writing_system_prompt(&skills, Medium::Game);
        assert!(p.contains("玩法与结构特色"), "游戏介质要用游戏的章名");
        assert!(p.contains("世界观与背景设定"));
        assert!(p.contains("wiki-write"), "要有技能目录");
        assert!(p.contains("待核实"));
    }
}

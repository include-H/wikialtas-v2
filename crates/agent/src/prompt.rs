//! 技能加载与系统提示组装（`docs/prompts.md §1 §4` · `docs/skills.md`）。
//!
//! 两条纪律：
//! 1. **技能段只渲染 L0 目录**（name + 一句话 description），正文靠模型自己 `read_skill`——
//!    这是 v2 相对 v1 的改法（`behavior-notes.md §6`）。
//! 2. **数字与状态从代码注入**（`behavior-notes.md §0`）：骨架、质量门判据都从代码出一份，
//!    不手抄进技能文本。

use std::path::{Path, PathBuf};

/// 一个技能：`name` + `description`（目录用）+ 正文（`read_skill` 用）。
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    /// UI 分组；缺省归入「其它」。
    pub group: String,
    /// 0 = 总入口，1 = 通用子技能，2 = 介质/领域细化。
    pub layer: u8,
    pub body: String,
    /// 实际来源文件；`SKILL.md` 或 `references/**/*.md`。
    pub source: PathBuf,
}

/// 技能根。默认就是仓库里的 `skills/`（D3 的规则层基准）。
pub fn skills_root() -> PathBuf {
    if let Some(configured) = std::env::var("WIKIATLAS_SKILLS_DIR")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
    {
        return configured;
    }

    // 二进制可能从仓库根运行，也可能从 crates/agent 下运行；优先使用编译时项目根，
    // 再兼容旧部署路径。不能只写死 `/root/Atlas`——当前实际工作区是 WikiAltas。
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(|p| p.join("skills"));
    project_root
        .into_iter()
        .chain([
            PathBuf::from("skills"),
            PathBuf::from("/root/WikiAltas/skills"),
            PathBuf::from("/root/Atlas/skills"),
        ])
        .find(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from("skills"))
}

/// 只扫描可发现入口：`skills/<name>/SKILL.md`。
///
/// 入口正文里的 `references/**/*.md` 不会被当作独立 Skill 暴露给初始目录；
/// 它们只在入口路由或 `read_skill` 读取时进入上下文。
pub fn load(root: &Path) -> Vec<Skill> {
    let mut out = vec![];
    let Ok(dir) = std::fs::read_dir(root) else { return out };
    for e in dir.flatten() {
        let p = e.path().join("SKILL.md");
        if !p.is_file() {
            continue;
        }
        if let Some(skill) = parse_skill_file(&p, Some(&e.file_name().to_string_lossy())) {
            out.push(skill);
        }
    }
    out.sort_by(|a, b| a.layer.cmp(&b.layer).then(a.group.cmp(&b.group)).then(a.name.cmp(&b.name)));
    out
}

/// 扫描各入口 `references/` 下带 frontmatter 的模块，用于 `list_skills` / `read_skill`。
pub fn load_modules(root: &Path) -> Vec<Skill> {
    let mut out = Vec::new();
    for entry in load(root) {
        let refs = entry.source.parent().unwrap_or(root).join("references");
        walk_markdown(&refs, &mut out);
    }
    out.sort_by(|a, b| a.layer.cmp(&b.layer).then(a.group.cmp(&b.group)).then(a.name.cmp(&b.name)));
    out
}

/// 入口 + references 模块；仅用于显式技能目录/读取工具，不用于 L0 常驻目录。
pub fn load_all(root: &Path) -> Vec<Skill> {
    let mut out = load(root);
    out.extend(load_modules(root));
    out.sort_by(|a, b| a.layer.cmp(&b.layer).then(a.group.cmp(&b.group)).then(a.name.cmp(&b.name)));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

fn walk_markdown(dir: &Path, out: &mut Vec<Skill>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_markdown(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
            // references 里的说明/样例不一定有 name；只有带 name 的才是可读模块。
            if let Some(skill) = parse_skill_file(&path, None) {
                out.push(skill);
            }
        }
    }
}

fn parse_skill_file(path: &Path, fallback_name: Option<&str>) -> Option<Skill> {
    let text = std::fs::read_to_string(path).ok()?;
    let (meta, body) = split_front_matter(&text);
    let name = meta
        .iter()
        .find(|(k, _)| k == "name")
        .map(|(_, v)| v.clone())
        .or_else(|| fallback_name.map(str::to_string))?;
    let description = meta
        .iter()
        .find(|(k, _)| k == "description")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    let group = meta
        .iter()
        .find(|(k, _)| k == "metadata.group" || k == "group")
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| "其它".into());
    let layer = meta
        .iter()
        .find(|(k, _)| k == "metadata.layer" || k == "layer")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(1);
    let mut desc = description;
    if let Some(w) = meta
        .iter()
        .find(|(k, _)| k == "metadata.when" || k == "when")
        .map(|(_, v)| v.clone())
    {
        desc.push_str(&format!("｜何时用：{w}"));
    }
    if let Some(w) = meta
        .iter()
        .find(|(k, _)| k == "metadata.when_not" || k == "when_not")
        .map(|(_, v)| v.clone())
    {
        desc.push_str(&format!("｜何时不用：{w}"));
    }
    Some(Skill {
        name,
        description: desc,
        group,
        layer,
        body: body.trim().to_string(),
        source: path.to_path_buf(),
    })
}

/// 基准语料目录（实际位于入口的 `references/samples/`）。
pub fn samples_root() -> PathBuf {
    let root = skills_root();
    let nested = root.join("atlas/references/samples");
    if nested.is_dir() {
        nested
    } else {
        root.join("samples")
    }
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
            let mut parent = String::new();
            for line in head.lines() {
                let indent = line.len() - line.trim_start().len();
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Some((k, v)) = trimmed.split_once(':') {
                    if !key.is_empty() {
                        meta.push((key.clone(), buf.trim().to_string()));
                    }
                    if indent == 0 {
                        parent.clear();
                        if v.trim().is_empty() {
                            key.clear();
                            buf.clear();
                            parent = k.trim().to_string();
                        } else {
                            key = k.trim().to_string();
                            buf = v.trim().to_string();
                        }
                    } else if !parent.is_empty() {
                        key = format!("{parent}.{}", k.trim());
                        buf = v.trim().to_string();
                    } else {
                        key = k.trim().to_string();
                        buf = v.trim().to_string();
                    }
                } else if !key.is_empty() {
                    buf.push(' ');
                    buf.push_str(trimmed);
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

/// 用户从 UI 显式加载的技能正文。它比模型自动 `read_skill` 的优先级更高，
/// 但仍保留 L0 目录，让模型知道还可以按需补读其它技能。
pub fn loaded_skill_prompt(skills: &[Skill], names: &[String]) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut blocks = Vec::new();
    for name in names {
        if !seen.insert(name.as_str()) {
            continue;
        }
        let Some(skill) = skills.iter().find(|s| s.name == *name) else {
            continue;
        };
        blocks.push(format!(
            "### {}\n{}\n\n技能正文共 {} 字。把它当作本轮已加载的操作规范；\
             用户当前要求优先于技能里的泛化触发描述。",
            skill.name,
            skill.body,
            skill.body.chars().count()
        ));
    }
    if blocks.is_empty() {
        return String::new();
    }
    format!(
        "## 用户显式加载的技能\n\
         以下正文已真实载入本轮上下文，不是只给了名字。按用户选择执行；\
         需要补充其它规范时仍可调用 `read_skill`。\n\n{}",
        blocks.join("\n")
    )
}

/// 读技能正文（`read_skill` 工具的实现）。`section` 给了就只回那一节（`## 标题` 起）。
pub fn read_skill(root: &Path, name: &str, section: Option<&str>) -> Option<String> {
    // 防目录穿越：name 只能是元数据里的逻辑名，不能拼路径。
    if name.contains('/') || name.contains("..") {
        return None;
    }
    let skill = load_all(root).into_iter().find(|s| s.name == name)?;
    let text = std::fs::read_to_string(skill.source).ok()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 真技能根能加载出目录() {
        let skills = load(&skills_root());
        assert_eq!(skills.len(), 1, "初始发现应只暴露总入口");
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"atlas"));
        // 目录里要有"何时用/何时不用"
        assert!(skills[0].description.contains("何时用"));
        assert_eq!(skills[0].name, "atlas");
        assert_eq!(skills[0].layer, 0);
        assert_eq!(skills[0].group, "总入口");

        let modules = load_modules(&skills_root());
        let module_names: Vec<&str> = modules.iter().map(|s| s.name.as_str()).collect();
        assert!(module_names.contains(&"wiki-write"));
        assert!(module_names.contains(&"wiki-skeleton"));
        assert!(module_names.contains(&"media-game"));
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
    fn 显式加载会把技能正文真正拼进上下文() {
        let skills = load(&skills_root());
        let prompt = loaded_skill_prompt(&skills, &["atlas".into()]);
        assert!(prompt.contains("用户显式加载的技能"));
        assert!(prompt.contains("# Atlas Skill 路由"));
        assert!(prompt.contains("技能正文共"));
    }
}

//! 标题归一化：匹配的唯一口径（`docs/database.md` §2 的 `title_norm`）。
//!
//! 做四件事：全角 → 半角、中日韩标点不动、空白折叠成一个空格、转小写。
//! **不做**分词与拼音（那些留给检索方案，别把词典绑进领域层）。

/// 归一化。`"  涼宮 ハルヒ の 憂鬱　"` → `"涼宮 ハルヒ の 憂鬱"`。
pub fn normalize_title(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_space = false;
    for ch in s.chars() {
        let ch = fullwidth_to_halfwidth(ch);
        if ch.is_whitespace() {
            // 前导空白直接丢；中间连续空白折成一个
            if !out.is_empty() {
                pending_space = true;
            }
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.extend(ch.to_lowercase());
        }
    }
    out
}

/// 全角 ASCII（U+FF01–U+FF5E）与全角空格（U+3000）折成半角。
fn fullwidth_to_halfwidth(ch: char) -> char {
    match ch as u32 {
        0xFF01..=0xFF5E => char::from_u32(ch as u32 - 0xFEE0).unwrap_or(ch),
        0x3000 => ' ',
        _ => ch,
    }
}

/// 正文的规范化。**写路径上只在这里做一次**（人 / 模型 / 宿主都走同一条）。
///
/// 治一个具体的脏：富文本编辑器把「空段落」序列化成**一行 `<br />`**
/// （Milkdown 的 commonmark 序列化器就这样），直接存会污染 `body_md`——
/// 它会一路漏进 AI 的上下文和最终渲染。顺带把连续空行折成一个。
///
/// 保留：真正的硬换行（`文字<br />文字`，同一行里）不动。
pub fn normalize_markdown(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank_run = 0usize;
    for line in s.lines() {
        let trimmed = line.trim();
        if matches!(trimmed, "<br />" | "<br/>" | "<br>") {
            continue; // 空段落留下的孤立硬换行：丢掉
        }
        if trimmed.is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue; // 连续空行折成一个
            }
        } else {
            blank_run = 0;
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// 从正文中提取简介：第一个**普通段落**，跳过标题、说明、题记、引用、列表、
/// 表格、分隔线与代码块；压平空白后按字符数截断（`…` 也占一个字符）。
///
/// 这是 V1 已验证的摘要规则（`archive/v1/backend/internal/httpapi/library_gameatlas.go`），
/// V2 只把截断上限参数化：GameAtlas 沿用 200，Komga 按 D34 用 500。
pub fn markdown_summary(md: &str, max_chars: usize) -> Option<String> {
    if max_chars == 0 {
        return None;
    }
    let normalized = md.replace("\r\n", "\n");
    for paragraph in normalized.split("\n\n") {
        let text = paragraph.trim();
        if text.is_empty() {
            continue;
        }
        let first = text.lines().next().unwrap_or("").trim_start();
        if first.is_empty()
            || first.starts_with('#')
            || first.starts_with('>')
            || first.starts_with(":::")
            || first.starts_with('|')
            || first.starts_with("```")
            || first.starts_with("~~~")
            || first == "---"
            || first.starts_with("- ")
            || first.starts_with("* ")
            || first.starts_with("+ ")
            || first
                .split_once(". ")
                .is_some_and(|(n, _)| n.chars().all(|c| c.is_ascii_digit()))
        {
            continue;
        }
        let flat = strip_inline_markdown(text);
        let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
        if flat.is_empty() {
            continue;
        }
        let mut chars = flat.chars();
        let clipped: String = chars.by_ref().take(max_chars).collect();
        return if chars.next().is_some() {
            Some(format!("{}…", clipped))
        } else {
            Some(clipped)
        };
    }
    None
}

fn strip_inline_markdown(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            '`' => {
                while let Some(next) = chars.next() {
                    if next == '`' {
                        break;
                    }
                    out.push(next);
                }
            }
            '*' | '_' | '~' => {}
            '[' => {
                // [text](url) → text；普通方括号保留。
                let mut label = String::new();
                let mut closed = false;
                for next in chars.by_ref() {
                    if next == ']' {
                        closed = true;
                        break;
                    }
                    label.push(next);
                }
                if closed && chars.peek() == Some(&'(') {
                    chars.next();
                    let mut depth = 1usize;
                    for next in chars.by_ref() {
                        if next == '(' {
                            depth += 1;
                        } else if next == ')' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                    out.push_str(&label);
                } else {
                    out.push('[');
                    out.push_str(&label);
                    if closed {
                        out.push(']');
                    }
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

/// 字符 trigram 相似度（Jaccard）。用于重挂候选，**确定性、零模型**。
/// 完全相等直接 1；过短字符串退化成字符集合比较。
pub fn trigram_similarity(a: &str, b: &str) -> f64 {
    let a = normalize_title(a);
    let b = normalize_title(b);
    if a == b {
        return if a.is_empty() { 0.0 } else { 1.0 };
    }
    let ag = trigrams(&a);
    let bg = trigrams(&b);
    if ag.is_empty() || bg.is_empty() {
        return 0.0;
    }
    let mut inter = 0usize;
    let mut used = vec![false; bg.len()];
    for x in &ag {
        if let Some(i) = bg.iter().enumerate().find_map(|(i, y)| {
            (!used[i] && x == y).then_some(i)
        }) {
            used[i] = true;
            inter += 1;
        }
    }
    let union = ag.len() + bg.len() - inter;
    inter as f64 / union as f64
}

/// 应用侧中文 bigram 兜底（D36）：字符二元组 Dice 系数。
/// 不依赖分词词典，适合标题/短查询；正文召回仍优先用 SQL 子串。
pub fn bigram_similarity(query: &str, text: &str) -> f64 {
    let query = normalize_title(query);
    let text = normalize_title(text);
    if query.is_empty() || text.is_empty() {
        return 0.0;
    }
    if text.contains(&query) {
        return 1.0;
    }
    let q = bigrams(&query);
    let t = bigrams(&text);
    if q.is_empty() || t.is_empty() {
        return 0.0;
    }
    let mut used = vec![false; t.len()];
    let mut inter = 0usize;
    for x in &q {
        if let Some(i) = t
            .iter()
            .enumerate()
            .find_map(|(i, y)| (!used[i] && x == y).then_some(i))
        {
            used[i] = true;
            inter += 1;
        }
    }
    (2.0 * inter as f64) / (q.len() + t.len()) as f64
}

fn bigrams(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 2 {
        return chars.into_iter().map(|c| c.to_string()).collect();
    }
    chars.windows(2).map(|w| w.iter().collect()).collect()
}

fn trigrams(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 3 {
        return chars.into_iter().map(|c| c.to_string()).collect();
    }
    chars
        .windows(3)
        .map(|w| w.iter().collect::<String>())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 去首尾空白_折叠中间空白() {
        assert_eq!(normalize_title("  上 古 卷轴  "), "上 古 卷轴");
        assert_eq!(normalize_title("a\t\tb\nc"), "a b c");
    }

    #[test]
    fn 空段落留下的_br_被丢掉且空行折一() {
        assert_eq!(
            normalize_markdown("# 标题\n\n<br />\n\n正文\n"),
            "# 标题\n\n正文\n"
        );
        // 连续空行也折成一个
        assert_eq!(normalize_markdown("a\n\n\n\nb\n"), "a\n\nb\n");
    }

    #[test]
    fn 行内的硬换行保留() {
        assert_eq!(normalize_markdown("上<br />下\n"), "上<br />下\n");
    }

    #[test]
    fn 全角折半角() {
        assert_eq!(normalize_title("Ａ Ｂ　Ｃ"), "a b c");
        assert_eq!(normalize_title("０７　Ｖ"), "07 v");
    }

    #[test]
    fn 大小写归一() {
        assert_eq!(normalize_title("CLANNAD"), "clannad");
        assert_eq!(normalize_title("Clannad"), "clannad");
    }

    #[test]
    fn 中文与日文原样保留() {
        assert_eq!(normalize_title("你的名字"), "你的名字");
        assert_eq!(normalize_title("銀の意志"), "銀の意志");
    }

    #[test]
    fn 空与非空边界() {
        assert_eq!(normalize_title(""), "");
        assert_eq!(normalize_title("   "), "");
    }

    #[test]
    fn 摘要跳过文档头与结构化块() {
        let md = "# 标题\n\n> 说明：不是正文。\n\n:::epigraph\n题记\n:::\n\n---\n\n这是**第一段**正文，含[链接](https://example.com)。\n\n第二段。";
        assert_eq!(
            markdown_summary(md, 500).as_deref(),
            Some("这是第一段正文，含链接。")
        );
    }

    #[test]
    fn 摘要按字符截断并明确省略() {
        assert_eq!(markdown_summary("一二三四五", 4).as_deref(), Some("一二三四…"));
        assert_eq!(markdown_summary("一二三四", 4).as_deref(), Some("一二三四"));
        assert_eq!(markdown_summary("# 只有标题\n\n- 只有列表", 500), None);
    }

    #[test]
    fn trigram_相似度_相同最高_相近高于无关() {
        assert_eq!(trigram_similarity("空之轨迹 SC", "空之轨迹 sc"), 1.0);
        let near = trigram_similarity("空之轨迹 SC", "空之轨迹 the 3rd");
        let far = trigram_similarity("空之轨迹 SC", "使命召唤：无限战争");
        assert!(near > far, "near={near}, far={far}");
        assert_eq!(trigram_similarity("", ""), 0.0);
    }

    #[test]
    fn bigram兜底_子串满分_相近高于无关() {
        assert_eq!(bigram_similarity("青羽", "这是青羽测试"), 1.0);
        assert!(
            bigram_similarity("空之轨迹", "空之轨迹 SC")
                > bigram_similarity("空之轨迹", "使命召唤")
        );
    }
}

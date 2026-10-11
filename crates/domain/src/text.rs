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
}

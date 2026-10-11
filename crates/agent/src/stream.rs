//! 流式正文投影：从不完整的 `function_call_arguments` JSON 里增量抽出正文。
//!
//! `docs/behavior-notes.md §5`：模型写正文时，正文是从**工具参数流**里出来的
//! （Responses 的 `function_call_arguments.delta` 累积成一段 JSON，正文在其中一个字符串
//! 字段里）。要在**不完整 JSON** 上增量抽出它——这是"实时感"的来源，也是最容易写错的地方，
//! 所以它是独立模块 + **重点单测**。
//!
//! 三条纪律：
//! 1. **不猜**：JSON 没给全/转义序列断在半路，就停在能确定的位置，别补全（`behavior-notes.md §3`）。
//! 2. **单调**：同一个累积前缀抽出来的值，随前缀增长**只增不改**（前面已经确定的字不会再变）。
//! 3. **只认字段起点**：`"content_md"` 必须出现在对象的键位置（前一个非空白是 `{` 或 `,`），
//!    否则会把别的字段**值**里的同名文本误认成键。

use std::time::{Duration, Instant};

/// 累积的 `function_call_arguments` 前缀 → 目标字符串字段的当前值。
///
/// 用法：每来一个 `...arguments.delta` 就 `push(delta)`，拿返回值去发 `content.delta`。
#[derive(Debug, Clone)]
pub struct ArgProjector {
    field: String,
    buf: String,
    last: String,
}

impl ArgProjector {
    pub fn new(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            buf: String::new(),
            last: String::new(),
        }
    }

    /// 追加一段 delta，返回**当前累积出来的值**（没开始/还是空就是空串）。
    pub fn push(&mut self, delta: &str) -> &str {
        self.buf.push_str(delta);
        if let Some(v) = extract_string_field(&self.buf, &self.field) {
            if v.len() >= self.last.len() {
                self.last = v;
            }
        }
        &self.last
    }

    /// 当前值（不追加）。
    pub fn value(&self) -> &str {
        &self.last
    }

    /// 累积的原始 JSON 前缀（收尾时解析完整参数用）。
    pub fn raw(&self) -> &str {
        &self.buf
    }
}

/// 从（可能不完整的）JSON 文本里抽 `field` 的字符串值。
///
/// 返回 `None` = 还没见到这个字段（或见到键但还没到值的引号）。
/// 返回 `Some("")` = 字段值已经开始（可能内容还没到）。
pub fn extract_string_field(buf: &str, field: &str) -> Option<String> {
    let key = format!("\"{field}\"");
    let bytes = buf.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = buf[from..].find(&key) {
        let at = from + rel;
        // 只认「对象键」位置：前一个非空白字符必须是 `{` 或 `,`
        let before_ok = buf[..at]
            .trim_end()
            .chars()
            .last()
            .is_some_and(|c| c == '{' || c == ',');
        from = at + key.len();
        if !before_ok {
            continue;
        }
        // 键后面：可空白 → ':' → 可空白 → '"'
        let rest = buf[from..].trim_start();
        let Some(rest) = rest.strip_prefix(':') else { continue };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('"') else { continue };
        return Some(decode_json_string_prefix(rest));
    }
    let _ = bytes;
    None
}

/// 解一个**可能被截断**的 JSON 字符串（不含开头的引号）：遇到收尾引号或解不动就停。
///
/// 解不动的两种：转义序列没写全（`\` 后面没东西 / `\u12`）、UTF-16 代理对只到了一半。
/// 这两种都**不吐半个字符**——不然前端会看到一闪而过的乱码。
fn decode_json_string_prefix(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' => break,
            '\\' => {
                let Some(e) = it.next() else { break };
                match e {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    '/' => out.push('/'),
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    'u' => match read_hex4(&mut it) {
                        None => break, // \u 位没凑齐 → 停
                        Some(cp) => match decode_codepoint(cp, &mut it) {
                            None => break, // 代理对只到一半 → 停
                            Some(ch) => out.push(ch),
                        },
                    },
                    // JSON 里不该出现别的转义；出现了就按字面收，别丢字（容错优先）
                    other => out.push(other),
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// 读 4 位十六进制；凑不齐返回 None（且**不消费**已经读到的位——反正调用方会停）。
fn read_hex4(it: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<u32> {
    let mut v = 0u32;
    for _ in 0..4 {
        let c = it.peek().copied()?;
        let d = c.to_digit(16)?;
        it.next();
        v = v * 16 + d;
    }
    Some(v)
}

/// 处理 `\uXXXX`：高代理要等低代理一起到（凑不齐就返回 None = 停）。
fn decode_codepoint(
    cp: u32,
    it: &mut std::iter::Peekable<std::str::Chars<'_>>,
) -> Option<char> {
    if (0xD800..0xDC00).contains(&cp) {
        // 高代理：必须紧跟 \uDC00–\uDFFF
        let save = it.clone();
        if it.next() != Some('\\') || it.next() != Some('u') {
            return None;
        }
        let Some(lo) = read_hex4(it) else {
            *it = save;
            return None;
        };
        if !(0xDC00..0xE000).contains(&lo) {
            *it = save;
            return None;
        }
        let combined = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
        char::from_u32(combined)
    } else {
        char::from_u32(cp)
    }
}

/// 200ms 节流器：正文流按段推（`docs/behavior-notes.md §5`）。
///
/// 传 `now` 进来（而不是内部取），才能单测。
#[derive(Debug)]
pub struct Throttle {
    interval: Duration,
    last: Option<Instant>,
}

impl Throttle {
    pub fn new(interval: Duration) -> Self {
        Self { interval, last: None }
    }

    /// 到点了吗？到点会顺手记下这次的时间。
    pub fn ready(&mut self, now: Instant) -> bool {
        match self.last {
            Some(t) if now.duration_since(t) < self.interval => false,
            _ => {
                self.last = Some(now);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 一次给全也能抽出来() {
        let json = r##"{"node_id":"n1","content_md":"# 标题\n\n正文"}"##;
        assert_eq!(extract_string_field(json, "content_md").unwrap(), "# 标题\n\n正文");
    }

    #[test]
    fn 增量累积_抽出的值单调增长() {
        let full = r#"{"content_md":"第一章 开始"}"#;
        // 按每 3 个字符切一刀，模拟 delta 流
        let mut p = ArgProjector::new("content_md");
        let mut seen = vec![];
        let chars: Vec<char> = full.chars().collect();
        for chunk in chars.chunks(3) {
            let s: String = chunk.iter().collect();
            seen.push(p.push(&s).to_string());
        }
        assert_eq!(p.value(), "第一章 开始");
        // 单调：后一个一定是前一个的前缀延长
        for w in seen.windows(2) {
            assert!(w[1].starts_with(&w[0]), "{:?} 不是 {:?} 的延长", w[1], w[0]);
        }
    }

    #[test]
    fn 转义序列断在半路不吐半个字符() {
        // 正解是「换行 + 引号」，但 delta 只到了 `\` 与 `\"`
        let mut p = ArgProjector::new("content_md");
        assert_eq!(p.push(r#"{"content_md":"a"#), "a");
        assert_eq!(p.push("\\"), "a", "半个转义不能吐出来");
        assert_eq!(p.push("n"), "a\n");
        assert_eq!(p.push("b\\"), "a\nb");
        assert_eq!(p.push("\""), "a\nb\"");
    }

    #[test]
    fn unicode_转义断在半路不吐乱码() {
        let mut p = ArgProjector::new("content_md");
        p.push(r#"{"content_md":"中"#);
        assert_eq!(p.push("\\u"), "中");
        assert_eq!(p.push("4e"), "中", "\\u 没凑够 4 位不吐");
        assert_eq!(p.push("2d"), "中中");
    }

    #[test]
    fn 代理对要等低代理() {
        let mut p = ArgProjector::new("content_md");
        p.push(r#"{"content_md":""#);
        assert_eq!(p.push("\\ud83d"), "", "高代理单独到 → 先不吐");
        assert_eq!(p.push("\\ude00"), "😀");
    }

    #[test]
    fn 别的字段值里出现同名字符串不算键() {
        let json = r#"{"note":"content_md 是个好字段","content_md":"真正文"}"#;
        assert_eq!(extract_string_field(json, "content_md").unwrap(), "真正文");
    }

    #[test]
    fn 只出现键但值还没到() {
        assert_eq!(extract_string_field(r#"{"content_md""#, "content_md"), None);
        assert_eq!(extract_string_field(r#"{"content_md":"#, "content_md"), None);
        assert_eq!(extract_string_field(r#"{"content_md":" "#, "content_md").unwrap(), " ");
    }

    #[test]
    fn 换行与制表按字面() {
        let mut p = ArgProjector::new("content_md");
        p.push(r#"{"content_md":"a\n\tb"}"#);
        assert_eq!(p.value(), "a\n\tb");
    }

    #[test]
    fn 节流按间隔放行() {
        let mut t = Throttle::new(Duration::from_millis(200));
        let t0 = Instant::now();
        assert!(t.ready(t0), "第一次立刻放行");
        assert!(!t.ready(t0 + Duration::from_millis(50)));
        assert!(!t.ready(t0 + Duration::from_millis(199)));
        assert!(t.ready(t0 + Duration::from_millis(200)));
    }
}

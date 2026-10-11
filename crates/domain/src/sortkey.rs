//! 同级排序键：**可插值**。
//!
//! v1 用整数 `sort_order`，结果是「想在两者之间插一个」只能整批重排。
//! 这里用 base62 字符串当分数：**字典序 = 顺序**。
//!
//! 字母表刻意按 ASCII 升序排（`0-9` < `A-Z` < `a-z`），
//! 所以 Postgres 的 `C.UTF-8` collation 下 `ORDER BY sort_key` 就是我们要的顺序。

const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const BASE: u32 = 62;

fn digit_of(b: u8) -> u32 {
    ALPHABET
        .iter()
        .position(|&x| x == b)
        .unwrap_or_else(|| panic!("排序键含非法字符：{:?}", b as char)) as u32
}

fn char_of(d: u32) -> char {
    ALPHABET[d as usize] as char
}

/// 生成一个排在 `prev` 之后的键（`None` = 生成第一个）。
pub fn after(prev: Option<&str>) -> String {
    between(prev, None)
}

/// 生成一个**严格**夹在 `a` 与 `b` 之间的键。要求 `a < b`（两者都可为 None）。
pub fn between(a: Option<&str>, b: Option<&str>) -> String {
    let da: Vec<u32> = a.map(|s| s.bytes().map(digit_of).collect()).unwrap_or_default();
    let db: Vec<u32> = b.map(|s| s.bytes().map(digit_of).collect()).unwrap_or_default();
    let mut out = String::new();
    let mut i = 0usize;
    loop {
        match (da.get(i).copied(), db.get(i).copied()) {
            // 两端都还有位：能取中点就取中点，否则抄下这一位继续往后看
            (Some(x), Some(y)) => {
                debug_assert!(x < y, "between 要求 a < b");
                if y - x > 1 {
                    out.push(char_of((x + y) / 2));
                    break;
                }
                out.push(char_of(x));
            }
            // b 已耗尽而 a 未尽：在 x 与 BASE 之间取中点
            (Some(x), None) => {
                if BASE - x > 1 {
                    out.push(char_of((x + BASE) / 2));
                    break;
                }
                out.push(char_of(x));
            }
            // a 已耗尽：在 0 与 y 之间取中点
            (None, Some(y)) => {
                if y > 1 {
                    out.push(char_of(y / 2));
                    break;
                }
                out.push(char_of(0));
            }
            // 都耗尽：取字母表中点
            (None, None) => {
                out.push(char_of(BASE / 2));
                break;
            }
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 第一个与最后一个() {
        let first = after(None);
        assert!(!first.is_empty());
        assert!(after(Some(&first)) > first);
        // 特殊字符位：'z' 之后还能再生
        let last = after(Some("z"));
        assert!(last > "z".to_string());
    }

    #[test]
    fn 依次追加是单调的() {
        let mut keys = vec![after(None)];
        for _ in 0..200 {
            let next = after(Some(keys.last().unwrap()));
            assert!(next > *keys.last().unwrap(), "{} 应大于 {}", next, keys.last().unwrap());
            keys.push(next);
        }
    }

    #[test]
    fn 反复往中间插也收敛() {
        // 这是排序键存在的理由：在两个键之间插 50 次，不能崩、也不能插不进去
        let lo = after(None);
        let hi = after(Some(&lo));
        let mut a = lo.clone();
        let mut b = hi.clone();
        for _ in 0..50 {
            let mid = between(Some(&a), Some(&b));
            assert!(mid > a, "{mid} 应大于 {a}");
            assert!(mid < b, "{mid} 应小于 {b}");
            a = mid;
        }
        // 最坏情况键会变长，但有上界
        assert!(a.len() <= 60, "键长失控：{}", a.len());
    }

    #[test]
    fn 中间插入对具体端点也对() {
        // 相邻位要靠「抄下来继续」才能插进去
        let mid = between(Some("0z"), Some("1"));
        assert!(mid > "0z".to_string(), "{mid}");
        assert!(mid < "1".to_string(), "{mid}");
    }
}

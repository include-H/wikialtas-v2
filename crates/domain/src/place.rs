//! 建档落位：一条库条目该落到哪个节点（`docs/library-sync.md` §2.3）。
//!
//! 从 v1 的 `domain/library_place.go` 搬来（那条规则在 v1 里被真库磨过）。**零 IO**：
//! 这里只有纯规则，查库 / 建节点 / 挂链由 `store` 按计划执行。
//!
//! 要解决的具体问题（v1 原话）：**同名不同来源**——Komga 的《16bit的感动》漫画
//! 和 Emby 的《16bit的感动》番剧，各自建档会造出两个同名节点。正确结构是：
//!
//! ```text
//! 16bit的感动           (series)
//! ├─ 16bit的感动（剧集）  ← Emby 条目
//! └─ 16bit的感动（漫画）  ← Komga 条目
//! ```

use crate::library::{ContainerKind, Source};
use crate::{text, Medium};

/// 清单条目的 `(source, kind, format)` → 介质。落位与节点介质都用这一处口径。
pub fn medium_of(source: Source, kind: &str, format: Option<&str>) -> Medium {
    let k = kind.trim();
    let f = format.unwrap_or("").trim();
    match source {
        Source::GameAtlas => Medium::Game,
        Source::Komga => match f {
            "novel" => Medium::Book,
            "comic" => Medium::Manga,
            // format 缺省时按漫画（Komga 部署里 noval 一定是 epub → 已由客户端标 novel）
            _ => Medium::Manga,
        },
        Source::Emby => match k {
            "movie" => Medium::Movie,
            // BoxSet 之类的"合集"不是剧集，别误标
            "collection" => Medium::Other,
            _ => Medium::Tv,
        },
    }
}

/// 介质后缀用的词（与前端 `MEDIUM_LABEL` 同一套）。
pub fn medium_label(m: Medium) -> &'static str {
    match m {
        Medium::Game => "游戏",
        Medium::Movie => "电影",
        Medium::Tv => "剧集",
        Medium::Manga => "漫画",
        Medium::Book => "图书",
        Medium::Other => "其他",
    }
}

/// 容器是不是「作品系列」（GA / Komga 的 series）。是 → 可以当父节点。
pub fn is_series_container(kind: ContainerKind) -> bool {
    kind == ContainerKind::Series
}

/// GA 里没有系列的游戏用的哨兵容器键（`sources::gameatlas` 写的值）。
pub const NO_CONTAINER: &str = "series:none";

/// 这个容器键要不要真的建一个系列节点。
///
/// **库**（Emby 的 view）不成系列——拿库名建节点毫无意义（`docs/features.md` §1），
/// 所以 Emby 的条目落顶级；只有「作品系列」才当父节点。
pub fn container_makes_parent(kind: ContainerKind, container_key: &str) -> bool {
    is_series_container(kind) && !container_key.trim().is_empty() && container_key != NO_CONTAINER
}

/// 子节点标题：默认就是条目名；**只有**与父节点同名、或与已有兄弟撞名时，才加介质后缀
/// （`《X》→《X（剧集）》`）。后缀也撞（同介质同名）时加序号兜底。
///
/// 纯函数：父标题与兄弟标题由调用方查好传进来，规则本身能单测。
pub fn child_title(
    entry_title: &str,
    medium: Medium,
    parent_title: Option<&str>,
    siblings: &[String],
) -> String {
    let title = entry_title.trim();
    // 「同一作品的兄弟」也算占位：已有《X（剧集）》时，再来《X（漫画）》不能又叫《X》——
    // 否则同一系列下会混着《X（剧集）》和《X》。所以比较时先剥掉标题尾部的介质后缀。
    let bare = |s: &str| match s.rfind('（') {
        Some(i) => s[..i].trim().to_string(),
        None => s.trim().to_string(),
    };
    let taken = |t: &str| {
        let want = text::normalize_title(t);
        siblings.iter().any(|s| {
            let n = text::normalize_title(s);
            n == want || text::normalize_title(&bare(s)) == want
        })
    };
    let same_as_parent = parent_title
        .map(|p| text::normalize_title(p) == text::normalize_title(title))
        .unwrap_or(false);

    if !taken(title) && !same_as_parent {
        return title.to_string();
    }
    let base = format!("{title}（{}）", medium_label(medium));
    if !taken(&base) {
        return base;
    }
    for i in 2u32.. {
        let cand = format!("{base} {i}");
        if !taken(&cand) {
            return cand;
        }
    }
    unreachable!("序号兜底一定收敛")
}

/// 复用判据（v1 `reuseChild`）：标题或别名等于**条目名**，或等于**带介质后缀的条目名**。
/// 后者是幂等的关键——重跑同一个条目该复用《X（剧集）》，而不是造《X（剧集） 2》。
pub fn reuses_existing(
    node_title: &str,
    node_medium: Option<Medium>,
    node_aliases: &[String],
    entry_title: &str,
    medium: Medium,
) -> bool {
    // **介质不同就不复用**：同一作品的不同媒体按 `docs/library-sync.md` §2.3 的示例
    // 各建一个节点（《X（剧集）》/《X（漫画）》），不能挤在同一个节点上。
    if node_medium != Some(medium) {
        return false;
    }
    let want = text::normalize_title(entry_title);
    let want_suffixed = text::normalize_title(&format!("{entry_title}（{}）", medium_label(medium)));
    let mut candidates = vec![node_title.to_string()];
    candidates.extend(node_aliases.iter().cloned());
    candidates.iter().any(|c| {
        let n = text::normalize_title(c);
        n == want || n == want_suffixed
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 介质由源与类型推() {
        assert_eq!(medium_of(Source::GameAtlas, "game", None), Medium::Game);
        assert_eq!(medium_of(Source::Komga, "book", Some("comic")), Medium::Manga);
        assert_eq!(medium_of(Source::Komga, "book", Some("novel")), Medium::Book);
        assert_eq!(medium_of(Source::Emby, "movie", None), Medium::Movie);
        assert_eq!(medium_of(Source::Emby, "tv", None), Medium::Tv);
        assert_eq!(medium_of(Source::Emby, "collection", None), Medium::Other);
    }

    #[test]
    fn 不撞名就用条目名() {
        let t = child_title("空之轨迹 SC", Medium::Game, Some("轨迹系列"), &[]);
        assert_eq!(t, "空之轨迹 SC");
    }

    #[test]
    fn 与父同名才加介质后缀() {
        // 同名的系列 + 子节点是常态（Komga 分卷系列就是系列名=作品名）
        let t = child_title("16bit的感动", Medium::Tv, Some("16bit的感动"), &[]);
        assert_eq!(t, "16bit的感动（剧集）");
        let t = child_title("16bit的感动", Medium::Manga, Some("16bit的感动"), &[]);
        assert_eq!(t, "16bit的感动（漫画）");
    }

    #[test]
    fn 与兄弟撞名也加后缀() {
        let sibs = vec!["16bit的感动".to_string()];
        let t = child_title("16bit的感动", Medium::Manga, Some("某个系列"), &sibs);
        assert_eq!(t, "16bit的感动（漫画）");
    }

    #[test]
    fn 同一作品的另一种媒体也不去占用裸标题() {
        // 兄弟里已经有《X（剧集）》，再来《X（漫画）》不能又叫《X》
        let sibs = vec!["16bit的感动（剧集）".to_string()];
        let t = child_title("16bit的感动", Medium::Manga, Some("某个系列"), &sibs);
        assert_eq!(t, "16bit的感动（漫画）");
    }

    #[test]
    fn 后缀再撞就加序号兜底() {
        let sibs = vec!["X".to_string(), "X（漫画）".to_string()];
        let t = child_title("X", Medium::Manga, None, &sibs);
        assert_eq!(t, "X（漫画） 2");
    }

    #[test]
    fn 复用认两种写法_但介质必须一致() {
        let tv = Some(Medium::Tv);
        assert!(reuses_existing("16bit的感动（剧集）", tv, &[], "16bit的感动", Medium::Tv));
        assert!(reuses_existing("16bit的感动", tv, &[], "16bit的感动", Medium::Tv));
        assert!(reuses_existing("别名", tv, &["16bit的感动".into()], "16bit的感动", Medium::Tv));
        assert!(!reuses_existing("别的东西", tv, &[], "16bit的感动", Medium::Tv));
        // 剧集节点不能拿来复用成漫画
        assert!(!reuses_existing("16bit的感动（剧集）", tv, &[], "16bit的感动", Medium::Manga));
        assert!(!reuses_existing("16bit的感动", None, &[], "16bit的感动", Medium::Tv));
    }

    #[test]
    fn 只有作品系列才当父节点() {
        assert!(container_makes_parent(ContainerKind::Series, "series:abc"));
        assert!(!container_makes_parent(ContainerKind::Library, "view:1"));
        assert!(!container_makes_parent(ContainerKind::Mixed, "view:2"));
        assert!(!container_makes_parent(ContainerKind::Series, NO_CONTAINER));
    }
}

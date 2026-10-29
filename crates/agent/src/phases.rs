//! 三段的阶段定义与各段系统提示（`docs/design.md §2` · D55 D57 D58）。
//!
//! **阶段切换不改工单**：研究 → 骨架 → 生成是同一个 run 的三个 `checkpoint.phase`；
//! 模型在每一段看到的能力面不同——**生成段手里没有检索工具，从结构上不可能"只查不写"**
//! （这正是 `design.md §1` 诊断出的病根）。
//!
//! 提示词纪律（`prompts.md §1.2`）：数字与预算从代码注入，不手抄进技能。

use crate::ledger::{requirements, Ledger};
use crate::prompt::{l0_catalog, Skill};
use crate::skeleton::Outline;
use wikiatlas_domain::Medium;

/// 检索预算（`design.md §2.2`，可配；先按文档默认值）。
pub const SEARCH_BUDGET: u32 = 24;
pub const FETCH_BUDGET: u32 = 16;
pub const RESEARCH_MAX_ITERS: u32 = 48;
pub const GENERATION_MAX_ITERS: u32 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Research,
    Outline,
    Generation,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Research => "research",
            Phase::Outline => "outline",
            Phase::Generation => "generation",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "research" => Some(Phase::Research),
            "outline" => Some(Phase::Outline),
            "generation" => Some(Phase::Generation),
            _ => None,
        }
    }
    /// 给人看的中文名（右栏时间线按阶段分组）。
    pub fn label(self) -> &'static str {
        match self {
            Phase::Research => "研究",
            Phase::Outline => "骨架",
            Phase::Generation => "生成",
        }
    }
}

/// 必填字段清单（给人读 / 给模型看）。
fn required_list(medium: Medium) -> String {
    requirements(medium)
        .iter()
        .map(|r| {
            if r.min() > 1 {
                format!("{}（≥{}）", r.label(), r.min())
            } else {
                r.label().to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// 研究段的系统提示。
pub fn research_prompt(
    skills: &[Skill],
    medium: Medium,
    node_title: &str,
    search_budget: u32,
    fetch_budget: u32,
) -> String {
    let catalog = l0_catalog(skills);
    format!(
        "你是 Atlas 的馆员 Atlas，正在为中文 ACG 资料库的《{node_title}》做**研究**。\n\
         这一段**只查资料、记事实账本，不写正文**（写正文是下一段的事，这一段没有写工具）。\n\n\
         ## 目标\n\
         把下面这份必填字段清单查满（每查到一个事实就用 ledger_put 记一行）：\n\
         {reqs}\n\n\
         ## 工具\n\
         - `read_skill`：先读 `fact-check`（来源分层与缺口处理）、该介质那份（{media}）、需要时 `multi-source`。\n\
         - `web_search`：检索。优先官方 / 专业数据库 / 权威媒体；社区与自媒体只能当线索。\n\
         - `web_fetch`：打开某一页读正文（超出 12k 字会被截断并明说）。\n\
         - `ledger_put`：**每查到一条事实就记一行**（字段 / 断言 / 状态），来源写在 source_title + source_url + quote 里，宿主替你编号。\n\n\
         ## 纪律\n\
         - 事实、推断、评价分开；**查不到就记 `待核实`**，绝不编造台词 / 日期 / 数字 / 奖项。\n\
         - ⚠️ 媒体库里的年份常是「再发行年」，**首发日期一律以检索为准**。\n\
         - 预算：检索 ≤ {sb} 次、抓取 ≤ {fb} 页。宿主每轮会用代码算覆盖率，并告诉你「还缺什么」。\n\
         - 覆盖率达标（或预算用尽）就**停下**，不要为了凑数空转。\n\n\
         ## 技能目录（read_skill 取正文）\n{catalog}",
        reqs = required_list(medium),
        media = media_skill(medium),
        sb = search_budget,
        fb = fetch_budget,
    )
}

/// 骨架段的系统提示：**无工具**，只产出一份 JSON。
pub fn outline_prompt(skills: &[Skill], medium: Medium, node_title: &str, ledger: &Ledger) -> String {
    let catalog = l0_catalog(skills);
    format!(
        "你是 Atlas 的馆员 Atlas。下面是为《{node_title}》整理好的**事实账本**；\n\
         请据此产出这份 wiki 的**章节骨架**（只输出一个 JSON 对象，不要写正文、不要调用工具）。\n\n\
         ## 事实账本\n{ledger_md}\n\n\
         ## 输出 JSON（严格按这个形状）\n\
         ```json\n\
         {{\n\
           \"title\": \"# 作品名（介质）Wiki\",\n\
           \"note\": \"说明行内容（整理范围 / 依据 / 剧透 / 检索时间）\",\n\
           \"epigraph\": {{\"kind\": \"quote|scene|poem\", \"source_id\": \"S1\"}},\n\
           \"chapters\": [\n\
             {{\"no\": 1, \"name\": \"作品概览\", \"sections\": [\n\
               {{\"name\": \"小节名\", \"ledger_rows\": [1,2]}}\n\
             ]}}\n\
           ]\n\
         }}\n\
         ```\n\n\
         ## 纪律\n\
         - 章名**必须照这个用**（介质 = {medium_label}）：{chapters}。\n\
           「一句话总结」「参考资料」永远最后两章；有可指出处的名句/名场面才插「名台词与名场面」并顺延章号。\n\
           用不上的小节删掉、不留空号。**第 5 章的名字一个字都不能改**（宿主按介质查它）。\n\
         - **每个小节至少挂 1 条账本行**（`ledger_rows` 填账本断言的 `#`）；**没有事实支撑的小节不许出现**。\n\
         - 题记：账本里有可指出处的名句就用 `quote` 并把 `source_id` 填成对应来源编号；否则用 `scene`。\n\
         - 只输出 JSON，前后不要写别的解释。\n\n\
         ## 技能目录（仅作参考，本段不读正文）\n{catalog}",
        ledger_md = ledger.render(),
        medium_label = medium_label(medium),
        chapters = chapter_list(medium),
    )
}

/// 标准章名清单（给人 / 给模型读的「1 作品概览 / 2 …」）。
fn chapter_list(medium: Medium) -> String {
    crate::skeleton::canonical_chapters(medium)
        .iter()
        .enumerate()
        .map(|(i, n)| format!("{} {}", i + 1, n))
        .collect::<Vec<_>>()
        .join(" / ")
}

fn medium_label(medium: Medium) -> &'static str {
    match medium {
        Medium::Game => "游戏",
        Medium::Movie => "电影",
        Medium::Tv => "剧集",
        Medium::Manga => "漫画",
        Medium::Book => "图书",
        Medium::Other => "作品",
    }
}

/// 生成段的系统提示：**检索工具不在场**，只有 patch_section（与只读技能/账本）。
pub fn generation_prompt(
    skills: &[Skill],
    medium: Medium,
    node_title: &str,
    outline: &Outline,
    ledger: &Ledger,
) -> String {
    let catalog = l0_catalog(skills);
    let c5 = crate::skeleton::chapter5_name(medium);
    let skeleton = render_outline(outline);
    format!(
        "你是 Atlas 的馆员 Atlas，正在为《{node_title}》写**正文**。\n\
         你只有 `patch_section` 这一个写工具（加只读的 read_skill / read_ledger）；**没有检索工具**——\n\
         资料已经在账本里了，照着账本写，不许编造。\n\n\
         ## 交付物\n\
         整篇按下面的骨架写：**每章一次 `patch_section` 调用**，锚点就是该章的标题行。\n\
         文档头（标题 / 说明行 / 题记）作为「第 0 章」先写，锚点是标题 `# …`。\n\
         `patch_section.new_markdown` 必须**以锚点开头**，只含这一章（含它的小节与正文）。\n\n\
         ## 骨架\n{skeleton}\n\n\
         ## 事实账本（事实只能来自这里；不在账本里的写 `待核实`）\n{ledger_md}\n\n\
         ## 硬规则\n\
         中文第三人称；每句落到具体对象；事实 / 推断 / 评价分开；查不到写 `待核实`（只这三个字）。\n\
         禁套话（据说 / 广受好评 / 神作 / 天花板 / 不容错过）。第 5 章是「{c5}」。\n\
         题记块**内不许有空行**。参考资料章每一条都要标账本来源 `- [Sn] …`。\n\
         长度服从信息量，按基准密度写（游戏侧基准约 7000–12000 汉字）。\n\n\
         ## 工作方式\n\
         1) 需要时先 read_skill（wiki-write / wiki-skeleton / 该介质那份 / epigraph）。\n\
         2) 按骨架顺序，一章一章调 patch_section 写下去；**不要只把正文写在回复文字里**。\n\
         3) 宿主每轮会告诉你现在写哪一章、锚点是什么；写完这一章就进入下一章。\n\n\
         ## 技能目录（read_skill 取正文）\n{catalog}",
        skeleton = skeleton,
        ledger_md = ledger.render(),
        c5 = c5,
    )
}

/// 通用 harness 的单轮工具步数上限（`ai.md §1`：够用就收，别空转）。
pub const ATLAS_MAX_ITERS: u32 = 48;

/// Atlas 通用 harness 的系统提示（D71）：身份 + 能做什么 + **怎么判断答还是做** + 纪律 + 技能目录。
///
/// 通用 harness 只有一个循环、一套工具面——**是答是写由模型按用户的话判断**，
/// 不由 UI 的"模式开关"决定（对齐 dsh / Codex / OpenCode：模式只引导行为，不切换能力面）。
pub fn atlas_prompt(
    skills: &[Skill],
    node_title: &str,
    doc_mode: &str,
    can_write: bool,
) -> String {
    let catalog = l0_catalog(skills);
    let mode = match (doc_mode, can_write) {
        ("proofread", true) => {
            "校对：可以调用写工具，但默认只做事实核对、一致性检查、措辞/格式纠错。用户没有要求改时，报告问题；明确说“修/改”时才落修改。不要借校对扩写整篇。"
        }
        (_, false) => {
            "只读 / one-shot：所有写工具都不在工具面里；即使模型误调用也会被动作层拒绝。"
        }
        _ => "编辑：可以读，也可以改。先判断用户是在问问题，还是明确要求你写/改；不要显式要求用户切换问答模式。",
    };
    format!(
        "你是 Atlas，Atlas 的馆员，为一个以作品为节点、以 wiki 文档为真源的中文 ACG 资料库工作。\n\
         当前目标是《{node_title}》。你是一个能使用工具的 agent，不是只会给建议的聊天机器人。\n\n\
         ## 当前运行约束\n\
         - 模式：{mode}\n\
         - 联网：始终可用。需要外部资料时直接调用 `search_web` / `fetch_url`；\
           外部来源只作证据，不替代库内真源。\n\
         - 单轮最多 {n} 步。工具描述已经写明边界；先读相关工具，再决定动作，不要靠猜测补参数。\n\n\
         ## 工作方式\n\
         1. 先判断用户要的是「回答」还是「改变库」。两者可以并存，但改变必须由用户意图支撑。\n\
         2. 查节点先用 `search_works` / `get_tree` / `list_works`；拿不准名字时不要猜 id。\n\
         3. 回答事实优先读 `read_ledger`，再读 `read_node` / `read_doc`；没有证据就说没有。\n\
         4. 要动正文时，能局部改就不整篇写：`edit` → `patch_section(s)` → `write_content` → `write_wiki`，按改动范围从小到大选。\n\
         5. 结构操作直接用对应工具：节点元数据、关系、媒体链、综合页、归档各有专用工具；不要把结构修改塞进正文。\n\
        6. 搜索结果必须保留 URL；网页内容与库内资料冲突时显式写出分歧。\n\n\
         ## 写库纪律\n\
         - 正文工具永远只落草稿，不直接定稿。\n\
         - 空写、异常截断、归档节点写入会被代码拒绝；被拒绝后换正确动作，不要原样重试。\n\
         - 批量关系与批量章节先预检，失败即整批拒绝；不要绕过失败逐条硬塞。\n\
         - 删除默认理解为归档；解链保留文档；重挂保留旧链。用户没说删，就不要做不可恢复操作。\n\
         - 多步任务先用 `todo_write` 列清单，完成一步就更新；不要只把计划说给用户而不执行。\n\n\
         ## 交付\n\
         - 回答用 `answer`，`refs` 只放真正读过并支撑结论的节点 id。\n\
         - 缺少会改变结果的必要信息才用 `ask_user`；能自己查到的不要问用户。\n\
         - 任务完成且无额外说明时用 `finish`。不要用冗长过程叙述代替工具结果。\n\n\
         ## 技能目录（需要正文时用 read_skill；不要一次读全部）\n{catalog}",
        n = ATLAS_MAX_ITERS,
    )
}

/// 把骨架渲染成模型好读的清单。
pub fn render_outline(o: &Outline) -> String {
    let mut s = String::new();
    s.push_str(&format!("{}\n\n> {}\n\n", o.title.trim(), o.note.trim()));
    if let Some(ep) = &o.epigraph {
        s.push_str(&format!("题记写法：{:?}（来源 {}）\n\n", ep.kind, ep.source_id));
    }
    for c in &o.chapters {
        s.push_str(&format!("## {}. {}\n", c.no, c.name));
        for sec in &c.sections {
            let rows: Vec<String> = sec.ledger_rows.iter().map(|r| format!("#{r}")).collect();
            s.push_str(&format!("  - {}（账本 {}）\n", sec.name, rows.join(",")));
        }
    }
    s
}
/// 介质 → 技能名。
pub fn media_skill(medium: Medium) -> &'static str {
    match medium {
        Medium::Game => "media-game",
        Medium::Movie | Medium::Tv => "media-video",
        Medium::Manga | Medium::Book => "media-book",
        Medium::Other => "wiki-write",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::Status;

    #[test]
    fn 阶段名来回() {
        assert_eq!(Phase::parse("research"), Some(Phase::Research));
        assert_eq!(Phase::Outline.as_str(), "outline");
        assert_eq!(Phase::Generation.label(), "生成");
        assert_eq!(Phase::parse("nope"), None);
    }

    #[test]
    fn 研究提示带必填清单与预算() {
        let p = research_prompt(&[], Medium::Game, "空之轨迹 FC", 24, 16);
        assert!(p.contains("首发日期"));
        assert!(p.contains("玩法"));
        assert!(p.contains("≤ 24 次"));
        assert!(p.contains("只查资料"));
    }

    #[test]
    fn 骨架提示要求每个小节挂账本行() {
        let mut l = Ledger::default();
        l.put("首发日期", "2006", vec!["S1".into()], Status::Verified);
        let p = outline_prompt(&[], Medium::Game, "空之轨迹 FC", &l);
        assert!(p.contains("每个小节至少挂 1 条账本行"));
        assert!(p.contains("## 断言"));
    }

    #[test]
    fn 骨架提示注入标准章名() {
        // 章名从代码注入（D24）：模型照抄，别自己猜（真跑实测：猜成「玩法」被门拦）
        let p = outline_prompt(&[], Medium::Game, "空之轨迹 FC", &Ledger::default());
        assert!(p.contains("5 玩法与结构特色"), "游戏介质第 5 章名要注入");
        assert!(p.contains("2 基础信息速览"));
        // 换介质 → 换名字
        let pm = outline_prompt(&[], Medium::Manga, "某漫画", &Ledger::default());
        assert!(pm.contains("创作特色"));
        assert!(!pm.contains("玩法与结构特色"));
    }

    #[test]
    fn 生成提示没有检索工具且带骨架() {
        let o = Outline::parse(r#"{"title":"T（游戏）Wiki","note":"n","chapters":[{"no":1,"name":"作品概览","sections":[{"name":"s","ledger_rows":[1]}]}]}"#).unwrap();
        let p = generation_prompt(&[], Medium::Game, "空之轨迹 FC", &o, &Ledger::default());
        assert!(p.contains("没有检索工具"));
        assert!(p.contains("## 1. 作品概览"));
        assert!(p.contains("玩法与结构特色"));
    }

    #[test]
    fn atlas提示同时讲清答与写() {
        let p = atlas_prompt(&[], "空之轨迹 FC", "edit", true);
        assert!(p.contains("answer"));
        assert!(p.contains("write_wiki"));
        assert!(p.contains("判断用户要的是"));
    }

    #[test]
    fn 校对模式保留写工具但默认只报告() {
        let p = atlas_prompt(&[], "空之轨迹 FC", "proofread", true);
        assert!(p.contains("校对"));
        assert!(p.contains("用户没有要求改时，报告问题"));
    }
}

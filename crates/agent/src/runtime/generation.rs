//! 生成段（`docs/design.md §2.5 §3.2`）：**按章写**，一章一次 `patch_section`。
//!
//! 这一段**没有检索工具**——资料都在账本里；写不出来也不许再去查（从结构上不可能"只查不写"）。
//! 宿主按骨架顺序逐章驱动，单章失败只损失一章；流式体验是"一章一章长出来"。

use crate::client::ResponsesClient;
use crate::ledger::Ledger;
use crate::phases::{self, Phase};
use crate::runtime::common::{emit, push_call, run_turn, DraftProjector, ToolCall};
use crate::skeleton::Outline;
use crate::{citations, prompt, quality, splice, tools, Result};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain::Medium;
use wikiatlas_store::content;

/// 单章最多试几轮（读技能 + 写；防原地绕圈）。
const MAX_TARGET_ITERS: u32 = 6;

/// 生成段结果。
pub struct GenOutcome {
    pub body: String,
    pub quality: quality::Report,
    pub citations: citations::CitationGate,
    pub wrote: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    /// 文档头（标题 / 说明行 / 题记）。
    Head,
    Chapter(u32),
}

pub async fn run(
    pool: &PgPool,
    client: &ResponsesClient,
    run_id: &str,
    node_id: Uuid,
    medium: Medium,
    node_title: &str,
    outline: &Outline,
    ledger: &Ledger,
    initial_body: String,
    base_ver: i32,
    loaded_skills: &[String],
) -> Result<GenOutcome> {
    let mut body = initial_body;
    let mut wrote = false;

    // 文档头先写（第 0 章），再按骨架顺序逐章
    let mut targets = vec![Target::Head];
    targets.extend(outline.chapters.iter().map(|c| Target::Chapter(c.no)));

    for t in &targets {
        crate::runtime::common::ensure_not_cancelled(pool, run_id).await?;
        let ok = write_target(pool, client, run_id, node_id, medium, node_title, outline, ledger, *t, &mut body, base_ver, None, loaded_skills).await?;
        wrote |= ok;
    }

    // 质量门 + 引用门；不过就只重写不过的章一次（`design.md §2.5`）
    let mut q = quality::check(medium, &body, quality::MIN_CHARS_FULL);
    let mut c = citations::check(&body, ledger);
    if !q.passed() || !c.passed() {
        let fix = targets_for(&q, &c, outline, &body);
        let note = correction_note(&q, &c);
        for t in &fix {
            crate::runtime::common::ensure_not_cancelled(pool, run_id).await?;
            write_target(pool, client, run_id, node_id, medium, node_title, outline, ledger, *t, &mut body, base_ver, Some(note.clone()), loaded_skills).await?;
        }
        q = quality::check(medium, &body, quality::MIN_CHARS_FULL);
        c = citations::check(&body, ledger);
    }

    emit(pool, run_id, "wikiatlas.quality", &serde_json::to_value(&q).unwrap_or(Value::Null)).await?;
    emit(pool, run_id, "wikiatlas.citations", &serde_json::to_value(&c).unwrap_or(Value::Null)).await?;

    Ok(GenOutcome { body, quality: q, citations: c, wrote })
}

/// 写一章：按锚点调 `patch_section`，流式落草稿。
#[allow(clippy::too_many_arguments)]
async fn write_target(
    pool: &PgPool,
    client: &ResponsesClient,
    run_id: &str,
    node_id: Uuid,
    medium: Medium,
    node_title: &str,
    outline: &Outline,
    ledger: &Ledger,
    target: Target,
    body: &mut String,
    base_ver: i32,
    correction: Option<String>,
    loaded_skills: &[String],
) -> Result<bool> {
    let anchor = anchor_of(outline, target);
    let label = match target {
        Target::Head => "文档头（标题 / 说明行 / 题记）".to_string(),
        Target::Chapter(no) => format!("第 {no} 章"),
    };
    let skills = prompt::load(&prompt::skills_root());
    let mut sys = phases::generation_prompt(&skills, medium, node_title, outline, ledger);
    let loaded = prompt::loaded_skill_prompt(&skills, loaded_skills);
    if !loaded.is_empty() {
        sys.push_str("\n\n");
        sys.push_str(&loaded);
    }
    let correction_line = correction
        .map(|c| format!("\n\n⚠️ 这一章上一轮没过质量门，请重写并逐条改掉：{c}"))
        .unwrap_or_default();
    let user = format!(
        "请写{label}：目标节点 id 是 `{node_id}`（`patch_section.node_id` 必须填它）。\
         锚点必须是「{anchor}」，`patch_section.new_markdown` 必须以它开头、只含这一章。\
         不要写别的章，也不要把正文写在回复文字里。{correction_line}"
    );
    let mut input = json!([
        { "role": "system", "content": [{ "type": "input_text", "text": sys }] },
        { "role": "user", "content": [{ "type": "input_text", "text": user }] }
    ]);

    let by = format!("llm:{run_id}");
    emit(pool, run_id, "wikiatlas.phase", &json!({ "phase": Phase::Generation.as_str(), "label": Phase::Generation.label(), "target": anchor })).await?;

    for it in 0..MAX_TARGET_ITERS {
        let choice = if it == 0 { "required" } else { "auto" };
        let mut proj = Some(DraftProjector::new(pool, node_id, base_ver, by.clone(), body.clone()));
        let turn = run_turn(pool, run_id, client, input.clone(), tools::generation_tool_defs(), choice, &mut proj).await?;

        let mut patched = false;
        for call in &turn.calls {
            let out = apply_call(pool, node_id, body, base_ver, &by, run_id, ledger, call, &mut patched).await;
            emit(
                pool,
                run_id,
                "wikiatlas.tool.result",
                &json!({ "name": call.name, "ok": out.get("ok"), "reason": out.get("reason") }),
            )
            .await?;
            push_call(&mut input, call, &out);
        }
        if patched {
            return Ok(true);
        }
    }
    emit(pool, run_id, "wikiatlas.notice", &json!({ "text": format!("{label} 这一轮没写出来，跳过（草稿保底）") })).await?;
    Ok(false)
}

/// 处理一个生成段工具调用；`patched` 置 true 表示这一章写成了。
#[allow(clippy::too_many_arguments)]
async fn apply_call(
    pool: &PgPool,
    node_id: Uuid,
    body: &mut String,
    base_ver: i32,
    by: &str,
    _run_id: &str,
    ledger: &Ledger,
    call: &ToolCall,
    patched: &mut bool,
) -> Value {
    match call.name.as_str() {
        tools::READ_SKILL_TOOL => tools::call_read_skill(&call.arguments),
        tools::READ_LEDGER_TOOL => tools::call_read_ledger(ledger),
        tools::PATCH_SECTION_TOOL => {
            #[derive(serde::Deserialize)]
            struct Args {
                #[serde(default)]
                node_id: String,
                anchor: String,
                new_markdown: String,
            }
            let args: Args = match serde_json::from_str(&call.arguments) {
                Ok(a) => a,
                Err(e) => return json!({ "ok": false, "reason": format!("参数不是合法 JSON：{e}") }),
            };
            if !args.node_id.is_empty() && args.node_id != node_id.to_string() {
                return json!({ "ok": false, "reason": "只允许写当前工单的目标节点" });
            }
            match splice::apply(body, &args.anchor, &args.new_markdown) {
                Ok(new_body) => {
                    if let Err(e) = content::put_draft(pool, node_id, &new_body, base_ver, by).await {
                        return json!({ "ok": false, "reason": format!("落草稿失败：{e}") });
                    }
                    *body = new_body;
                    *patched = true;
                    json!({ "ok": true, "wrote": true, "anchor": args.anchor, "chars": body.chars().count() })
                }
                Err(e) => json!({ "ok": false, "reason": e }),
            }
        }
        other => json!({ "ok": false, "reason": format!("生成段没有这个工具：{other}") }),
    }
}

/// 这一章（或文档头）的锚点。
fn anchor_of(outline: &Outline, target: Target) -> String {
    match target {
        Target::Head => {
            let t = outline.title.trim();
            if t.starts_with("# ") {
                t.to_string()
            } else if t.is_empty() {
                "# 作品 Wiki".to_string()
            } else {
                format!("# {t}")
            }
        }
        Target::Chapter(no) => outline
            .chapters
            .iter()
            .find(|c| c.no == no)
            .map(|c| format!("## {}. {}", c.no, c.name))
            .unwrap_or_else(|| format!("## {no}. ")),
    }
}

/// 哪些章要回炉（不过门时）。
fn targets_for(q: &quality::Report, c: &citations::CitationGate, outline: &Outline, body: &str) -> Vec<Target> {
    let mut out: Vec<Target> = vec![];
    let mut all = false;
    for i in q.issues.iter().filter(|i| i.hard) {
        match i.code {
            "title" | "note" | "epigraph" => push_unique(&mut out, Target::Head),
            "chapter5" => push_unique(&mut out, Target::Chapter(5)),
            "chapters" => match first_chapter_no(&i.message) {
                Some(n) => push_unique(&mut out, Target::Chapter(n)),
                None => all = true,
            },
            _ => all = true, // banned / length / 其他全局项 → 整篇回炉
        }
    }
    if !c.issues.is_empty() {
        match references_chapter_no(body) {
            Some(n) => push_unique(&mut out, Target::Chapter(n)),
            None => all = true,
        }
    }
    if all || out.is_empty() {
        return std::iter::once(Target::Head).chain(outline.chapters.iter().map(|c| Target::Chapter(c.no))).collect();
    }
    out
}

fn push_unique(v: &mut Vec<Target>, t: Target) {
    if !v.contains(&t) {
        v.push(t);
    }
}

/// 从「缺第 4 章」这类消息里抠出章号。
fn first_chapter_no(msg: &str) -> Option<u32> {
    let i = msg.find('第')?;
    let rest = msg[i + '第'.len_utf8()..].trim_start();
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// 正文里「参考资料」章的编号。
fn references_chapter_no(body: &str) -> Option<u32> {
    for l in body.lines() {
        if let Some(rest) = l.strip_prefix("## ") {
            if rest.contains("参考资料") {
                let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                return num.parse().ok();
            }
        }
    }
    None
}

fn correction_note(q: &quality::Report, c: &citations::CitationGate) -> String {
    let mut parts: Vec<String> = q.hard_messages();
    parts.extend(c.issues.clone());
    parts.join("；")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quality;

    #[test]
    fn 锚点按骨架生成() {
        let o = Outline::parse(
            r##"{"title":"# 空之轨迹 FC（游戏）Wiki","note":"n","chapters":[{"no":1,"name":"作品概览","sections":[]}]}"##,
        )
        .unwrap();
        assert_eq!(anchor_of(&o, Target::Head), "# 空之轨迹 FC（游戏）Wiki");
        assert_eq!(anchor_of(&o, Target::Chapter(1)), "## 1. 作品概览");
    }

    #[test]
    fn 从消息里抠章号() {
        assert_eq!(first_chapter_no("缺第 4 章（`## 4. …`）"), Some(4));
        assert_eq!(first_chapter_no("章号跳号：3 → 5"), None);
    }

    #[test]
    fn 质量门不过时映射到对应章() {
        let o = Outline::parse(
            r#"{"title":"T","note":"n","chapters":[
                {"no":1,"name":"作品概览","sections":[]},
                {"no":2,"name":"速览","sections":[]},
                {"no":5,"name":"叙事与影像特色","sections":[]}]}"#,
        )
        .unwrap();
        // 缺章号的问题 → 只回炉那一章
        let body = "## 1. 作品概览\n\n## 2. 速览\n";
        let q = quality::Report { issues: vec![quality::Issue { code: "chapters", message: "缺第 5 章".into(), hard: true }], chars: 0, chapters: vec![] };
        let c = citations::CitationGate { references: 0, issues: vec![], reminders: vec![] };
        let t = targets_for(&q, &c, &o, body);
        assert_eq!(t, vec![Target::Chapter(5)]);
    }

    #[test]
    fn 全局项触发整篇回炉() {
        let o = Outline::parse(r#"{"title":"T","chapters":[{"no":1,"name":"a","sections":[]}]}"#).unwrap();
        let q = quality::Report { issues: vec![quality::Issue { code: "length", message: "太短".into(), hard: true }], chars: 0, chapters: vec![] };
        let c = citations::CitationGate { references: 0, issues: vec![], reminders: vec![] };
        let t = targets_for(&q, &c, &o, "");
        assert_eq!(t, vec![Target::Head, Target::Chapter(1)]);
    }
}

//! 骨架段（`docs/design.md §2.4`）：一次结构化输出，**无工具**。
//!
//! 产出 JSON 骨架 → 代码跑骨架门 → 不过回模型一次；再不过就**宿主用确定路径兜底**
//! （`design.md §2.4` 的兜底纪律：关键产物即使模型漏了，也不能卡死整条链）。

use crate::client::ResponsesClient;
use crate::ledger::Ledger;
use crate::phases::{self, Phase};
use crate::runtime::common::{emit, push_item, run_turn};
use crate::skeleton::{Chapter, Epigraph, EpigraphKind, Outline, Section};
use crate::{prompt, Result};
use serde_json::json;
use sqlx::PgPool;
use wikiatlas_domain::Medium;

/// 跑骨架段。返回一份**一定可用**的骨架（兜底路径保证非空）。
pub async fn run(
    pool: &PgPool,
    client: &ResponsesClient,
    run_id: &str,
    medium: Medium,
    node_title: &str,
    ledger: &Ledger,
    loaded_skills: &[String],
) -> Result<Outline> {
    let skills = prompt::load(&prompt::skills_root());
    let mut sys = phases::outline_prompt(&skills, medium, node_title, ledger);
    let loaded = prompt::loaded_skill_prompt(&skills, loaded_skills);
    if !loaded.is_empty() {
        sys.push_str("\n\n");
        sys.push_str(&loaded);
    }
    let mut input = json!([
        { "role": "system", "content": [{ "type": "input_text", "text": sys }] },
        { "role": "user", "content": [{ "type": "input_text",
            "text": "请只输出骨架 JSON（不要写正文、不要调用工具）。" }] }
    ]);

    let mut last_err = String::new();
    for attempt in 0..2 {
        if attempt > 0 {
            push_item(
                &mut input,
                json!({ "role": "user", "content": [{ "type": "input_text", "text": format!(
                    "上一版骨架没过宿主骨架门：{last_err}。请只输出改正后的 JSON。"
                ) }] }),
            );
        }
        // 无工具的一轮（tools 空数组 → client 不发 tools 字段）
        let turn = run_turn(pool, run_id, client, input.clone(), json!([]), "", &mut None).await?;
        match Outline::parse(&turn.text) {
            Ok(o) => {
                let gate = o.gate(medium, ledger);
                emit(
                    pool,
                    run_id,
                    "wikiatlas.outline",
                    &json!({ "phase": Phase::Outline.as_str(), "passed": gate.passed(), "chapters": gate.chapters, "issues": gate.issues }),
                )
                .await?;
                if gate.passed() {
                    return Ok(o);
                }
                last_err = gate.issues.join("；");
            }
            Err(e) => last_err = e,
        }
    }

    // 兜底：宿主按标准骨架造一份（每节挂全部账本行，保证能过骨架门的"有事实支撑"这条）
    let rows: Vec<u32> = ledger.entries.iter().map(|e| e.no).collect();
    let o = fallback_outline(medium, node_title, rows);
    emit(
        pool,
        run_id,
        "wikiatlas.notice",
        &json!({ "text": format!("骨架没产出合格版本，宿主用标准骨架兜底（原因：{last_err}）") }),
    )
    .await?;
    Ok(o)
}

/// 宿主兜底骨架：标准 1–7 + 总结 + 参考资料，每节挂全部账本行。
pub fn fallback_outline(medium: Medium, node_title: &str, rows: Vec<u32>) -> Outline {
    let names = crate::skeleton::canonical_chapters(medium);
    let mut chapters = vec![];
    for (i, name) in names.iter().enumerate() {
        let no = (i + 1) as u32;
        chapters.push(Chapter {
            no,
            name: name.to_string(),
            sections: vec![Section { name: name.to_string(), ledger_rows: rows.clone() }],
        });
    }
    Outline {
        title: format!("# {node_title}（{}）Wiki", medium_label(medium)),
        note: "本文依据公开资料整理，含剧透。检索时间以账本为准。".into(),
        epigraph: Some(Epigraph { kind: EpigraphKind::Scene, source_id: String::new() }),
        chapters,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 兜底骨架章名按介质且能过骨架门() {
        let mut l = Ledger::default();
        for i in 1..=5 {
            l.put(&format!("字段{i}"), "值", vec![], crate::ledger::Status::Verified);
        }
        let rows: Vec<u32> = l.entries.iter().map(|e| e.no).collect();
        let o = fallback_outline(Medium::Game, "空之轨迹 FC", rows);
        assert_eq!(o.chapters.len(), 9);
        assert_eq!(o.chapters[2].name, "世界观与背景设定");
        assert_eq!(o.chapters[4].name, "玩法与结构特色");
        assert!(o.chapters.iter().all(|c| !c.sections.is_empty()));
        // 章号 1..=9 连续
        let nos: Vec<u32> = o.chapters.iter().map(|c| c.no).collect();
        assert_eq!(nos, (1..=9).collect::<Vec<u32>>());
        // 骨架门应通过（兜底行都存在）
        let g = o.gate(Medium::Game, &l);
        assert!(g.passed(), "{:?}", g.issues);
    }
}

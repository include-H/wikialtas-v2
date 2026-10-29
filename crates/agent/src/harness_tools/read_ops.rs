//! 通用 Harness 的额外读工具实现。

use crate::{prompt, tools};
use serde_json::{json, Value};
use sqlx::PgPool;
use wikiatlas_domain::library::Source;
use wikiatlas_store::{library, runs};

const SOURCE_MAX_CHARS: usize = 12_000;

fn bad(reason: impl Into<String>) -> Value {
    json!({ "ok": false, "reason": reason.into() })
}

pub async fn dispatch(
    pool: &PgPool,
    name: &str,
    arguments: &str,
) -> Option<Value> {
    Some(match name {
        "list_skills" => list_skills(),
        "read_source" => read_source(arguments),
        "list_library" => list_library(pool, arguments).await,
        "search_sessions" => search_sessions(pool, arguments).await,
        "read_session" => read_session(pool, arguments).await,
        "list_relation_types" => crate::harness_tools::relation_ops::dispatch(
            pool,
            "list_relation_types",
            arguments,
            "",
        )
        .await
        .unwrap_or_else(|| bad("关系类型工具没有实现")),
        "search_web" => tools::call_web_search(arguments).await,
        "fetch_url" => tools::call_web_fetch(arguments).await,
        _ => return None,
    })
}

fn list_skills() -> Value {
    let rows: Vec<Value> = prompt::load_all(&prompt::skills_root())
        .into_iter()
        .map(|s| json!({ "name": s.name, "description": s.description }))
        .collect();
    json!({ "ok": true, "count": rows.len(), "skills": rows })
}

fn read_source(arguments: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct Args {
        path: String,
        #[serde(default)]
        section: String,
    }
    let args: Args = match serde_json::from_str(arguments) {
        Ok(v) => v,
        Err(e) => return bad(format!("参数不是合法 JSON：{e}")),
    };
    let root = prompt::samples_root();
    let rel = args
        .path
        .strip_prefix("skills/atlas/references/samples/")
        .or_else(|| args.path.strip_prefix("skills/samples/"));
    let Some(rel) = rel else {
        return bad("read_source 只能读 skills/atlas/references/samples/ 下的参考成品");
    };
    let wanted = root.join(rel);
    let Ok(root) = root.canonicalize() else {
        return bad("找不到 skills/atlas/references/samples");
    };
    let Ok(path) = wanted.canonicalize() else {
        return bad(format!("文件不存在：{}", args.path));
    };
    if !path.starts_with(&root) {
        return bad("路径越界");
    }
    let Ok(text) = std::fs::read_to_string(&path) else {
        return bad("这个文件不是 UTF-8 文本");
    };
    let text = if args.section.trim().is_empty() {
        text
    } else {
        extract_section(&text, args.section.trim())
    };
    let total = text.chars().count();
    let clipped: String = text.chars().take(SOURCE_MAX_CHARS).collect();
    let mut out = json!({ "ok": true, "path": args.path, "chars": total, "text": clipped });
    if total > SOURCE_MAX_CHARS {
        out["truncated"] = json!(true);
        out["notice"] = json!(format!(
            "文件共 {total} 字，已截断到 {SOURCE_MAX_CHARS} 字；需要全文请缩小 section"
        ));
    }
    out
}

fn extract_section(text: &str, wanted: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let Some(start) = lines
        .iter()
        .position(|l| l.starts_with('#') && l.contains(wanted))
    else {
        return format!("没有找到包含「{wanted}」的标题。");
    };
    let level = lines[start].chars().take_while(|c| *c == '#').count();
    let mut out = String::new();
    for line in &lines[start..] {
        let l = line.chars().take_while(|c| *c == '#').count();
        if !out.is_empty() && l > 0 && l <= level {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

async fn list_library(pool: &PgPool, arguments: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct Args {
        #[serde(default)]
        source: String,
        #[serde(default)]
        has_doc: Option<bool>,
        #[serde(default)]
        include_missing: bool,
        #[serde(default)]
        limit: Option<i64>,
    }
    let args: Args = match serde_json::from_str(arguments) {
        Ok(v) => v,
        Err(e) => return bad(format!("参数不是合法 JSON：{e}")),
    };
    let source = if args.source.trim().is_empty() {
        None
    } else {
        match Source::parse(&args.source) {
            Ok(s) => Some(s),
            Err(_) => return bad(format!("未知来源：{}", args.source)),
        }
    };
    let filter = library::ManifestFilter {
        source,
        container_key: None,
        has_doc: args.has_doc,
        include_missing: args.include_missing,
        limit: args.limit.unwrap_or(100),
    };
    match library::list_pool(pool, &filter).await {
        Ok(rows) => json!({ "ok": true, "count": rows.len(), "entries": rows }),
        Err(e) => bad(format!("读媒体库池子失败：{e}")),
    }
}

async fn search_sessions(pool: &PgPool, arguments: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct Args {
        query: String,
        #[serde(default)]
        limit: Option<i64>,
    }
    let args: Args = match serde_json::from_str(arguments) {
        Ok(v) => v,
        Err(e) => return bad(format!("参数不是合法 JSON：{e}")),
    };
    match runs::search_sessions(pool, &args.query, args.limit.unwrap_or(10)).await {
        Ok(rows) => json!({ "ok": true, "count": rows.len(), "sessions": rows }),
        Err(e) => bad(format!("搜索会话失败：{e}")),
    }
}

async fn read_session(pool: &PgPool, arguments: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct Args {
        session_id: String,
        #[serde(default)]
        limit: Option<i64>,
    }
    let args: Args = match serde_json::from_str(arguments) {
        Ok(v) => v,
        Err(e) => return bad(format!("参数不是合法 JSON：{e}")),
    };
    match runs::read_session(pool, &args.session_id, args.limit.unwrap_or(40)).await {
        Ok(rows) => json!({
            "ok": true,
            "count": rows.len(),
            "messages": rows.into_iter().map(|m| json!({
                "run_id": m.run_id, "turn": m.turn, "role": m.role,
                "content": m.content, "tool_name": m.tool_name
            })).collect::<Vec<_>>()
        }),
        Err(e) => bad(format!("读会话失败：{e}")),
    }
}

//! 读工具 / 控制工具的实现（`read_tools` 的「做」那一半）。
//!
//! 全是只读（`answer` 只是交付文本 + 校验引用节点存在）；没有一条会改库。

use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use wikiatlas_domain::{Medium, Node, NodeKind, NodeStatus};
use wikiatlas_store::{content, ledger, nodes};

/// 一次读工具返回的节点数 / 正文字数上限（护栏「截断必须说话」）。
const TREE_LIMIT: usize = 300;
const READ_MAX_CHARS: usize = 12_000;

fn bad(reason: impl Into<String>) -> Value {
    json!({ "ok": false, "reason": reason.into() })
}

fn node_json(n: &Node) -> Value {
    json!({
        "id": n.id,
        "title": n.title,
        "kind": n.kind.as_str(),
        "medium": n.medium.map(|m| m.as_str()),
        "status": n.status.as_str(),
        "parent_id": n.parent_id,
        "aliases": n.aliases,
    })
}

pub(super) async fn get_tree(pool: &PgPool, arguments: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct Args {
        #[serde(default)]
        root_id: String,
    }
    let args: Args = match serde_json::from_str(arguments) {
        Ok(a) => a,
        Err(e) => return bad(format!("参数不是合法 JSON：{e}")),
    };
    let all = match nodes::tree(pool).await {
        Ok(v) => v,
        Err(e) => return bad(format!("读树失败：{e}")),
    };
    let mut rows: Vec<&Node> = all.iter().collect();
    if !args.root_id.trim().is_empty() {
        let root = match Uuid::parse_str(args.root_id.trim()) {
            Ok(id) => id,
            Err(_) => return bad("root_id 不是合法 uuid"),
        };
        let mut keep = vec![root];
        // 逐层收集子树（tree() 是扁平返回，父在前）
        loop {
            let mut added = false;
            for n in &all {
                if let Some(p) = n.parent_id {
                    if keep.contains(&p) && !keep.contains(&n.id) {
                        keep.push(n.id);
                        added = true;
                    }
                }
            }
            if !added {
                break;
            }
        }
        rows.retain(|n| keep.contains(&n.id));
    }
    let truncated = rows.len() > TREE_LIMIT;
    rows.truncate(TREE_LIMIT);
    let mut out = json!({
        "ok": true,
        "count": rows.len(),
        "nodes": rows.iter().map(|n| node_json(n)).collect::<Vec<_>>(),
    });
    if truncated {
        out["truncated"] = json!(true);
        out["notice"] = json!(format!("树太大，只回了前 {TREE_LIMIT} 个节点"));
    }
    out
}

pub(super) async fn list_works(pool: &PgPool, arguments: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct Args {
        #[serde(default)]
        medium: String,
        #[serde(default)]
        status: String,
        #[serde(default)]
        limit: Option<i64>,
    }
    let args: Args = match serde_json::from_str(arguments) {
        Ok(a) => a,
        Err(e) => return bad(format!("参数不是合法 JSON：{e}")),
    };
    let medium = match args.medium.trim() {
        "" => None,
        s => match Medium::parse(s) {
            Ok(m) => Some(m),
            Err(_) => return bad(format!("未知介质：{s}")),
        },
    };
    let status = match args.status.trim() {
        "" => None,
        s => match NodeStatus::parse(s) {
            Ok(v) => Some(v),
            Err(_) => return bad(format!("未知状态：{s}")),
        },
    };
    match nodes::list_works(pool, medium, status, args.limit.unwrap_or(30)).await {
        Ok(list) => json!({
            "ok": true,
            "count": list.len(),
            "works": list.iter().map(node_json).collect::<Vec<_>>(),
        }),
        Err(e) => bad(format!("列作品失败：{e}")),
    }
}

pub(super) async fn search(pool: &PgPool, arguments: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct Args {
        query: String,
        #[serde(default)]
        limit: Option<i64>,
    }
    let args: Args = match serde_json::from_str(arguments) {
        Ok(a) => a,
        Err(e) => return bad(format!("参数不是合法 JSON：{e}")),
    };
    if args.query.trim().is_empty() {
        return bad("query 不能为空");
    }
    match wikiatlas_store::search::search(pool, &args.query, args.limit.unwrap_or(10)).await {
        Ok(hits) => json!({
            "ok": true,
            "count": hits.len(),
            "nodes": hits.iter().map(|h| json!({
                "id": h.node_id,
                "title": h.title,
                "kind": h.kind,
                "medium": h.medium,
                "status": h.status,
                "snippet": h.snippet,
            })).collect::<Vec<_>>(),
        }),
        Err(e) => bad(format!("检索失败：{e}")),
    }
}

/// `read_node`（`doc_only=false`）/ `read_doc`（`doc_only=true`）：都返回**定稿正文**（超长截断）。
pub(super) async fn read_node(pool: &PgPool, arguments: &str, doc_only: bool) -> Value {
    #[derive(serde::Deserialize)]
    struct Args {
        node_id: String,
    }
    let args: Args = match serde_json::from_str(arguments) {
        Ok(a) => a,
        Err(e) => return bad(format!("参数不是合法 JSON：{e}")),
    };
    let Ok(id) = Uuid::parse_str(args.node_id.trim()) else {
        return bad("node_id 不是合法 uuid");
    };
    let detail = match content::read(pool, id).await {
        Ok(d) => d,
        Err(e) => return bad(format!("读节点失败：{e}")),
    };
    if doc_only && detail.node.kind != NodeKind::Doc {
        return bad(format!(
            "《{}》不是资料节点（种类 {}）；作品正文用 read_node",
            detail.node.title,
            detail.node.kind.as_str()
        ));
    }
    let body = detail.content.as_ref().map(|c| c.body_md.clone()).unwrap_or_default();
    let total = body.chars().count();
    let text = chars_take(&body, READ_MAX_CHARS);
    let truncated = total > READ_MAX_CHARS;
    let mut out = json!({
        "ok": true,
        "node": node_json(&detail.node),
        "ver": detail.content.as_ref().map(|c| c.ver).unwrap_or(0),
        "has_draft": detail.draft.is_some(),
        "chars": total,
        "body_md": text,
    });
    if truncated {
        out["truncated"] = json!(true);
        out["notice"] = json!(format!("正文共 {total} 字，已截断到 {READ_MAX_CHARS} 字——你没看到全部内容"));
    }
    out
}

pub(super) async fn read_ledger(pool: &PgPool, target: Uuid) -> Value {
    match ledger::load(pool, target).await {
        Ok(Some(md)) => json!({ "ok": true, "markdown": md }),
        Ok(None) => json!({ "ok": true, "markdown": "", "notice": "这个作品还没有事实账本（没做过研究）" }),
        Err(e) => bad(format!("读账本失败：{e}")),
    }
}

pub(super) async fn answer(pool: &PgPool, arguments: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct Args {
        text: String,
        #[serde(default)]
        refs: Vec<Value>,
    }
    let args: Args = match serde_json::from_str(arguments) {
        Ok(a) => a,
        Err(e) => return bad(format!("参数不是合法 JSON：{e}")),
    };
    if args.text.trim().is_empty() {
        return bad("text 不能为空");
    }
    let mut refs: Vec<Value> = vec![];
    for r in &args.refs {
        let id = r
            .as_str()
            .map(|s| s.to_string())
            .or_else(|| r.get("node_id").and_then(Value::as_str).map(|s| s.to_string()));
        let Some(id) = id else { continue };
        let Ok(uuid) = Uuid::parse_str(id.trim()) else { continue };
        if refs.iter().any(|x| x.get("node_id").and_then(Value::as_str) == Some(&id)) {
            continue;
        }
        if let Ok(n) = nodes::get(pool, uuid).await {
            refs.push(json!({ "node_id": n.id, "title": n.title, "kind": n.kind.as_str() }));
        }
    }
    json!({ "ok": true, "text": args.text.trim(), "refs": refs })
}

fn chars_take(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        s.chars().take(max).collect()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 截断守则() {
        assert_eq!(chars_take("abcdef", 3), "abc");
        assert_eq!(chars_take("ab", 3), "ab");
    }
}

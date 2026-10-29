//! Atlas MCP server（stdio，只读）。
//!
//! 给外部 agent 暴露检索 / 读树 / 读节点；没有写工具、没有媒体库密钥。

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = std::env::var("WIKIATLAS_DB_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .map_err(|_| anyhow::anyhow!("缺少 WIKIATLAS_DB_URL"))?;
    let pool = wikiatlas_store::connect(&url).await?;
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                write(&mut stdout, error(Value::Null, -32700, &format!("JSON 解析失败：{e}"))).await?;
                continue;
            }
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        if req.get("id").is_none() {
            continue; // notifications/initialized 等通知不回包
        }
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");
        let resp = match method {
            "initialize" => json!({
                "jsonrpc": "2.0", "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "wikiatlas-readonly", "version": "0.1.0" }
                }
            }),
            "ping" => json!({ "jsonrpc": "2.0", "id": id, "result": {} }),
            "tools/list" => json!({ "jsonrpc": "2.0", "id": id, "result": { "tools": tools() } }),
            "tools/call" => match call(&pool, &req).await {
                Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                Err(msg) => json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {
                        "content": [{ "type": "text", "text": msg }],
                        "isError": true
                    }
                }),
            },
            other => error(id, -32601, &format!("未知方法：{other}")),
        };
        write(&mut stdout, resp).await?;
    }
    Ok(())
}

async fn write(stdout: &mut tokio::io::Stdout, value: Value) -> anyhow::Result<()> {
    let mut line = serde_json::to_vec(&value)?;
    line.push(b'\n');
    stdout.write_all(&line).await?;
    stdout.flush().await?;
    Ok(())
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tools() -> Value {
    json!([
        {
            "name": "search_works",
            "description": "检索 Atlas 的标题、别名与定稿正文，返回节点 id 和原文片段。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
                },
                "required": ["query"]
            }
        },
        {
            "name": "read_node",
            "description": "按节点 id 读取元数据与定稿正文。",
            "inputSchema": {
                "type": "object",
                "properties": { "node_id": { "type": "string" } },
                "required": ["node_id"]
            }
        },
        {
            "name": "get_tree",
            "description": "读取 Atlas 节点树（扁平、父在前）。",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}

async fn call(
    pool: &sqlx::PgPool,
    req: &Value,
) -> std::result::Result<Value, String> {
    let params = req.get("params").cloned().unwrap_or_else(|| json!({}));
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let payload = match name {
        "search_works" => {
            let q = args.get("query").and_then(Value::as_str).unwrap_or("");
            let limit = args.get("limit").and_then(Value::as_i64).unwrap_or(10);
            let rows = wikiatlas_store::search::search(pool, q, limit)
                .await
                .map_err(|e| e.to_string())?;
            json!({ "results": rows })
        }
        "get_tree" => {
            let rows = wikiatlas_store::nodes::tree(pool)
                .await
                .map_err(|e| e.to_string())?;
            json!({ "nodes": rows })
        }
        "read_node" => {
            let id = args
                .get("node_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "缺 node_id".to_string())
                .and_then(|s| uuid_parse(s))?;
            let row = wikiatlas_store::content::read(pool, id)
                .await
                .map_err(|e| e.to_string())?;
            json!(row)
        }
        other => return Err(format!("没有这个只读工具：{other}")),
    };
    let text = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    Ok(json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false
    }))
}

fn uuid_parse(s: &str) -> std::result::Result<uuid::Uuid, String> {
    uuid::Uuid::parse_str(s.trim()).map_err(|e| format!("node_id 不是 UUID：{e}"))
}

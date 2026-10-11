//! SSE 拆帧 + Responses 事件映射（`docs/behavior-notes.md §5` 的入口）。
//!
//! 从 `client.rs` 拆出来（单文件 ≤300 行，D41）。**事件名是实测来的**，不是照文档猜的：
//! `response.output_item.added`（`item.type=function_call` 带 `name`/`call_id`）·
//! `response.function_call_arguments.delta/done` · `response.output_text.delta` · `response.completed`。

use serde_json::Value;

/// 一条 SSE 帧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseFrame {
    pub event: Option<String>,
    pub data: String,
}

/// SSE 拆帧器：**按 `\n\n` 切块**，块内 `event:` / `data:`（多行 data 用 `\n` 接）。
/// 网络给的是任意切分的字节块，所以必须带缓冲。
#[derive(Debug, Default)]
pub struct SseReader {
    buf: String,
}

impl SseReader {
    pub fn push(&mut self, chunk: &str) -> Vec<SseFrame> {
        self.buf.push_str(chunk);
        let mut out = vec![];
        loop {
            // 容忍 \n\n 与 \r\n\r\n
            let Some((idx, len)) = find_sep(&self.buf) else { break };
            let block = self.buf[..idx].to_string();
            self.buf.drain(..idx + len);
            if let Some(f) = parse_block(&block) {
                out.push(f);
            }
        }
        out
    }
}

fn find_sep(s: &str) -> Option<(usize, usize)> {
    let a = s.find("\n\n").map(|i| (i, 2));
    let b = s.find("\r\n\r\n").map(|i| (i, 4));
    match (a, b) {
        (Some(x), Some(y)) => Some(if x.0 <= y.0 { x } else { y }),
        (Some(x), None) => Some(x),
        (None, Some(y)) => Some(y),
        (None, None) => None,
    }
}

fn parse_block(block: &str) -> Option<SseFrame> {
    let mut event = None;
    let mut data = String::new();
    for line in block.lines() {
        if let Some(v) = line.strip_prefix("event:") {
            event = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(v.strip_prefix(' ').unwrap_or(v));
        }
    }
    if data.is_empty() && event.is_none() {
        return None;
    }
    Some(SseFrame { event, data })
}

/// 从 Responses 事件里挑我们关心的部分。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    /// 正文文本增量（模型说话，不是写工具的正文）
    TextDelta(String),
    /// 思考摘要增量（只用于展示，别进上下文）
    ReasoningDelta(String),
    /// 一个函数调用开始（拿到 name / call_id）
    FunctionCallAdded { item_id: String, name: String, call_id: String },
    /// 函数参数 JSON 的增量 —— **正文就从这里抽**（`behavior-notes.md §5`）
    ArgsDelta { item_id: String, delta: String },
    /// 参数给全了
    ArgsDone { item_id: String, name: String, arguments: String },
    /// 一轮结束（带最终 output / usage）
    Completed { output: Value, usage: Option<Value> },
    /// 其他（created / in_progress / reasoning_part…）——不入我们的库
    Other,
}

/// 按 `type` 把 data 解成我们的枚举。
pub fn parse_stream_event(data: &str) -> StreamEvent {
    let Ok(v) = serde_json::from_str::<Value>(data) else {
        return StreamEvent::Other;
    };
    let ty = v.get("type").and_then(Value::as_str).unwrap_or("");
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    match ty {
        "response.output_text.delta" => StreamEvent::TextDelta(s("delta")),
        "response.reasoning_text.delta" | "response.reasoning_summary_text.delta" => {
            StreamEvent::ReasoningDelta(s("delta"))
        }
        "response.output_item.added" => {
            let item = v.get("item").cloned().unwrap_or(Value::Null);
            if item.get("type").and_then(Value::as_str) == Some("function_call") {
                StreamEvent::FunctionCallAdded {
                    item_id: item.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
                    name: item.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                    call_id: item.get("call_id").and_then(Value::as_str).unwrap_or("").to_string(),
                }
            } else {
                StreamEvent::Other
            }
        }
        "response.function_call_arguments.delta" => StreamEvent::ArgsDelta {
            item_id: s("item_id"),
            delta: s("delta"),
        },
        "response.function_call_arguments.done" => StreamEvent::ArgsDone {
            item_id: s("item_id"),
            name: s("name"),
            arguments: s("arguments"),
        },
        "response.completed" => StreamEvent::Completed {
            output: v.get("response").and_then(|r| r.get("output")).cloned().unwrap_or(Value::Null),
            usage: v.get("response").and_then(|r| r.get("usage")).cloned(),
        },
        _ => StreamEvent::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_按空行切块_跨块缓冲() {
        let mut r = SseReader::default();
        // 前半块（不含空行）不出帧
        assert!(r.push("event: response.output_text.delta\ndata: {\"delta\":\"你").is_empty());
        let frames = r.push("\"}\n\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event.as_deref(), Some("response.output_text.delta"));
        assert_eq!(frames[0].data, "{\"delta\":\"你\"}");
    }

    #[test]
    fn sse_多行data用换行接() {
        let mut r = SseReader::default();
        let f = r.push("data: a\ndata: b\n\n");
        assert_eq!(f[0].data, "a\nb");
    }

    #[test]
    fn 事件映射_正文增量() {
        let e = parse_stream_event(r#"{"type":"response.output_text.delta","delta":"你好"}"#);
        assert_eq!(e, StreamEvent::TextDelta("你好".into()));
    }

    #[test]
    fn 事件映射_函数调用与参数增量() {
        let add = parse_stream_event(
            r#"{"type":"response.output_item.added","item":{"id":"i1","type":"function_call","name":"write_content","call_id":"c1"}}"#,
        );
        assert_eq!(
            add,
            StreamEvent::FunctionCallAdded {
                item_id: "i1".into(),
                name: "write_content".into(),
                call_id: "c1".into()
            }
        );
        let d = parse_stream_event(
            r#"{"type":"response.function_call_arguments.delta","item_id":"i1","delta":"{\"content"}"#,
        );
        assert_eq!(d, StreamEvent::ArgsDelta { item_id: "i1".into(), delta: "{\"content".into() });
    }

    #[test]
    fn 事件映射_完成带output() {
        let e = parse_stream_event(
            r#"{"type":"response.completed","response":{"output":[{"type":"function_call"}],"usage":{"total_tokens":9}}}"#,
        );
        match e {
            StreamEvent::Completed { output, usage } => {
                assert_eq!(output[0]["type"], "function_call");
                assert_eq!(usage.unwrap()["total_tokens"], 9);
            }
            other => panic!("得到 {other:?}"),
        }
    }
}

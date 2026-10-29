//! 聚合网页搜索：Exa / Brave / DeepSeek 并行查询、URL 去重、RRF 融合。
//!
//! 工具层只暴露一个 `search_web`；provider 差异、密钥和失败降级都留在这里。

use crate::tools::html_to_text;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::HashMap;
use std::time::Duration;
use wikiatlas_store::secrets;

const PROVIDERS: &[&str] = &["exa", "brave", "deepseek"];
const FETCH_MAX_CHARS: usize = 12_000;

#[derive(Debug, Clone)]
struct Hit {
    title: String,
    url: String,
    snippet: String,
    summary: String,
    content: String,
    provider: String,
}

#[derive(Debug)]
struct ProviderResult {
    provider: &'static str,
    ok: bool,
    message: String,
    hits: Vec<Hit>,
}

pub async fn search(raw: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct A {
        query: String,
        #[serde(default)]
        num: Option<u32>,
        #[serde(default)]
        providers: Vec<String>,
    }
    let a: A = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(e) => return json!({ "ok": false, "reason": format!("参数不是合法 JSON：{e}") }),
    };
    let query = a.query.trim();
    if query.is_empty() {
        return json!({ "ok": false, "reason": "query 不能为空" });
    }
    let num = a.num.unwrap_or(20).clamp(1, 50) as usize;
    let Some(pool) = db_pool().await else {
        return json!({ "ok": false, "reason": "搜索不可用：数据库连接串缺失" });
    };
    let mut cfg = match Config::load(&pool).await {
        Ok(v) => v,
        Err(e) => return json!({ "ok": false, "reason": format!("读搜索配置失败：{e}") }),
    };
    if !a.providers.is_empty() {
        cfg.enabled.retain(|p| a.providers.iter().any(|x| x == p));
    }
    if !cfg.any_enabled() {
        return json!({
            "ok": false,
            "reason": "搜索不可用：没有已配置且启用的 search provider（Exa / Brave / DeepSeek）",
            "providers": provider_status(&cfg)
        });
    }
    let (exa, brave, deepseek) = tokio::join!(
        run_one(&cfg, "exa", query, num),
        run_one(&cfg, "brave", query, num),
        run_one(&cfg, "deepseek", query, num),
    );
    let results = [exa, brave, deepseek];
    let (hits, statuses) = aggregate(results, num.max(20));
    if hits.is_empty() {
        let errors = statuses
            .iter()
            .filter_map(|s| {
                s.get("message")
                    .and_then(Value::as_str)
                    .filter(|m| !m.is_empty())
                    .map(|m| format!("{}：{m}", s["provider"].as_str().unwrap_or("")))
            })
            .collect::<Vec<_>>()
            .join("；");
        return json!({
            "ok": false,
            "reason": if errors.is_empty() { "搜索没有结果".to_string() } else { errors },
            "providers": statuses
        });
    }
    json!({
        "ok": true,
        "query": query,
        "count": hits.len(),
        "results": hits,
        "providers": statuses,
        "notice": "聚合结果按 URL 去重，并用 RRF 融合各 provider 排名；具体来源见 providers 字段。"
    })
}

pub async fn fetch(raw: &str) -> Value {
    #[derive(serde::Deserialize)]
    struct A {
        url: String,
    }
    let a: A = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(e) => return json!({ "ok": false, "reason": format!("参数不是合法 JSON：{e}") }),
    };
    if !(a.url.starts_with("http://") || a.url.starts_with("https://")) {
        return json!({ "ok": false, "reason": "只支持 http/https URL" });
    }
    let Some(pool) = db_pool().await else {
        return local_fetch(&a.url).await;
    };
    if let Ok(cfg) = Config::load(&pool).await {
        if let Some(key) = cfg.key("deepseek") {
            if let Ok(v) = deepseek_fetch(&cfg.deepseek_url, key, &a.url).await {
                return v;
            }
        }
    }
    local_fetch(&a.url).await
}

async fn db_pool() -> Option<PgPool> {
    let url = std::env::var("WIKIATLAS_DB_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()?;
    wikiatlas_store::connect(&url).await.ok()
}

struct Config {
    enabled: Vec<String>,
    exa_key: Option<String>,
    brave_key: Option<String>,
    deepseek_key: Option<String>,
    exa_url: String,
    brave_url: String,
    deepseek_url: String,
}

impl Config {
    async fn load(pool: &PgPool) -> wikiatlas_store::Result<Self> {
        let enabled = secrets::get_setting(pool, "search.providers")
            .await?
            .and_then(|v| {
                v.as_array().map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
            })
            .unwrap_or_else(|| PROVIDERS.iter().map(|s| s.to_string()).collect());
        Ok(Self {
            enabled,
            exa_key: key(pool, "exa.api_key").await?,
            brave_key: key(pool, "brave.api_key").await?,
            deepseek_key: key(pool, "deepseek.api_key").await?,
            exa_url: base(pool, "search.exa_base_url", "https://api.exa.ai").await?,
            brave_url: base(
                pool,
                "search.brave_base_url",
                "https://api.search.brave.com",
            )
            .await?,
            deepseek_url: base(
                pool,
                "search.deepseek_base_url",
                "https://api.deepseek.com",
            )
            .await?,
        })
    }

    fn any_enabled(&self) -> bool {
        self.key("exa").is_some() || self.key("brave").is_some() || self.key("deepseek").is_some()
    }

    fn key(&self, provider: &str) -> Option<&str> {
        if !self.enabled.iter().any(|p| p == provider) {
            return None;
        }
        match provider {
            "exa" => self.exa_key.as_deref(),
            "brave" => self.brave_key.as_deref(),
            "deepseek" => self.deepseek_key.as_deref(),
            _ => None,
        }
    }
}

async fn key(pool: &PgPool, name: &str) -> wikiatlas_store::Result<Option<String>> {
    if let Some(v) = secrets::get_secret(pool, name)
        .await?
        .filter(|v| !v.trim().is_empty())
    {
        return Ok(Some(v));
    }
    if name == "deepseek.api_key" {
        if let Some(v) = secrets::get_secret(pool, "llm.api_key")
            .await?
            .filter(|v| !v.trim().is_empty())
        {
            return Ok(Some(v));
        }
    }
    let env = match name {
        "exa.api_key" => "WIKIATLAS_EXA_API_KEY",
        "brave.api_key" => "WIKIATLAS_BRAVE_API_KEY",
        "deepseek.api_key" => "WIKIATLAS_DEEPSEEK_API_KEY",
        _ => return Ok(None),
    };
    Ok(std::env::var(env).ok().filter(|v| !v.trim().is_empty()))
}

async fn base(
    pool: &PgPool,
    name: &str,
    default: &str,
) -> wikiatlas_store::Result<String> {
    Ok(secrets::get_setting(pool, name)
        .await?
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| default.into())
        .trim_end_matches('/')
        .to_string())
}

async fn run_one(cfg: &Config, provider: &'static str, query: &str, num: usize) -> ProviderResult {
    let Some(key) = cfg.key(provider) else {
        return ProviderResult {
            provider,
            ok: false,
            message: "未配置或已停用".into(),
            hits: vec![],
        };
    };
    let result = match provider {
        "exa" => exa(&cfg.exa_url, key, query, num).await,
        "brave" => brave(&cfg.brave_url, key, query, num).await,
        "deepseek" => deepseek(&cfg.deepseek_url, key, query, num).await,
        _ => Err("未知 provider".into()),
    };
    match result {
        Ok(hits) => ProviderResult {
            provider,
            ok: true,
            message: String::new(),
            hits,
        },
        Err(message) => ProviderResult {
            provider,
            ok: false,
            message,
            hits: vec![],
        },
    }
}

async fn exa(base: &str, key: &str, query: &str, num: usize) -> Result<Vec<Hit>, String> {
    let client = http()?;
    let provider_num = (num * 3).clamp(20, 100);
    let resp = client
        .post(format!("{base}/search"))
        .header("x-api-key", key)
        .json(&json!({
            "query": query,
            "numResults": provider_num,
            "contents": { "text": { "maxCharacters": 700 } }
        }))
        .send()
        .await
        .map_err(|e| format!("连接失败：{e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("HTTP {status}：{}", clip(&text, 160)));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("响应解析失败：{e}"))?;
    Ok(v.get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|r| Hit {
            title: s(r, "title"),
            url: s(r, "url"),
            snippet: s(r, "text"),
            summary: String::new(),
            content: s(r, "text"),
            provider: "exa".into(),
        })
        .filter(|h| !h.url.is_empty())
        .collect())
}

async fn brave(base: &str, key: &str, query: &str, num: usize) -> Result<Vec<Hit>, String> {
    let client = http()?;
    let provider_num = num.clamp(1, 20);
    let resp = client
        .get(format!("{base}/res/v1/web/search"))
        .header("X-Subscription-Token", key)
        .query(&[("q", query), ("count", &provider_num.to_string())])
        .send()
        .await
        .map_err(|e| format!("连接失败：{e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("HTTP {status}：{}", clip(&text, 160)));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("响应解析失败：{e}"))?;
    Ok(v.pointer("/web/results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|r| Hit {
            title: s(r, "title"),
            url: s(r, "url"),
            snippet: s(r, "description"),
            summary: String::new(),
            content: String::new(),
            provider: "brave".into(),
        })
        .filter(|h| !h.url.is_empty())
        .collect())
}

async fn deepseek(base: &str, key: &str, query: &str, num: usize) -> Result<Vec<Hit>, String> {
    let client = http()?;
    let provider_num = (num * 3).clamp(20, 50);
    let resp = client
        .post(format!("{base}/v1/tools/search"))
        .bearer_auth(key)
        .json(&json!({
            "query": query,
            "count": provider_num,
            "need_content": true
        }))
        .send()
        .await
        .map_err(|e| format!("连接失败：{e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("HTTP {status}：{}", clip(&text, 160)));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("响应解析失败：{e}"))?;
    Ok(v.get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|r| Hit {
            title: s(r, "title"),
            url: s(r, "url"),
            snippet: clip(&s(r, "snippet"), 400),
            summary: clip(&s(r, "summary"), 1_200),
            content: clip(&s(r, "content"), 2_000),
            provider: "deepseek".into(),
        })
        .filter(|h| !h.url.is_empty())
        .collect())
}

fn aggregate(results: [ProviderResult; 3], limit: usize) -> (Vec<Value>, Vec<Value>) {
    let mut statuses = vec![];
    let mut merged: HashMap<String, (Hit, f64, Vec<String>)> = HashMap::new();
    for result in results {
        statuses.push(json!({
            "provider": result.provider,
            "ok": result.ok,
            "count": result.hits.len(),
            "message": result.message,
        }));
        for (rank, hit) in result.hits.into_iter().enumerate() {
            let key = canonical_url(&hit.url);
            let rrf = 1.0 / (60.0 + rank as f64 + 1.0);
            merged
                .entry(key)
                .and_modify(|(old, score, providers)| {
                    *score += rrf;
                    if old.snippet.is_empty() {
                        old.snippet = hit.snippet.clone();
                    }
                    if old.summary.is_empty() {
                        old.summary = hit.summary.clone();
                    }
                    if old.content.is_empty() {
                        old.content = hit.content.clone();
                    }
                    if !providers.contains(&hit.provider) {
                        providers.push(hit.provider.clone());
                    }
                })
                .or_insert((hit.clone(), rrf, vec![hit.provider]));
        }
    }
    let mut rows: Vec<_> = merged.into_values().collect();
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    let hits = rows
        .into_iter()
        .take(limit)
        .map(|(h, score, providers)| {
            json!({
                "title": h.title,
                "url": h.url,
                "snippet": h.snippet,
                "summary": h.summary,
                "content": h.content,
                "providers": providers,
                "score": (score * 10000.0).round() / 10000.0,
            })
        })
        .collect();
    (hits, statuses)
}

fn provider_status(cfg: &Config) -> Value {
    json!({
        "exa": { "enabled": cfg.enabled.iter().any(|p| p == "exa"), "configured": cfg.exa_key.is_some() },
        "brave": { "enabled": cfg.enabled.iter().any(|p| p == "brave"), "configured": cfg.brave_key.is_some() },
        "deepseek": { "enabled": cfg.enabled.iter().any(|p| p == "deepseek"), "configured": cfg.deepseek_key.is_some() },
    })
}

fn canonical_url(url: &str) -> String {
    url.trim()
        .split('#')
        .next()
        .unwrap_or("")
        .trim_end_matches('/')
        .to_lowercase()
}

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(40))
        .user_agent("Atlas/0.1 (aggregate-search)")
        .build()
        .map_err(|e| e.to_string())
}

async fn deepseek_fetch(base: &str, key: &str, url: &str) -> Result<Value, String> {
    let resp = http()?
        .post(format!("{base}/v1/tools/fetch"))
        .bearer_auth(key)
        .json(&json!({ "url": url }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let v: Value = resp.json().await.map_err(|e| e.to_string())?;
    let text = s(&v, "content");
    Ok(fetch_value(url, s(&v, "title"), text, "deepseek"))
}

async fn local_fetch(url: &str) -> Value {
    let client = match http() {
        Ok(v) => v,
        Err(e) => return json!({ "ok": false, "reason": format!("HTTP 客户端建不起来：{e}") }),
    };
    let resp = match client.get(url).send().await {
        Ok(v) => v,
        Err(e) => return json!({ "ok": false, "reason": format!("抓取失败：{e}") }),
    };
    if !resp.status().is_success() {
        return json!({ "ok": false, "reason": format!("抓取 HTTP {}", resp.status()) });
    }
    let html = resp.text().await.unwrap_or_default();
    fetch_value(url, String::new(), html_to_text(&html), "local")
}

fn fetch_value(url: &str, title: String, text: String, provider: &str) -> Value {
    let total = text.chars().count();
    let body: String = text.chars().take(FETCH_MAX_CHARS).collect();
    let mut out = json!({
        "ok": true, "url": url, "title": title, "chars": total,
        "text": body, "provider": provider
    });
    if total > FETCH_MAX_CHARS {
        out["truncated"] = json!(true);
        out["notice"] = json!(format!(
            "正文共 {total} 字，已截断到 {FETCH_MAX_CHARS} 字——你没看到全部内容"
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use std::sync::Arc;
    use tokio::net::TcpListener;

    fn hit(url: &str, provider: &str) -> Hit {
        Hit {
            title: url.into(),
            url: url.into(),
            snippet: format!("from {provider}"),
            summary: String::new(),
            content: String::new(),
            provider: provider.into(),
        }
    }

    #[test]
    fn 聚合按url去重并融合多provider排名() {
        let results = [
            ProviderResult { provider: "exa", ok: true, message: String::new(), hits: vec![hit("https://a", "exa"), hit("https://b", "exa")] },
            ProviderResult { provider: "brave", ok: true, message: String::new(), hits: vec![hit("https://b/", "brave"), hit("https://c", "brave")] },
            ProviderResult { provider: "deepseek", ok: false, message: "down".into(), hits: vec![] },
        ];
        let (hits, statuses) = aggregate(results, 10);
        assert_eq!(hits.len(), 3);
        assert_eq!(statuses.len(), 3);
        let b = hits.iter().find(|h| h["url"] == "https://b").unwrap();
        assert_eq!(b["providers"].as_array().unwrap().len(), 2);
        assert_eq!(hits[0]["url"], "https://b");
    }

    #[tokio::test]
    async fn 三家provider响应都能归一化() {
        type Seen = Arc<std::sync::Mutex<Vec<String>>>;
        let seen: Seen = Arc::default();
        async fn h_exa(
            State(seen): State<Seen>,
            headers: HeaderMap,
            Json(body): Json<Value>,
        ) -> Json<Value> {
            assert_eq!(headers["x-api-key"], "exa-key");
            seen.lock().unwrap().push(format!("exa:{}", body["query"].as_str().unwrap()));
            Json(json!({ "results": [{ "title": "Exa Hit", "url": "https://a", "text": "exa text" }] }))
        }
        async fn h_brave(
            State(seen): State<Seen>,
            headers: HeaderMap,
            axum::extract::Query(q): axum::extract::Query<HashMap<String, String>>,
        ) -> Json<Value> {
            assert_eq!(headers["x-subscription-token"], "brave-key");
            seen.lock().unwrap().push(format!("brave:{}", q["q"]));
            Json(json!({ "web": { "results": [{ "title": "Brave Hit", "url": "https://b", "description": "brave text" }] } }))
        }
        async fn h_deepseek(
            State(seen): State<Seen>,
            headers: HeaderMap,
            Json(body): Json<Value>,
        ) -> Json<Value> {
            assert_eq!(headers["authorization"], "Bearer ds-key");
            assert_eq!(body["need_content"], true);
            seen.lock().unwrap().push(format!("deepseek:{}", body["query"].as_str().unwrap()));
            Json(json!({ "results": [{ "title": "DS Hit", "url": "https://c", "snippet": "ds text", "summary": "summary", "content": "content" }] }))
        }
        async fn missing() -> StatusCode {
            StatusCode::NO_CONTENT
        }
        let app = Router::new()
            .route("/search", post(h_exa))
            .route("/res/v1/web/search", get(h_brave))
            .route("/v1/tools/search", post(h_deepseek))
            .route("/v1/tools/fetch", post(missing))
            .with_state(seen.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let base = format!("http://{addr}");
        let exa_hits = exa(&base, "exa-key", "one", 5).await.unwrap();
        let brave_hits = brave(&base, "brave-key", "two", 5).await.unwrap();
        let ds_hits = deepseek(&base, "ds-key", "three", 5).await.unwrap();
        assert_eq!(exa_hits[0].provider, "exa");
        assert_eq!(brave_hits[0].snippet, "brave text");
        assert_eq!(ds_hits[0].summary, "summary");
        assert_eq!(seen.lock().unwrap().len(), 3);
    }
}

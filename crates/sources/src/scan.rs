//! 扫描编排（`docs/library-sync.md` §3）：拉清单 → 落 `library_manifest` → 记 `scan_runs`。
//!
//! **一个源失败不影响其他源**：每个源单独记账，失败写进它自己的 `scan_runs` 行。
//! **零 LLM**、**确定性**：这里没有任何模型调用（「AI 对一遍」是 §7 的可选动作，不在扫描里）。

use crate::config::{self, GameAtlasConfig, KomgaConfig, SourceConfig};
use crate::{emby::EmbyClient, gameatlas::GameAtlasClient, komga::KomgaClient, Result};
use serde::Serialize;
use sqlx::PgPool;
use wikiatlas_domain::library::{ManifestEntry, Source};
use wikiatlas_store::library as store;

/// 单个源的扫描结果。
#[derive(Debug, Clone, Serialize)]
pub struct SourceOutcome {
    pub source: Source,
    pub ok: bool,
    pub seen: usize,
    pub inserted: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub missing: u64,
    pub message: String,
}

/// 一次扫描的全部源结果。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanOutcome {
    pub results: Vec<SourceOutcome>,
}

impl ScanOutcome {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// 扫全部已配置的源（`only` 给 Some 时只扫其中列出的）。
pub async fn scan(pool: &PgPool, only: Option<Vec<Source>>) -> Result<ScanOutcome> {
    let cfg = config::load(pool).await?;
    Ok(scan_with_config(pool, &cfg, only).await)
}

/// 用已加载的配置扫（测试与 worker 复用；不重复读 secrets）。
pub async fn scan_with_config(
    pool: &PgPool,
    cfg: &SourceConfig,
    only: Option<Vec<Source>>,
) -> ScanOutcome {
    let want = |s: Source| only.as_ref().is_none_or(|v| v.contains(&s));
    let mut results = vec![];

    if want(Source::GameAtlas) {
        if let Some(c) = &cfg.gameatlas {
            results.push(run(pool, Source::GameAtlas, fetch_gameatlas(c)).await);
        }
    }
    if want(Source::Emby) {
        if let Some(c) = &cfg.emby {
            let mixed = c.mixed_views.clone();
            let client = EmbyClient::connect(c);
            results.push(run_emby(pool, client, mixed).await);
        }
    }
    if want(Source::Komga) {
        if let Some(c) = &cfg.komga {
            results.push(run(pool, Source::Komga, fetch_komga(c)).await);
        }
    }
    ScanOutcome { results }
}

async fn fetch_gameatlas(cfg: &GameAtlasConfig) -> Result<Vec<ManifestEntry>> {
    GameAtlasClient::connect(cfg).await?.fetch_manifest().await
}

async fn fetch_komga(cfg: &KomgaConfig) -> Result<Vec<ManifestEntry>> {
    KomgaClient::connect(cfg)?.fetch_manifest().await
}

async fn run_emby(
    pool: &PgPool,
    client: Result<EmbyClient>,
    mixed: Vec<String>,
) -> SourceOutcome {
    match client {
        Ok(c) => run(pool, Source::Emby, c.fetch_manifest(&mixed)).await,
        Err(e) => failure(Source::Emby, e.to_string()),
    }
}

/// 跑一个源：开记账 → 拉 → 落库 → 记账结束。**失败不 panic、不中断其他源**。
async fn run<F>(pool: &PgPool, source: Source, fut: F) -> SourceOutcome
where
    F: std::future::Future<Output = Result<Vec<ManifestEntry>>>,
{
    let run_id = match store::start_scan_run(pool, source).await {
        Ok(id) => id,
        Err(e) => return failure(source, format!("记账失败：{e}")),
    };
    match fut.await {
        Ok(entries) => match store::upsert_manifest(pool, &entries).await {
            Ok(s) => {
                let stats = serde_json::to_value(&s).unwrap_or(serde_json::Value::Null);
                let _ = store::finish_scan_run(pool, run_id, true, &stats, "").await;
                SourceOutcome {
                    source,
                    ok: true,
                    seen: s.seen,
                    inserted: s.inserted,
                    changed: s.changed,
                    unchanged: s.unchanged,
                    missing: s.missing,
                    message: String::new(),
                }
            }
            Err(e) => {
                let msg = format!("落清单失败：{e}");
                let _ = store::finish_scan_run(pool, run_id, false, &serde_json::json!({}), &msg).await;
                SourceOutcome { source, ok: false, ..empty(source, msg) }
            }
        },
        Err(e) => {
            let msg = e.to_string();
            let _ = store::finish_scan_run(pool, run_id, false, &serde_json::json!({}), &msg).await;
            SourceOutcome { source, ok: false, ..empty(source, msg) }
        }
    }
}

fn failure(source: Source, msg: String) -> SourceOutcome {
    tracing::warn!(source = source.as_str(), "扫描失败：{msg}");
    empty(source, msg)
}

fn empty(source: Source, message: String) -> SourceOutcome {
    SourceOutcome {
        source,
        ok: false,
        seen: 0,
        inserted: 0,
        changed: 0,
        unchanged: 0,
        missing: 0,
        message,
    }
}

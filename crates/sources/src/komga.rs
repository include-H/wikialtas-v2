//! Komga 客户端（`docs/library-sync.md` §1）：`X-API-Key`，双向源（M1 只读）。
//!
//! 层级（**确定性、零 LLM**，从 v1 的兜底启发式搬来，见 v1 `library_komga.go`）：
//! - 分卷系列（书大多是「第N卷 / Vol.01」形态）→ 清单条目 = **系列本身**（容器 = 该系列）；
//! - 抽屉系列（多本书、多数不是分卷形态）→ **拆单册**，每条 = 一本书（容器 = 该系列）。
//! v1 用 LLM 判型、启发式兜底；v2 扫描零 LLM，所以先只跑启发式。
//! 需要更准时走「AI 对一遍」（`docs/library-sync.md` §7，可选、产出建议、人确认）。

use crate::config::KomgaConfig;
use crate::{http_client, Result, SourceError};
use serde::Deserialize;
use wikiatlas_domain::library::{ContainerKind, ManifestEntry, Source};

const SRC: &str = "komga";

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct Series {
    pub id: String,
    #[serde(default)]
    pub library_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub metadata: SeriesMeta,
}

#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub struct SeriesMeta {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub title_sort: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub language: String,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct Book {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub media: BookMedia,
    #[serde(default)]
    pub metadata: BookMeta,
}

#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub struct BookMedia {
    #[serde(default, rename = "mediaType")]
    pub media_type: String,
}

#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub struct BookMeta {
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Deserialize)]
struct Page<T> {
    content: Vec<T>,
    #[serde(default, rename = "totalPages")]
    total_pages: i64,
}

pub struct KomgaClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl KomgaClient {
    pub fn connect(cfg: &KomgaConfig) -> Result<Self> {
        let mut base = cfg.base_url.trim().trim_end_matches('/').to_string();
        // 用户可能从 Swagger 页复制带 /api/v1 的地址——统一归一。
        if let Some(stripped) = base.strip_suffix("/api/v1") {
            base = stripped.trim_end_matches('/').to_string();
        }
        if base.is_empty() || cfg.api_key.trim().is_empty() {
            return Err(SourceError::NotConfigured(SRC.into()));
        }
        Ok(Self {
            http: http_client()?,
            base_url: base,
            api_key: cfg.api_key.clone(),
        })
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self
            .http
            .get(format!("{}{}", self.base_url, path))
            .header("X-API-Key", &self.api_key)
            .send()
            .await
            .map_err(|e| SourceError::Http { src: SRC, message: e.to_string() })?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| SourceError::Http { src: SRC, message: e.to_string() })?;
        if !status.is_success() {
            let msg: String = body.chars().take(120).collect();
            return Err(SourceError::BadResponse {
                src: SRC,
                message: format!("HTTP {status} {msg}"),
            });
        }
        serde_json::from_str(&body)
            .map_err(|e| SourceError::BadResponse { src: SRC, message: e.to_string() })
    }

    async fn list_series(&self) -> Result<Vec<Series>> {
        let mut all = vec![];
        let mut page = 0;
        loop {
            let p: Page<Series> = self
                .get_json(&format!("/api/v1/series?page={page}&size=100&sort=metadata.titleSort,asc"))
                .await?;
            let n = p.content.len();
            all.extend(p.content);
            page += 1;
            if n == 0 || page >= p.total_pages {
                return Ok(all);
            }
        }
    }

    async fn list_books(&self, series_id: &str) -> Result<Vec<Book>> {
        let mut all = vec![];
        let mut page = 0;
        loop {
            let p: Page<Book> = self
                .get_json(&format!("/api/v1/series/{series_id}/books?size=200&page={page}"))
                .await?;
            let n = p.content.len();
            all.extend(p.content);
            page += 1;
            if n == 0 || page >= p.total_pages {
                return Ok(all);
            }
        }
    }

    /// 全量清单：逐系列判分卷 / 抽屉，再映射。
    pub async fn fetch_manifest(&self) -> Result<Vec<ManifestEntry>> {
        let mut out = vec![];
        for sr in self.list_series().await? {
            let books = self.list_books(&sr.id).await?;
            out.extend(self.entries_for_series(sr, books));
        }
        Ok(out)
    }

    fn entries_for_series(&self, sr: Series, books: Vec<Book>) -> Vec<ManifestEntry> {
        let novel = books.iter().filter(|b| b.media.media_type.contains("epub")).count() * 2
            > books.len();
        let format = if novel { "novel" } else { "comic" };
        let volumes = books
            .iter()
            .filter(|b| book_is_volume(&book_title(b), &series_name(&sr)))
            .count();
        let is_drawer = books.len() > 1 && volumes * 2 < books.len();

        if is_drawer {
            return books
                .into_iter()
                .map(|b| {
                    let raw = serde_json::to_value(&b).unwrap_or(serde_json::Value::Null);
                    let comic = !b.media.media_type.contains("epub");
                    ManifestEntry {
                        source: Source::Komga,
                        external_id: b.id.clone(),
                        title: book_title(&b),
                        subtitle: None,
                        aliases: vec![],
                        year: None,
                        kind: "book".into(),
                        format: Some(if comic { "comic" } else { "novel" }.into()),
                        cover_url: None,
                        url: Some(format!("{}/book/{}", self.base_url, b.id)),
                        container_key: format!("series:{}", sr.id),
                        container_title: series_name(&sr),
                        container_kind: ContainerKind::Series,
                        extra: "抽屉系列拆出的单册".into(),
                        raw,
                    }
                })
                .collect();
        }

        let raw = serde_json::to_value(&sr).unwrap_or(serde_json::Value::Null);
        vec![ManifestEntry {
            source: Source::Komga,
            external_id: sr.id.clone(),
            title: series_name(&sr),
            subtitle: None,
            aliases: vec![],
            year: None,
            kind: "book".into(),
            format: Some(format.into()),
            cover_url: None,
            url: Some(format!("{}/series/{}", self.base_url, sr.id)),
            container_key: format!("series:{}", sr.id),
            container_title: series_name(&sr),
            container_kind: ContainerKind::Series,
            extra: String::new(),
            raw,
        }]
    }
}

fn series_name(sr: &Series) -> String {
    if sr.metadata.title.trim().is_empty() {
        sr.name.clone()
    } else {
        sr.metadata.title.clone()
    }
}

fn book_title(b: &Book) -> String {
    if b.metadata.title.trim().is_empty() {
        b.name.clone()
    } else {
        b.metadata.title.clone()
    }
}

/// 卷号噪音：数字、第/卷/册/话…、括号与常见标点（照 v1 搬）。
fn strip_volume_noise(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| {
            !(c.is_ascii_digit()
                || ('０'..='９').contains(c)
                || "第卷巻册冊话話集部回全之-—–~～:：.。·•,，()（）[]【】《》".contains(*c)
                || c.is_whitespace())
        })
        .collect()
}

/// 「这本书是不是分卷」的确定性启发式（v1 `komgaBookIsVolume` 的移植）。
pub fn book_is_volume(title: &str, series_name: &str) -> bool {
    let cleaned = strip_volume_noise(title);
    if cleaned.is_empty() || matches!(cleaned.as_str(), "vol" | "volume" | "ch" | "chapter" | "ep" | "episode" | "no" | "sp") {
        return true;
    }
    if cleaned.ends_with("vol") {
        return true;
    }
    let s = strip_volume_noise(series_name);
    if s.is_empty() {
        return false;
    }
    cleaned.contains(&s) || s.contains(&cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 分卷形态判得出来() {
        assert!(book_is_volume("葬送的芙莉莲 Vol.01", "葬送的芙莉蓮"));
        assert!(book_is_volume("第3卷", "某系列"));
        assert!(book_is_volume("Ch.12", "某系列"));
    }

    #[test]
    fn 抽屉里的独立单册不是分卷() {
        // 抽屉系列：每本书是独立作品，标题与系列名无关 → 不是分卷
        assert!(!book_is_volume("完全版 设定集", "作者合集"));
        assert!(!book_is_volume("另一个故事", "短篇集"));
    }
}

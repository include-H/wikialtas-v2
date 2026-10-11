//! Emby 客户端（`docs/library-sync.md` §1）：`X-Emby-Token`，**只读**。
//!
//! Emby 的 **library（view）** 是"库"（拿库名建节点毫无意义），所以容器 kind = library；
//! 被标成混杂的库 kind = mixed，条目落链时 role=mix。
//! 封面：Emby 缩略图要带 token，M1 不把 token 塞进清单（避免泄漏给前端）——
//! 封面代理端点随设置页做（M9）。`raw` 里保留了上游字段供那时使用。

use crate::config::EmbyConfig;
use crate::{http_client, Result, SourceError};
use serde::Deserialize;

use tokio::sync::OnceCell;
use wikiatlas_domain::library::{ContainerKind, ManifestEntry, Source};

const SRC: &str = "emby";
const FIELDS: &str = "ProductionYear,OriginalTitle,SeriesName,Path,ImageTags";

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct Item {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "OriginalTitle", default)]
    pub original_title: Option<String>,
    #[serde(rename = "Type", default)]
    pub kind: String,
    #[serde(rename = "ProductionYear", default)]
    pub production_year: Option<i32>,
    #[serde(rename = "ParentId", default)]
    pub parent_id: String,
}

#[derive(Debug, Deserialize)]
struct View {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Name", default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct ItemsPage {
    #[serde(rename = "Items", default)]
    items: Vec<Item>,
    #[serde(rename = "TotalRecordCount", default)]
    total: i64,
}

pub struct EmbyClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
    user_id: OnceCell<String>,
}

impl EmbyClient {
    pub fn connect(cfg: &EmbyConfig) -> Result<Self> {
        let base = cfg.base_url.trim().trim_end_matches('/').to_string();
        if base.is_empty() || cfg.token.trim().is_empty() {
            return Err(SourceError::NotConfigured(SRC.into()));
        }
        Ok(Self {
            http: http_client()?,
            base_url: base,
            token: cfg.token.clone(),
            user_id: OnceCell::new(),
        })
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self
            .http
            .get(format!("{}{}", self.base_url, path))
            .header("X-Emby-Token", &self.token)
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

    async fn user_id(&self) -> Result<&str> {
        self.user_id
            .get_or_try_init(|| async {
                #[derive(Deserialize)]
                struct User {
                    #[serde(rename = "Id")]
                    id: String,
                    #[serde(rename = "Policy", default)]
                    policy: Option<Policy>,
                }
                #[derive(Deserialize)]
                struct Policy {
                    #[serde(rename = "IsAdministrator", default)]
                    is_admin: bool,
                }
                let users: Vec<User> = self.get_json("/Users").await?;
                users
                    .iter()
                    .find(|u| u.policy.as_ref().is_some_and(|p| p.is_admin) && !u.id.is_empty())
                    .or_else(|| users.iter().find(|u| !u.id.is_empty()))
                    .map(|u| u.id.clone())
                    .ok_or_else(|| SourceError::Empty("emby 里没有可用用户（token 是否有效？）".into()))
            })
            .await
            .map(|s| s.as_str())
    }

    async fn list_views(&self) -> Result<Vec<(String, String)>> {
        let uid = self.user_id().await?;
        #[derive(Deserialize)]
        struct Out {
            #[serde(rename = "Items", default)]
            items: Vec<View>,
        }
        let out: Out = self.get_json(&format!("/Users/{uid}/Views")).await?;
        Ok(out.items.into_iter().map(|v| (v.id, v.name)).collect())
    }

    async fn list_items(&self, parent_id: &str) -> Result<Vec<Item>> {
        let uid = self.user_id().await?;
        let mut all = vec![];
        let mut start = 0usize;
        loop {
            let path = format!(
                "/Users/{uid}/Items?Recursive=true&IncludeItemTypes=Series,Movie,BoxSet\
                 &Fields={FIELDS}&SortBy=SortName&StartIndex={start}&Limit=500&ParentId={parent_id}"
            );
            let page: ItemsPage = self.get_json(&path).await?;
            let n = page.items.len();
            all.extend(page.items);
            start += n;
            if n == 0 || (start as i64) >= page.total {
                return Ok(all);
            }
        }
    }

    /// 全量清单：每个库 → 条目（容器 = 库）。`mixed_views` 命中 → container_kind=mixed。
    pub async fn fetch_manifest(&self, mixed_views: &[String]) -> Result<Vec<ManifestEntry>> {
        let mut out = vec![];
        for (view_id, view_name) in self.list_views().await? {
            let kind = if mixed_views.iter().any(|m| m == &view_id || m == &view_name) {
                ContainerKind::Mixed
            } else {
                ContainerKind::Library
            };
            for it in self.list_items(&view_id).await? {
                if let Some(e) = self.to_entry(&view_id, &view_name, kind, it) {
                    out.push(e);
                }
            }
        }
        Ok(out)
    }

    fn to_entry(
        &self,
        view_id: &str,
        view_name: &str,
        container_kind: ContainerKind,
        it: Item,
    ) -> Option<ManifestEntry> {
        let kind = match it.kind.as_str() {
            "Series" => "tv",
            "Movie" => "movie",
            "BoxSet" => "collection",
            _ => return None,
        };
        let raw = serde_json::to_value(&it).unwrap_or(serde_json::Value::Null);
        Some(ManifestEntry {
            source: Source::Emby,
            external_id: it.id.clone(),
            title: if it.name.trim().is_empty() { it.id.clone() } else { it.name },
            subtitle: None,
            aliases: it.original_title.iter().filter(|s| !s.trim().is_empty()).cloned().collect(),
            year: it.production_year,
            kind: kind.into(),
            format: None,
            cover_url: None, // 见文件头：Emby 封面要 token，M1 不落清单
            url: Some(format!(
                "{}/web/index.html#!/item?id={}",
                self.base_url,
                urlencode(&it.id)
            )),
            container_key: format!("view:{view_id}"),
            container_title: view_name.to_string(),
            container_kind,
            extra: String::new(),
            raw,
        })
    }
}

/// 极简 query 转义（id 是十六进制/连字符，够用；避免为一个字符引依赖）。
fn urlencode(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || "-_.~".contains(c) { c.to_string() } else { format!("%{:02X}", c as u32) })
        .collect()
}

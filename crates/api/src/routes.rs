//! 路由处理器：只做**编排**（取参 → 调 store → 出 JSON）。
//! 业务规则不在这一层——v1 的教训就是规则塞进了 handler，于是没有测试基建。

use axum::extract::{Path, State};
use axum::Json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::dto::{
    CommitReq, CommitResp, CreateNodeReq, CreateRelationReq, CreateSessionReq, CreateSuggestionReq,
    DecisionReq, DraftReq, FindingQuery, RelationQuery, RelinkReq, ReviewQuery, SaveAnswerReq,
    SearchQuery, SessionMessageQuery, SessionQuery, SynthesisReq, VersionResp,
};
use crate::error::ApiError;
use wikiatlas_domain as domain;

/// 还没有登录体系：人写一律记成这个身份（写者矩阵落地后由中间件给）。
const HUMAN: &str = "human:local";

pub async fn healthz() -> &'static str {
    "ok"
}

pub async fn tree(State(pool): State<PgPool>) -> Result<Json<Vec<domain::Node>>, ApiError> {
    Ok(Json(wikiatlas_store::nodes::tree(&pool).await?))
}

pub async fn create_node(
    State(pool): State<PgPool>,
    Json(req): Json<CreateNodeReq>,
) -> Result<Json<domain::Node>, ApiError> {
    let new = req.into_new()?;
    Ok(Json(wikiatlas_store::nodes::create(&pool, &new).await?))
}

pub async fn node_detail(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<domain::NodeContent>, ApiError> {
    Ok(Json(wikiatlas_store::content::read(&pool, id).await?))
}

/// POST /api/nodes/{id}/archive：默认删除策略。整棵子树移出知识库，数据保留。
pub async fn archive_node(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<domain::Node>, ApiError> {
    Ok(Json(wikiatlas_store::nodes::archive(&pool, id).await?))
}

pub async fn put_draft(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(req): Json<DraftReq>,
) -> Result<Json<domain::Draft>, ApiError> {
    let draft =
        wikiatlas_store::content::put_draft(&pool, id, &req.body_md, req.base_ver, HUMAN).await?;
    Ok(Json(draft))
}

/// 丢弃草稿：v2 里「拒绝 AI 修订」的落点（接受 = 定稿）。
pub async fn discard_draft(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let discarded = wikiatlas_store::content::discard_draft(&pool, id).await?;
    Ok(Json(serde_json::json!({ "discarded": discarded })))
}

pub async fn commit(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(req): Json<CommitReq>,
) -> Result<Json<CommitResp>, ApiError> {
    let out = wikiatlas_store::content::commit(
        &pool,
        id,
        &req.body_md,
        req.base_ver,
        &req.summary,
        HUMAN,
    )
    .await?;
    Ok(Json(CommitResp {
        ver: out.ver,
        updated_at: out.updated_at,
        sync_job_id: out.sync_job_id,
    }))
}

pub async fn versions(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<VersionResp>>, ApiError> {
    let rows = wikiatlas_store::content::versions(&pool, id, 50).await?;
    Ok(Json(
        rows.into_iter()
            .map(|(ver, created_at, summary)| VersionResp {
                ver,
                created_at,
                summary,
            })
            .collect(),
    ))
}

/// GET /api/overview：首页统计卡 + 最近更新（`design.md §6.1`，只读聚合）。
pub async fn overview(
    State(pool): State<PgPool>,
) -> Result<Json<wikiatlas_store::overview::HomeStats>, ApiError> {
    Ok(Json(wikiatlas_store::overview::home(&pool).await?))
}

/// GET /api/nodes/{id}/links：节点的来源链（`design.md §6.3` 的来源条）。
pub async fn node_links(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<wikiatlas_store::overview::NodeLink>>, ApiError> {
    Ok(Json(
        wikiatlas_store::overview::links_of_node(&pool, id).await?,
    ))
}

/// GET /api/nodes/{id}/versions/{ver}：某版快照正文（版本历史预览）。
pub async fn version_body(
    State(pool): State<PgPool>,
    Path((id, ver)): Path<(Uuid, i32)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let body = wikiatlas_store::content::version_body(&pool, id, ver).await?;
    Ok(Json(json!({ "ver": ver, "body_md": body })))
}

// ============================================================ 关系与综合（M5）

pub async fn relation_types(
    State(pool): State<PgPool>,
) -> Result<Json<Vec<domain::relation::RelationType>>, ApiError> {
    Ok(Json(wikiatlas_store::relations::list_types(&pool).await?))
}

pub async fn list_relations(
    State(pool): State<PgPool>,
    Query(q): Query<RelationQuery>,
) -> Result<Json<Vec<domain::relation::RelationView>>, ApiError> {
    Ok(Json(
        wikiatlas_store::relations::list_for_node(&pool, q.node_id).await?,
    ))
}

pub async fn create_relation(
    State(pool): State<PgPool>,
    Json(req): Json<CreateRelationReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = wikiatlas_store::relations::create(&pool, req.from_id, req.to_id, &req.r#type, HUMAN)
        .await?;
    Ok(Json(json!({ "id": id })))
}

pub async fn delete_relation(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    wikiatlas_store::relations::delete(&pool, id).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn list_synthesis(
    State(pool): State<PgPool>,
) -> Result<Json<Vec<domain::Node>>, ApiError> {
    Ok(Json(
        wikiatlas_store::relations::list_synthesis(&pool).await?,
    ))
}

pub async fn create_synthesis(
    State(pool): State<PgPool>,
    Json(req): Json<SynthesisReq>,
) -> Result<Json<domain::Node>, ApiError> {
    Ok(Json(
        wikiatlas_store::relations::create_synthesis(
            &pool,
            req.parent_id,
            &req.title,
            &req.doc_type,
            &req.work_ids,
        )
        .await?,
    ))
}

/// POST /api/nodes/{id}/index：宿主按当前子树重算系列 / 宇宙索引页。
pub async fn sync_index(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<domain::Node>, ApiError> {
    Ok(Json(
        wikiatlas_store::relations::sync_index(&pool, id).await?,
    ))
}

// ============================================================ Review / 体检（M6）

pub async fn list_suggestions(
    State(pool): State<PgPool>,
    Query(q): Query<ReviewQuery>,
) -> Result<Json<Vec<domain::review::Suggestion>>, ApiError> {
    let state = q
        .state
        .as_deref()
        .map(domain::review::SuggestionState::parse)
        .transpose()?;
    Ok(Json(wikiatlas_store::review::list(&pool, state).await?))
}

pub async fn create_suggestion(
    State(pool): State<PgPool>,
    Json(req): Json<CreateSuggestionReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let kind = domain::review::SuggestionKind::parse(&req.kind)?;
    let source = req
        .source
        .as_deref()
        .map(domain::library::Source::parse)
        .transpose()?;
    let id = wikiatlas_store::review::create(
        &pool,
        kind,
        req.node_id,
        source,
        req.external_id,
        req.payload,
        HUMAN,
    )
    .await?;
    Ok(Json(json!({ "id": id })))
}

pub async fn accept_suggestion(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    body: Option<Json<DecisionReq>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let reason = body.map(|Json(b)| b.reason).unwrap_or_default();
    let out = wikiatlas_store::review::decide(&pool, id, true, &reason).await?;
    Ok(Json(
        json!({ "suggestion": out.suggestion, "action": out.action }),
    ))
}

pub async fn reject_suggestion(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    body: Option<Json<DecisionReq>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let reason = body.map(|Json(b)| b.reason).unwrap_or_default();
    let out = wikiatlas_store::review::decide(&pool, id, false, &reason).await?;
    Ok(Json(
        json!({ "suggestion": out.suggestion, "action": out.action }),
    ))
}

pub async fn health_findings(
    State(pool): State<PgPool>,
    Query(q): Query<FindingQuery>,
) -> Result<Json<Vec<domain::review::HealthFinding>>, ApiError> {
    let state = q
        .state
        .as_deref()
        .map(domain::review::FindingState::parse)
        .transpose()?;
    Ok(Json(wikiatlas_store::audit::list(&pool, state).await?))
}

pub async fn run_audit(State(pool): State<PgPool>) -> Result<Json<serde_json::Value>, ApiError> {
    let id = jobs::enqueue(&pool, "audit", &json!({})).await?;
    Ok(Json(json!({ "jobId": id })))
}

// ============================================================ 检索 / 存为资料（M8）

pub async fn search(
    State(pool): State<PgPool>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Vec<wikiatlas_store::search::SearchHit>>, ApiError> {
    Ok(Json(
        wikiatlas_store::search::search(&pool, &q.q, q.limit.unwrap_or(20)).await?,
    ))
}

pub async fn save_answer(
    State(pool): State<PgPool>,
    Json(req): Json<SaveAnswerReq>,
) -> Result<Json<domain::Node>, ApiError> {
    Ok(Json(
        wikiatlas_store::answers::save_as_doc(
            &pool,
            req.parent_id,
            &req.title,
            &req.body_md,
            &req.ref_ids,
        )
        .await?,
    ))
}

// ============================================================ 设置 / 用量 / 导出导入（M9）

pub async fn get_settings(State(pool): State<PgPool>) -> Result<Json<serde_json::Value>, ApiError> {
    let settings = wikiatlas_store::secrets::list_settings(&pool).await?;
    let keys = wikiatlas_store::secrets::list_keys(&pool).await?;
    let mut values = serde_json::Map::new();
    for (k, v) in settings {
        values.insert(k, v);
    }
    let configured = keys
        .into_iter()
        .map(|k| (k, serde_json::Value::Bool(true)))
        .collect::<serde_json::Map<_, _>>();
    Ok(Json(json!({
        "settings": values,
        "secrets_configured": configured,
    })))
}

pub async fn put_settings(
    State(pool): State<PgPool>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let obj = body.as_object().ok_or_else(|| {
        ApiError::from(domain::Error::Validation("设置必须是 JSON object".into()))
    })?;
    for (key, value) in obj {
        if key.contains("api_key") || key.contains("password") || key.ends_with(".token") {
            return Err(ApiError::from(domain::Error::Validation(format!(
                "{key} 是密钥，必须走 /api/secrets"
            ))));
        }
        wikiatlas_store::secrets::put_setting(&pool, key, value).await?;
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn put_secrets(
    State(pool): State<PgPool>,
    Json(body): Json<serde_json::Map<String, serde_json::Value>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    for (key, value) in body {
        let value = value.as_str().ok_or_else(|| {
            ApiError::from(domain::Error::Validation(format!(
                "密钥 {key} 的值必须是字符串"
            )))
        })?;
        if key.trim().is_empty() || value.trim().is_empty() {
            return Err(ApiError::from(domain::Error::Validation(
                "密钥名和值都不能为空".into(),
            )));
        }
        wikiatlas_store::secrets::put_secret(&pool, &key, value).await?;
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn usage(
    State(pool): State<PgPool>,
) -> Result<Json<wikiatlas_store::usage::UsageSummary>, ApiError> {
    Ok(Json(wikiatlas_store::usage::summary(&pool).await?))
}

pub async fn export_project(
    State(pool): State<PgPool>,
) -> Result<Json<wikiatlas_store::transfer::ProjectBundle>, ApiError> {
    Ok(Json(wikiatlas_store::transfer::export(&pool).await?))
}

pub async fn import_project(
    State(pool): State<PgPool>,
    Json(bundle): Json<wikiatlas_store::transfer::ProjectBundle>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let nodes = wikiatlas_store::transfer::import(&pool, &bundle).await?;
    Ok(Json(json!({ "ok": true, "nodes": nodes })))
}

// ============================================================ 媒体库（M1）

use axum::extract::Query;
use serde_json::json;
use wikiatlas_store::{jobs, secrets};

use crate::dto::{
    AfterQuery, BuildLinkReq, BuiltDocResp, CreateRunReq, IgnoreReq, LibraryQuery, ScanReq,
};

pub async fn library_pool(
    State(pool): State<PgPool>,
    Query(q): Query<LibraryQuery>,
) -> Result<Json<Vec<domain::library::PoolEntry>>, ApiError> {
    let f = q.into_filter()?;
    Ok(Json(wikiatlas_store::library::list_pool(&pool, &f).await?))
}

pub async fn library_manifest(
    State(pool): State<PgPool>,
    Query(q): Query<LibraryQuery>,
) -> Result<Json<Vec<domain::library::ManifestRow>>, ApiError> {
    let f = q.into_filter()?;
    Ok(Json(
        wikiatlas_store::library::list_manifest(&pool, &f).await?,
    ))
}

/// POST /api/library/scan：**只入队**（扫描在 worker 里跑，见 docs/backend.md §1）。
pub async fn library_scan(
    State(pool): State<PgPool>,
    body: Option<Json<ScanReq>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let sources = body.map(|Json(b)| b.sources).unwrap_or_default();
    // 空数组 = 扫全部已配置的源：payload 里干脆不放 sources 键（worker 按缺失处理）。
    let payload = if sources.is_empty() {
        json!({})
    } else {
        json!({ "sources": sources })
    };
    let id = jobs::enqueue(&pool, "scan", &payload).await?;
    Ok(Json(json!({ "jobId": id })))
}

pub async fn library_scan_runs(
    State(pool): State<PgPool>,
) -> Result<Json<Vec<domain::library::ScanRun>>, ApiError> {
    Ok(Json(
        wikiatlas_store::library::list_scan_runs(&pool, 50).await?,
    ))
}

/// 池子页顶部：哪些源配好了（**只有布尔，没有值**）+ 队列里还有没有活。
pub async fn library_status(
    State(pool): State<PgPool>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let keys = secrets::list_keys(&pool).await?;
    let has = |k: &str| keys.iter().any(|x| x == k);
    let configured = json!({
        "gameatlas": has("gameatlas.base_url") && has("gameatlas.password"),
        "emby": has("emby.base_url") && has("emby.token"),
        "komga": has("komga.base_url") && has("komga.api_key"),
    });
    let depth = jobs::queue_depth(&pool).await?;
    Ok(Json(
        json!({ "configured": configured, "queueDepth": depth }),
    ))
}

// ============================================================ 建档 / 解链 / 忽略（M2）

/// POST /api/library/links：**原子**建【节点 + 链】（不建库、不联网，零 LLM）。
pub async fn build_link(
    State(pool): State<PgPool>,
    Json(req): Json<BuildLinkReq>,
) -> Result<Json<BuiltDocResp>, ApiError> {
    let out = wikiatlas_store::library::build_doc(&pool, &req.into_build()?).await?;
    Ok(Json(BuiltDocResp {
        node: out.node,
        link_id: out.link_id,
        reused: out.reused,
    }))
}

/// DELETE /api/library/links/{id}：**解链，保留文档**。返回文档节点 id（前端提示用）。
pub async fn unlink(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let node_id = wikiatlas_store::library::unlink(&pool, id).await?;
    Ok(Json(json!({ "nodeId": node_id })))
}

pub async fn list_ignores(
    State(pool): State<PgPool>,
) -> Result<Json<Vec<domain::library::Ignore>>, ApiError> {
    Ok(Json(wikiatlas_store::library::list_ignores(&pool).await?))
}

pub async fn add_ignore(
    State(pool): State<PgPool>,
    Json(req): Json<IgnoreReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let source = req
        .source
        .as_deref()
        .map(domain::library::Source::parse)
        .transpose()?;
    let id = wikiatlas_store::library::add_ignore(
        &pool,
        source,
        req.external_id,
        req.group_key,
        &req.reason,
    )
    .await?;
    Ok(Json(json!({ "id": id })))
}

pub async fn remove_ignore(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    wikiatlas_store::library::remove_ignore(&pool, id).await?;
    Ok(Json(json!({ "ok": true })))
}

/// GET /api/library/links/{id}/relink-candidates：失效链的同源候选（零 LLM）。
pub async fn relink_candidates(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<domain::sync::RelinkCandidate>>, ApiError> {
    Ok(Json(
        wikiatlas_store::sync::relink_candidates(&pool, id).await?,
    ))
}

/// POST /api/library/links/{id}/relink：旧链 replaced，新链 active；文档不动。
pub async fn relink(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(req): Json<RelinkReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let new_id = wikiatlas_store::sync::relink(
        &pool,
        id,
        &req.external_id,
        domain::library::LinkedBy::Manual,
    )
    .await?;
    Ok(Json(json!({ "linkId": new_id })))
}

/// POST /api/library/links/{id}/sync：手动重推。`force` 允许同版本再推一次。
pub async fn resync_link(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // 先在 api 侧明确拒绝不可反哺链；worker 仍会再校验一次。
    wikiatlas_store::sync::sync_job_for_link(&pool, id).await?;
    let job_id = jobs::enqueue(&pool, "sync", &json!({ "link_id": id, "force": true })).await?;
    Ok(Json(json!({ "jobId": job_id })))
}

// ============================================================ 工单 / SSE（M3）

pub async fn list_sessions(
    State(pool): State<PgPool>,
    Query(q): Query<SessionQuery>,
) -> Result<Json<Vec<wikiatlas_store::runs::SessionInfo>>, ApiError> {
    Ok(Json(
        wikiatlas_store::runs::sessions_for_node(&pool, q.node_id).await?,
    ))
}

pub async fn create_session(
    State(pool): State<PgPool>,
    Json(req): Json<CreateSessionReq>,
) -> Result<Json<wikiatlas_store::runs::SessionInfo>, ApiError> {
    let node = wikiatlas_store::nodes::get(&pool, req.node_id).await?;
    let id = format!("work:{}:{}", req.node_id, Uuid::now_v7());
    let title = req
        .title
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "新对话".into());
    wikiatlas_store::runs::ensure_session(&pool, &id, &req.node_id.to_string(), &title).await?;
    let rows = wikiatlas_store::runs::sessions_for_node(&pool, node.id).await?;
    let session = rows
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| ApiError::from(domain::Error::NotFound("新建会话".into())))?;
    Ok(Json(session))
}

pub async fn session_detail(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
    Query(q): Query<SessionMessageQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let messages =
        wikiatlas_store::runs::session_messages(&pool, &id, q.limit.unwrap_or(200)).await?;
    Ok(Json(json!({ "session_id": id, "messages": messages })))
}

/// GET /api/skills：从项目 `skills/*/SKILL.md` 实时扫描，供对话输入框显式加载。
pub async fn list_skills() -> Json<serde_json::Value> {
    let rows: Vec<serde_json::Value> =
        wikiatlas_agent::prompt::load(&wikiatlas_agent::prompt::skills_root())
            .into_iter()
            .map(|s| {
                json!({
                    "name": s.name,
                    "description": s.description,
                    "group": s.group,
                    "layer": s.layer,
                    "chars": s.body.chars().count()
                })
            })
            .collect();
    Json(json!({ "skills": rows }))
}

/// POST /api/runs：**建工单 + 入队**（跑活在 worker；`docs/backend.md §1`）。
pub async fn create_run(
    State(pool): State<PgPool>,
    Json(req): Json<CreateRunReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // 目标节点必须在（早失败，别让 worker 里才报）
    let node = wikiatlas_store::nodes::get(&pool, req.node_id).await?;
    let cfg = wikiatlas_agent::client::LlmConfig::load(&pool)
        .await
        .unwrap_or(None);
    let model = cfg.map(|c| c.model).unwrap_or_else(|| "未配置".into());
    let session_id = req
        .session_id
        .unwrap_or_else(|| format!("work:{}", node.id));
    let prefix = format!("work:{}", node.id);
    if session_id != prefix && !session_id.starts_with(&format!("{prefix}:")) {
        return Err(ApiError::from(domain::Error::Validation(
            "session_id 不属于当前节点".into(),
        )));
    }
    let session_title = if session_id == prefix {
        node.title.clone()
    } else {
        "新对话".into()
    };
    wikiatlas_store::runs::ensure_session(&pool, &session_id, &node.id.to_string(), &session_title)
        .await?;
    let catalog = wikiatlas_agent::prompt::load(&wikiatlas_agent::prompt::skills_root());
    let known: std::collections::HashSet<&str> = catalog.iter().map(|s| s.name.as_str()).collect();
    let skills: Vec<String> = req
        .context
        .skills
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if let Some(unknown) = skills.iter().find(|s| !known.contains(s.as_str())) {
        return Err(ApiError::from(domain::Error::Validation(format!(
            "技能不存在：{unknown}"
        ))));
    }
    let context = json!({
        "doc_mode": req.context.doc_mode.unwrap_or_else(|| "edit".into()),
        "one_shot": req.context.one_shot,
        "skills": skills,
    });
    let run_id = format!("run:{}", Uuid::now_v7());
    wikiatlas_store::runs::create(
        &pool,
        &wikiatlas_store::runs::NewRun {
            id: run_id.clone(),
            workspace: "default".into(),
            intent: req.intent.unwrap_or_else(|| "对话".into()),
            goal: req.prompt,
            model,
            checkpoint: json!({
                "node_id": node.id,
                "session_id": session_id,
                "context": context
            }),
        },
    )
    .await?;
    let job_id = jobs::enqueue(&pool, "run", &json!({ "run_id": run_id })).await?;
    Ok(Json(
        json!({ "runId": run_id, "jobId": job_id, "sessionId": session_id }),
    ))
}

/// GET /api/runs/{id}/events：SSE。**事件是从 `run_events` 轮询出来的**——
/// api 与 worker 只共用 PG，不共享内存（`docs/backend.md §1` 的纪律）。
pub async fn run_events(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
    Query(q): Query<AfterQuery>,
) -> Result<
    axum::response::sse::Sse<
        impl futures_core::Stream<
            Item = std::result::Result<axum::response::sse::Event, std::convert::Infallible>,
        >,
    >,
    ApiError,
> {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use std::time::Duration;
    wikiatlas_store::runs::get(&pool, &id).await?; // 不存在就 404

    let stream = async_stream::stream! {
        let mut cursor = q.after;
        loop {
            let evs = wikiatlas_store::runs::events_after(&pool, &id, cursor)
                .await
                .unwrap_or_default();
            let n = evs.len();
            for e in evs {
                cursor = e.seq;
                let data = json!({ "seq": e.seq, "type": e.r#type, "payload": e.payload });
                if let Ok(ev) = Event::default().event(e.r#type).json_data(data) {
                    yield Ok(ev);
                }
            }
            if n == 0 {
                if let Ok(run) = wikiatlas_store::runs::get(&pool, &id).await {
                    if matches!(run.status.as_str(), "completed" | "failed" | "cancelled") {
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

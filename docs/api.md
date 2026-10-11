# WikiAtlas v2 · API 与事件契约

> **这是后端与前端之间唯一的接口契约。** 前端 `src/api.ts` 的 DTO 必须与本文一致；事件类型由后端生成给前端，禁止手抄。
> **关联**：工具面（AI 用的）见 `tools.md`；数据库见 `database.md`；前端状态模型见 `frontend.md §4 §5`。
> **进度标记**：✅ = M0 已落地；⬜ = 规划中（里程碑见 `README.md §4`）。

---

## 0. 通用约定

- **前缀**：所有接口在 `/api` 下。除登录/健康检查外，写接口要求管理态会话。
- **命名**：JSON 字段一律 **snake_case**（Rust serde 默认）。**别在前端改名**——契约只有一份。
- **ID**：`nodes.id` / `relations.id` / `library_links.id` 是 UUIDv7 字符串；`runs.id` / `sessions.id` 是**自由 text**（v1 沿用，如 `work:<uuid>`）。
- **时间**：一律 RFC3339 UTC 字符串（DB `timestamptz`）。
- **空值**：**空列表给 `[]` 不给 `null`**（前端 `.length` 直接崩）。
- **取版本**：正文写入一律走乐观并发（`base_ver`）；不符返回 `409`。
- **错误体**（统一形状）：

```json
{ "error": "not_found | duplicated | version_conflict | validation | internal",
  "message": "人类可读",
  "current_ver": 12,        // 仅 version_conflict
  "base_ver": 11 }          // 仅 version_conflict
```

| HTTP | error | 触发 |
|---|---|---|
| 400 | `validation` | 领域校验失败（如 work 缺介质） |
| 401 | — | 未登录访问写接口 / 私有节点 |
| 403 | — | 无权限 |
| 404 | `not_found` | 节点 / 版本 / 运行不存在，或对访客不可见 |
| 409 | `version_conflict` | 定稿时 `base_ver` 与库中不符 |
| 409 | `duplicated` | 唯一约束撞车（如同级 sort_key、链 `(source,external_id)`） |
| 500 | `internal` | 未预期 |

> 错误映射在 `crates/api/src/error.rs`（v1 的 `ErrNotFound/ErrConflict/ErrValidation` → HTTP 的同款）。**新增错误变体要同时改这里和前端 `ApiErrorBody`。**

---

## 1. REST 端点清单

### 1.1 健康与树

| 方法 | 路径 | 说明 | 状态 |
|---|---|---|---|
| GET | `/api/healthz` | 存活探针 | ✅ |
| GET | `/api/tree` | **扁平**节点列表（父在前、同级按 `sort_key`）；前端拼树 | ✅ |
| GET | `/api/nodes/{id}` | 节点 + 定稿 + 草稿（`NodeContent`） | ✅ |
| POST | `/api/nodes` | 建节点（走领域校验：work 必带介质、doc 不带） | ✅ |
| PATCH | `/api/nodes/{id}` | 改标题/别名/标签/父/状态/可见性 | ⬜ |
| POST | `/api/nodes/{id}/archive` | 归档（默认删除策略） | ⬜ |
| POST | `/api/nodes/{id}/move` | 移动（父/排序） | ⬜ |

### 1.2 内容（人的写路径；AI 复用同一条）

| 方法 | 路径 | 说明 | 状态 |
|---|---|---|---|
| PUT | `/api/nodes/{id}/draft` | 存草稿（覆盖写，带 `lease_until`） | ✅ |
| POST | `/api/nodes/{id}/commit` | 定稿（乐观并发；不符 → 409 + `current_ver`） | ✅ |
| GET | `/api/nodes/{id}/versions` | 快照列表 | ✅ |
| GET | `/api/nodes/{id}/versions/{ver}` | 单版快照（diff 用） | ⬜ |
| POST | `/api/nodes/{id}/revert` | 回滚到某版（= 提交一个新版本） | ⬜ |

> **人与模型共用同一条写路径**：`content_drafts`（防抖/节流覆盖写）→ `commit` 事务（`contents` ver+1 + `content_versions` 快照 + `write_events` + 清草稿）。

### 1.3 关系

| 方法 | 路径 | 说明 | 状态 |
|---|---|---|---|
| GET | `/api/relations?node_id=` | 某节点的关系（出边 + 入边，带方向标签） | ⬜ |
| POST | `/api/relations` | 建关系（type 必须在 `relation_types`；成环应用层校验） | ⬜ |
| DELETE | `/api/relations/{id}` | 删关系 | ⬜ |
| GET | `/api/relation-types` | 关系类型契约表（UI 渲染方向标签用） | ⬜ |

### 1.4 媒体库

| 方法 | 路径 | 说明 | 状态 |
|---|---|---|---|
| POST | `/api/library/scan` | 立即扫描（入队 scan job；返回 job_id） | ⬜ |
| GET | `/api/library/manifest` | 清单（可按 source / 容器 / 有无文档过滤） | ⬜ |
| GET | `/api/library/pool` | **池子**：无文档的库条目 | ⬜ |
| POST | `/api/library/links` | 建档 / 挂链（**原子**：节点 + 链同事务） | ⬜ |
| DELETE | `/api/library/links/{id}` | 解链（保留文档） | ⬜ |
| POST | `/api/library/links/{id}/relink` | 重挂（旧链 `replaced`） | ⬜ |
| GET | `/api/library/links/{id}/relink-candidates` | 宿主建议的候选（归一化 + trigram） | ⬜ |
| POST | `/api/library/ignores` | 忽略（单条 / 整组） | ⬜ |
| POST | `/api/library/links/{id}/sync` | 手动重推反哺 | ⬜ |
| GET | `/api/library/suggestions` | 该条目的库建议（含 reject 理由） | ⬜ |

### 1.5 工单 / 会话 / 运行

| 方法 | 路径 | 说明 | 状态 |
|---|---|---|---|
| GET | `/api/sessions` | 会话列表（一个文档 = 一个默认会话） | ⬜ |
| POST | `/api/sessions` | 新会话（挂在节点 / 资料 / 全局） | ⬜ |
| GET | `/api/sessions/{id}` | 会话详情 + 消息投影 | ⬜ |
| POST | `/api/runs` | 发起工单（`CreateRunBody`，见 §2） | ⬜ |
| GET | `/api/runs/{id}` | 工单状态 | ⬜ |
| POST | `/api/runs/{id}/resume` | 续跑 | ⬜ |
| POST | `/api/runs/{id}/cancel` | 取消（置 `cancel_requested`，worker 在工具边界检查） | ⬜ |
| GET | `/api/runs/{id}/events?cursor=` | **SSE**（见 §4） | ⬜ |

### 1.6 Review / 体检 / 检索

| 方法 | 路径 | 说明 | 状态 |
|---|---|---|---|
| GET | `/api/suggestions` | Review 队列（建档/关联/合并/研究/忽略） | ⬜ |
| POST | `/api/suggestions/{id}/accept` | 接受（执行对应动作） | ⬜ |
| POST | `/api/suggestions/{id}/reject` | 拒绝（可带理由，回喂） | ⬜ |
| GET | `/api/health-findings` | 体检结果（确定性 + 结构洞察） | ⬜ |
| POST | `/api/health-findings/run` | 跑一次体检（入队 audit job） | ⬜ |
| GET | `/api/search?q=` | 检索（标题 + 正文 + 别名；bigram/trigram） | ⬜ |

### 1.7 设置 / 运维

| 方法 | 路径 | 说明 | 状态 |
|---|---|---|---|
| GET/PUT | `/api/settings` | 行为配置（模型路由 / 窗口 / 扫描周期 / 三源地址） | ⬜ |
| PUT | `/api/secrets` | 密钥（独立表，写入不回显） | ⬜ |
| POST | `/api/login` / `POST /api/logout` | 访问密码登录（HttpOnly 会话 Cookie） | ⬜ |
| GET | `/api/usage` | 用量与成本汇总 | ⬜ |
| POST | `/api/export` / `POST /api/import` | 整树导出/导入 | ⬜ |

---

## 2. 关键 DTO

```ts
// —— 树与内容（✅ 已落地）——
type NodeKind = 'universe' | 'series' | 'work' | 'doc'
type Medium   = 'game' | 'movie' | 'tv' | 'manga' | 'book' | 'other'
type NodeStatus = 'draft' | 'ready' | 'archived'
type Visibility = 'private' | 'public'

interface Node { id, parent_id: string|null, kind, medium: Medium|null,
  title, title_norm, aliases: string[], tags: string[], status, visibility,
  sort_key, created_at, updated_at }

interface Content  { node_id, body_md, ver: number, updated_by, updated_at }
interface Draft    { node_id, body_md, base_ver: number, by, lease_until, updated_at }
interface NodeContent { node: Node; content: Content|null; draft: Draft|null }

interface CreateNodeReq { parent_id?: string|null; kind: NodeKind; medium?: Medium|null; title: string }
interface PutDraftReq   { body_md: string; base_ver: number; lease_until: string }
interface CommitReq     { base_ver: number; body_md: string; summary?: string }
interface CommitResp    { ver: number; updated_at: string }
interface Version       { ver: number; created_at: string; summary: string }

// —— 工单（⬜ 规划）——
interface CreateRunBody {
  session_id?: string           // 缺省则新建
  node_id?: string              // 目标节点（写正文/修订/问答）
  goal: string                  // 用户那句话
  intent: 'create_wiki' | 'continue_wiki' | 'rewrite_section'
        | 'write_doc' | 'organize_tree' | 'answer' | 'sync_library'
  context?: RunContext          // 三件套之一，见 tools.md：docMode / oneShot / 选段 / @引用
  model?: string                // 覆盖默认（可选）
}
interface RunContext {          // 显式字段，别铺平 Record（v1 的 @ 引用就这样坏过）
  doc_mode?: 'edit' | 'read'
  one_shot?: boolean
  selection?: { text: string; prompt: string }
  mentions?: string[]           // 引用的 node_id
  attachments?: string[]
}

// —— 媒体库（⬜ 规划）——
interface ManifestEntry { id, source: 'gameatlas'|'emby'|'komga',
  external_id, title, subtitle?, aliases: string[], cover_url?,
  year?, container: { kind: 'series'|'library', id?, name }, kind, format?, url, extra }
interface LibraryLink { id, node_id, source, external_id, url?, title_hint?,
  role: 'work'|'mix', state: 'active'|'stale'|'replaced',
  missing_since?, replaced_by?, linked_by: 'manual'|'pool'|'archive'|'scan', created_at }
interface SyncTarget { link_id, target: 'gameatlas'|'komga',
  synced_ver?: number, status: 'pending'|'ok'|'failed', message, synced_at? }
```

---

## 3. 内容写入时序（人 / AI 共用）

```
前端编辑器（防抖 1.5s）
  → PUT /api/nodes/{id}/draft  { body_md, base_ver, lease_until }
  → 用户点「定稿」
  → POST /api/nodes/{id}/commit { base_ver, body_md, summary }
       ├─ 事务：contents.ver+1 / content_versions 快照 / write_events / 清草稿
       ├─ 成功 → 200 { ver, updated_at }
       └─ base_ver 不符 → 409 { current_ver }
  → 定稿事件 → 入队 sync job（反哺）

AI 侧同路径：写工具产出 body_md → 草稿（节流）→ 质量门 → 定稿（人点，或宿主按质量门自动）。
```

**规范形**：所有写路径入口都过 `domain::text::normalize_markdown`（丢孤立 `<br />` 行、连续空行折一个、**总以换行结尾**）。前端必须**同规则再来一份**（`src/markdown.ts`），否则状态条会一直显示"有改动"。

---

## 4. 事件契约

后端出**两条事件线**（v1 的设计，保留）：

### 4.1 `response.*` —— OpenAI Responses 形状（交给 X runtime）

- `response.created` · `response.output_item.added/done` · `response.output_text.delta`
- `response.reasoning_summary_text.delta` · `response.function_call_arguments.delta/done`
- `response.completed`

规矩：
- **输出项自足**：`output_item.done` 带最终形态（完整文本 / 完整参数 / 工具结果，工具结果摘要挂在项上）。
- `sequence_number` **只加在 `response.*` 上**（store 落库时统一发号，从 0 连续 +1）。

### 4.2 `wikiatlas.*` —— 宿主旁路（交给宿主部件）

`todo` · `usage` · `notice` · `content.staging` · `content.delta` · `content.committed` ·
`tree` · `link.*`（链状态变化）· `sync.*`（反哺状态）· `run.end` · `context.*`（压缩）

规矩：**不占 `sequence_number`**；宿主部件消费，不 dump payload。

### 4.3 SSE 纪律

1. **游标只在成功处理后推进**（处理失败不推进，重放同一条）。
2. seq 缺口 / 解析失败 → **重新基线**（重拉快照，不硬接）。
3. 断线**指数退避**重连。
4. **草稿只读**：`content.delta` 仅是加速通道；**重连后一律从服务端的草稿行重新基线**。
5. `content.delta` 是**节流后的"段"**（~200ms），**逐条 `function_call_arguments.delta` 对写正文工具不转发**。

### 4.4 事件类型生成

- 事件清单**不得出现在三处**（v1 的 `types.ts` + 两个数组）。
- 由 Rust 事件常量**生成 TS 联合类型**（`ts-rs` / `typeshare`）；**这是消灭漂移的硬要求**。
- 适配层（`response.*` → X 会话模型）**单独成模块**（`src/ai/transport/`）——换 UI 框架时只重写这里。

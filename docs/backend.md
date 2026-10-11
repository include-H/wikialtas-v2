# WikiAtlas v2 · 后端设计（Rust）

> 功能清单见 `docs/features.md`，数据层见 `docs/database.md`，接口见 `docs/api.md`，工具/提示词见 `docs/tools.md` / `docs/prompts.md`，队列与 agent 的行为证据见 `docs/behavior-notes.md`。
> ✅ **Rust 已确认**（决策 D38）：llm_wiki 的 agent core 本身就是 Rust（`runtime.rs` 5816 行 / `tools.rs` 3888 行，含工具循环 / 流式 / skills），可行性已被验证。§9 保留**退出条件**（尖刺 B 做完若"改一次 prompt 往返明显慢于 v1"才回 Go），但**不再是默认路径**。

## 1. 进程形态

```
     浏览器（AntD X）
        │ HTTP + SSE
   ┌────▼─────┐        ┌──────────────┐
   │  api     │        │  worker      │   两个进程，同一个 PG
   │ 路由/SSE  │        │ run 执行器    │
   │ 短事务读  │        │ 扫描 / 反哺   │
   └────┬─────┘        │ 体检        │
        │              └──────┬───────┘
        └────────┬─────────────┘
            ┌────▼─────┐
            │ Postgres │  ← 唯一真相
            └──────────┘
```

- **api**：HTTP + SSE + 短事务（人的读写）。**不认识 LLM，不跑工单。**
- **worker**：消费 `jobs`（`FOR UPDATE SKIP LOCKED` + 租约）；跑 run 执行器 / 媒体库扫描 / 反哺 / 体检。**LLM 调用永远在事务外。**
- 两者共用 domain 与 store，不共享内存状态（v1 的进程内锁与内存标志是重写要消灭的东西）。

## 2. crate 选型

| 用途 | 选 | 备注 |
|---|---|---|
| HTTP | **axum** | 与 tokio/tower 同族；SSE 直接用 `axum::response::sse` |
| 异步运行时 | **tokio** | — |
| 中间件/可观测 | **tower-http**（trace / compression / cors） | — |
| 数据库 | **sqlx**（PG + 编译期校验 + 自带 migrate） | 不引 ORM：v1 的 SQL 是人写的、且需要 `FOR UPDATE SKIP LOCKED` 这类控制 |
| 序列化 | **serde** / **serde_json** | 事件 DTO |
| 错误 | **thiserror**（域错误）+ anyhow（边界） | 对应 v1 的 `ErrNotFound/ErrConflict/ErrValidation` → 统一映射到 HTTP |
| 日志 | **tracing** + `tracing-subscriber`（JSON） | 事件表 + 结构化日志 |
| ID | **uuid**（v7） | 与 v1 数据一致（时间有序） |
| 时间 | **time** 或 chrono | 实现时定，别两套混用 |
| LLM 客户端 | **reqwest** + `eventsource-stream`（自写 Responses API 客户端） | 我们的后端事件本来就用 Responses API 形状；**自写比套 SDK 更可控**（v1 也是自写） |
| 工具 schema | **schemars** | 从 Rust 类型生成 JSON Schema → 消灭"schema 与实现漂移" |
| TS 类型 | **ts-rs** 或 **typeshare** | 把事件/工具类型生成给前端（v1 三处手抄） |
| 媒体源 | reqwest + 各源鉴权（cookie / token / api key） | — |
| 前端产物嵌入 | `rust-embed` | 单二进制部署 |
| 测试 | `sqlx::test`（每个测试一个 PG schema）· **insta**（prompt 渲染快照）· 真跑副本库 | — |

版本实现时锁死，不在文档里写死版本号（v1 的教训之一就是文档跟不上代码）。

## 3. crate 结构（分层与边界）

```
crates/
  domain/    纯类型 + 纯规则，零 IO
             · 树与节点、链、草稿/定稿/快照、事件枚举、工具名与写工具清单
             · 落位规则（一个条目一篇文档 / 系列容器 / 介质后缀）
             · 归一化与匹配（标题归一、别名、失效链的重挂候选）
             · 上下文预算与裁剪顺序
             → 单测重灾区（v1 的教训：规则写在 httpapi 里，没有测试基建）
  store/     sqlx 仓储：nodes / contents / drafts / versions / write_events /
             relations / library_links / sync_targets / manifest / jobs /
             runs / run_events / sessions / health_findings
             · WithTx 助手；一切多步写在一个事务里（v1 有非事务的建档路径）
  sources/   三个媒体库客户端 + 清单构建 + 扫描比对（新增/消失/变更）
  agent/     run 执行器：prompt 渲染、技能注入、工具循环、流式投影、质量门
  api/       axum 路由 + SSE + DTO（serde）
  worker/    队列消费：run / scan / sync（反哺）/ audit（体检）
```

- **边界纪律**：`domain` 不依赖任何 IO；`api`/`worker` 只做编排，业务规则不许塞进 handler（v1 的 AGENTS.md 就写了这条，v2 继续）。
- 单文件行数上限沿用 300 行；**例外**：`agent::runtime` / `agent::tools` 按子模块拆分后允许更高上限，但文件头必须写清拆分理由（决策 D41）。

## 4. 工具面（护栏沿用 v1，它们验证过）

- `trait Tool { fn name(&self) -> &'static str; fn schema(&self) -> Value; fn kind(&self) -> ToolKind /* Read|Write|Control */; async fn call(...) }`
- **只有用户显式选择**（`docMode=read` / `oneShot`）才剥写工具；**意图不决定能力面**。
- 运行时二次校验：被剥掉的写工具即使被模型幻觉调用也拒绝执行（v1 的双层防护保留）。
- 批量工具一律"波次 + 预检在前 + 全成或全不成 + 上限"。
- 新工具里**必须有链路工具**（v1 缺，靠 API）：解链 / 重挂 / 重建档。
- 工具名与 schema 的一致性由测试守（v1 的 `schemas_test` 思路保留；v2 用 schemars 生成，一致性更强）。

## 5. 最难搬的一块：流式正文投影

模型写正文时，正文是从工具调用的**参数流**里出来的（`function_call_arguments.delta` 累积成一段 JSON，正文在其中一个字符串字段里）。要在**不完整 JSON** 上增量抽出正文并推送：

- v1 的做法：解析累积 JSON 的前缀（`partialStringArg` / `closedStringArg`）+ 按 200ms 节流 + `patch_sections` 的"波次"（按章节分槽）；`write_content` / `patch_section` / `patch_sections` 三个工具各有一条投影路径。
- v2：把它做成**独立的增量 JSON 状态机**（单独 crate 或 `agent::stream` 模块）+ **重点单测**；这是"实时感"的来源，也是最容易写错的地方。
- 配套：草稿落库（节流写）+ `content.delta` 事件（加速通道）；**SSE 只是加速，草稿行才是真相**。

## 6. 队列、租约与恢复

- `jobs(id, kind, payload, state, attempts, run_after, locked_by, locked_until)`；领取用 `SELECT ... FOR UPDATE SKIP LOCKED`。
- **租约过期自动回收** → worker 崩了任务回队列（取代 v1 "重启把所有 running 标成 interrupted 且不续跑"）。
- 取消：`runs.cancel_requested` 标志 + worker 在工具边界检查（不依赖进程内 `CancellationToken` 传递）。
- 上下文重放：`sessions` / `session_messages`（append-only）这套 v1 设计是对的 → **结构保留，直接重用**。
- 并发上限进 DB（不再靠进程内计数器）。

## 7. 反哺子系统（新）

- 触发：**定稿事件**（`write_events` 里的定稿行）→ 入队 `sync` job。
- 每个 (链 × 目标) 一行 `sync_targets`：`synced_ver / status / message / synced_at`。
- 目标能力不同：GameAtlas 写 **正文 + 摘要**（`PUT /api/games/{id}/wiki`）；Komga 写 **metadata 摘要**（`PATCH`）；**Emby 只读，不进队**。
- 语义：**WikiAtlas 独占这些字段，以最新定稿覆盖**；不合并、不做双向 diff。
- 失败可见可重试；重推是幂等的（同 `synced_ver` 不重复推）。

## 8. 媒体库与链的状态机

- 三源客户端：GA（登录表单拿 cookies）· Emby（`X-Emby-Token`，只读）· Komga（`X-API-Key`，读写）。
- 扫描产出清单（表存，不再是一坨 KV）。
- 扫描后比对链，产出三种事件并落 `link.*` 事件：
  - `新增`（清单里有、链上无）→ 池子里出现（"可以建档"）
  - `消失`（链上有、清单里无）→ 链 `stale` + `missing_since`；再次出现 → **自动回 `active`**
  - `变更`（元数据变了）→ 体检项（文档信息要不要跟着改）
- 重挂：宿主用归一化标题给候选（**零 LLM**），用户确认；旧链标 `replaced`，**文档不动**。

## 9. 风险、尖刺与退出条件（因为这是"尝试"）

**风险（诚实说）**
1. **行为知识会丢**：v1 里最值钱的不是代码，是散在注释与工具描述里的**调优经验**（"模型会这样，所以别那样"）。→ **已落 `docs/behavior-notes.md`（M3 前必读，逐条对照）**。
2. **流式投影**（§5）实现难度不低，且它是体验核心。
3. **生态**：没有成熟的 Responses-API 客户端 → 自写（可控，但要自己处理重试/中断/半包）。
4. **速度**：Rust 的迭代速度比 Go 慢，而模型行为的调优是**高频试错**。

**尖刺（P0，建议限时两周，只求一个纵向切片）**
> **尖刺 A（已选，先做）「人的编辑与存储」**：PG 空库 → 建节点 → 编辑器里改正文 → 落库（草稿/定稿/快照/写入事实）→ 乐观并发冲突 → 刷新后内容与版本一致。**不含 LLM**。
> **尖刺 B（下一轮）**：跑一次**真 run**（含流式正文投影 + 至少一个写工具）→ SSE 推到浏览器 → 定稿落库。顺带验证：`skills/` 文件原样可用、工具 schema 生成通、事件契约前端吃得下。
> 分两步的理由：尖刺 A 已经能验证 Rust 生态的**主要不确定性**（axum + sqlx + 事务 + 类型生成 + 前端吃得下），却把最难的流式投影隔离出来单独打。
> 种子数据用 `skills/samples/` 那 19 篇基准（真空库、真文本），不依赖 v1。

**退出条件**：尖刺做不出来、或做出来但改动一次 prompt 的往返时间明显长于 v1 → **回 Go**（PG schema、前端、功能设计全部不受影响；Go 版后端按同一份文档实现）。

## 10. v1 数据与「行为知识」

**不迁 v1 数据**（2026-10 决定）：v2 从**空库**开始，v1 只作**代码与设计参考**（`archive/v1/`）。原先拟的「SQLite → PG 一次性迁移 + 双读核对」随之取消，`docs/database.md` §4 记了这个决定。

**种子数据**用 `skills/samples/` 的 19 篇基准（真文本），够撑起编辑/检索/流式的开发与测试。

**行为知识清单（已完成）**：v1 最值钱的不是代码，是散在注释、工具描述与 prompt 里的调优经验（"模型会这样，所以别那样"，见 §9 风险 1）。这些经验已逐条摘进 **`docs/behavior-notes.md`**，作为 agent 层的实现依据与验收标准。**M3 开工前必读，重写语言不该把踩过的坑再踩一遍。**

## 11. 已落地（P0 尖刺 A：人的编辑与存储）

**环境**：Rust **1.99.0**（rustup，走清华镜像）· PostgreSQL **18.6**（见 `docs/database.md` §7）· 前端 Node 24 + Vite 8。

**crate 现状**（按 §3 的分层，只先立了三层）

| crate | 状态 | 里面有什么 |
|---|---|---|
| `domain` | ✅ | 纯类型（NodeKind/Medium/NodeStatus/WriteSource…）+ 纯规则（标题归一化 · 排序键 · 介质规则）+ **13 条单测** |
| `store` | ✅ | sqlx 仓储：`nodes`（树/建/排序键分配）· `content`（定稿事务/草稿/快照/版本表）· **6 条 `#[sqlx::test]` 集成测** |
| `api` | ✅ | axum 6 个路由 + 错误映射（404/409/400/500）+ CORS |
| `sources` | ✅ M1 | 三个媒体库客户端（GA cookie / Emby token / Komga api-key）+ 清单构建 + 扫描比对（零 LLM） |
| `worker` | 🟡 M1 | `jobs` 消费（`FOR UPDATE SKIP LOCKED` + 租约）；`scan` 已通；`run` / `sync` / `audit` 收到就明确失败（M3/M7/M6 落地） |
| `agent` | ⬜ | 尖刺 B（M3） |

**接口**（人都走这几个；`by` 暂时固定 `human:local`，登录体系落地后由中间件给）

```
GET  /api/healthz
GET  /api/tree                       → 扁平节点列表（父在前、同级按 sort_key）
POST /api/nodes                      → 建节点（走领域校验：work 必带介质、doc 不带）
GET  /api/nodes/{id}                 → 节点 + 定稿 + 草稿
PUT  /api/nodes/{id}/draft           → 存草稿（覆盖写，带 lease）
POST /api/nodes/{id}/commit          → 定稿（乐观并发，不符 → 409 + currentVer）
GET  /api/nodes/{id}/versions        → 快照列表

# 媒体库（M1）
GET  /api/library/pool               → 池子：清单条目 + 有无文档（?source=&container=&has_doc=&include_missing=）
GET  /api/library/manifest           → 清单（同一组过滤）
POST /api/library/scan               → **只入队** scan job（跑在 worker），返回 {jobId}
GET  /api/library/scan-runs          → 最近扫描记账
GET  /api/library/status             → 哪些源配好了（布尔）+ 队列深度
```

**怎么跑**

```bash
set -a; . .secrets/wikiatlas.pg.env; set +a      # 提供 WIKIATLAS_DB_URL
export DATABASE_URL="$WIKIATLAS_DB_URL"          # sqlx::test 要用它建临时库
cargo run -p wikiatlas-api                       # 起在 127.0.0.1:8088，启动时自动跑迁移
cargo run -p wikiatlas-worker                    # **扫描/反哺/体检都在这**（api 只入队）
cargo test --workspace                           # domain 19 · sources 2 · store 9
cd frontend && npm run dev                       # 起在 5173，/api 代理到 8088

# 源密钥（设置页落地前）：secrets 表（真相）或 WIKIATLAS_* 环境变量兜底，见 D53
```

**踩到的两个坑（都是环境性的，记下来免得复现）**

1. **本机全局 `NODE_ENV=production` + npm `omit=dev`** → `npm install` 会**静默跳过所有 devDependencies**（`vite` 都不装），而 `npm ls` 显示"up to date"。修法：`frontend/.npmrc` 里写 `include=dev`。
2. **`typescript` 的 npm 最新已是 7.x**（原生端口），模块解析还不成熟（连 `vite`、`react-dom/client` 都解析不出来）→ 降到 `typescript@^5.9`。

**验收（真跑，不是"编译过了"）**

```bash
cargo test --workspace          # domain 19 · sources 2 · store 9（真库真迁移）
bash scripts/e2e-ui.sh          # 31 项：真浏览器走「建节点 → 打字 → 自动存草稿 → 定稿 → 刷新 → 题记」
bash scripts/e2e-library.sh     # 15 项（M1）：真库扫三源 → 清单 → 池子「有/无文档」+ 池子页渲染/过滤
bash scripts/layout-check.sh    # 32 项：三档视口 + 触底 + 窄屏抽屉（量像素）
```

`scripts/e2e-ui.sh` 每一步都回服务端核对（草稿行、定稿版本、快照、写入事实、界面状态条），并**在应用脚本前挂错误钩子**抓白屏时的 JS 错误——第一次跑就抓出了"编辑器把空段落写成 `<br />`"的真 bug。

**已落地（M1：媒体库进料）**

- `crates/sources`：GA（`/api/auth/login` → Cookie → `/api/games/all`）· Emby（`X-Emby-Token`，Views/Items 翻页）· Komga（`X-API-Key`，series + books，含**确定性**的卷/抽屉判型）。
- `store::library`：清单 upsert（新增/变更/未变/消失四态，**一个事务**）+ 池子（LEFT JOIN 链）+ `scan_runs`；`content_hash` 做增量缓存。
- `store::{jobs,secrets}`：队列（`FOR UPDATE SKIP LOCKED` + 租约）与密钥/配置读写（**值不进 DTO**）。
- 迁移 `0002_library_manifest`：把契约字段（副标题/别名/封面/URL/容器/format/extra/hash/first_seen）落成真列。
- 前端：左栏「媒体库池子」视图（源/有无文档/关键词过滤 + 「扫描媒体库」入队轮询）。

**已落地（M2：建档与关联）**

- `domain::place`：落位规则（纯函数 + 7 条单测）——介质由 `(source, kind, format)` 推；子标题默认条目名，**与父同名或与兄弟撞名才加介质后缀**，后缀再撞加序号；复用只认同介质（同作品不同媒体各建一个节点）。
- `store::library::build_doc`：**建【容器系列（缺则建）+ work 节点 + 链】三件事同一个事务**；`unlink` 只删链（文档保留）；`library_ignores` 单条 / 整组 + 恢复。
- `nodes::{create_in_tx,get_in_tx}`：把「读同级末尾 → 分配 sort_key → 插节点」抽出来，两条建节点路径共用一份规则。
- API：`POST /api/library/links` · `DELETE /api/library/links/{id}` · `GET/POST /api/library/ignores` · `DELETE /api/library/ignores/{id}`。
- 前端：池子行「建档」（父节点 / 正片位·混杂位可覆盖，默认按容器自动）/「解链」/「忽略（单条·整组）」+「已忽略」恢复入口。

**已落地（M3：Agent 内核 / 尖刺 B）**

- `agent::client`：自写 Responses API 客户端（SSE 拆帧器 + 事件映射）。实测事件名：`response.output_item.added`（`item.type=function_call` 带 `name`/`call_id`）· `response.function_call_arguments.delta/done` · `response.output_text.delta` · `response.completed`。请求只发一次配置（**窗口只有一个来源**）。
- `agent::runtime`：run 执行器。第一轮 `tool_choice=required`（实测模型爱"把正文写在回复里"而不调工具）；目标节点 id 由**宿主注入**到 user 文本（模型不该猜 id）；正文边流边 `put_draft`（200ms 节流），收尾由工具调用落最终稿；工具循环上限 6 轮；`run_events` 是 SSE 的真相源。
- `agent::stream` / `agent::guard`：见 M3 起步那一条（增量 JSON 状态机 + 五条护栏）。
- `store::runs`：`runs` / `run_events` / `sessions` / `session_messages`（v1 形状原样搬，append-only）。
- worker 认领 `run` 工单；**没配模型就明确失败**（不假装跑过）。
- API：`POST /api/runs`（建工单 + 入队）+ `GET /api/runs/{id}/events`（SSE）。
- 前端：Altas 右栏真接上——发送 → SSE → 实时正文 + 工具结果状态 → 收工刷节点。

**下一步（M4 写作与质量门）**：两步式（analysis→generation，`ai.md §4`）+ `prompts.md §2.3` 的写作流程 + 质量门（代码）→ 目标是把正文写到 **`/root/Game_Wiki` 那一档**（D3），再过门 → 定稿 → 快照。

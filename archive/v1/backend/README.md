# WikiAltas 后端

单进程 Go 服务：把散落在 **Emby / Komga / GameAtlas** 里的媒体库收藏整理成一棵「宇宙 → 系列 → 单作」的知识树，并让一个 LLM 馆员（**Altas**）按 `skills/wiki-writing` 的规范写出条目。写入完成后再由宿主把作品与媒体库条目自动对号。

后端同时托管前端产物（`web/web.go` 把 `web/dist` 用 `go:embed` 打进制），单容器单端口即可部署（见根 `Dockerfile`）。

---

## 1. 技术栈与依赖

| 项 | 说明 |
| --- | --- |
| 语言 | Go 1.24（构建镜像用 1.25） |
| 数据库 | `modernc.org/sqlite` —— **纯 Go，无 CGO** |
| ID | `github.com/google/uuid`，一律 **UUIDv7**（`store.NewID()`） |
| 网络 | 标准库 `net/http`，无第三方路由/ORM |

就这四个硬依赖。协议客户端、工具注册、skill 加载全是自写的薄层。

- 约束：SQL 全参数化；错误上抛，**禁止 `_ = err`**；正文唯一入口是 `PUT content`（人和 LLM 共用）。
- 仓库里**没有** Proposal 表、没有 Entity/Fact 真相表；关系 `type` 枚举写死在代码（`domain.ValidRelationTypes`）。

---

## 2. 分层与目录

调用链严格单向：`cmd → httpapi → domain → store`；`store` 不 import `run`/`llm`。
`run`（工单执行器）才是编排层，它同时依赖 `store`、`llm`、`skill`、`tools`。

```
backend/
├── cmd/wikiatlas/        main.go：装配 store → run.Manager → httpapi.Server，(signal 优雅关停)
├── web/
│   ├── web.go            go:embed all:dist，内嵌前端产物（单容器自托管）
│   └── dist/             前端构建产物（release 时由 Dockerfile 拷入，开发期放 .gitkeep）
├── scripts/
│   └── smoke.sh          端到端冒烟建单冒烟（建作品→写正文→建 run→查树→搜）
├── data/                 默认 SQLite（wikiatlas.db，WAL；可写别的 -db）
└── internal/
    ├── domain/           models.go：所有跨层结构体 + 枚举（唯一真相，camelCase JSON tag）
    ├── store/            SQLite 持久层 + 迁移 + FTS5（auth/docs/works/runs/… 分文件）
    ├── httpapi/          REST + SSE 路由、认证门、媒体库 handler（emby/komga/gameatlas/sweep）
    ├── run/              工单生命周期：Manager → executor(工具循环) → responses(事件发射)
    ├── llm/              协议客户端层（responses.go=OpenAI Responses API；client/tools/mock）
    ├── tools/            馆员工具集：librarian.go（19 个工具 + schema）
    ├── skill/            skill 加载器（按 intent/medium/kind 选 md 文件）
    └── library/          三个媒体源客户端（emby/komga/gameatlas），只管拉数据不做判定
```

**依赖方向**：`domain` 在最底（纯数据）；`store`、`llm`、`skill`、`library` 互不依赖（`store` 不 import `run`/`llm`）；`tools` 依赖 `store`+`skill`；`run` 依赖 `store`+`llm`+`skill`+`tools`；`httpapi` 依赖 `store`+`run`+`library`+`llm`。

---

## 3. 启动与配置

### 3.1 命令行参数（`cmd/wikiatlas`）

```bash
go run ./cmd/wikiatlas -db ./data/wikiatlas.db -addr :8080 \
    -seed -skill /root/WikiAltas/skills/wiki-writing
```

| flag | 默认 | 说明 |
| --- | --- | --- |
| `-db` | `./data/wikiatlas.db` | SQLite 路径（目录会自动建） |
| `-addr` | `:8080` | HTTP 监听地址 |
| `-seed` | false | 空库时种一个示例宇宙（猎魔人 → 巫师系列 → 巫师3 / 白狼崛起） |
| `-skill` | 空 | `wiki-writing` skill 根目录；空则走 settings.skillRoot 或 `skill.DefaultCandidates()` |

### 3.2 环境变量（进程级 / 兜底）

| 变量 | 用途 |
| --- | --- |
| `WIKIATLAS_LLM_API_KEY` / `OPENAI_API_KEY` | 模型 Key（**没有则退回 echo → 走 mock 流**） |
| `WIKIATLAS_LLM_BASE_URL` / `WIKIATLAS_LLM_ENDPOINT` | API 基址 |
| `WIKIATLAS_LLM_MODEL` | 模型名（缺省 gpt-4o-mini） |
| `WIKIATLAS_LLM_MAX_TOKENS` / `..._TEMPERATURE` / `..._REASONING_EFFORT` / `..._PROTOCOL` | 输出上限 / 温度 / 思考档 / 协议 |
| `WIKIATLAS_EXA_API_KEY` | 联网检索（search_web）；不配则检索工具返回“未配置，请标待核实” |
| `WIKIATLAS_MAX_CONCURRENT_RUNS` | 并发 worker 上限（1–16，缺省 2） |
| `WIKIATLAS_SKILL_ROOT` | skill 根覆盖 |
| `WIKIATLAS_ADDR` | `scripts/start-backend.sh` 用的监听地址 |

### 3.3 配置优先级与热生效

**运行期配置以 SQLite `settings` 表（设置页）为唯一权威**：首次启动把 env 里的 `WIKIATLAS_*` 导入一次，之后在设置页改（模型/Key/思考档/窗口/代理/媒体源）下次工单立即生效，不需要重启。`llm.ConfigFromEnv()` 只在“设置页没给到”时兜底。

> 思考档合法值只有 `none|low|medium|high|xhigh|max`——**没有 "off"**（旧值在配置入口归一到 none）。

### 3.4 一键启动脚本

```bash
scripts/start-backend.sh        # 构建到 /tmp/wikiatlas-v2 并 setsid 起（-seed -skill ...），打印 health/tree
```

> 注意：脚本里 `pkill -f wikiatlas-v2`。**在命令文本里别出现它 pkill 的目标名**，否则 `pkill -f` 会命中自己（exit 144）。

---

## 4. 数据模型（SQLite schema，`store.Migrate()`）

### 4.1 核心表

| 表 | 作用 |
| --- | --- |
| `works` | 知识树节点：`universe / series / work`，自引用 `parent_id`；自带 `content_md` + `content_ver`、`status`、`visibility`、`aliases_json` |
| `docs` | 挂在作品下的「资料」文档（`folder_of → works.id`），同样带 `content_ver` |
| `revisions` | 每次正文提交的快照：`(target_type, target_id, version)` 唯一 —— **内容回滚的事实源** |
| `relations` | 作品间**有向**类型边：`(from_id, to_id, type)` 唯一 |
| `library_links` | 作品↔媒体源条目：`(source, external_id)` 唯一（确认后才落库） |
| `runs` | 工单：状态机 + `checkpoint` + `tool_cache`（resume 从这里续） |
| `run_events` | SSE 事件持久化（回放 / Last-Event-ID 续传用），`seq` 单调 |
| `sessions` | 会话（`id = runs.workspace`），`target` 记归属页 |
| `settings` | KV：设置 JSON、API Key、库扫描清单、AI 建议、Komga 判定缓存… |

FTS5（trigram，支持 CJK，external-content 模式）：`works_fts(title, content_md, aliases_json)` / `docs_fts(title, content_md)`，配 insert/update/delete 触发器，单一真相在 works/docs。

### 4.2 关键枚举（`domain/models.go`，改代码才能换）

- **WorkKind**：`universe | series | work`
- **Medium**：`game | movie | tv | manga | book | other`
- **Visibility**：`public | private`（访客要**自身与所有祖先都 public** 才可见）
- **RelationType**：`adaptation_of | sequel_to | spin_off_of | remake_of | expansion_of | references | same_series`
  - 前 5 种在 `DirectionalRelationTypes` 里 —— 建边时**查环**（A→B 与 B→A 同时存在会自相矛盾）。`same_series` / `references` 天然互相、不查环。
- **RunIntent**：`create_wiki | continue_wiki | rewrite_section | write_doc | organize_tree | answer | sync_library`
- **RunStatus**：`running | interrupted | completed | failed | expired`
- **Author**：`human | llm | import`（进 revision，回滚时能看出谁写的）

### 4.3 内容版本与回滚

写入正文（`store.CommitContent`）**幂等 commit**：同一 `expectedVersion` 重复提交返回现有版本；冲突时（`expectedVersion` 落后）抛 `ErrConflict`。每次 commit 在 `revisions` 落一条快照，恢复某版本 = 拿快照正文重新 commit。`revisions` 是回滚的唯一凭据。

### 4.4 settings 里的“共用 KV”（`store/settings.go`）

`settings` 表当 KV 用，几个大块 JSON 同住一列。三个值得注意的：

- **LibraryManifest**（库扫描快照）：纯确定性数据，扫描时**保留忽略项**（单条 `ignoredKeys` / 整组 `ignoredGroups`）。
- **LibraryAiSuggestions**（AI 建议）：sweep、池子、per-node 忽略**三套字段共存同一个 KV**——保存时只替换自己那份，且必须持 `suggestionsMu` 锁；**不要写“空列表=没记录”这种守卫**（会吞掉同记录里其它字段）。
- **KomgaJudgments**（层级判定缓存）：key = 书单指纹 + `komgaJudgeVersion`。**改了判定 prompt 必须把版本号加一**，否则旧判定永不重跑。

> API 形状纪律：空列表一律给 `[]` 不给 `null`（前端 `.length` 会崩）。

---

## 5. 核心领域概念

### 5.1 作品树

三层树：`universe`（宇宙/总盘）→ `series`（系列）→ `work`（单作）。每个节点可带正文；`series/universe` 层是「合集/主文」，写作规则由 skill 的 `readme.md` 补（见 §8.2）。`work` 下可以挂 `docs`（资料文档，如解说、攻略、访谈）。

### 5.2 Run（工单）

用户一句需求 = 一个 run。run 携带：
- `intent`（任务类型，决定 skill 预载与提示语气）
- `goal`（用户原话）
- `context`（`workId / docId / parentId / medium / section / docMode / selection / extra`）
- `workspace`（= **会话 id**；同一页面同一段连续对话）
- 可选 `reasoningEffort`（本条工单覆盖思考档）

`docMode`（`read | edit | revision`）决定作业姿态：只读=只分析不改；编辑=直接改；修订=定点改 + 给理由。**注意：docMode 只影响提示里的“执行偏向”，不决定工具面**（工具面只按 read 与否剥写工具）。

批量建档：`POST /api/runs/batch` = 一个共享 workspace（`batch:<id>`) + N 个子工单，全排进同一 worker 池逐个执行（不会并发打 N 路 LLM）。

### 5.3 Session（会话）

会话 id 即 `runs.workspace`。执行器通过 `sessionlog` 把**模型看到的每一条消息**都落库成会话日志；上下文永远由日志**投影**而来（不重建）。这条不变量是整套“可回放、可 resume、模型可见 ⟺ 有日志”的地基（见 §7 的“重建不变量”）。

### 5.4 媒体库三源（AI 只建议，不执行）

**LLM 永不主动建库 / 挂链**。确定性的部分（扫描、题名对齐、层级判定）交给宿主代码，只有“判相关性 / 给建议”这一步动模型。两条互补链路：

- **节点扫库（node-sweep）**：视角 *节点 → 库*。写完之后触发（`run.Manager.SetOnWikiWritten` → `httpapi.SweepLibraryForWork`），拿刚写完的作品标题 + 别名 + 模型补的“题名变体”去三源召回卫星，一次 LLM 判相关性，产出带可信度（≥`sweepMinConfidence`）的建议，存 `library_ai_suggestions`，节点页展示。**并发闸 `sweepSem`（2）+ `suggestionsMu` 锁**。
- **池子建议（AI 对一遍，库 → 节点全量匹配）**：后台扫描清单 + `library_pool.go` 出建档 / 关联 / 忽略建议。

三源客户端在 `internal/library/`（`emby.go` / `komga.go` / `gameatlas.go`），只负责把异构 API 归一化成 `LibraryManifestEntry`。**反哺**：Wiki → GameAtlas（正文+简介）、Komga（简介）、Emby（仅关联）。

---

## 6. 认证与隐私

单一管理员，**没有账号体系**，只有一个访问密码（出厂 `1234`，启动时日志提醒改掉）。

- 密码只存 **PBKDF2-SHA256 哈希**，接口只回 `passwordConfigured` / `usingDefaultPassword`，**永不回原文**。
- 登录发 session（secret 存 settings，`IsPublicWork` 用 fingerprint 判访客）。
- **访客白名单**（`httpapi.Handler → withAuth`）：未登录只能走只读公开内容 + 白名单端点。
- 节点级 `public/private`：访客能看到当且仅当 **自身与所有祖先都 public**（`store.PublicWorkIDs()` 给搜索/树做过滤）。
- 设置页的 `newNodeVisibility` 控制新节点默认可见性（默认 private）。

---

## 7. Run 执行器（`run/executor.go`，核心）

`Manager.execute` 决定走 mock 还是真 LLM；真 LLM 走 `executeLLM` —— 一个**工具调用循环**（`maxToolRounds = 40`）。这层是整个后端最密的地方，关键设计：

### 7.1 会话日志 = 事实源

每轮消息（system / user / assistant / tool）都经 `logTurn` 落会话日志，上下文从日志投影。resume 走同一条路，所以“断之前看到的”和“续上看到的”天然一致。**重建不变量**：每轮用日志独立投影一遍和内存逐条比对，不一致就发 `wikiatlas.notice`（记成 bug）。

### 7.2 前缀稳定 = 前缀缓存不失效

`requestShape.check` 要求**本次请求必须是上一次的追加延长**（append-only），否则前缀缓存必然失效——这在网关不报缓存用量（如 vLLM）时是“前缀有没有被悄悄改写”的唯一验证手段。压缩 / 换会话头会显式记录原因（`shape.reason`）。

### 7.3 上下文窗口与压缩

窗口是**模型一次能装多少 token** 这一个数（settings，缺省 262144）。到 `compactRatio(0.75)` 就压一次：压缩 = 会话日志上的一次**显式替换**（把中间工具往返换成摘要，发 `wikiatlas.context`），必然打断前缀缓存所以既发事件又告诉 shape 原因。**入口绝不截断工具结果**——会同时切碎信息 + 打碎缓存；普通大小上限归各工具（read_work 30k / read_skill 24k / fetch_url 12k / search_web ~9k）。`outputReserveTokens(8192)` 给模型自己留写东西的余量。

### 7.4 护栏（都在工具层拦截，不是靠提示词祈祷）

| 护栏 | 触发 | 动作 |
| --- | --- | --- |
| 只读模式 | `docMode==read` 且调用写工具 | 拒绝执行，返回“请用 answer 分析” |
| 搜索预算 | `search_web` > 12 / `fetch_url` > 12 | 一刀切，要标“待核实” |
| 重复调用 | 同 `name+规范化参数指纹` 3/5/8 次 | 温和提醒 → 具体提醒 → **硬拦**（≥8） |
| 同章重写 | `patch_section` 同 heading > 2 次 | 阻止第 3 次，转标“待核实”继续 |
| 残稿 | `write_content` 空 / 被截断 | 拒收，退回拆小重写（防一次写飞把整篇清空） |
| 答完交付协议 | `answer` 且 intent=answer | 无写工具：答案即交付物，立刻 break（防把答案说两遍+一串收尾）。有写工具：第一次 answer 回执附协议注记、宽限一轮（正则把工作消息猜成问句、模型先答了话，活还能救回）；第二次 answer 必停 |
| 坏 JSON 参数 | 工具参数不是合法 JSON | 拒收 + 提示拆小（防网关 400 整段会话报废） |

**规范化参数指纹**：`{"a":1,"b":2}` 与 `{"b":2,"a":1}` 是同一调用（属性顺序变了也能判重）。这些计数**每次执行一份**（一个 run = 用户一句请求），绝不挂到 Manager 上。

### 7.5 重试与收尾

`callModel` 对**可重试错误**（限流 / 5xx / 网络抖动）退避重试（1s/4s/10s），`llm.IsRetryable` 判定。成功后过**质量门**（`applyQualityGate` → `quality.CheckWikiQuality`，`minCharsFloor=5000` 等），过了才把 work 状态推到 `ready`。最后异步触发 `onWikiWritten` 钩子（节点扫库），不占 worker 槽位。

事件发射集中在 `responses.go` 的 `em`（见 §8）。

---

## 8. LLM 层与事件流

### 8.1 协议（`internal/llm/`）

**协议差异只允许收在 llm 层**；往上只有一种形状——OpenAI **Responses API**（`/v1/responses`，`responses.go`）的输出项与流事件。`Config.Protocol` 是“以后加新协议”的位置：加常量 + 客户端 + `NewClient` 一个 case，上层与前端一行不动。

三种客户端：
- **真客户端** `ResponsesClient`（HTTP SSE，`streamIdleTimeout=150s`——长答不受影响，卡死才断）。
- **`EchoClient`**：没配 Key 时的占位 → 执行器识别后跑 **mock 流程**（`mock.go`，离线/演示可用）。
- **`ErrorClient`**：配置就是错的（如填了不支持的协议）→ **故意不复退 echo**，让错误在第一次调模型时炸出来，而不是静默跑 mock 假象。

### 8.2 Skill（`internal/skill/`）

运行时读 `skills/wiki-writing/`，**不注入 `examples/`**（个例证据只进 examples，不进运行时规则）。按 `FilesForIntentKind(intent, medium, kind)` 选文件：

- create/continue/rewrite：`SKILL.md` + `core.md` + 一个 `media-*.md`（按介质：game→media-game，movie/tv→media-video，manga/book→media-book）
- write_doc：仅 `SKILL.md`
- **目标是 `series`/`universe` 时额外注入 `readme.md`**（系列主文/合集写法）——否则模型只按单作 9 章骨架走，看不到合集该改哪
- answer/organize_tree/sync_library：不整包注入

路径安全：`ReadFile` 拒绝绝对路径 / `..` / 越出 root。

### 8.3 事件流（`run/responses.go`，唯一事件真相）

两条命名空间，**同流不同类**：

- **`response.*`**：OpenAI Responses 规范流事件，形状照抄规范 → 前端整条交给 **Semi 原生流式归约器**（不自己造投影）。
  `created / in_progress / completed / failed / output_item.added|done / output_text.delta|done / reasoning_summary_text.delta|done / function_call_arguments.delta|done`
- **`wikiatlas.*`**：规范里没有、产品要的 → 宿主部件（任务面板 / 用量环 / 提醒条 / 元信息）。
  `meta`（工单元信息：session/intent/model/workId/docId…）、`todo`（任务清单）、`usage`（用量+**cacheReadTokens** 命中信号）、`notice`（播报 info/warn/error）、`request`（每轮请求头 + appendOnly/rebuildOK）、`context`（压缩事件）、`tree`（作品树变更）、`content.staging|committed`（正文载体，权威）、`content.delta`（**正文正在长出来**的流式通道，以 `.delta` 结尾 = 只走实时流不进历史回放）。

正文流：写正文工具的**流式参数**（`write_content.contentMd` / `patch_section.newMarkdown`）被后端投影成 `wikiatlas.content.delta` 的“段”（~250ms 节流，`streamEmitInterval`），逐条 `function_call_arguments.delta` **不再转发**（没人消费、又会把全量归约卡死）。收尾两种语义：`commit()`（留末稿防闪回）/ `abort()`（中断退回真实内容）。

**cacheReadTokens 是生产信号**：追加式对话、头部不变，第二次以后每次都该命中缓存；它掉到 0 就说明前缀被改写了（见 §7.2）。

---

## 9. 馆员工具集（`internal/tools/`, 19 个）

工具注册与 `ToolSchemas()`（发给模型的定义）共用**同一份描述常量**——一个事实一个 owner，防止模型读到两套说法（真实踩过：纪律写在注册的 desc 上，模型读的是 schema，等于没写）。

**读 / 查**
- `read_skill` · `search_works` · `read_work`（支持 `section` 只读一章，截断 30k）· `read_doc` · `get_tree`
- `search_web`（Exa，带检索预算）· `fetch_url`（可 `useProxy`，超时/被挡会**给出下一步动作**而非裸报 http 403）
- `search_library` · `list_library`
- `search_sessions` · `read_session`（跨会话回忆）

**写（只读模式下被拦）**
- `write_content`（整篇；空/残稿拒收）· `patch_section`（按 heading 原位替换，同章最多改 2 次）· `edit`（str_replace 语义，未命中/多处带行号报错）
- `upsert_work`（建/改节点）· `upsert_relation`（建边带查环）· `create_doc` · `attach_library_link`（确认才挂链）

**控制面（不出工具卡、重复不拦）**
- `todo_write`（任务清单）

> 这里曾经还有两个"交付工具" `narrative`（进度）与 `answer`（结论）：模型对用户说话必须先过它们。
> 已退役——回复就该是模型自己的 `message` 输出项（有工具调用 = 过程叙述，没有 = 结论并结束）。
> 理由与替代见 `internal/run/prompt.go` identitySection 与 `internal/run/executor.go` controlTool 的注释。

工具返回都是 JSON（`{"ok":…, "message":…}`），预算提醒由**工具自己写进结果里**（构造时），执行器不再事后改——保证“模型看见的 = 落库/事件里的那份”。

---

## 10. HTTP API 一览（`httpapi/server.go`）

### 认证
| 方法 路径 | 说明 |
| --- | --- |
| `POST /api/auth/login` · `POST /api/auth/logout` · `GET /api/auth/me` | 登录 / 登出 / 当前管理员 |

### 树 / 作品 / 文档
| 方法 路径 | 说明 |
| --- | --- |
| `GET /api/tree` | 作品树（访客只回公开） |
| `GET/POST /api/works` · `PATCH/DELETE /api/works/{id}` | 节点 CRUD |
| `PUT /api/works/{id}/content` · `PUT /api/docs/{id}/content` | **正文唯一入口**（幂等 commit + expectedVersion 冲突检查） |
| `GET/POST /api/works/{id}/docs` · `GET/PATCH /api/docs/{id}` | 资料文档 |
| `GET /api/works/{id}/relations` · `POST/DELETE /api/relations[/{id}]` | 关系边 |
| `GET /api/works/{id}/revisions` · `POST /api/works/{id}/revisions/{revId}/restore` | 版本历史 / 回滚 |

### Run / Session（工单 + 流）
| 方法 路径 | 说明 |
| --- | --- |
| `POST /api/runs` · `POST /api/runs/batch` | 建单 / 批量建档 |
| `POST /api/runs/{id}/resume` · `.../cancel` · `DELETE /api/runs/{id}` · `GET /api/runs[/{id}]` | 续跑 / 取消 / 删除 / 查 |
| `GET /api/runs/{id}/events/stream` | **SSE**：先回放 backlog（`Last-Event-ID` / `?lastEventId=`），再转实时；心跳 `: ping` 长连 |
| `GET/POST /api/sessions` · `PATCH/DELETE /api/sessions/{id}` | 会话（= runs.workspace）管理 |

### 媒体库
| 方法 路径 | 说明 |
| --- | --- |
| `GET /api/library/sources` · `POST /api/library/sync` | 源配置 / 触发同步 run |
| GameAtlas：`suggest / archive / push / search / link / unlink` | 孪生：建议建档 / 反哺 / 关联 |
| Emby：`views / suggest / archive / related / search / link / unlink` | 库角色（work/mixed 在设置里标） |
| Komga：`suggest / archive / link / unlink` + 层级判定（`komgaJudgeVersion` 缓存） | 漫画/轻小说 |

### 其他
| 方法 路径 | 说明 |
| --- | --- |
| `GET /api/search?q=&kind=` | FTS 搜索（访客只回公开） |
| `GET/PUT /api/settings` | 设置（只回“是否已配置”，**绝不回密钥原文**） |
| `GET /api/health` | 健康检查 |

---

## 11. 后台任务与运维

- **worker 池**：所有 run（含批量）排进 `queue`；固定起 `workerCeiling(8)` 个 worker，真正并发闸在 worker 内**实时读** `settings.Runs.MaxConcurrentRuns`（缺省 2）——设置页改并发数不用重启；队列 >256 退回直接跑防丢。
- **stale 清理**：`ExpireStaleRuns(expireDays)` / `DeleteOldRunEvents(keepDays)`（默认 7 / 90 天）由 `ticker` 低频触发。
- **优雅关停**：监听 `SIGINT/SIGTERM` → `server.Close()` + `mgr.Stop()` + `st.Close()`。
- **重启前先看工单**：读 `data/wikiatlas.db` 的 `runs` 表，有 `running` 就等（重启会杀掉 run）。

### 联调 / 冒烟
```bash
# 后端测试
wsl -d Ubuntu -- bash -lc 'export PATH=/usr/local/go/bin:$PATH; export GOCACHE=/root/.cache/wikiatlas-go-build; cd /root/WikiAltas/backend && go test ./...'
# mock 长草稿联调：第二个后端走真分流路径（无 key 即 mock）
WIKIATLAS_MOCK_WRITE_DELAY_MS=24000 go run ./cmd/wikiatlas -db /tmp/x.db -addr 127.0.0.1:8090 -seed -skill <root>
# smoke
bash scripts/smoke.sh    # BASE=http://127.0.0.1:18080 端
```

测试用 `store.OpenMemory()`（每开一个独立 in-memory 库，互不串）+ 注入的 fake/echo 客户端；`manager_test` / `mock_events_demo_test` 覆盖事件流形状。

---

## 12. 设计红线（改动前先想这些）

1. **分层不越界**：`store` 不 import `run/llm`；协议差异只收在 `llm`；事件形状只由 `run/responses.go` 一处拥有。
2. **正文唯一入口 = `PUT content`**；`revisions` 是回滚唯一凭据；commit 幂等。
3. **模型可见 ⟺ 有日志**：每条上下文必须落会话日志，执行器每轮做重建不变量比对。
4. **前缀追加式**：请求必须 append-only 才不炸前缀缓存；能缩小上下文的只有压缩（且要留痕）。
5. **AI 只建议不执行**：库 / 挂链全靠人确认；确定性流程零 LLM。
6. **关系枚举 + 查环**：类型写死代码；有向边建环前查环。
7. **密钥永不出后端**：设置只回“已配置”，hex/密码永不进前端/代码/文档。
8. **空列表给 `[]` 不给 `null`**；共用 KV 只替换自己那份并持锁。

> 改视觉 / 组件 API 走前端线（`VISUAL_SPEC.md` + Semi MCP）；本文只覆盖后端。

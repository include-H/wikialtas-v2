# WikiAltas 前端

基于飞书文档观感的 React 单页应用。它把后端那棵「宇宙 → 系列 → 单作」知识树变成可编辑的中文百科界面，并托管一个名叫 **Altas** 的 LLM 馆员——你说一句话，它自己判断读什么 skill、查什么、改哪些条目，写出来的正文**实时长在编辑器里**。

视觉母本是 `VISUAL_SPEC.md`（飞书），与 `DESIGN_V2.md` 冲突时以它为准。

---

## 1. 技术栈

| 项 | 说明 |
| --- | --- |
| 框架 | React 18 + TypeScript + Vite 6 |
| 组件库 | **Semi Design（`@douyinfe/semi-ui` v2）** + 飞书品牌主题（`@semi-bot/semi-theme-feishu`） |
| 富文本 | **BlockNote**（`@blocknote/*` + `@mantine/core`，BlockNote 的官方宿主） |
| Markdown 渲染 | `marked` + `dompurify`（只读预览 / 题记 HTML） |
| 路由 | react-router-dom v6 |
| 主题注入 | `@douyinfe/semi-vite-plugin`（构建期把飞书主题包注入 Semi 的 `--semi-*` token） |

约束（来自 AGENTS.md）：

- **Semi UI 组件优先；颜色全走 Semi token，禁止新写 hex，禁止反向覆盖 `.semi-*`。**
- **紫色（purple / violet / indigo）是 AI 专属强调色**，非 AI 元素一律不许用。
- 组件 API **先查 Semi MCP（semi-mcp）文档再写，禁止凭记忆造 props**。
- 代码分 `src/views` / `src/components/**` / `src/lib`；**单文件建议 ≤300 行**（SettingsView、Settings 侧几个是已知超出的大块）。

---

## 2. 运行 / 构建 / 联调

```bash
npm install
npm run dev        # 开发服务器 http://localhost:5173，/api → 代理到后端
npm run build      # tsc -b && vite build（产物进 dist/，被后端 go:embed 收录）
npm run lint       # tsc --noEmit
```

- 默认 `/api` 代理到 `http://localhost:8080`（vite `server.proxy`）。
- **第二套前端指测试后端**（联调正文流时常用）：
  ```bash
  WIKIATLAS_DEV_PORT=5200 WIKIATLAS_API_TARGET=http://127.0.0.1:8090 npx vite
  ```
- 生产环境后端把自己构建好的 `web/dist` 用 `go:embed` 打进制并自托管，**单容器单端口**。前端开发期走 vite dev server，`dist/` 里只是占位。

> 验证命令（AGENTS.md 指定，走 WSL + nvm 的 node）：
> `wsl -d Ubuntu -- bash -lc 'export PATH=/root/.nvm/versions/node/v24.20.0/bin:$PATH; cd /root/WikiAltas/frontend && npm run build'`

---

## 3. 路由（UUID-first，`lib/routes.ts`）

**身份永远是 UUID**——路径里不写 slug，UUID 本身就是那篇文章（避免一个内容多个 URL、转义、保留字、改名后陈旧链接这些 slug 成本）。

| 路径 | 视图 | 说明 |
| --- | --- | --- |
| `/` | `Home` | 最近作品、置顶区、stub 计数、活动工单 |
| `/w/:id` · `/w/:id/:slug` | `WorkView` | 作品页（树 + 大纲 + 编辑器）；`/w/:id/:slug` 是**容忍路由**（不生成，旧链接点进来照样能开） |
| `/w/:id/folder` | `WorkView` | 资料夹视图：左栏切成资料列表，正文仍打开这篇 Wiki |
| `/w/:id/folder/:docId` | `DocView` | 资料文档编辑（两段都是 UUID） |
| `/runs` | `RunList` | 工单历史 / 批次 |
| `/library` | `LibraryPool` | 媒体库建议池（建档 / 关联 / 忽略仪表盘） |
| `/settings` | `SettingsView` | LLM / 检索 / 并发 / skill / 访问 / 媒体库 / 运行态 |
| `/login` | `LoginView` | 访问密码（软门槛） |

深链参数：`?ai=1` 打开 AI 助手；`?batch=<id>` 打开批次卡。哨兵 `:id = '-'`（`UNKNOWN_WORK_ID`）表示只知道资料 UUID（如搜索命中）的深链。

---

## 4. 布局与 AI 助手（`components/shell/`）

```
┌────────┬──────────────────────────────────────────────┐
│ Side   │ TopBar                                        │
│ Nav    │ ┌──────────────────────────────────────────┐ │
│ (works │ │  <Outlet/>  = 当前视图                    │ │
│  tree, │ │            （WorkView / Home / Doc …）    │ │
│  全局) │ └──────────────────────────────────────────┘ │
└────────┴──────────────────────────────────────────────┘
```

- **`AppShell`**：`Layout hasSider` + `SideNav`（左侧作品树 / 全局导航）+ `TopBar`；按登录态与 `aiPanelOpen` 切换 AI 助手。
- **AI 助手**：要么是**可拖拽 / 可缩放的悬浮窗**（`ChatWindow`）装着对话体（`AiPanel`），要么收起时页面上留一个 `FloatButton` 重新唤起。只有 `me.authed` 才显示。
  - `ChatWindow` 用 Semi 原生 `DragMove`（拖，带 `constrainer` 不许拖出屏）+ `Resizable`（八向缩放，官方移植的 re-resizable）——不手搓鼠标事件。
  - 位置 / 尺寸进 `localStorage`（`wikiatlas.chatRect`），换显示器 / 缩窗口后用 `clampRect` 夹回视口内，不会掉出去找不回来。
  - 取舍写在代码里：悬浮窗会挡住正文（读时得挪开），换取宽度不受布局挤占——SideNav 侧栏那种被挤到 `nowrap`/`min-width:auto` 爆发的设备适配问题自然消失。

> AGENTS.md / VISUAL_SPEC §1.5 把 AI 助手表述为“右侧可收起侧栏”；当前实现是等价的可收起悬浮窗（`aiPanelOpen` 开合，`chatRect` 记位置）。

`AiPanel`（对话主体）：会话列表 / 重命名 / 删除、思考档选择、发工单（`createRun`）、SSE 订阅、批次卡（`BatchCard`）、用量环（`ContextMeter`）、提醒条（`NoticeDock`）、消息操作（`MessageActions`）。

---

## 5. 全局状态（`lib/store.tsx` + `lib/liveDraft.tsx`）

**两个 Provider，职责刻意分开**（`App.tsx`：`<AppProvider><LiveDraftProvider>`）：

### `AppProvider` / `useAppStore` — 低频、全局

- `nodes`（作品树，`refreshTree`）· `treeLoading/treeError`
- `me` / `meLoaded`（`/api/auth/me`；访客 = 未登录，只读公开）
- `aiPanelOpen` + `chatRect`（AI 助手开合 + 悬浮窗几何，后者进 localStorage）
- `activeRuns` + `refreshRuns`（**只有登录态才请求** `/api/runs`，访客不轮询以免每轮吃 401）
- `contentStamp` / `lastCommitted`（Altas 提交正文后 bump，让视图刷新）· `staging`（`wikiatlas.content.staging` 带来的“正在写入”标记）
- `docMode`（`edit | revision | read`，对齐飞书顶栏「编辑 ▾」；编辑器内部不放模式按钮）
- `docSelection` / `askSelection` / `clearAsk`（正文选区 → AI 面板共用；`ask` 用于“把选段交给面板 / 自动发一句 / intent=answer 翻译这段”）
- `sidebarCollapsed` · `activeBatchId`（当前批次，`wikiatlas.batch`）

### `LiveDraftProvider` / `useLiveDraft` — 高频通道，单独开 Provider

正文是**经写正文工具的流式参数**流出来的（`wikiatlas.content.delta`，~5 段/秒、持续几分钟）。它不进全局 store——否则每个 `useAppStore` 消费者都会跟着重渲染。这里把**值**（`draft`）与**写入口**（`push/commit/abort`，引用恒定）拆成两个 context：

- 读值的只有正在显示草稿的编辑器视图；写入口引用恒定 → 生产者不因它重渲染。
- `write_content` 段 = 整篇替换；`patch_section` 段 = 某一节新正文（消费方按 heading 用 `mdOutline.spliceSection` 原位拼接，**镜像后端 `tools.ReplaceSection` 的匹配规则，改一边要同步另一边**）。
- **收尾两种语义**：`commit()`（正文已落库，末稿留在编辑器等正式内容无缝对齐，防闪回）/ `abort()`（工单中断/失败，正文没变，直接退回真实内容）。

---

## 6. Run 事件消费：两条线（`lib/sse.ts` + `lib/responses.ts`）

后端一条 SSE 里混着两种事件，前端**同流不同类**地拆开：

```
SSE: GET /api/runs/{id}/events/stream
        │  (lib/sse.ts: streamRunEvents, EventSource + Last-Event-ID 续传/重连)
        ▼
lib/responses.ts  —— 只分流，不做投影 ──┬── response.*  → 整条交给 Semi 原生流式归约器
                                         │                     (streamingResponseToMessage)
                                         │                     拼助手消息 / 乱序缓冲 / 缺块容错，都不归我们管
                                         └── wikiatlas.* → 宿主部件
                                         TodoPanel / ContextMeter / NoticeDock / 元信息
```

- **`response.*`**：OpenAI Responses 规范的流事件。这一层**只重编号、不归约**——交给 Semi 的 `streamingResponseToMessage` 折出那条对话消息，前端不自己造投影。
- **`wikiatlas.*`**：`todo`（任务清单）/ `usage`（用量 + `cacheReadTokens` 命中）/ `notice`（提醒条 info/warn/error）/ `meta` / `request` / `context`（压缩）/ `tree` / `content.staging|committed` / `content.delta`（→ LiveDraft）。
- **两侧都不 dump payload**：`response.*` 让 Semi 归约，`wikiatlas.*` 只读对应字段给部件。

**EventSource 坑**：它只派发**显式订阅过**的命名事件，所以 `responses.ts` 里的 `RUN_EVENT_TYPES` 是 `RESPONSE_EVENT_TYPES + WIKIATLAS_EVENT_TYPES` 的**整份清单**——漏一个那一类就**静默收不到**（不报错、不提示，只是界面少一块）。清单所有权在后端 `internal/run/responses.go`，前端是同一份契约的镜像。

**流式平滑（`lib/smoothing.ts`）**：网关不总逐字推（见过“整段 430 字一个增量砸下来”），历史回放更是一次到位。归约层保持权威不动，这一层只在**渲染前**给每个文本片段加一条“已吐字数”水位线，大块按 ~50ms 节奏摊开成打字机效果，追平后零开销。

---

## 7. 正文流与编辑器（`components/editor/` + 几个 lib）

后端把写正文工具的流式参数投影成 `content.delta`，前端在 `LiveDraftProvider` 累积，`WorkView`/`DocView` 拼好后经 `liveMd` 进编辑器**只读显示**，落块动画对齐飞书写文档（流式帧增量替换 → 当前段 `.live-tail` 压暗 + 文末紫色脉动条 → 写完摘类渐显回正文）。

| 文件 | 作用 |
| --- | --- |
| `MarkdownEditor` / `BlockMarkdownEditor` | BlockNote 富文本封装；`liveMd` 只读显示流式草稿；`lastLoaded` 基准存**替换后实际序列化出来的样子**（题记块往返有损：750 字进 739 字出），否则模式切换的 change 回声会把正文误标“未保存” |
| `FeishuToolbar` | 顶栏（模式下拉、题记、媒体标签、AI 触发等） |
| `EpigraphBlock` | 题记自定义块（见下） |
| `MarkdownPreview` | 只读 Markdown 渲染 |
| `DocActions` | 节点/文档操作 |
| `PendingRevisions` | 待确认修订 |
| `WorkSidePanel` | 作品右侧资料 / 关系 / 媒体面板 |

关键 lib：

- **`lib/blockMd.ts`**：存储 Markdown ⇄ BlockNote 文档互转。BlockNote 的解析器不认识 `:::epigraph` 自定义围栏 → 载入时抠出来换占位段、解析完再换回 epigraph 块；保存时按块序列化、遇到 epigraph 写回围栏。
- **`lib/docMarkdown.ts`**：正文 H1（`# 标题`）与文档标题（`works.title`）的唯一真相——载入前 strip 开头 H1（避免标题出现两次），保存时 ensure 回来（content_md 自包含）。只动最开那个 H1。
- **`lib/epigraph.ts`**：题记排版（对齐 GameManager）——无底色无边框、对角装饰引号、居中、署名右对齐；**外文原文小字 / 中译大字逐行交替**，判据是“这一行是不是中文”（非中文书写系统一律小字）。
- **`lib/mdOutline.ts`**：`parseOutline` 抽 `##/###` 大纲；`spliceSection` 按 heading 原位拼接 patch（镜像后端 `ReplaceSection`）。
- **`lib/useLiveRefresh.ts`**：staging 期间按间隔静默重取正文（不闪 loading、不覆盖用户正在打的字）。

> BlockNote 已知两个雷（改编辑器前必读）：①`replaceBlocks` 的删除集**不能为空**（源码取 `t[0].id` 会炸），纯追加帧走 `insertBlocks`；②想让“渐显”有节点可落就不能整篇重建（全量替换会把动画全重播）。

---

## 8. 媒体库（`components/library/` + `lib/mediaTags.ts`）

建议池（`LibraryPool` 视图）与节点关联建议（`WorkSidePanel` / `LibrarySuggest` / `LibraryRelated`）的 UI。组件：`LibraryDiscovery`、`LibraryLinkModal`、`PoolStubCard`/`PoolEntryRow`/`PoolGroupCard`/`PoolAiCard`/`PoolIgnoredCard`/`PoolModals`/`PoolSources`。

**配色纪律（`lib/mediaTags.ts`，VISUAL_SPEC §2）**：紫色是 AI 唯一强调色，媒体库这些**展示性标签一律不许用紫系**——`kind=blue`、`format=cyan`、`source=grey`（中性，不跟类别色抢视线）、可信度=绿/橙/灰三档。分栏顺序与后端一致：`gameatlas → emby → komga`。

---

## 9. 认证与隐私（`lib/store.tsx` + `LoginView`）

- 单管理员、无账号体系，只有一个访问密码。`LoginView` 是**软门槛**（挡一下不适合公开展示的内容），**不是安全边界**。
- 真正的公开/私有边界在**节点可见性**上（访客只能读 `public`）。
- `me.authed` 控制三件事：是否显示 AI 助手 / 是否轮询 `activeRuns` / 写操作入口。

---

## 10. API 层（`lib/api.ts`）

对后端 REST 的**薄包装**（`BASE = ''`，同源；开发期靠 vite 代理转发）。分组与后端 `httpapi` 路由一一对应：

- 树/作品/文档：`getTree / getWork / createWork / patchWork / putWorkContent / getWorkDocs / createDoc / putDocContent / …`
- 关系/版本：`createRelation / deleteRelation / getWorkRevisions / restoreWorkRevision`
- Run：`createRun / createBatchWiki / resumeRun / cancelRun / listRuns / getRun / streamRunEvents(SSE)`
- 会话：`listSessions / createSession / renameSession / deleteSession`
- 媒体库：`getLibrarySources / scanLibrary / suggestLibrary / linkLibrary / searchLibrary`、三源 `push* / unlink*`、`getNodeSweep / runNodeSweep / dismissNodeSweep`
- 其他：`search / getSettings / putSettings / testLLM / getRuntime / login / logout / getMe`

所有响应遵循后端纪律：**空列表一律 `[]` 不给 `null`**（前端 `.length` 直接崩的那种）。

---

## 11. 视觉与主题（`lib/theme.ts` + `vite.config.ts` + `index.css`）

- 飞书品牌主题（`@semi-bot/semi-theme-feishu`）由 `@douyinfe/semi-vite-plugin` 在**构建期**注入为 `--semi-*` token 覆盖（主色 `51,112,255`、小圆角 4px、中性色盘）。
- 明暗主题：`lib/theme.ts` 的 `applyTheme`/`getStoredTheme`（localStorage），`main.tsx` 启动时应用。
- `main.tsx` 在 DEV 下过滤掉 Semi 2.x 内部 `findDOMNode` 的 React 18 弃用警告（组件库内部实现，不改它，只静音这一条）。
- **颜色纪律**：全走 Semi token、禁新 hex、禁反向覆盖 `.semi-*`、紫色专属 AI。

---

## 12. 目录速览

```
src/
├── App.tsx                 路由 + Provider 装配
├── main.tsx                入口：主题 + dev 降噪 + createRoot
├── index.css               全局样式
├── types.ts                前端侧类型（与后端 domain 的 camelCase 契约对齐）
├── views/                  路由级大视图（Home / WorkView / DocView / RunList / SettingsView / LibraryPool / LoginView）
├── components/
│   ├── shell/              布局骨架（AppShell / SideNav / TopBar / AiPanel / ChatWindow / WorkTree / OutlinePane / FolderPane / SearchModal / WorkDialogs）
│   ├── editor/             富文本与编辑（MarkdownEditor / BlockMarkdownEditor / FeishuToolbar / EpigraphBlock / MarkdownPreview / DocActions / PendingRevisions / WorkSidePanel）
│   ├── run/                工单事件宿主部件（BatchCard / ContextMeter / TodoPanel / NoticeDock / ToolCard / MessageActions）
│   └── library/            媒体库（Pool* / Library*）
└── lib/                    无 UI 的纯逻辑（api / store / sse / responses / liveDraft / blockMd / docMarkdown / epigraph / mdOutline / intent / smoothing / mediaTags / theme / tree / routes / pins / sessionPointer / useLiveRefresh）
```

> 与后端配套见 `../backend/README.md`（数据模型、Run 执行器、事件形状、媒体库三源）。

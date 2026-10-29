# Atlas v2 · 旧前端迁移谱系

> 输入：`archive/v1/frontend/`（Semi Design + BlockNote 的完整应用，**17,530 行 / 70 文件**）。
> 目标：`frontend/`（React 19 + Vite + TS strict + **Ant Design v6** + **@ant-design/x** + TanStack Query + Zustand + Milkdown），视觉照 `archive/v1/VISUAL_SPEC.md`、布局照 `docs/frontend.md` §2。
> 事件契约见 `docs/api.md §4`；验收见 `docs/testing.md §2`。
> 纪律：单文件 ≤300 行；颜色只用 AntD token；紫色只给 AI；不反向覆盖 `.ant-*`。

---

## 0. 现状盘点

v2 前端当前只落了**骨架 + 文档核心**（约 2,300 行）：

| 已落 | 文件 | 说明 |
|---|---|---|
| 三面板外壳 | `App.tsx` | 左树 / 中文档 / 右 Atlas，断点收起 |
| 树 | `panels/TreePanel.tsx` | 扁平列表拼树 + 本地过滤 + 新建节点 |
| 文档 | `panels/DocPanel.tsx` | 草稿防抖 / 定稿 / 版本冲突横幅 / 只读态 |
| 编辑器 | `editor/*` | Milkdown + 斜杠菜单 + 悬浮工具条 |
| 大纲 | `panels/Outline.tsx` | |
| 题记 | `panels/Epigraph.tsx` + `epigraph.ts` | v1 排版模型移植 |
| AI 壳 | `panels/AiPanel.tsx` | 仅空壳，未接模型 |
| 客户端状态 | `store.ts` | selectedId / docMode / selectionAsk |
| API | `api.ts` | `/api/tree` `/api/nodes` `/draft` `/commit` `/versions` |

**缺口 = v1 的 views / library / run / settings / shell 全部**（详见 §1）。

---

## 1. v1 → v2 文件对照（迁移清单）

图例：✅ 已迁 · 🟡 部分 · ⬜ 待迁

### 1.1 外壳 shell/
| v1 | v2 目标 | 状态 |
|---|---|---|
| `App.tsx`（Router + Routes） | `App.tsx`（react-router v7） | ⬜ |
| `components/shell/AppShell.tsx` | `shell/AppShell.tsx` | 🟡（现内联在 App.tsx） |
| `components/shell/SideNav.tsx` | `shell/SideNav.tsx` | ⬜ |
| `components/shell/TopBar.tsx` | `shell/TopBar.tsx` | 🟡（现内联） |
| `components/shell/SearchModal.tsx` | `shell/SearchModal.tsx` | ⬜ |
| `components/shell/RightSidebar.tsx` | `shell/RightSidebar.tsx` | 🟡（现内联，无标签页） |
| `components/shell/WorkTree.tsx` | `panels/TreePanel.tsx` | 🟡 |
| `components/shell/FolderPane.tsx` | `shell/FolderPane.tsx` | ⬜ |
| `components/shell/WorkDialogs.tsx` | `shell/WorkDialogs.tsx` | 🟡（现内联新建弹窗） |
| `components/shell/OutlinePane.tsx` | `panels/Outline.tsx` | 🟡 |

### 1.2 视图 views/
| v1 | v2 目标 | 状态 |
|---|---|---|
| `views/Home.tsx` | `views/Home.tsx` | ⬜ |
| `views/WorkView.tsx` | `views/WorkView.tsx` | ⬜ |
| `views/DocView.tsx` | `views/DocView.tsx` | 🟡（≈ `panels/DocPanel.tsx`） |
| `views/LibraryPool.tsx` | `views/LibraryPool.tsx` | ⬜ |
| `views/RunList.tsx` | `views/RunList.tsx` | ⬜ |
| `views/SettingsView.tsx` | `views/SettingsView.tsx` | ⬜ |
| `views/LoginView.tsx` | `views/LoginView.tsx` | ⬜ |
| `views/StubQueue.tsx` | `views/StubQueue.tsx` | ⬜ |

### 1.3 媒体库 library/
`LibraryLinkModal` · `LibraryPending` · `LibraryRelations` · `LibraryRowMenu` · `LibrarySlots` · `LibrarySupplements` · `PoolBulkBar` · `PoolContainerCard` · `PoolEntryRow` · `PoolIgnoredCard` · `PoolModals` · `PoolOtherSources` · `PoolPosterTile` · `PoolRowMenu` · `RelatedMediaPanel` · `StubTable` —— 全部 ⬜。

### 1.4 工单 run/ + AI
`AiPanel`（962 行，拆件）· `BatchCard` · `ContextMeter` · `MessageActions` · `NoticeDock` · `TodoPanel` · `conversation/*`（RunStatusRow / ThinkingRow / ToolDetail / ToolGroupChip / TurnRow / renderers）—— 全部 ⬜。
事件归约 `lib/reduceRun.ts` / `responses.ts` / `runView.ts` / `runItems.ts` / `toolFamilies.ts` / `sse.ts` → v2 拆到 `ai/transport/`（`docs/frontend.md` §4、§8）。

### 1.5 设置 settings/
`AccountCard` · `LibraryCard` · `LlmCard` · `RoleEditorModal` · `SearchCard` · `StatusCard` · `WorkCards` · `common` —— 全部 ⬜。

### 1.6 编辑器 editor/
`BlockMarkdownEditor` · `DocActions` · `EpigraphBlock` · `FeishuToolbar` · `MarkdownEditor` · `MarkdownPreview` · `PendingRevisions` → v2 `editor/*` + `panels/DocPanel`；`PendingRevisions` 不再迁（**D74**：接受 = 定稿、拒绝 = 丢弃草稿）。✅

### 1.7 纯逻辑 lib/
`api` · `store` · `liveDraft` · `mdOutline` · `blockMd` · `docMarkdown` · `epigraph`(✅) · `tree` · `theme` · `useLibraryPool` · `useWorkLibrary` · `useLiveRefresh` · `poolBatch` · `poolSections` · `mediaTags` · `settingsForm` · `useEscapeClose` · `intent` · `sessionPointer` · `routes`。🟡

---

## 2. 迁移批次（每批独立可构建、可截图验收）

| 批 | 内容 | 依赖后端 |
|---|---|---|
| **B0 基座** | react-router 路由表 · AppShell/SideNav/TopBar/SearchModal · RightSidebar 标签壳 · FolderPane · WorkDialogs · store 扩容 · api 扩容 | `/api/auth/me` `/api/search` |
| **B1 文档核心** | Home · WorkView · DocView · Outline · 题记 · 修订 / 版本回滚 / diff · 文件夹 | 已有 + `/api/works/:id/*` |
| **B2 媒体库** | LibraryPool + library/* 全部 · RelatedMediaPanel | `/api/library/*` |
| **B3 工单 + Atlas** | RunList + run/* 全部 + `ai/transport`（response.* → X runtime）+ @ant-design/x 组件 | `/api/runs` `/api/ai/*` SSE |
| **B4 设置 + 登录** | SettingsView + settings/* · LoginView | `/api/settings` `/api/auth/*` |
| **B5 待写正文** | StubQueue + StubTable | `/api/list-works?hasContent=false` |

> **域模型接缝**：v1 的 `Work` / `Doc` 两分，在 v2 归一到 `Node{kind}`（universe/series/work/doc）。v1 的 WorkView = v2 的 `kind∈{universe,series,work}` 节点页；v1 的 DocView = v2 的 `kind=doc` 节点页。移植时**统一走 Node**，不要再引入 Work/Doc 概念。

---

## 3. 组件对照（Semi → AntD）

| Semi | AntD | 备注 |
|---|---|---|
| `Button.Tooltip` | `Button.Tooltip` | |
| `Toast` | `App.useApp().message` | |
| `Empty` | `Empty` | |
| `Tag` | `Tag` | |
| `Dropdown.Menu/Item` | `Dropdown` + `items` | v6 menu → `menu={{items}}` |
| `Layout.Sider` | 自绘 `aside`（保持 v1 宽度常量） | |
| `Modal` | `Modal` | `visible`→`open` |
| `Input showClear` | `Input allowClear` | |
| `IconXxx`（semi-icons） | `@ant-design/icons` | 见 §4 |
| `Avatar` | `Avatar` | |
| `Spin` | `Spin` | |
| `Skeleton` | `Skeleton` | |
| `Table` | `Table` | |
| `Form` | `Form` | API 近似 |

图标对照见 `docs/frontend.md`；语义相同即可，不追求图形一致。

---

## 4. 风险与待决

1. **后端契约缺口**：B0/B1 部分端点（auth/search/works）v2 Rust 侧尚未实现。前端按 v1 语义先行，端点落地即通；缺端点时 UI 需优雅降级（访客态 / 空态），不得白屏。
2. **Work/Doc → Node**：`docs/frontend.md` §5 的草稿/定稿模型以 Node 为准，移植时以 v2 为准，不照抄 v1 的 `/api/works`。
3. **AiPanel 拆分**：v1 962 行单文件必须拆（`ai/transport` · `ai/session` · 会话组件 · 宿主部件），不得照搬。
4. **编辑器**：Milkdown 方案已定（`docs/frontend.md` §1），v1 的 BlockNote 不迁。
5. **大样式表**：v1 `index.css` 3730 行不迁；用 AntD token + CSS Modules。

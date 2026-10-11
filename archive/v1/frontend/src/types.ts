// Shared API contract for WikiAltas v2. Mirrors DESIGN_V2.md §3 and §6.
// Keep in sync with shared/types.ts

export type UUID = string

export type WorkKind = 'universe' | 'series' | 'work'
export type Medium = 'game' | 'movie' | 'tv' | 'manga' | 'book' | 'other'
export type WorkStatus = 'stub' | 'draft' | 'ready' | 'archived'
export type Visibility = 'public' | 'private'
export type Author = 'human' | 'llm' | 'import'
export type RelationType =
  | 'adaptation_of'
  | 'sequel_to'
  | 'spin_off_of'
  | 'remake_of'
  | 'expansion_of'
  | 'references'

export type LibrarySource = 'emby' | 'komga' | 'gameatlas'

export type RunIntent =
  | 'create_wiki'
  | 'continue_wiki'
  | 'rewrite_section'
  | 'write_doc'
  | 'organize_tree'
  | 'answer'

export type RunStatus = 'running' | 'interrupted' | 'completed' | 'failed' | 'expired'

export interface Work {
  id: UUID
  parentId: UUID | null
  kind: WorkKind
  medium: Medium | null
  title: string
  aliases: string[]
  tags: string[]
  contentMd: string | null
  contentVer: number
  status: WorkStatus
  visibility: Visibility
  sortOrder: number
  createdAt: string
  updatedAt: string
}

export interface WorkSummary {
  id: UUID
  parentId: UUID | null
  kind: WorkKind
  medium: Medium | null
  title: string
  /** 别名：媒体库条目标题与它精确同名时 = "Wiki 已有这个条目"（池子页靠它给「挂到《X》下」）。 */
  aliases?: string[]
  status: WorkStatus
  visibility: Visibility
  hasContent: boolean
  hasLibraryLink: boolean
  sortOrder: number
  updatedAt: string
}

export interface Doc {
  id: UUID
  folderOf: UUID
  title: string
  contentMd: string
  contentVer: number
  links: UUID[]
  createdAt: string
  updatedAt: string
}

export interface Revision {
  id: UUID
  targetType: 'work' | 'doc'
  targetId: UUID
  version: number
  author: Author
  runId: UUID | null
  summary: string
  createdAt: string
  contentMd?: string
}

export interface Relation {
  id: UUID
  fromId: UUID
  toId: UUID
  type: RelationType
  createdAt: string
}

export interface LibraryLink {
  id: UUID
  workId: UUID
  source: LibrarySource
  externalId: string
  url: string | null
  titleHint: string | null
  /** 角色：work=正文/正片级 / mix=混杂内容级（OST、解析…）。 */
  role: 'work' | 'mix'
  /** 挂链出处：sweep/pool/manual/archive/legacy。 */
  linkedBy?: string
  /** 展示用：读取时从扫描清单补齐的封面小图（没扫到就没有）。 */
  coverImage?: string
  createdAt: string
}

/** 库搜索结果 / 关联候选：带挂链状态。 */
export interface LibrarySearchEntry {
  publicId: string
  title: string
  titleAlt: string | null
  releaseDate: string | null
  coverImage: string | null
  series?: { id: number; name: string } | null
  url: string
  linked: boolean
  linkedWorkId?: string
  /** Emby：movie | series | album | collection；Komga：book */
  kind?: string
  /** Komga：comic | novel */
  format?: string
}

/** Emby 媒体库（设置页选角色用）。 */
export interface EmbyLibraryView {
  id: string
  name: string
  collectionType: string
}

/** 设置里存的 Emby 库角色：work = 正片（建档建议）；mixed = 混杂内容（关联候选）。 */
export interface EmbyLibraryRoleSetting {
  id?: string
  name: string
  role: 'work' | 'mixed'
}

/** 建议池条目：后台扫描清单的一条（挂链状态是实时的）。 */
export interface PoolEntry {
  source: LibrarySource
  publicId: string
  title: string
  titleAlt?: string | null
  releaseDate?: string | null
  coverImage?: string
  url: string
  kind: string
  format?: string
  extra?: string
  linkedWorkId?: string
  linkedWorkTitle?: string
  firstSeenAt?: string
  isNew?: boolean
  /** 条目固有角色：work=正片级 / mix=混杂级。 */
  role?: 'work' | 'mix'
}

/** 建议池分组：按源 → 容器（库/系列/合集）。 */
export interface PoolGroup {
  source: LibrarySource
  containerKey: string
  containerTitle: string
  containerKind?: string
  unlinked: PoolEntry[]
  linkedCount: number
  ignoredCount: number
  newCount: number
}

// 这里曾有 PoolAiSuggestion / PoolAiRejected（「AI 建议」那套）：已随池子页重做退役。
// 节点页扫库的建议是另一套类型（NodeSweepSuggestion 等），不受影响。

export interface PoolIgnoredGroup {
  source: LibrarySource
  containerKey: string
  containerTitle: string
}

export interface LibraryPoolResponse {
  scannedAt: string
  previousScannedAt?: string
  scanIntervalMinutes: number
  sourcesConfigured: { gameatlas?: boolean; emby?: boolean; komga?: boolean }
  rolesConfigured: boolean
  groups: PoolGroup[]
  ignoredEntries: PoolEntry[]
  ignoredGroups: PoolIgnoredGroup[]
}

/** 节点扫库：一部作品的「媒体库发现」（写完之后自动扫，也可手动重扫）。 */
export interface NodeSweepSuggestion {
  key: string
  source: LibrarySource
  entryId: string
  title: string
  kind?: string
  format?: string
  extra?: string
  coverImage?: string
  url: string
  role?: 'work' | 'mix'
  /** 手动扫完才有:挂本节点(link)还是建档(archive),以及归属哪个节点 */
  action?: 'link' | 'archive'
  targetNodeId?: string
  confidence: number
  reason: string
}

export interface NodeSweepSourceStatus {
  source: LibrarySource
  status: 'ok' | 'unconfigured' | 'error'
  message?: string
  candidates: number
}

export interface NodeSweepDismissed {
  workId: string
  key: string
  source: LibrarySource
  title: string
  /** 忽略时写的理由（可选，有当儿的会作为反馈喂给「AI 对一遍」）。 */
  reason?: string
}

/** 手动扫一遍的响应：不含 dismissed（忽略记录在 GET 里读）。 */
export interface NodeSweepRunResponse {
  ok: boolean
  workId: string
  model?: string
  generatedAt?: string
  sources?: NodeSweepSourceStatus[]
  suggestions: NodeSweepSuggestion[]
  total?: number
}

// --- 节点页「关联与媒体」统一数据面（GET /api/library/node/{workId}）---

/** 节点页一条链的展示行：链本身 + 清单补的条目信息。 */
export interface NodeLibraryLink {
  id: string
  source: LibrarySource
  role: 'work' | 'mix'
  externalId: string
  url?: string
  title: string
  coverImage?: string
  kind?: string
  format?: string
  releaseDate?: string
  extra?: string
  linkedBy?: string
  createdAt: string
}

/** 节点页一条「待确认」：目标 = 本节点（link 挂本节点 / archive 在本节点下建档）。 */
export interface NodePendingItem {
  key: string
  source: LibrarySource
  entryId: string
  title: string
  kind?: string
  format?: string
  extra?: string
  coverImage?: string
  url?: string
  role: 'work' | 'mix'
  action: 'link' | 'archive'
  targetNodeId: string
  confidence: number
  reason: string
  origin?: string
  /** 非空 = 这条挂法被拒绝过、模型这次又提了（行不藏，让用户看见复读）。 */
  rejectedReason?: string
}

export interface NodeLibrarySource {
  source: LibrarySource
  status: 'ok' | 'unconfigured'
  message?: string
}

export interface NodeLibraryResponse {
  workId: string
  work: {
    id: string
    title: string
    kind: WorkKind
    medium?: string
    status: string
    aliases?: string[]
  }
  links: { work: NodeLibraryLink[]; mix: NodeLibraryLink[] }
  pending: NodePendingItem[]
  dismissed: NodeSweepDismissed[]
  sources: NodeLibrarySource[]
  generatedAt?: string
  model?: string
}

export interface Run {
  id: UUID
  intent: RunIntent
  goal: string
  status: RunStatus
  result: Record<string, unknown> | null
  error: Record<string, unknown> | null
  /** 从 checkpoint 投影出来的上下文（workId 等），用于跳转 */
  context?: { workId?: UUID; docId?: UUID; parentId?: UUID; medium?: string }
  model: string
  startedAt: string
  lastActive: string
  expiresAt: string | null
  completedAt: string | null
}

/** 会话 = 一段连续对话（后端 runs.workspace 即会话 id）。 */
export interface Session {
  id: string
  /** 归属页面：work:<id> / doc:<id> / home */
  target: string
  title: string
  /** 最近一句用户消息：列表的副标题用它（标题只在建会话时定一次） */
  lastGoal: string
  runCount: number
  /** 最近一次执行的状态；有 running 优先；空串 = 还没有执行 */
  status: '' | RunStatus
  createdAt: string
  updatedAt: string
}

export interface BatchResult {
  batchId: string
  workspace: string
  runIds: string[]
  skippedIds?: string[]
}

/**
 * Run 事件类型。两条线**同流不同类**：
 *
 *  · `response.*` —— OpenAI Responses 规范的流事件。前端把它们整条喂给 Semi 的
 *    流式归约器（`streamingResponseToMessage`），模型说了什么、调了什么工具，
 *    都由归约器折出来，我们不再自己造形状。
 *  · `wikiatlas.*` —— 规范里没有、产品需要的东西（任务清单 / 用量与缓存命中 /
 *    宿主播报与护栏提醒 / 元信息）。它们交给宿主的部件渲染，**不进对话**。
 *
 * 类型清单的 owner 是后端 `internal/run/responses.go`；这里是同一份契约的前端侧。
 */
export type RunEventType =
  // ── Responses 流事件 ──
  | 'response.created'
  | 'response.in_progress'
  | 'response.output_item.added'
  | 'response.output_item.done'
  | 'response.output_text.delta'
  | 'response.output_text.done'
  | 'response.reasoning_summary_text.delta'
  | 'response.reasoning_summary_text.done'
  | 'response.function_call_arguments.delta'
  | 'response.function_call_arguments.done'
  | 'response.completed'
  | 'response.failed'
  // ── wikiatlas.* 旁路 ──
  | 'wikiatlas.todo'
  | 'wikiatlas.usage'
  | 'wikiatlas.notice'
  | 'wikiatlas.meta'
  | 'wikiatlas.request'
  | 'wikiatlas.context'
  | 'wikiatlas.tree'
  | 'wikiatlas.content.staging'
  | 'wikiatlas.content.committed'
  | 'wikiatlas.content.delta'
  | 'wikiatlas.run.end'

/** 正文流的一段（wikiatlas.content.delta）：写正文类工具的流式参数被投影成的文本段。 */
export interface ContentDeltaPayload {
  callId?: string
  name?: string
  targetType?: string
  targetId?: string
  /** patch_section 的目标章节（替换哪一节） */
  heading?: string
  /** 新增的一段正文 */
  delta?: string
  /** 前缀对不上时的整体重发：前端用 text 替换缓冲（而不是追加） */
  reset?: boolean
  text?: string
}

/** 任务清单里的一条（todo_write 工具的全量清单）。 */
export interface RunTaskInfo {
  id: string
  content: string
  /** 进行式的说法：正在做的那条显示它（"正在核实主创" 而不是 "核实主创"） */
  activeForm?: string
  status: 'pending' | 'in_progress' | 'completed' | string
}

export interface RunEvent {
  id: string
  runId: UUID
  /** 落库序号（1 起）。协议里的 sequence_number = seq - 1，见 payload。 */
  seq: number
  type: RunEventType
  payload: Record<string, unknown>
  createdAt: string
}

export interface SearchHit {
  id: UUID
  kind: 'work' | 'doc'
  title: string
  snippet: string
}

export interface CreateWorkBody {
  parentId?: UUID | null
  kind: WorkKind
  medium?: Medium
  title: string
  visibility?: Visibility
}

export interface PatchWorkBody {
  title?: string
  parentId?: UUID | null
  medium?: Medium
  status?: WorkStatus
  visibility?: Visibility
  sortOrder?: number
  aliases?: string[]
  tags?: string[]
}

/** 简易用户系统：当前会话状态（访客=authed:false）。 */
export interface Me {
  authed: boolean
  username: string
  adminPasswordConfigured: boolean
}

export interface PutContentBody {
  contentMd: string
  author: Author
  runId?: UUID
  summary?: string
  expectedVersion?: number
}

export interface CreateDocBody {
  title: string
  contentMd?: string
  links?: UUID[]
}

export interface CreateRunBody {
  intent: RunIntent
  goal: string
  /** 会话键：work:<id> / doc:<id> / home / batch:<id> */
  workspace?: string
  /** 思考等级（聊天框里选的档位）：none | low | medium | high | xhigh | max；留空用设置页的值 */
  reasoningEffort?: string
  context?: {
    workId?: UUID
    docId?: UUID
    parentId?: UUID
    medium?: Medium
    section?: string
    /** 发起工单时用户所处的文档模式：读=只分析、编辑=直接改、修订=定点改+给理由 */
    docMode?: 'read' | 'edit' | 'revision'
    /** 用户在正文里选中的文本（修订模式的作用域） */
    selection?: string
    /** 一次性入口的能力契约（工具栏翻译/解释）：运行时剥写工具，只输出不落盘 */
    oneShot?: boolean
    /** @ 引用的条目（"标题(type:id)、…"），执行器拼进工单消息 */
    mentioned?: string
    /**
     * 用户此刻所在的页面。会话键（workspace）只区分 work/doc/home，
     * 媒体库建议、工单页、设置页全被归成 home —— 模型因此不知道"你在哪一页"
     * （实测：在媒体库建议里问"你知道现在是什么页面吗"，它只能如实说看不到）。
     */
    page?: RunPage
    extra?: Record<string, unknown>
  }
}

/** 页面种类（与 App.tsx 的路由一一对应；模型看到的是一句人话，见后端 pageGlossary）。 */
export type RunPage = 'home' | 'work' | 'folder' | 'doc' | 'runs' | 'library' | 'settings'

export interface Settings {
  llm: {
    endpoint: string
    model: string
    apiKeyConfigured: boolean
    temperature?: number
    maxTokens?: number
    /** 思考等级：off | low | medium | high | xhigh | max */
    reasoningEffort?: string
    /** 上下文窗口（token），面板用量环的分母；0/未设 = 默认 262144 */
    contextWindow?: number
  }
  library: {
    embyUrl?: string
    embyApiKeyConfigured?: boolean
    komgaUrl?: string
    komgaApiKeyConfigured?: boolean
    gameatlasUrl?: string
    gameatlasApiKeyConfigured?: boolean
    /** Emby 媒体库角色（正片 / 混杂内容） */
    embyLibraryRoles?: EmbyLibraryRoleSetting[]
    /** 媒体库后台扫描间隔（分钟；0 = 关闭；未设为默认 360） */
    scanIntervalMinutes?: number
  }
  runs: {
    expireDays: number
    keepEventsDays: number
    maxConcurrentRuns: number
  }
  search: {
    exaApiKeyConfigured: boolean
    /** 出外网的 HTTP 代理。fetch_url 直连超时时，模型会带 useProxy 用它重试同一页。 */
    proxyUrl?: string
  }
  admin: {
    username: string
    passwordConfigured: boolean
    usingDefaultPassword: boolean
    newNodeVisibility: Visibility
  }
  skillRoot: string
}

/** 设置页提交的载荷（密钥留空=不修改，clearXxx=清空）。 */
export interface SettingsPayload {
  llm: {
    endpoint: string
    model: string
    apiKey?: string
    clearApiKey?: boolean
    temperature?: number
    maxTokens?: number
    protocol?: string
    reasoningEffort?: string
    contextWindow?: number
  }
  search: {
    exaApiKey?: string
    clearExaApiKey?: boolean
    /** 空串 = 清掉代理（文本输入框的自然语义） */
    proxyUrl?: string
  }
  library: {
    embyUrl?: string
    embyApiKey?: string
    komgaUrl?: string
    komgaApiKey?: string
    gameatlasUrl?: string
    gameatlasApiKey?: string
    /** nil = 不提交（不动）；非 nil = 整体替换（设置页按当前库清单生成） */
    embyLibraryRoles?: EmbyLibraryRoleSetting[]
    /** 媒体库后台扫描间隔（分钟；0 = 关闭） */
    scanIntervalMinutes?: number
  }
  runs: {
    expireDays: number
    keepEventsDays: number
    maxConcurrentRuns: number
  }
  admin?: {
    username?: string
    newPassword?: string
    clearPassword?: boolean
    newNodeVisibility?: Visibility
  }
  skillRoot: string
}

export interface RuntimeInfo {
  model: string
  /** 当前模型的上下文窗口（token），面板右下角用量环的分母 */
  contextWindow: number
  maxConcurrentRuns: number
  activeRuns: number
  queuedRuns: number
  skillRoot: string
  stats: {
    works: number
    docs: number
    revisions: number
    runs: number
    runEvents: number
    runningNow: number
  }
  features: string[]
}

export interface PutContentResult {
  id: UUID
  contentVer: number
  revisionId: UUID
}

export interface OutlineHeading {
  id: string
  level: 2 | 3
  /** 目录编号，如 "1." / "2.1"；正文里已带编号时原样保留 */
  number: string
  /** 去掉编号的标题文本，用于渲染「编号 + 文本」 */
  text: string
}

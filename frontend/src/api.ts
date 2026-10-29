// 后端 DTO 的镜像。字段名与 Rust 的 snake_case 一致（serde 默认），
// 别在这里改名——契约只有一份。
//
// 待办（下一轮）：由 Rust 生成 TS 类型（ts-rs），彻底消灭手抄漂移。

export type NodeKind = 'universe' | 'series' | 'work' | 'doc'
export type Medium = 'game' | 'movie' | 'tv' | 'manga' | 'book' | 'other'
export type NodeStatus = 'draft' | 'ready' | 'archived'

export interface Node {
  id: string
  parent_id: string | null
  kind: NodeKind
  medium: Medium | null
  title: string
  title_norm: string
  aliases: string[]
  tags: string[]
  status: NodeStatus
  visibility: 'private' | 'public'
  sort_key: string
  created_at: string
  updated_at: string
}

export interface Content {
  node_id: string
  body_md: string
  ver: number
  updated_by: string
  updated_at: string
}

export interface Draft {
  node_id: string
  body_md: string
  base_ver: number
  by: string
  lease_until: string
  updated_at: string
}

export interface NodeContent {
  node: Node
  content: Content | null
  draft: Draft | null
}

export interface CommitResp {
  ver: number
  updated_at: string
  sync_job_id?: number
}

export interface Version {
  ver: number
  created_at: string
  summary: string
}

// ---- 媒体库（M1）：清单 / 池子 / 扫描 ----

export type LibrarySource = 'gameatlas' | 'emby' | 'komga'
export type ContainerKind = 'series' | 'library' | 'mixed'
export type LinkRole = 'work' | 'mix'

export interface ManifestRow {
  source: LibrarySource
  external_id: string
  title: string
  subtitle: string | null
  aliases: string[]
  year: number | null
  kind: string
  format: string | null
  cover_url: string | null
  url: string | null
  container_key: string
  container_title: string
  container_kind: ContainerKind
  extra: string
  content_hash: string
  seen_at: string
  first_seen_at: string
  missing_since: string | null
}

export interface PoolEntry {
  entry: ManifestRow
  link_id: string | null
  node_id: string | null
  node_title: string | null
  node_kind: string | null
  role: LinkRole
}

export interface ScanRun {
  id: number
  source: LibrarySource
  started_at: string
  finished_at: string | null
  ok: boolean | null
  stats: { seen?: number; inserted?: number; changed?: number; unchanged?: number; missing?: number }
  message: string
}

export interface LibraryStatus {
  configured: Record<LibrarySource, boolean>
  queueDepth: number
}

export interface BuildLinkReq {
  source: LibrarySource
  external_id: string
  role?: LinkRole
  parent_id?: string | null
}

export interface BuiltDoc {
  node: Node
  link_id: string
  reused: boolean
}

export interface Ignore {
  id: string
  source: LibrarySource | null
  external_id: string | null
  group_key: string | null
  reason: string
  created_by: string
  created_at: string
}

export interface CreateRunResp {
  runId: string
  jobId: number
  sessionId: string
}

export interface SessionInfo {
  id: string
  target: string
  title: string
  message_count: number
  preview: string
  created_at: string
  updated_at: string
}

export interface SessionMessage {
  id: string
  seq: number
  role: 'user' | 'assistant'
  content: string
  created_at: string
}

export interface SkillInfo {
  name: string
  description: string
  group: string
  layer: number
  chars: number
}

export interface RecentNode {
  id: string
  title: string
  kind: NodeKind
  medium: Medium | null
  status: NodeStatus
  has_content: boolean
  ver: number
  updated_at: string
}

export interface HomeStats {
  works: number
  committed: number
  drafting: number
  unbuilt: number
  recent: RecentNode[]
}

export interface NodeLink {
  id: string
  source: LibrarySource
  external_id: string
  title_hint: string
  url: string | null
  role: string
  state: 'active' | 'stale' | 'replaced'
  missing_since: string | null
  replaced_by: string | null
  sync_targets: SyncTarget[]
}

export type SyncTargetName = 'gameatlas' | 'komga'
export type SyncStatus = 'pending' | 'ok' | 'failed' | 'skipped'

export interface SyncTarget {
  link_id: string
  target: SyncTargetName
  synced_ver: number | null
  status: SyncStatus
  message: string
  synced_at: string | null
}

export interface RelinkCandidate {
  source: LibrarySource
  external_id: string
  title: string
  subtitle: string | null
  aliases: string[]
  year: number | null
  cover_url: string | null
  url: string | null
  container_title: string
  score: number
}

export interface RelationType {
  type: string
  from_label: string
  to_label: string
  note: string
}

export interface RelationNode {
  id: string
  title: string
  kind: NodeKind
  medium: Medium | null
}

export interface RelationView {
  id: string
  type: string
  direction: 'out' | 'in'
  label: string
  other: RelationNode
  created_by: string
  created_at: string
}

export type SynthesisType = 'role' | 'worldbuilding' | 'timeline' | 'theme' | 'creator'

export interface SynthesisReq {
  parent_id?: string | null
  title: string
  doc_type: SynthesisType
  work_ids: string[]
}

export type SuggestionKind = 'create' | 'link' | 'merge' | 'research' | 'ignore' | 'split'
export type SuggestionState = 'pending' | 'accepted' | 'rejected' | 'ignored'

export interface Suggestion {
  id: string
  kind: SuggestionKind
  node_id: string | null
  source: LibrarySource | null
  external_id: string | null
  payload: Record<string, unknown>
  state: SuggestionState
  created_by: string
  created_at: string
  decided_at: string | null
}

export type FindingSeverity = 'info' | 'warn' | 'error'
export type FindingState = 'open' | 'resolved' | 'ignored'

export interface HealthFinding {
  id: string
  kind: string
  node_id: string | null
  severity: FindingSeverity
  detail: Record<string, unknown>
  state: FindingState
  created_at: string
  resolved_at: string | null
}

export interface SearchHit {
  node_id: string
  title: string
  kind: NodeKind
  medium: Medium | null
  status: NodeStatus
  score: number
  snippet: string
}

export interface SettingsResponse {
  settings: Record<string, unknown>
  secrets_configured: Record<string, boolean>
}

export interface ModelUsage {
  model: string
  runs: number
  input_tokens: number
  output_tokens: number
  total_tokens: number
  estimated_cost: number | null
}

export interface UsageSummary {
  runs: number
  input_tokens: number
  output_tokens: number
  total_tokens: number
  estimated_cost: number | null
  pricing_configured: boolean
  models: ModelUsage[]
}

export interface AuthStatus {
  configured: boolean
  authenticated: boolean
}

export interface PoolQuery {
  source?: LibrarySource
  container?: string
  has_doc?: boolean
  include_missing?: boolean
}

export interface CreateNodeReq {
  parent_id?: string | null
  kind: NodeKind
  medium?: Medium | null
  title: string
}

/** PATCH /api/nodes/{id}：只传要改的字段；`move_to_root` 处理“移到根”。 */
export interface UpdateNodeReq {
  title?: string
  aliases?: string[]
  tags?: string[]
  parent_id?: string
  move_to_root?: boolean
}

export interface RunInfo {
  id: string
  status: string
  cancel_requested: boolean
  goal: string
  model: string
  checkpoint: Record<string, unknown>
  result: unknown
  error: unknown
  started_at: string
  completed_at: string | null
}

/** 后端错误体（`crates/api/src/error.rs` 里那几个形状）。 */
export interface ApiErrorBody {
  error: 'not_found' | 'duplicated' | 'version_conflict' | 'validation' | 'internal'
  message: string
  baseVer?: number
  currentVer?: number
}

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly body: ApiErrorBody | null,
  ) {
    super(body?.message ?? `HTTP ${status}`)
  }
}

async function toJson<T>(r: Response): Promise<T> {
  const text = await r.text()
  const body = text ? JSON.parse(text) : null
  if (!r.ok) throw new ApiError(r.status, body as ApiErrorBody)
  return body as T
}

const jsonHeaders = { 'Content-Type': 'application/json' }

export const api = {
  authStatus: () => fetch('/api/auth').then(toJson<AuthStatus>),

  login: (password: string) =>
    fetch('/api/login', {
      method: 'POST',
      headers: jsonHeaders,
      body: JSON.stringify({ password }),
    }).then(toJson<{ ok: boolean; configured: boolean }>),

  logout: () => fetch('/api/logout', { method: 'POST' }).then(toJson<{ ok: boolean }>),

  tree: () => fetch('/api/tree').then(toJson<Node[]>),

  archived: () => fetch('/api/archived').then(toJson<Node[]>),

  overview: () => fetch('/api/overview').then(toJson<HomeStats>),

  nodeLinks: (id: string) => fetch(`/api/nodes/${id}/links`).then(toJson<NodeLink[]>),

  node: (id: string) => fetch(`/api/nodes/${id}`).then(toJson<NodeContent>),

  archiveNode: (id: string) =>
    fetch(`/api/nodes/${id}/archive`, { method: 'POST' }).then(toJson<Node>),

  unarchiveNode: (id: string) =>
    fetch(`/api/nodes/${id}/unarchive`, { method: 'POST' }).then(toJson<Node>),

  updateNode: (id: string, req: UpdateNodeReq) =>
    fetch(`/api/nodes/${id}`, {
      method: 'PATCH',
      headers: jsonHeaders,
      body: JSON.stringify(req),
    }).then(toJson<Node>),

  run: (id: string) => fetch(`/api/runs/${encodeURIComponent(id)}`).then(toJson<RunInfo>),

  cancelRun: (id: string) =>
    fetch(`/api/runs/${encodeURIComponent(id)}/cancel`, { method: 'POST' }).then(
      toJson<{ id: string; status: string; cancel_requested: boolean }>,
    ),

  createNode: (req: CreateNodeReq) =>
    fetch('/api/nodes', { method: 'POST', headers: jsonHeaders, body: JSON.stringify(req) }).then(
      toJson<Node>,
    ),

  putDraft: (id: string, body_md: string, base_ver: number) =>
    fetch(`/api/nodes/${id}/draft`, {
      method: 'PUT',
      headers: jsonHeaders,
      body: JSON.stringify({ body_md, base_ver }),
    }).then(toJson<Draft>),

  discardDraft: (id: string) =>
    fetch(`/api/nodes/${id}/draft`, { method: 'DELETE' }).then(
      toJson<{ discarded: boolean }>,
    ),

  commit: (id: string, body_md: string, base_ver: number, summary = '') =>
    fetch(`/api/nodes/${id}/commit`, {
      method: 'POST',
      headers: jsonHeaders,
      body: JSON.stringify({ body_md, base_ver, summary }),
    }).then(toJson<CommitResp>),

  versions: (id: string) => fetch(`/api/nodes/${id}/versions`).then(toJson<Version[]>),

  versionBody: (id: string, ver: number) =>
    fetch(`/api/nodes/${id}/versions/${ver}`).then(toJson<{ ver: number; body_md: string }>),

  relationTypes: () => fetch('/api/relation-types').then(toJson<RelationType[]>),

  relations: (nodeId: string) =>
    fetch(`/api/relations?node_id=${encodeURIComponent(nodeId)}`).then(toJson<RelationView[]>),

  createRelation: (from_id: string, to_id: string, type: string) =>
    fetch('/api/relations', {
      method: 'POST',
      headers: jsonHeaders,
      body: JSON.stringify({ from_id, to_id, type }),
    }).then(toJson<{ id: string }>),

  deleteRelation: (id: string) =>
    fetch(`/api/relations/${id}`, { method: 'DELETE' }).then(toJson<{ ok: boolean }>),

  listSynthesis: () => fetch('/api/synthesis').then(toJson<Node[]>),

  createSynthesis: (req: SynthesisReq) =>
    fetch('/api/synthesis', {
      method: 'POST',
      headers: jsonHeaders,
      body: JSON.stringify(req),
    }).then(toJson<Node>),

  syncIndex: (nodeId: string) =>
    fetch(`/api/nodes/${nodeId}/index`, {
      method: 'POST',
      headers: jsonHeaders,
      body: '{}',
    }).then(toJson<Node>),

  suggestions: (state: SuggestionState = 'pending') =>
    fetch(`/api/suggestions?state=${state}`).then(toJson<Suggestion[]>),

  acceptSuggestion: (id: string, reason = '') =>
    fetch(`/api/suggestions/${id}/accept`, {
      method: 'POST',
      headers: jsonHeaders,
      body: JSON.stringify({ reason }),
    }).then(toJson<{ suggestion: Suggestion; action: string }>),

  rejectSuggestion: (id: string, reason = '') =>
    fetch(`/api/suggestions/${id}/reject`, {
      method: 'POST',
      headers: jsonHeaders,
      body: JSON.stringify({ reason }),
    }).then(toJson<{ suggestion: Suggestion; action: string }>),

  healthFindings: (state: FindingState = 'open') =>
    fetch(`/api/health-findings?state=${state}`).then(toJson<HealthFinding[]>),

  runAudit: () =>
    fetch('/api/health-findings/run', {
      method: 'POST',
      headers: jsonHeaders,
      body: '{}',
    }).then(toJson<{ jobId: number }>),

  search: (q: string, limit = 20) =>
    fetch(`/api/search?q=${encodeURIComponent(q)}&limit=${limit}`).then(toJson<SearchHit[]>),

  saveAnswer: (req: {
    parent_id?: string | null
    title: string
    body_md: string
    ref_ids: string[]
  }) =>
    fetch('/api/answers', {
      method: 'POST',
      headers: jsonHeaders,
      body: JSON.stringify(req),
    }).then(toJson<Node>),

  settings: () => fetch('/api/settings').then(toJson<SettingsResponse>),

  putSettings: (settings: Record<string, unknown>) =>
    fetch('/api/settings', {
      method: 'PUT',
      headers: jsonHeaders,
      body: JSON.stringify(settings),
    }).then(toJson<{ ok: boolean }>),

  putSecrets: (secrets: Record<string, string>) =>
    fetch('/api/secrets', {
      method: 'PUT',
      headers: jsonHeaders,
      body: JSON.stringify(secrets),
    }).then(toJson<{ ok: boolean }>),

  usage: () => fetch('/api/usage').then(toJson<UsageSummary>),

  exportProject: () => fetch('/api/export').then(toJson<Record<string, unknown>>),

  importProject: (bundle: Record<string, unknown>) =>
    fetch('/api/import', {
      method: 'POST',
      headers: jsonHeaders,
      body: JSON.stringify(bundle),
    }).then(toJson<{ ok: boolean; nodes: number }>),

  libraryPool: (q: PoolQuery = {}) =>
    fetch(`/api/library/pool${qs({ ...q })}`).then(toJson<PoolEntry[]>),

  libraryStatus: () => fetch('/api/library/status').then(toJson<LibraryStatus>),

  libraryScanRuns: () => fetch('/api/library/scan-runs').then(toJson<ScanRun[]>),

  /** 建档：原子建【节点 + 链】（落位规则在后端）。 */
  buildLink: (req: BuildLinkReq) =>
    fetch('/api/library/links', {
      method: 'POST',
      headers: jsonHeaders,
      body: JSON.stringify(req),
    }).then(toJson<BuiltDoc>),

  /** 解链：**保留文档**。返回文档节点 id。 */
  unlink: (linkId: string) =>
    fetch(`/api/library/links/${linkId}`, { method: 'DELETE' }).then(toJson<{ nodeId: string }>),

  relinkCandidates: (linkId: string) =>
    fetch(`/api/library/links/${linkId}/relink-candidates`).then(toJson<RelinkCandidate[]>),

  relink: (linkId: string, external_id: string) =>
    fetch(`/api/library/links/${linkId}/relink`, {
      method: 'POST',
      headers: jsonHeaders,
      body: JSON.stringify({ external_id }),
    }).then(toJson<{ linkId: string }>),

  resyncLink: (linkId: string) =>
    fetch(`/api/library/links/${linkId}/sync`, {
      method: 'POST',
      headers: jsonHeaders,
      body: '{}',
    }).then(toJson<{ jobId: number }>),

  libraryIgnores: () => fetch('/api/library/ignores').then(toJson<Ignore[]>),

  addIgnore: (req: { source?: LibrarySource; external_id?: string; group_key?: string; reason?: string }) =>
    fetch('/api/library/ignores', {
      method: 'POST',
      headers: jsonHeaders,
      body: JSON.stringify(req),
    }).then(toJson<{ id: string }>),

  removeIgnore: (id: string) =>
    fetch(`/api/library/ignores/${id}`, { method: 'DELETE' }).then(toJson<{ ok: boolean }>),

  createRun: (req: {
    node_id: string
    prompt: string
    session_id?: string
    intent?: string
    context?: {
      doc_mode?: 'edit' | 'proofread' | 'read'
      one_shot?: boolean
      skills?: string[]
    }
  }) =>
    fetch('/api/runs', {
      method: 'POST',
      headers: jsonHeaders,
      body: JSON.stringify(req),
    }).then(toJson<CreateRunResp>),

  sessions: (nodeId: string) =>
    fetch(`/api/sessions?node_id=${encodeURIComponent(nodeId)}`).then(toJson<SessionInfo[]>),

  createSession: (nodeId: string, title?: string) =>
    fetch('/api/sessions', {
      method: 'POST',
      headers: jsonHeaders,
      body: JSON.stringify({ node_id: nodeId, title }),
    }).then(toJson<SessionInfo>),

  sessionMessages: (sessionId: string) =>
    fetch(`/api/sessions/${encodeURIComponent(sessionId)}`).then(
      toJson<{ session_id: string; messages: SessionMessage[] }>,
    ),

  skills: () => fetch('/api/skills').then(toJson<{ skills: SkillInfo[] }>),

  libraryScan: (sources?: LibrarySource[]) =>
    fetch('/api/library/scan', {
      method: 'POST',
      headers: jsonHeaders,
      body: JSON.stringify(sources && sources.length ? { sources } : {}),
    }).then(toJson<{ jobId: number }>),
}

/** 拼查询串：undefined 直接丢掉（后端的 Option 靠"参数缺失"表示不过滤）。 */
function qs(q: Record<string, unknown>): string {
  const parts = Object.entries(q)
    .filter(([, v]) => v !== undefined && v !== null && v !== '')
    .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(String(v))}`)
  return parts.length ? `?${parts.join('&')}` : ''
}

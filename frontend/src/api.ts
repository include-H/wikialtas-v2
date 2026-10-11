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
  tree: () => fetch('/api/tree').then(toJson<Node[]>),

  node: (id: string) => fetch(`/api/nodes/${id}`).then(toJson<NodeContent>),

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

  commit: (id: string, body_md: string, base_ver: number, summary = '') =>
    fetch(`/api/nodes/${id}/commit`, {
      method: 'POST',
      headers: jsonHeaders,
      body: JSON.stringify({ body_md, base_ver, summary }),
    }).then(toJson<CommitResp>),

  versions: (id: string) => fetch(`/api/nodes/${id}/versions`).then(toJson<Version[]>),

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

  libraryIgnores: () => fetch('/api/library/ignores').then(toJson<Ignore[]>),

  addIgnore: (req: { source?: LibrarySource; external_id?: string; group_key?: string; reason?: string }) =>
    fetch('/api/library/ignores', {
      method: 'POST',
      headers: jsonHeaders,
      body: JSON.stringify(req),
    }).then(toJson<{ id: string }>),

  removeIgnore: (id: string) =>
    fetch(`/api/library/ignores/${id}`, { method: 'DELETE' }).then(toJson<{ ok: boolean }>),

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

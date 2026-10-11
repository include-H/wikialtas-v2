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
}

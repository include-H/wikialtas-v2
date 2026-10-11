import type {
  BatchResult,
  CreateDocBody,
  CreateRunBody,
  CreateWorkBody,
  Doc,
  EmbyLibraryView,
  LibraryLink,
  LibraryPoolResponse,
  LibrarySearchEntry,
  Medium,
  Me,
  NodeLibraryResponse,
  NodeSweepRunResponse,
  PatchWorkBody,
  PutContentBody,
  PutContentResult,
  Relation,
  Revision,
  Run,
  RunEvent,
  RuntimeInfo,
  SearchHit,
  Session,
  Settings,
  SettingsPayload,
  Work,
  WorkKind,
  WorkStatus,
  WorkSummary,
} from '../types'

const BASE = ''

export class ApiError extends Error {
  code: string
  status: number

  constructor(status: number, code: string, message: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
    this.code = code
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response
  try {
    res = await fetch(`${BASE}${path}`, {
      headers: {
        'Content-Type': 'application/json',
        ...(init?.headers ?? {}),
      },
      ...init,
    })
  } catch {
    throw new ApiError(0, 'network', '无法连接后端服务，请确认 API 已启动')
  }

  if (res.status === 204) return undefined as T

  const text = await res.text()
  let body: unknown = null
  if (text) {
    try {
      body = JSON.parse(text)
    } catch {
      body = null
    }
  }

  if (!res.ok) {
    const err = body as { error?: { code?: string; message?: string } } | null
    throw new ApiError(
      res.status,
      err?.error?.code ?? 'http_error',
      err?.error?.message ?? `请求失败 (${res.status})`,
    )
  }

  return body as T
}

function qs(params: Record<string, string | number | undefined | null>): string {
  const sp = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v !== null && v !== '') sp.set(k, String(v))
  }
  const s = sp.toString()
  return s ? `?${s}` : ''
}

// --- Tree & Works ---

export function getTree(): Promise<{ nodes: WorkSummary[] }> {
  return request('/api/tree')
}

export function getWork(
  id: string,
): Promise<{ work: Work; libraryLinks: LibraryLink[]; relations: Relation[] }> {
  return request(`/api/works/${id}`)
}

export function createWork(body: CreateWorkBody): Promise<Work> {
  return request('/api/works', { method: 'POST', body: JSON.stringify(body) })
}

export function patchWork(id: string, body: PatchWorkBody): Promise<Work> {
  return request(`/api/works/${id}`, { method: 'PATCH', body: JSON.stringify(body) })
}

export function putWorkContent(id: string, body: PutContentBody): Promise<PutContentResult> {
  return request(`/api/works/${id}/content`, { method: 'PUT', body: JSON.stringify(body) })
}

export function deleteWork(id: string): Promise<void> {
  return request(`/api/works/${id}`, { method: 'DELETE' })
}

export interface ListWorksParams {
  kind?: WorkKind
  medium?: Medium
  status?: WorkStatus
  tag?: string
  hasContent?: boolean
  limit?: number
}

/** GET /api/list-works —— 结构筛选（非全文检索）。status 缺省=活的库（排除 archived）。 */
export function listWorks(params: ListWorksParams): Promise<{ works: WorkSummary[]; count: number }> {
  const q = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v !== null && v !== '') q.set(k, String(v))
  }
  const qs = q.toString()
  return request(`/api/list-works${qs ? `?${qs}` : ''}`)
}

// --- Docs ---

export function getWorkDocs(workId: string): Promise<{ docs: Doc[] }> {
  return request(`/api/works/${workId}/docs`)
}

/** GET /api/docs/:id —— 后端历史上有裸 doc / {doc} 两种形状，这里统一兜住。 */
export async function getDoc(id: string): Promise<{ doc: Doc }> {
  const raw = await request<Doc | { doc: Doc }>(`/api/docs/${id}`)
  if (raw && typeof raw === 'object' && 'doc' in raw && raw.doc) return raw
  return { doc: raw as Doc }
}

export function createDoc(workId: string, body: CreateDocBody): Promise<Doc> {
  return request(`/api/works/${workId}/docs`, { method: 'POST', body: JSON.stringify(body) })
}

export function putDocContent(id: string, body: PutContentBody): Promise<PutContentResult> {
  return request(`/api/docs/${id}/content`, { method: 'PUT', body: JSON.stringify(body) })
}

export function patchDoc(
  id: string,
  body: { title?: string; links?: string[] },
): Promise<Doc> {
  return request(`/api/docs/${id}`, { method: 'PATCH', body: JSON.stringify(body) })
}

export function deleteDoc(id: string): Promise<void> {
  return request(`/api/docs/${id}`, { method: 'DELETE' })
}

// --- 关系（只读：节点页关联面板。写侧走 LLM 的 upsert_relations 工具，图谱界面不做）---

export function getWorkRelations(id: string): Promise<{ relations: Relation[] }> {
  return request(`/api/works/${id}/relations`)
}

// --- Revisions ---

export function getWorkRevisions(
  id: string,
  limit = 20,
): Promise<{ revisions: Revision[] }> {
  return request(`/api/works/${id}/revisions${qs({ limit })}`)
}

export function restoreWorkRevision(id: string, revId: string): Promise<Revision> {
  return request(`/api/works/${id}/revisions/${revId}/restore`, { method: 'POST' })
}

// --- Runs ---

export function createRun(body: CreateRunBody): Promise<{ runId: string } | Run> {
  return request('/api/runs', { method: 'POST', body: JSON.stringify(body) })
}

/** 批量建档：一批 stub 节点各开一个 create_wiki 工单，共享一个批次会话。 */
export function createBatchWiki(body: {
  workIds: string[]
  batchSize?: number
  medium?: string
}): Promise<BatchResult> {
  return request('/api/runs/batch', { method: 'POST', body: JSON.stringify(body) })
}

export function resumeRun(id: string): Promise<{ runId: string } | Run> {
  return request(`/api/runs/${id}/resume`, { method: 'POST' })
}

export function cancelRun(id: string): Promise<void> {
  return request(`/api/runs/${id}/cancel`, { method: 'POST' })
}

export function listRuns(
  status?: string,
  limit = 50,
  workspace?: string,
): Promise<{ runs: Run[] }> {
  return request(`/api/runs${qs({ status, limit, workspace })}`)
}

export function getRun(
  id: string,
  opts?: { afterSeq?: number; limit?: number },
): Promise<{ run: Run; events?: RunEvent[] }> {
  // 默认要 1000 条权威事件：长工单（几十轮工具调用）200 条装不下，
  // 面板重建时前面的章节会看不到。增量事件后端已经默认过滤掉。
  return request(
    `/api/runs/${id}${qs({ afterSeq: opts?.afterSeq, limit: opts?.limit ?? 1000 })}`,
  )
}

// --- Sessions（会话 = 一段连续对话） ---

/** 列出会话。target 省略 = 全部会话（工单页要的就是"所有可继续的对话"）。 */
export function listSessions(target?: string): Promise<{ sessions: Session[] }> {
  return request(`/api/sessions${qs({ target })}`)
}

export function createSession(body: { target: string; title?: string }): Promise<{ session: Session }> {
  return request('/api/sessions', { method: 'POST', body: JSON.stringify(body) })
}

export function renameSession(id: string, title: string): Promise<{ ok: boolean }> {
  return request(`/api/sessions/${id}`, { method: 'PATCH', body: JSON.stringify({ title }) })
}

export function deleteSession(id: string): Promise<{ ok: boolean }> {
  return request(`/api/sessions/${id}`, { method: 'DELETE' })
}

// --- Library · 媒体库（GameAtlas / Emby 通用）---

export type LibraryApiSource = 'gameatlas' | 'emby' | 'komga'

/** 一键建档：建 stub 子节点并挂上该库的外链。 */
export function archiveLibrary(
  source: LibraryApiSource,
  workId: string,
  publicId: string,
): Promise<{ work: Work; link: LibraryLink }> {
  return request(`/api/library/${source}/archive`, {
    method: 'POST',
    body: JSON.stringify({ workId, publicId }),
  })
}

/** 关联既有条目（Emby 允许多条、GameAtlas 一条）。 */
/**
 * 批量「建为待写」：落位规则在后端（`domain.Placer`，有单测）——
 * 命中单作就关联、命中系列就在其下建子节点、都没有就"建系列+建单作+关联"；
 * parentId 省略 = 走规则；`""` = 「不归属」（顶级）；seriesTitle = 用这个标题当系列。
 */
export function archivePlace(body: {
  items: { source: string; externalId: string }[]
  parentId?: string
  seriesTitle?: string
}): Promise<{
  created: number
  linked: number
  failed: { externalId: string; title: string; error: string }[]
}> {
  return request('/api/library/archive-place', { method: 'POST', body: JSON.stringify(body) })
}

export function linkLibrary(
  source: LibraryApiSource,
  workId: string,
  publicId: string,
): Promise<{ link: LibraryLink }> {
  return request(`/api/library/${source}/link`, {
    method: 'POST',
    body: JSON.stringify({ workId, publicId }),
  })
}

/** 统一挂链入口：origin 落到链的 linked_by（sweep=节点扫库建议 / pool=AI 对一遍 / manual=手动）。 */
export function linkLibraryUnified(
  body: { workId: string; source: LibraryApiSource; publicId: string; origin?: 'sweep' | 'pool' | 'manual' },
): Promise<{ link: LibraryLink }> {
  return request('/api/library/link', {
    method: 'POST',
    body: JSON.stringify(body),
  })
}

/** 改判一条链的角色（正片级 ↔ 混杂级）。 */
export function setLinkRole(id: string, role: 'work' | 'mix'): Promise<{ ok: boolean; link: unknown }> {
  return request('/api/library/link', {
    method: 'PATCH',
    body: JSON.stringify({ id, role }),
  })
}

/** 全库搜索（关联弹窗用）。 */
export function searchLibrary(
  source: LibraryApiSource,
  q: string,
): Promise<{ entries: LibrarySearchEntry[] }> {
  return request(`/api/library/${source}/search?q=${encodeURIComponent(q)}`)
}

/** 反哺：把节点正文（可选带简介）写回 GameAtlas 条目。 */
export function pushGameAtlasWiki(
  workId: string,
  summary?: string,
): Promise<{ ok: boolean; gameUrl?: string; summary?: string }> {
  return request('/api/library/gameatlas/push', {
    method: 'POST',
    body: JSON.stringify(summary != null ? { workId, summary } : { workId }),
  })
}

/** 解除 GameAtlas 关联（一条，按 workId）。 */
export function unlinkGameAtlas(workId: string): Promise<{ ok: boolean }> {
  return request(`/api/library/gameatlas/link?workId=${encodeURIComponent(workId)}`, {
    method: 'DELETE',
  })
}

/** 解除 Emby 关联（多条，按链接 id）。 */
export function unlinkEmbyLink(linkId: string): Promise<{ ok: boolean }> {
  return request(`/api/library/emby/link?id=${encodeURIComponent(linkId)}`, {
    method: 'DELETE',
  })
}

/** 解除 Komga 关联（多条，按链接 id）。 */
export function unlinkKomgaLink(linkId: string): Promise<{ ok: boolean }> {
  return request(`/api/library/komga/link?id=${encodeURIComponent(linkId)}`, {
    method: 'DELETE',
  })
}

/** 反哺到 Komga：把简介 PATCH 到节点所有 Komga 链（系列/单册）的元数据。 */
export function pushKomgaSummary(
  workId: string,
  summary?: string,
): Promise<{ ok: boolean; count?: number; summary?: string }> {
  return request('/api/library/komga/push', {
    method: 'POST',
    body: JSON.stringify(summary != null ? { workId, summary } : { workId }),
  })
}

/** 设置页：拉 Emby 媒体库清单（给每个库挑角色用）。 */
export function getEmbyViews(): Promise<{ views: EmbyLibraryView[] }> {
  return request('/api/library/emby/views')
}

/** 建议池：读后台扫描清单（秒开，不实时轰库）。 */
export function getLibraryPool(): Promise<LibraryPoolResponse> {
  return request('/api/library/pool')
}

/** 扫描媒体库（后台低频扫描的手动触发）：把三源清单落成快照。 */
export function scanLibrary(): Promise<{
  ok: boolean
  scannedAt: string
  entries: number
  linked: number
  unlinked: number
}> {
  return request('/api/library/scan', { method: 'POST' })
}

/** 忽略：单条（entryId）或整组（containerKey），持久化在清单里。 */
export function ignoreLibrary(body: {
  source: string
  entryId?: string
  containerKey?: string
  containerTitle?: string
}): Promise<{ ok: boolean }> {
  return request('/api/library/ignore', { method: 'POST', body: JSON.stringify(body) })
}

/** 恢复忽略。 */
export function unignoreLibrary(q: {
  source: string
  entryId?: string
  containerKey?: string
}): Promise<{ ok: boolean }> {
  const params = new URLSearchParams({ source: q.source })
  if (q.entryId) params.set('entryId', q.entryId)
  if (q.containerKey) params.set('containerKey', q.containerKey)
  return request(`/api/library/ignore?${params.toString()}`, { method: 'DELETE' })
}




/** 节点页「关联与媒体」统一数据面：正片级链 / 混杂链 / 待确认 / 已忽略 / 源状态。 */
export function getNodeLibrary(workId: string): Promise<NodeLibraryResponse> {
  return request(`/api/library/node/${encodeURIComponent(workId)}`)
}

/** 节点扫库：立即扫一遍（题名变体 → 三源候选 → 判官），产出待确认的建议。 */
export function runNodeSweep(workId: string, term?: string): Promise<NodeSweepRunResponse> {
  return request('/api/library/node-sweep', {
    method: 'POST',
    body: JSON.stringify(term ? { workId, term } : { workId }),
  })
}

/** 逐条忽略一条建议（per-node：只是不再向这个节点提起，不等于全局忽略）。 */
export function dismissNodeSweep(workId: string, key: string, reason?: string): Promise<{ ok: boolean }> {
  return request('/api/library/node-sweep/dismiss', {
    method: 'POST',
    body: JSON.stringify({ workId, key, reason: reason?.trim() ? reason : undefined }),
  })
}

/** 恢复被忽略的一条建议。 */
export function restoreNodeSweep(workId: string, key: string): Promise<{ ok: boolean }> {
  const params = new URLSearchParams({ workId, key })
  return request(`/api/library/node-sweep/dismiss?${params.toString()}`, { method: 'DELETE' })
}

// --- Search ---

export function search(q: string, kind?: 'work' | 'doc'): Promise<{ hits: SearchHit[] }> {
  return request(`/api/search${qs({ q, kind })}`)
}

// --- Settings ---

export function getSettings(): Promise<Settings> {
  return request('/api/settings')
}

export function putSettings(body: SettingsPayload): Promise<Settings> {
  return request('/api/settings', { method: 'PUT', body: JSON.stringify(body) })
}

/** 用当前配置发一次极小请求，验证模型连通性。 */
export function testLLM(): Promise<{
  ok: boolean
  model?: string
  latencyMs?: number
  reply?: string
  error?: string
}> {
  return request('/api/settings/test-llm', { method: 'POST' })
}

export function getRuntime(): Promise<RuntimeInfo> {
  return request('/api/runtime')
}

// --- 简易用户系统 ---

export function getMe(): Promise<Me> {
  return request('/api/auth/me')
}

/** 访问密码登录（用户名可省略，默认管理员）。 */
export function login(password: string, username?: string): Promise<{ ok: boolean; username: string }> {
  return request('/api/auth/login', {
    method: 'POST',
    body: JSON.stringify(username ? { username, password } : { password }),
  })
}

export function logout(): Promise<{ ok: boolean }> {
  return request('/api/auth/logout', { method: 'POST' })
}

// --- SSE（实现见 lib/sse.ts，这里保持统一入口）---
export { streamRunEvents } from './sse'
export type { SseHandle } from './sse'

// 工具名 → 人话：族 / 措辞 / 图标类别 / 一行摘要。纯数据 + 纯函数，**不含 JSX**。
//
// 图标是渲染层的事，这里只回答"用哪一类图标"（iconKind），由渲染器映射成具体组件。
// 名字清单的 owner 是后端 `internal/tools/librarian.go` 的 ToolSchemas——这里漏一个，
// 该工具就掉进「已执行 xxx」的兜底措辞（难看但安全，不会崩）。
//
// 为什么单独成文件：ToolCard 以前把"族判定 + 措辞 + 图标 + 摘要 + 渲染"揉在一个
// 组件里；折叠条要按族统计（「已读取 2 处 · 已检索 3 次」），统计与措辞必须在
// 渲染之外可复用，否则折叠条只能再抄一遍。

import type { FunctionCallItem } from './runItems'

export type ToolFamily = 'read' | 'search' | 'fetch' | 'write' | 'plan' | 'say' | 'other'
export type ToolIconKind = 'read' | 'search' | 'write' | 'plan' | 'tool'

export interface ToolMeta {
  family: ToolFamily
  /** 芯片上的动宾短语（「已读取…」），语气对齐飞书。 */
  label: string
  icon: ToolIconKind
}

const TOOLS: Record<string, ToolMeta> = {
  // 读
  read_skill: { family: 'read', label: '已读取 wiki-writing skill', icon: 'read' },
  read_work: { family: 'read', label: '已读取作品正文', icon: 'read' },
  read_doc: { family: 'read', label: '已读取资料', icon: 'read' },
  read_session: { family: 'read', label: '已读取早先会话', icon: 'read' },
  get_tree: { family: 'read', label: '已读取作品树', icon: 'read' },
  list_works: { family: 'read', label: '已列出作品', icon: 'read' },
  list_library: { family: 'read', label: '已列出媒体库条目', icon: 'read' },
  audit: { family: 'read', label: '已审计库状态', icon: 'read' },
  // 检索
  search_works: { family: 'search', label: '已检索站内条目', icon: 'search' },
  search_sessions: { family: 'search', label: '已检索跨会话记录', icon: 'search' },
  search_web: { family: 'search', label: '已联网检索', icon: 'search' },
  search_library: { family: 'search', label: '已检索媒体库', icon: 'search' },
  // 抓取
  fetch_url: { family: 'fetch', label: '已抓取网页', icon: 'search' },
  // 写
  write_content: { family: 'write', label: '已写入正文', icon: 'write' },
  patch_section: { family: 'write', label: '已改写章节', icon: 'write' },
  patch_sections: { family: 'write', label: '已批量改写章节', icon: 'write' },
  edit: { family: 'write', label: '已订正文字', icon: 'write' },
  create_doc: { family: 'write', label: '已创建资料', icon: 'write' },
  delete_doc: { family: 'write', label: '已删除资料', icon: 'write' },
  upsert_work: { family: 'write', label: '已更新作品节点', icon: 'write' },
  delete_work: { family: 'write', label: '已删除作品节点', icon: 'write' },
  upsert_relation: { family: 'write', label: '已建立作品关系', icon: 'write' },
  upsert_relations: { family: 'write', label: '已批量建立作品关系', icon: 'write' },
  delete_relation: { family: 'write', label: '已解除作品关系', icon: 'write' },
  attach_library_link: { family: 'write', label: '已挂接媒体库链接', icon: 'write' },
  // 计划 / 说话
  todo_write: { family: 'plan', label: '已更新任务清单', icon: 'plan' },
  narrative: { family: 'say', label: '已说明进度', icon: 'tool' },
  answer: { family: 'say', label: '已作答', icon: 'tool' },
}

const FALLBACK: ToolMeta = { family: 'other', label: '', icon: 'tool' }

export function toolMeta(name?: string): ToolMeta {
  return (name && TOOLS[name]) || FALLBACK
}

/** 工具项在界面上的两个状态位（措辞与摘要都要看它）。 */
export function toolStateOf(item: FunctionCallItem): { running: boolean; failed: boolean } {
  const extra = item.wikiatlas ?? {}
  return {
    running: item.status === 'in_progress',
    failed: item.status === 'failed' || extra.ok === false,
  }
}

/** 芯片措辞：成功的用人话短语，失败的显式说"未执行"。 */
export function labelOf(item: FunctionCallItem): string {
  const name = item.name || 'tool'
  const meta = toolMeta(name)
  if (meta.label) return meta.label
  return toolStateOf(item).failed ? `未执行 ${name}` : `已执行 ${name}`
}

/** 折叠行摘要的字数上限。见 detailOf：这不是排版偏好，是防撑宽。 */
export const ROW_DETAIL_MAX = 42

function cap(s: string): string {
  const flat = s.replace(/\s+/g, ' ').trim()
  return flat.length > ROW_DETAIL_MAX ? `${flat.slice(0, ROW_DETAIL_MAX)}…` : flat
}

/**
 * 参数 → 一行摘要。参数是模型写的 JSON，可能半截、可能很长，都要能读。
 */
export function summarizeArgs(name?: string, raw?: string): string {
  const flat = (raw ?? '')
    .replace(/\s+/g, ' ')
    .replace(/"(contentMd|newMarkdown|previewMd|content)"\s*:\s*"(?:[^"\\]|\\.)*"\s*,?/g, '')
    .replace(/[{}]/g, '')
    .replace(/"([^"]*)"\s*:\s*/g, '$1: ')
    .replace(/"([^"]*)"/g, '$1')
    .replace(/\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b/gi, '…')
    .trim()
  if (!flat) return ''
  const cut = (s: string, n: number) => (s.length > n ? `${s.slice(0, n)}…` : s)
  if (name === 'search_web' || name === 'search_works') {
    const q = /q:\s*(.+?)(?:·|参考|count=|$)/.exec(flat)?.[1]?.trim()
    if (q) return `「${cut(q, 20)}」`
  }
  if (name === 'read_work' || name === 'read_doc') {
    const id = /(?:workId|docId|id):\s*(\S+)/.exec(flat)?.[1]
    if (id && id !== '…') return id
  }
  return cut(flat, 40)
}

/**
 * 单行摘要：结果摘要优先（它说的是"做成了什么"，参数说的是"打算做什么"）。
 *
 * **在 JS 里截断**，不只是靠 CSS 省略号：这一行是 nowrap，元素的 min-content
 * 就是整串文字宽——完整错误信息（几十个汉字）会把整条消息行顶宽，一路撑到
 * 对话列表出横向滚动条。截断后 min-content 天然受控，跟 CSS 补不补无关。
 */
export function detailOf(item: FunctionCallItem): string {
  const extra = item.wikiatlas ?? {}
  const { running, failed } = toolStateOf(item)
  const raw = extra.outputSummary || (failed && !running ? '失败' : summarizeArgs(item.name, item.arguments))
  return cap(raw)
}

/** 摘要全文（展开区用），没被截断时返回空串以免重复展示。 */
export function fullDetailOf(item: FunctionCallItem): string {
  const flat = (item.wikiatlas?.outputSummary ?? '').replace(/\s+/g, ' ').trim()
  return flat.length > ROW_DETAIL_MAX ? flat : ''
}

/** 展开区有没有内容可看（决定折叠条/工具行是否画那个 caret）。 */
export function hasDetail(item: FunctionCallItem): boolean {
  return !!item.wikiatlas?.artifact?.lines?.length || fullDetailOf(item).length > 0
}

/** 一组工具的改动行数合计（write_content / patch_section / edit 才带）。 */
export function diffOf(items: FunctionCallItem[]): { additions: number; deletions: number } {
  let additions = 0
  let deletions = 0
  for (const it of items) {
    additions += it.wikiatlas?.additions ?? 0
    deletions += it.wikiatlas?.deletions ?? 0
  }
  return { additions, deletions }
}

/** 一组工具的总耗时（只统计到秒级的，短调用不显示——噪声）。 */
export function durationOf(items: FunctionCallItem[]): number {
  return items.reduce((n, it) => n + (it.wikiatlas?.durationMs ?? 0), 0)
}

const FAMILY_ORDER: ToolFamily[] = ['read', 'search', 'fetch', 'write', 'plan', 'say', 'other']

const FAMILY_PHRASE: Record<ToolFamily, (n: number) => string> = {
  read: (n) => `已读取 ${n} 处`,
  search: (n) => `已检索 ${n} 次`,
  fetch: (n) => `已抓取 ${n} 篇`,
  write: (n) => `已写入 ${n} 次`,
  plan: () => '已更新任务清单',
  say: () => '已作答',
  other: (n) => `已执行 ${n} 步`,
}

/**
 * 一组工具调用的折叠条标题：单个给具体措辞（「已读取 wiki-writing skill」），
 * 多个按族计数（「已读取 2 处 · 已检索 3 次」）。族序固定，避免同组两次渲染
 * 顺序不同造成跳动。
 */
export function summarizeGroup(items: FunctionCallItem[]): string {
  if (items.length === 0) return ''
  if (items.length === 1) return labelOf(items[0])
  const counts = new Map<ToolFamily, number>()
  for (const it of items) {
    const f = toolMeta(it.name).family
    counts.set(f, (counts.get(f) ?? 0) + 1)
  }
  return FAMILY_ORDER.filter((f) => counts.has(f))
    .map((f) => FAMILY_PHRASE[f](counts.get(f) as number))
    .join(' · ')
}

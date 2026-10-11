// 派生层：把「归约出来的助手消息」+「wikiatlas.* 事件」变成界面直接能画的东西。
//
// 纯函数、无 React、无 DOM。归约层（reduceRun）只负责把 response.* 折成 message，
// 渲染层（components/run/conversation）只负责画；**"该画成什么"全部在这里决定**。
// 以前这件事散在 AiPanel / RunList / ToolCard 三处各写一份，改一处漏两处——现在
// 只有这一个 owner。

import type { RunEvent, RunTaskInfo } from '../types'
import { dedupeEvents, runToMessage, type DialogueMessage, type RunShape } from './responses'
import {
  isFunctionCall,
  type ContentItemLike,
  type FunctionCallItem,
  type ToolGroupItem,
} from './runItems'

/** 用户自己发的那句：会话视图里以 user 角色渲染。 */
export function userMessage(id: string, text: string): DialogueMessage {
  return { id, role: 'user', status: 'completed', content: text }
}

/**
 * 把 content[] 里**连续**的 `function_call` 项折成一条折叠条
 * （`wikiatlas.toolgroup`，Semi 按 type 找自定义渲染器）。
 *
 * 为什么在这层做、而不是在渲染器里：折叠条要按族统计（「已读取 2 处 · 已检索 3 次」），
 * 那需要看到**一组**；单个渲染器只看得到自己那一项。中间夹了叙事句/思考就不合并——
 * 顺流时间线的顺序不能被重排（VISUAL_SPEC §4）。
 *
 * 幂等：折叠后数组里不再有裸 function_call，再跑一遍是恒等。
 */
export function groupToolCalls(content: ContentItemLike[]): ContentItemLike[] {
  const out: ContentItemLike[] = []
  let run: FunctionCallItem[] = []
  const flush = () => {
    if (!run.length) return
    const group: ToolGroupItem = {
      type: 'wikiatlas.toolgroup',
      // 取组内首项的 id 当组 id：插入新项时组身份不变，React 不整体重挂。
      id: run[0].id ?? `toolgroup-${out.length}`,
      items: run,
    }
    out.push(group)
    run = []
  }
  for (const item of content) {
    if (isFunctionCall(item)) {
      run.push(item)
      continue
    }
    flush()
    out.push(item)
  }
  flush()
  return out
}

/**
 * run 边界行（head/tail 的 wikiatlas.notice）贴进对话流两端。
 * 对齐 dsh 的 turn/start · turn/end：生命周期事实是事件，渲染在内容的前后两端。
 */
function boundaryTurns(events: RunEvent[]): { head: ContentItemLike[]; tail: ContentItemLike[] } {
  const { head, tail } = splitBoundaryNotices(events)
  const turn = (phase: 'start' | 'end', text: string, tone: string): ContentItemLike => ({
    type: 'wikiatlas.turn',
    phase,
    text,
    tone,
  })
  return {
    head: head.map((n) => turn('start', n.text, n.tone)),
    tail: tail.map((n) => turn('end', n.text, n.tone)),
  }
}

/**
 * 助手消息 → 界面要画的最终形状：折叠工具组 + 两端边界行 + run 状态（以 run 为准）。
 *
 * 增量路径（当前单，每帧）与全量路径（历史冻结、工单页）都走这里，只有"消息从哪来"
 * 不同——所以流式中与刷新后看到的是同一个形状，不会出现"流式一套、回放另一套"。
 */
export function decorateAssistant(
  msg: DialogueMessage,
  events: RunEvent[],
  run: RunShape,
): DialogueMessage {
  const evs = dedupeEvents(events)
  const { head, tail } = boundaryTurns(evs)
  const items = groupToolCalls(Array.isArray(msg.content) ? (msg.content as ContentItemLike[]) : [])
  return {
    ...msg,
    id: `a-${run.id}`,
    status: runMessageStatus(run),
    content: [...head, ...items, ...tail],
  }
}

/**
 * 一个 run → 这一轮的对话消息（用户那句 + 助手那句）。**全量、一次性**：
 * 只用于历史加载与"发新单时冻结当前单"，实时流走 reduceRun 的增量路径。
 */
export function runMessages(run: RunShape, events: RunEvent[]): DialogueMessage[] {
  const out: DialogueMessage[] = [userMessage(`q-${run.id}`, run.goal)]
  const evs = dedupeEvents(events)
  const msg = runToMessage(evs)
  if (msg) out.push(decorateAssistant(msg, evs, run))
  return out
}

/**
 * 把 run 的 `wikiatlas.notice` 按**位置**分三段：
 *   · head —— 第一段内容之前：运行前的准备事实（如"已加载 skill…"）；
 *   · tail —— 最后一段内容之后：收尾结论（质量自检、失败原因）；
 *   · mid  —— 夹在中间的：过程性提醒（预算、护栏），仍归浮层 dock。
 * 没有内容项（起步即失败）时全部算 tail。
 */
export function splitBoundaryNotices(events: RunEvent[]): {
  head: Notice[]
  tail: Notice[]
  mid: Notice[]
} {
  let firstItem = -1
  let lastItem = -1
  for (const ev of events) {
    if (ev.type === 'response.output_item.added' && firstItem < 0) firstItem = ev.seq
    if (ev.type === 'response.output_item.done') lastItem = Math.max(lastItem, ev.seq)
  }
  const head: Notice[] = []
  const tail: Notice[] = []
  const mid: Notice[] = []
  for (const ev of events) {
    if (ev.type !== 'wikiatlas.notice') continue
    const text = typeof ev.payload?.text === 'string' ? ev.payload.text.trim() : ''
    if (!text) continue
    const tone = ev.payload?.tone
    const n: Notice = {
      id: ev.id,
      text,
      tone: tone === 'warn' || tone === 'error' ? tone : 'info',
      at: ev.createdAt,
    }
    if (firstItem < 0 || ev.seq > lastItem) tail.push(n)
    else if (ev.seq < firstItem) head.push(n)
    else mid.push(n)
  }
  return { head, tail, mid }
}

export interface Notice {
  id: string
  text: string
  tone: 'info' | 'warn' | 'error'
  at: string
}

/**
 * 取**当前**的任务清单：最后一次 wikiatlas.todo 的全量列表。
 *
 * 用途是输入框上方那个任务面板——它是"现在该做什么"的实时视图。
 */
export function latestTasks(events: RunEvent[]): RunTaskInfo[] {
  let out: RunTaskInfo[] = []
  for (const ev of events) {
    if (ev.type !== 'wikiatlas.todo') continue
    const raw = ev.payload?.tasks as RunTaskInfo[] | undefined
    if (Array.isArray(raw) && raw.length) out = raw
  }
  return out
}

export interface UsageInfo {
  steps: number
  prompt: number
  completion: number
  total: number
  cacheRead: number
  cacheWrite: number
  /** 最后一轮的 promptTokens = 此刻窗口里装了多少（不是累计） */
  contextUsed: number
  /** 该网关到底报不报缓存用量：不报就是不可观测，不要假装命中率为 0 */
  cacheReported: boolean
  /** 见过非零命中之后又掉到 0 = 前缀被改写了（护栏信号） */
  cacheDropped: boolean
}

const asNum = (v: unknown): number => (typeof v === 'number' && Number.isFinite(v) ? v : 0)

/** 汇总 run 的用量（wikiatlas.usage，每轮一条）。 */
export function usageOf(events: RunEvent[]): UsageInfo {
  const u: UsageInfo = {
    steps: 0, prompt: 0, completion: 0, total: 0,
    cacheRead: 0, cacheWrite: 0, contextUsed: 0,
    cacheReported: false, cacheDropped: false,
  }
  // 网关会间歇性地某一轮不报缓存（实测：step3 报 0、step4 又回到 12544，
  // 而服务端追加式不变量没报改写）——所以连续**两轮**为 0 才算"掉到 0"，
  // 单轮闪断不再吓用户。
  let zeroStreak = 0
  for (const ev of events) {
    if (ev.type !== 'wikiatlas.usage') continue
    const p = ev.payload ?? {}
    const read = asNum(p.cacheReadTokens)
    u.steps += 1
    u.prompt += asNum(p.promptTokens)
    u.completion += asNum(p.completionTokens)
    u.total += asNum(p.totalTokens)
    u.cacheRead += read
    u.cacheWrite += asNum(p.cacheWriteTokens)
    u.contextUsed = asNum(p.promptTokens) || u.contextUsed
    if (read > 0) {
      u.cacheReported = true
      zeroStreak = 0
    } else if (u.cacheReported && asNum(p.step) > 1) {
      zeroStreak += 1
      if (zeroStreak >= 2) u.cacheDropped = true
    }
  }
  return u
}

/**
 * run 状态 → Semi 消息状态（归约器只认 completed，失败/中断由 run 说了算）。
 *
 * Semi 的 DialogueContent 对 failed/cancelled 会画那个红色感叹号，所以这里的
 * 关键是分清"真死了"和"被清走了"：
 *
 * 后端保留策略会把完成久的工单从 completed/failed **改写**成 `expired`（回收
 * tool_cache 占位）。那种 expired 已经有终局（result 或 error + completed_at），
 * 按 cancelled 渲染会给**正常完成**的工单挂上红色感叹号——"质量自检通过，已
 * 标记为 ready"却亮红感叹号，就是这个原因。
 */
export function runMessageStatus(run: {
  status: string
  completedAt?: string | null
  result?: Record<string, unknown> | null
  error?: Record<string, unknown> | null
}): string {
  switch (run.status) {
    case 'completed':
      return 'completed'
    case 'failed':
      return 'failed'
    case 'interrupted':
      return 'cancelled'
    case 'expired':
      // 有 completed_at = 当时正常收尾、只是现在被保留策略清走 → 还原终局
      if (run.completedAt) return run.result ? 'completed' : 'failed'
      // 没有 completed_at = 执行中途死了被超时清走 → cancelled
      return 'cancelled'
    default:
      return 'in_progress'
  }
}

// ── run 的状态行（时间线的头/尾） ─────────────────────────────────────────────

export interface RunStatusInfo {
  /** running | completed | failed | interrupted | expired */
  status: string
  /** 起始时刻（epoch ms）；来自 wikiatlas.meta.startedAt。进行中用它现算时长。 */
  startedAt?: number
  /** 总时长（终态才有）；来自 wikiatlas.run.end.durationMs。 */
  durationMs?: number
  reason?: string
}

/**
 * run 的状态行数据。终态以 `wikiatlas.run.end` 为准——它是流里的终态事实，
 * 不必再回头查 GET /api/runs/{id}（这正是加这个事件的原因）；查不到时退回
 * run.status，保证老工单（事件里没有 run.end）也画得出来。
 */
export function runStatusOf(
  events: RunEvent[],
  run?: { status?: string; startedAt?: string | null } | null,
): RunStatusInfo {
  let startedAt: number | undefined
  let end: RunStatusInfo | undefined
  for (const ev of events) {
    if (ev.type === 'wikiatlas.meta') {
      const t = parseTime(ev.payload?.startedAt)
      if (t != null) startedAt = t
    }
    if (ev.type === 'wikiatlas.run.end') {
      const p = ev.payload ?? {}
      end = {
        status: typeof p.status === 'string' ? p.status : 'completed',
        durationMs: typeof p.durationMs === 'number' ? p.durationMs : undefined,
        reason: typeof p.reason === 'string' ? p.reason : undefined,
      }
    }
  }
  const fromRun = parseTime(run?.startedAt)
  return {
    status: end?.status ?? run?.status ?? 'running',
    startedAt: startedAt ?? fromRun,
    durationMs: end?.durationMs,
    reason: end?.reason,
  }
}

function parseTime(v: unknown): number | undefined {
  if (typeof v === 'number' && Number.isFinite(v)) return v
  if (typeof v !== 'string' || !v) return undefined
  const t = Date.parse(v)
  return Number.isFinite(t) ? t : undefined
}

/** 时长 → 人能读的一行（「4m 12s」/「12s」/「1h 03m」）。 */
export function formatDuration(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000))
  if (total < 60) return `${total}s`
  const m = Math.floor(total / 60)
  const s = total % 60
  if (m < 60) return `${m}m ${String(s).padStart(2, '0')}s`
  const h = Math.floor(m / 60)
  return `${h}h ${String(m % 60).padStart(2, '0')}m`
}

export type { DialogueMessage, RunShape }

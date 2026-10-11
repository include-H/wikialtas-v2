import { streamingResponseToMessage } from '@douyinfe/semi-ui'
import type { RunEvent, RunEventType } from '../types'

// ── Responses 协议层 ────────────────────────────────────────────────────────
//
// 这一层只做**协议**，不做投影、也不做派生（派生在 `runView.ts`）：
//
//   1. 声明事件清单（前端的订阅依据，owner 是后端 `internal/run/responses.go`）；
//   2. 把事件流还原成规范能吃的流块，交给 Semi 原生的归约器
//      （`streamingResponseToMessage`）——协议怎么拼、乱序怎么缓冲、缺块怎么容错，
//      都不归我们管；
//   3. 事件列表本身的两个工具（按 seq 去重 / 并入）。
//
// 分工：`responses.ts`（协议）→ `reduceRun.ts`（归约）→ `runView.ts`（派生）→
// `components/run/conversation`（渲染）。谁的事谁管，别互相抄。

/** Responses 规范里的流事件（前端的订阅清单；owner 在后端 responses.go）。 */
export const RESPONSE_EVENT_TYPES = [
  'response.created',
  'response.in_progress',
  'response.output_item.added',
  'response.output_item.done',
  'response.output_text.delta',
  'response.output_text.done',
  'response.reasoning_summary_text.delta',
  'response.reasoning_summary_text.done',
  'response.function_call_arguments.delta',
  'response.function_call_arguments.done',
  'response.completed',
  'response.failed',
] as const satisfies readonly RunEventType[]

/** 规范里没有、产品需要的那部分（独立命名空间 = "留后手"的位置）。 */
export const WIKIATLAS_EVENT_TYPES = [
  'wikiatlas.todo',
  'wikiatlas.usage',
  'wikiatlas.notice',
  'wikiatlas.meta',
  'wikiatlas.request',
  'wikiatlas.context',
  'wikiatlas.tree',
  'wikiatlas.content.staging',
  'wikiatlas.content.committed',
  'wikiatlas.content.delta',
  'wikiatlas.run.end',
] as const satisfies readonly RunEventType[]

/** EventSource 只派发**显式订阅过**的命名事件：这份清单漏一个，那一类就静默收不到。 */
export const RUN_EVENT_TYPES: readonly RunEventType[] = [...RESPONSE_EVENT_TYPES, ...WIKIATLAS_EVENT_TYPES]

/** Responses 的流块。字段与规范同名（output_index / content_index / delta / item …）。 */
export interface ResponseChunk {
  type: string
  sequence_number?: number
  response?: Record<string, unknown>
  item?: Record<string, unknown>
  output_index?: number
  content_index?: number
  summary_index?: number
  delta?: string
  text?: string
  name?: string
  arguments?: string
  [k: string]: unknown
}

/** 归约出来的助手消息（Semi 的 Message）：我们只读这几个字段，其余原样交给 Dialogue。 */
export interface DialogueMessage {
  id: string
  role: string
  content?: unknown
  output_text?: string
  status?: string
  createdAt?: number
  model?: string
  [k: string]: unknown
}

/** 对话线推导需要的 run 最小面（Run 的超集，历史冻结与当前 run 都用它）。 */
export interface RunShape {
  id: string
  goal: string
  status: string
  completedAt?: string | null
  result?: Record<string, unknown> | null
  error?: Record<string, unknown> | null
}

export function isResponseEvent(type: string): boolean {
  return type.startsWith('response.')
}

/** payload 就是规范里的那个事件对象；缺字段的老数据用 seq 兜底。 */
function chunkOf(ev: RunEvent): ResponseChunk {
  const p = (ev.payload ?? {}) as ResponseChunk
  const seq = typeof p.sequence_number === 'number' ? p.sequence_number : ev.seq - 1
  return { ...p, type: ev.type, sequence_number: seq }
}

/**
 * 事件流 → 归约器能吃的流块序列。
 *
 * **为什么要重编号**：历史回放故意丢掉 `*.delta`（约定的流式增量，见后端
 * store/runs.go：不丢的话一个长工单的回放会被半句话碎片占满窗口），于是
 * sequence_number 会出现空洞，而归约器只在**连续**序号上推进。
 *
 * 重编号不丢信息，因为每个输出项都是自足的——`response.output_item.done`
 * 带的是最终形态（完整文本 / 完整参数 / 工具结果），`response.completed`
 * 更是带整份 response。丢掉增量只丢"打字过程"。
 */
export function responsesChunks(events: RunEvent[]): ResponseChunk[] {
  const chunks = events
    .filter((ev) => isResponseEvent(ev.type))
    .map(chunkOf)
    .sort((a, b) => (a.sequence_number ?? 0) - (b.sequence_number ?? 0))
  return chunks.map((c, i) => ({ ...c, sequence_number: i }))
}

/**
 * 把归约器吐出的 message 规范化成我们渲染要的形状：抹掉 `output_text`。
 *
 * 为什么删（Semi 的一个坑，不是我们的数据问题）：
 *   ① 归约器把 content 里的文本拼成 message.output_text（streamingResponseToMessage 尾部）；
 *   ② 渲染器（dialogueContent）只要看到 output_text 非空，就**只渲染整段文本**、把
 *      content 数组整个跳过——而 content[] 里恰好装着思考块 / 工具卡 / 正文三项。
 * 结果：一条助手消息只要有正文，同一消息里的思考块和工具调用就全不渲染（文档却写着
 * 要靠 ContentItem[] 展示）。抹掉这个便利字段后渲染器走逐项分支，三样都在。
 * `renderDialogueContentItem.default` 救不了它（只拿得到文本字符串、拿不到 content[]）。
 * 类型上也站得住：原始 Responses 对象里根本没有 output_text，那是官方 SDK 在客户端加的。
 */
export function normalizeAssistant(raw: unknown): DialogueMessage | null {
  if (!raw) return null
  const m = raw as DialogueMessage
  delete m.output_text
  return m
}

/**
 * 一个 run 的助手消息 = 把它的 Responses 事件喂给 Semi 的归约器。
 *
 * 全量归约（不是增量）：仅用于**低频一次性**场景——历史加载、发新单时冻结当前单。
 * 实时流（每帧）走 `reduceRun.useReducedAssistant` 的增量归约，别在这里逐帧调。
 */
export function runToMessage(events: RunEvent[]): DialogueMessage | null {
  const chunks = responsesChunks(events)
  if (!chunks.length) return null
  const out = streamingResponseToMessage(chunks)
  return normalizeAssistant(out?.message)
}

/** SSE 重连时可能收到同一 seq 的重复事件，按 seq 去重。 */
export function dedupeEvents(events: RunEvent[]): RunEvent[] {
  const seen = new Set<number>()
  const out: RunEvent[] = []
  for (const ev of events) {
    if (seen.has(ev.seq)) continue
    seen.add(ev.seq)
    out.push(ev)
  }
  return out
}

/**
 * 把一条事件并进列表，按 **seq** 去重。
 *
 * 不能按 id 去重：同一条事件从 `getRun` 拿到的是库里的 uuid、从 SSE 拿到的是
 * `${runId}:${seq}`，两个 id 对不上。seq 是落库序号，两边一致，也是唯一稳的键。
 */
export function mergeEvent(list: RunEvent[], ev: RunEvent): RunEvent[] {
  if (list.some((e) => e.seq === ev.seq)) return list
  return [...list, ev]
}

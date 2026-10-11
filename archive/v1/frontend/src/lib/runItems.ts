// 对话内容项的形状：Responses 的三种输出项 + 我们自己合成的两种宿主项。
//
// 类型的 owner 在后端 `internal/run/responses.go`（message / reasoning /
// function_call 是规范形状），`wikiatlas.turn` 与 `wikiatlas.toolgroup` 是前端
// 派生层自己合成的（Semi 对内容项先查自定义渲染器，所以合成类型是官方机制）。
//
// 这一层零依赖、无逻辑，只把"线上长什么样"写下来，给派生层（runView）与渲染层
// （components/run/conversation）共用——两边各抄一份形状是漂移的开始。

export interface ToolArtifact {
  kind?: string
  lines?: string[]
  truncated?: boolean
  totalLines?: number
}

/** 挂在 function_call 项上的宿主附加信息（后端 itemExtraKey = "wikiatlas"）。 */
export interface ToolExtra {
  outputSummary?: string
  ok?: boolean
  durationMs?: number
  additions?: number
  deletions?: number
  artifact?: ToolArtifact
  cached?: boolean
}

export interface FunctionCallItem {
  type: 'function_call'
  id?: string
  call_id?: string
  name?: string
  arguments?: string
  status?: string
  wikiatlas?: ToolExtra
}

export interface ReasoningItem {
  type?: 'reasoning'
  id?: string
  status?: string
  summary?: { text?: string; type?: string }[]
  content?: { text?: string; type?: string }[]
}

export interface OutputTextPart {
  type?: string
  text?: string
  annotations?: unknown[]
}

export interface OutputMessageItem {
  type?: 'message'
  id?: string
  status?: string
  role?: string
  content?: OutputTextPart[]
}

/** run 边界事实行（运行前准备 / 收尾结论），由 responses.ts 折进 content[] 两端。 */
export interface TurnItem {
  type: 'wikiatlas.turn'
  phase?: 'start' | 'end'
  text?: string
  tone?: string
}

/** 连续的工具调用折成的一条折叠条（派生层合成，Semi 按 type 找渲染器）。 */
export interface ToolGroupItem {
  type: 'wikiatlas.toolgroup'
  id: string
  items: FunctionCallItem[]
}

/** content[] 里可能出现的一切（未识别的按未知处理，不崩）。 */
export type ContentItemLike =
  | OutputMessageItem
  | ReasoningItem
  | FunctionCallItem
  | TurnItem
  | ToolGroupItem
  | { type?: string; [k: string]: unknown }

export function isFunctionCall(item: ContentItemLike): item is FunctionCallItem {
  return item?.type === 'function_call'
}

export function isReasoning(item: ContentItemLike): item is ReasoningItem {
  return item?.type === 'reasoning'
}

export function isToolGroup(item: ContentItemLike): item is ToolGroupItem {
  return item?.type === 'wikiatlas.toolgroup'
}

export function isTurn(item: ContentItemLike): item is TurnItem {
  return item?.type === 'wikiatlas.turn'
}

/** message 项说的话；不是 message 项就是空串。 */
export function messageText(item: ContentItemLike): string {
  if (item?.type !== 'message' && item?.type !== undefined) return ''
  const parts = (item as OutputMessageItem).content
  if (!Array.isArray(parts)) return ''
  return parts.map((p) => p?.text ?? '').join('').trim()
}

/** reasoning 项的全文（summary 优先，退回 content）。 */
export function reasoningText(item: ReasoningItem): string {
  const parts = item.summary?.length ? item.summary : (item.content ?? [])
  return parts.map((p) => p?.text ?? '').join('\n').trim()
}

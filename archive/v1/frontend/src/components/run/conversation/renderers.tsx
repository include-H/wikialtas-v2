import type { ReactNode } from 'react'
import type { FunctionCallItem, ReasoningItem, ToolGroupItem, TurnItem } from '../../../lib/runItems'
import ThinkingRow from './ThinkingRow'
import ToolGroupChip from './ToolGroupChip'
import TurnRow from './TurnRow'

// 对话内容项的**唯一**渲染表，交给 AIChatDialogue 的 `renderDialogueContentItem`。
//
// 为什么集中在一处：以前 AiPanel 与工单页各维护一份 `renderDialogueContentItem`，
// 结果工单页少了 reasoning 与边界行的渲染器——那些项被**静默丢掉**（Semi 找不到
// 渲染器就返回 null，不报错）。一张表，两个页面共用。
//
// 扩展点说明（Semi 源码 dialogueContent.tsx）：自定义渲染器**优先于**内置渲染器；
// 且 `function_call` 支持**按工具名**嵌套分发（`{ function_call: { read_work: fn } }`）
// ——将来要给某个工具单独做视图，挂在这里即可，不用改成别的机制。

type Renderer = (item: any, message?: unknown) => ReactNode

/** 兜底：正常情况分组已把 function_call 折进 toolgroup，这里只防"没走分组"的路径。 */
function loneTool(item: FunctionCallItem): ToolGroupItem {
  return { type: 'wikiatlas.toolgroup', id: item.id ?? 'toolgroup-lone', items: [item] }
}

export const conversationRenderers: Record<string, Renderer> = {
  function_call: (item: FunctionCallItem) => <ToolGroupChip item={loneTool(item)} />,
  'wikiatlas.toolgroup': (item: ToolGroupItem) => <ToolGroupChip item={item} />,
  reasoning: (item: ReasoningItem) => <ThinkingRow item={item} />,
  'wikiatlas.turn': (item: TurnItem) => <TurnRow item={item} />,
}

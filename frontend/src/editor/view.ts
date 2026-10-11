import type { CmdKey, Editor } from '@milkdown/kit/core'
import { editorViewCtx } from '@milkdown/kit/core'
import type { EditorView } from '@milkdown/kit/prose/view'
import { callCommand } from '@milkdown/kit/utils'
import { useInstance } from '@milkdown/react'

/** 任意命令的 key（各命令 payload 类型不同，工具栏/菜单里按统一形状收；`CmdKey` 是不变的，所以用 any）。 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type AnyCommand = CmdKey<any>

/**
 * 拿到已创建的 `Editor`；编辑器还没就绪时返回 null。
 * （`useInstance` 是 Milkdown React 唯一的实例入口，返回 `[loading, get]`。）
 */
export function useEditorInstance(): Editor | null {
  const [loading, get] = useInstance()
  if (loading) return null
  return get()
}

/** 当前 `EditorView`（读选区、跑命令都要它）。 */
export function useEditorView(): EditorView | null {
  const editor = useEditorInstance()
  return editor ? editor.action((ctx) => ctx.get(editorViewCtx)) : null
}

/** 在当前编辑器上跑一个命令（`callCommand` 的 ctx 版本）。 */
export function runCommand(editor: Editor, key: AnyCommand, payload?: unknown): void {
  editor.action(callCommand(key, payload))
}

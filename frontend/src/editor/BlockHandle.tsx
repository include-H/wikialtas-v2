import type { RefObject } from 'react'
import { DeleteOutlined, HolderOutlined } from '@ant-design/icons'
import { editorViewCtx } from '@milkdown/kit/core'
import type { BlockProvider } from '@milkdown/kit/plugin/block'

import { useEditorInstance } from './view'

/**
 * 块侧边手柄（v1 的 `SideMenuController`）：悬停到某块时出现在它左边，
 * 拖拽排序（拖拽由 `@milkdown/plugin-block` 的 BlockService 接管，这里只出 DOM），
 * 另给一个删除按钮。
 */
export default function BlockHandle({
  providerRef,
}: {
  providerRef: RefObject<BlockProvider | null>
}) {
  const editor = useEditorInstance()

  // 空文档没有块可拖——别让手柄压在光标上
  const empty =
    !editor ||
    editor.action((ctx) => ctx.get(editorViewCtx).state.doc.textContent.trim() === '')
  if (empty) return null

  const removeBlock = () => {
    const active = providerRef.current?.active
    if (!editor || !active) return
    editor.action((ctx) => {
      const view = ctx.get(editorViewCtx)
      const { $pos, node } = active
      const pos = $pos.pos
      view.dispatch(view.state.tr.delete(pos, pos + node.nodeSize))
      view.focus()
    })
  }

  return (
    <div className="block-handle">
      <button
        type="button"
        className="block-handle-btn block-handle-grip"
        aria-label="拖拽移动这个块"
        title="拖拽移动"
      >
        <HolderOutlined />
      </button>
      <button
        type="button"
        className="block-handle-btn block-handle-del"
        aria-label="删除这个块"
        title="删除"
        draggable={false}
        onMouseDown={(e) => e.preventDefault()}
        onClick={removeBlock}
      >
        <DeleteOutlined />
      </button>
    </div>
  )
}

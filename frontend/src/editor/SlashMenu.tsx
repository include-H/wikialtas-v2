import { useEffect, useMemo, useState, type ReactNode } from 'react'
import {
  CodeOutlined,
  MessageOutlined,
  MinusOutlined,
  OrderedListOutlined,
  TableOutlined,
  UnorderedListOutlined,
} from '@ant-design/icons'
import type { EditorView } from '@milkdown/kit/prose/view'
import {
  createCodeBlockCommand,
  insertHrCommand,
  turnIntoTextCommand,
  wrapInBlockquoteCommand,
  wrapInBulletListCommand,
  wrapInHeadingCommand,
  wrapInOrderedListCommand,
} from '@milkdown/kit/preset/commonmark'
import { insertTableCommand } from '@milkdown/kit/preset/gfm'

import { useEditorInstance, useEditorView, runCommand, type AnyCommand } from './view'
import { useEditorTick } from './tick'

/**
 * 光标前是否是一个待匹配的斜杠命令，返回 `/` 之后的查询串；否则 null。
 * 只在空选区、文本块内、且 `/` 处于行首或空白之后时成立。
 * `MarkdownEditor` 的 `shouldShow` 与这里共用同一判据，两边不会打架。
 */
export function slashQuery(view: EditorView): string | null {
  const { selection } = view.state
  if (!selection.empty) return null
  const { $from } = selection
  if (!$from.parent.isTextblock) return null
  const text = $from.parent.textBetween(0, $from.parentOffset, undefined, '\uFFFC')
  const m = /(?:^|\s)\/([^\s/]*)$/.exec(text)
  return m ? m[1] : null
}

interface Item {
  id: string
  label: string
  hint: string
  icon: ReactNode
  key: AnyCommand
  payload?: unknown
  keywords: string
  big?: boolean
}

const ITEMS: Item[] = [
  { id: 'h1', label: '一级标题', hint: 'H1', big: true, key: wrapInHeadingCommand.key, payload: 1, keywords: 'h1 标题 heading 一级', icon: null },
  { id: 'h2', label: '二级标题', hint: 'H2', big: true, key: wrapInHeadingCommand.key, payload: 2, keywords: 'h2 标题 heading 二级', icon: null },
  { id: 'h3', label: '三级标题', hint: 'H3', big: true, key: wrapInHeadingCommand.key, payload: 3, keywords: 'h3 标题 heading 三级', icon: null },
  { id: 'text', label: '正文', hint: 'P', big: true, key: turnIntoTextCommand.key, keywords: '正文 段落 paragraph text', icon: null },
  { id: 'bullet', label: '无序列表', hint: '•', icon: <UnorderedListOutlined />, key: wrapInBulletListCommand.key, keywords: '无序 列表 bullet ul' },
  { id: 'ordered', label: '有序列表', hint: '1.', icon: <OrderedListOutlined />, key: wrapInOrderedListCommand.key, keywords: '有序 列表 ordered ol 编号' },
  { id: 'quote', label: '引用', hint: '❞', icon: <MessageOutlined />, key: wrapInBlockquoteCommand.key, keywords: '引用 quote blockquote' },
  { id: 'code', label: '代码块', hint: '</>', icon: <CodeOutlined />, key: createCodeBlockCommand.key, keywords: '代码 code block' },
  { id: 'table', label: '表格', hint: '▦', icon: <TableOutlined />, key: insertTableCommand.key, payload: { row: 3, col: 3 }, keywords: '表格 table' },
  { id: 'hr', label: '分隔线', hint: '—', icon: <MinusOutlined />, key: insertHrCommand.key, keywords: '分隔线 分割线 hr divider' },
]

/**
 * `/` 插入菜单（v1 空文档提示里那句「按 / 插入内容」）。
 * 由 `@milkdown/plugin-slash` 定位/显隐，这里出菜单本体 + 键盘导航。
 */
export default function SlashMenu({ container }: { container: HTMLElement }) {
  const editor = useEditorInstance()
  const view = useEditorView()
  useEditorTick()
  const [index, setIndex] = useState(0)
  const [dismissed, setDismissed] = useState(false)

  const query = view ? slashQuery(view) : null
  const items = useMemo(() => {
    if (query === null) return []
    const q = query.toLowerCase()
    if (!q) return ITEMS
    return ITEMS.filter((i) => i.keywords.toLowerCase().includes(q) || i.label.includes(query))
  }, [query])

  useEffect(() => {
    setIndex(0)
    setDismissed(false)
  }, [query])

  const close = () => {
    setDismissed(true)
    container.dataset.show = 'false'
  }

  const choose = (item: Item | undefined) => {
    if (!item || !editor || !view || query === null) return
    const { $from } = view.state.selection
    const start = $from.pos - (query.length + 1) // 连 `/` 一起删
    view.dispatch(view.state.tr.delete(start, $from.pos))
    runCommand(editor, item.key, item.payload)
    view.focus()
  }

  // 焦点仍在编辑器里，键盘事件不会落到菜单上——在 document 捕获阶段拦。
  useEffect(() => {
    if (query === null || dismissed) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        setIndex((i) => (items.length ? (i + 1) % items.length : 0))
      } else if (e.key === 'ArrowUp') {
        e.preventDefault()
        setIndex((i) => (items.length ? (i - 1 + items.length) % items.length : 0))
      } else if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault()
        choose(items[index])
      } else if (e.key === 'Escape') {
        e.preventDefault()
        close()
      }
    }
    document.addEventListener('keydown', onKey, true)
    return () => document.removeEventListener('keydown', onKey, true)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, dismissed, items, index])

  if (query === null || dismissed || items.length === 0) return null

  return (
    <div className="slash-menu">
      <div className="slash-menu-head">插入</div>
      {items.map((item, i) => (
        <button
          key={item.id}
          type="button"
          className={`slash-menu-item${i === index ? ' is-active' : ''}`}
          onMouseEnter={() => setIndex(i)}
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => choose(item)}
        >
          <span className="slash-menu-icon">{item.icon}</span>
          <span className="slash-menu-label">{item.label}</span>
          <span className="slash-menu-hint">{item.hint}</span>
        </button>
      ))}
    </div>
  )
}

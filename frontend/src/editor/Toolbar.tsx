import { useState, type ReactNode } from 'react'
import { App, Button, Divider, Input, Modal, Tooltip } from 'antd'
import {
  BoldOutlined,
  CodeOutlined,
  CommentOutlined,
  ItalicOutlined,
  LinkOutlined,
  MessageOutlined,
  MinusOutlined,
  OrderedListOutlined,
  StrikethroughOutlined,
  TableOutlined,
  ThunderboltOutlined,
  TranslationOutlined,
  UnorderedListOutlined,
} from '@ant-design/icons'
import {
  createCodeBlockCommand,
  insertHrCommand,
  toggleEmphasisCommand,
  toggleInlineCodeCommand,
  toggleLinkCommand,
  toggleStrongCommand,
  turnIntoTextCommand,
  wrapInBlockquoteCommand,
  wrapInBulletListCommand,
  wrapInHeadingCommand,
  wrapInOrderedListCommand,
} from '@milkdown/kit/preset/commonmark'
import { insertTableCommand, toggleStrikethroughCommand } from '@milkdown/kit/preset/gfm'

import { useUi } from '../store'
import { useEditorInstance, useEditorView, runCommand, type AnyCommand } from './view'
import { useEditorTick } from './tick'

interface TbProps {
  label: string
  active?: boolean
  disabled?: boolean
  icon?: ReactNode
  children?: ReactNode
  onClick: () => void
}

function Tb({ label, active, disabled, icon, children, onClick }: TbProps) {
  return (
    <Tooltip title={label}>
      <Button
        type="text"
        size="small"
        className={`tb-btn${active ? ' is-active' : ''}`}
        icon={icon}
        disabled={disabled}
        aria-label={label}
        // 别让工具条抢走编辑器焦点（否则点一下选区就没了）
        onMouseDown={(e) => e.preventDefault()}
        onClick={onClick}
      >
        {children}
      </Button>
    </Tooltip>
  )
}

/**
 * 选区格式化工具栏（v1 的 `FeishuToolbar`）。挂在 `@milkdown/plugin-tooltip` 上，
 * 由 Milkdown 负责定位/显隐；这里只出按钮。
 *
 * 与 v1 的差异（markdown 存储决定，不是漏做）：**下划线 / 高亮**没有 markdown 形态，
 * 放进来会在序列化时丢，所以不收。
 */
export default function Toolbar() {
  const editor = useEditorInstance()
  const view = useEditorView()
  useEditorTick() // 选区/文档一变就重算高亮
  const { message } = App.useApp()
  const askSelection = useUi((s) => s.askSelection)
  const [linkOpen, setLinkOpen] = useState(false)
  const [linkUrl, setLinkUrl] = useState('')

  const state = view?.state
  const $from = state?.selection.$from
  const marks = new Set<string>(
    (state?.storedMarks ?? $from?.marks() ?? []).map((m) => m.type.name),
  )
  const blockType = $from?.parent.type.name ?? 'paragraph'
  const level = Number($from?.parent.attrs?.level ?? 0)

  const call = (key: AnyCommand, payload?: unknown) => {
    if (!editor) return
    runCommand(editor, key, payload)
    view?.focus()
  }

  const selectedText = () => {
    if (!view) return ''
    const { from, to } = view.state.selection
    return view.state.doc.textBetween(from, to, '\n')
  }

  const ask = (prompt: string, autoSend: boolean) => {
    const text = selectedText().trim()
    if (!text) {
      message.info('先选中一段正文')
      return
    }
    askSelection({ text, prompt, autoSend })
    view?.focus()
  }

  const submitLink = () => {
    const url = linkUrl.trim()
    if (url) call(toggleLinkCommand.key, { href: url })
    setLinkOpen(false)
    setLinkUrl('')
  }

  const disabled = !editor

  return (
    <>
      <div className="feishu-toolbar">
        {/* AI 动作（v1 的紫色区：问 Atlas / 翻译 / 解释） */}
        <span className="tb-ai">
          <Tb label="问 Atlas（把选段挂上）" icon={<ThunderboltOutlined />} onClick={() => ask('', false)} />
          <Tb label="翻译这段" icon={<TranslationOutlined />} onClick={() => ask('翻译这段', true)} />
          <Tb label="解释这段" icon={<CommentOutlined />} onClick={() => ask('解释这段', true)} />
        </span>
        <Divider type="vertical" />

        {/* 块类型 */}
        <Tb label="正文" active={blockType === 'paragraph'} disabled={disabled} onClick={() => call(turnIntoTextCommand.key)}>
          正文
        </Tb>
        <Tb label="一级标题" active={blockType === 'heading' && level === 1} disabled={disabled} onClick={() => call(wrapInHeadingCommand.key, 1)}>
          H1
        </Tb>
        <Tb label="二级标题" active={blockType === 'heading' && level === 2} disabled={disabled} onClick={() => call(wrapInHeadingCommand.key, 2)}>
          H2
        </Tb>
        <Tb label="三级标题" active={blockType === 'heading' && level === 3} disabled={disabled} onClick={() => call(wrapInHeadingCommand.key, 3)}>
          H3
        </Tb>
        <Tb label="无序列表" active={blockType === 'bullet_list'} disabled={disabled} icon={<UnorderedListOutlined />} onClick={() => call(wrapInBulletListCommand.key)} />
        <Tb label="有序列表" active={blockType === 'ordered_list'} disabled={disabled} icon={<OrderedListOutlined />} onClick={() => call(wrapInOrderedListCommand.key)} />
        <Tb label="引用" active={blockType === 'blockquote'} disabled={disabled} icon={<MessageOutlined />} onClick={() => call(wrapInBlockquoteCommand.key)} />
        <Tb label="代码块" active={blockType === 'code_block'} disabled={disabled} icon={<CodeOutlined />} onClick={() => call(createCodeBlockCommand.key)} />
        <Tb label="表格" disabled={disabled} icon={<TableOutlined />} onClick={() => call(insertTableCommand.key, { row: 3, col: 3 })} />
        <Tb label="分隔线" disabled={disabled} icon={<MinusOutlined />} onClick={() => call(insertHrCommand.key)} />
        <Divider type="vertical" />

        {/* 行内样式 */}
        <Tb label="加粗" active={marks.has('strong')} disabled={disabled} icon={<BoldOutlined />} onClick={() => call(toggleStrongCommand.key)} />
        <Tb label="斜体" active={marks.has('emphasis')} disabled={disabled} icon={<ItalicOutlined />} onClick={() => call(toggleEmphasisCommand.key)} />
        <Tb label="删除线" active={marks.has('strike_through')} disabled={disabled} icon={<StrikethroughOutlined />} onClick={() => call(toggleStrikethroughCommand.key)} />
        <Tb label="行内代码" active={marks.has('inlineCode')} disabled={disabled} icon={<CodeOutlined />} onClick={() => call(toggleInlineCodeCommand.key)} />
        <Tb
          label="链接"
          active={marks.has('link')}
          disabled={disabled}
          icon={<LinkOutlined />}
          onClick={() => {
            setLinkUrl('')
            setLinkOpen(true)
          }}
        />
      </div>

      <Modal
        title="插入链接"
        open={linkOpen}
        onCancel={() => setLinkOpen(false)}
        onOk={submitLink}
        okText="插入"
        cancelText="取消"
        width={420}
      >
        <Input
          placeholder="https://"
          value={linkUrl}
          onChange={(e) => setLinkUrl(e.target.value)}
          onPressEnter={submitLink}
          autoFocus
        />
      </Modal>
    </>
  )
}

import { useCallback, useEffect, useRef, useState } from 'react'
import {
  Avatar,
  Banner,
  Button,
  Input,
  Toast,
  Typography,
} from '@douyinfe/semi-ui'
import { IconEdit, IconSave } from '@douyinfe/semi-icons'
import { IconAIFilledLevel1 } from '@douyinfe/semi-icons'
import type { PutContentBody, PutContentResult } from '../../types'
import { ensureLeadingH1, stripLeadingH1 } from '../../lib/docMarkdown'
import { absoluteTime } from '../../lib/tree'
import MarkdownPreview from './MarkdownPreview'
import BlockMarkdownEditor from './BlockMarkdownEditor'

const { Text, Title } = Typography

export type EditorMode = 'edit' | 'read'

interface Props {
  title: string
  updatedAt?: string
  author?: string
  contentMd: string
  contentVer: number
  saving?: boolean
  aiWriting?: boolean
  /**
   * Altas 正在流式写的那份草稿（wikiatlas.content.delta 累积出来的完整 Markdown）。
   * 非空期间编辑器只读显示它、正文随段生长；committed 后切回正式内容。
   */
  liveMd?: string | null
  /** 草稿最近的收尾是中断（工单被停/失败）：末稿不留，直接退回真实内容 */
  liveAborted?: boolean
  /** 受控的编辑/阅读态（顶栏眼睛图标与世界共用） */
  mode?: EditorMode
  /** 信息栏右侧：AI 共建 + 更多菜单 */
  actions?: React.ReactNode
  /** 空文档时正文里的行内 AI 入口（飞书「AI 帮我写」的位置） */
  onAiWrite?: () => void
  onSave: (body: PutContentBody) => Promise<PutContentResult>
  onTitleChange?: (title: string) => void
  /** Debounced draft for live outline extraction in parent panes. */
  onDraftChange?: (markdown: string) => void
}

export default function MarkdownEditor({
  title,
  updatedAt,
  author = '我',
  contentMd,
  contentVer,
  saving,
  aiWriting,
  liveMd,
  liveAborted,
  mode: modeProp,
  actions,
  onAiWrite,
  onSave,
  onTitleChange,
  onDraftChange,
}: Props) {
  // 默认编辑态 + 所见即所得（对齐 VISUAL_SPEC §1.4）
  const [modeState] = useState<EditorMode>('edit')
  const mode = modeProp ?? modeState
  const safeContent = contentMd ?? ''
  const [draft, setDraft] = useState(() => stripLeadingH1(safeContent))
  const [dirty, setDirty] = useState(false)
  const [localSaving, setLocalSaving] = useState(false)
  const [editTitle, setEditTitle] = useState(title)
  const lastExternal = useRef(stripLeadingH1(safeContent))

  // 外部内容变化（AI 提交 / 切换作品）时同步，正在编辑的内容不动
  useEffect(() => {
    const stripped = stripLeadingH1(safeContent)
    if (!dirty && lastExternal.current !== stripped) {
      setDraft(stripped)
      lastExternal.current = stripped
    }
    if (!dirty) setEditTitle(title)
  }, [safeContent, title, dirty])

  useEffect(() => {
    if (aiWriting) lastExternal.current = stripLeadingH1(safeContent)
  }, [aiWriting, safeContent])

  // ── Altas 实时草稿（正文流） ──
  // 写入期间编辑器显示正在长出来的草稿（只读）；用户有未保存内容时草稿让位
  // （别把人正在打的字盖掉）。
  const liveSafe = liveMd != null ? stripLeadingH1(liveMd) : null
  const showLive = liveSafe != null && !dirty
  const lastLive = useRef<string | null>(null)

  useEffect(() => {
    if (!showLive || liveSafe == null) return
    lastLive.current = liveSafe
    // 左栏大纲跟着草稿长（与手动编辑同一条 onDraftChange 通道）
    onDraftChange?.(ensureLeadingH1(liveSafe, editTitle))
  }, [showLive, liveSafe, editTitle, onDraftChange])

  // 草稿结束：提交（liveAborted=false）时用最后一段垫底，等落库的正式内容回来再对齐——
  // 不垫的话会先闪回旧正文（此刻 work 还没重新拉取）。中断（aborted）时什么都不做：
  // 正文没变，显示自然落回真实内容（draft 全程没被动过）。
  useEffect(() => {
    if (liveSafe != null) return
    const final = lastLive.current
    lastLive.current = null
    if (liveAborted || final == null) return
    if (!dirty) {
      setDraft(final)
      lastExternal.current = final
    }
  }, [liveSafe, liveAborted, dirty])

  // 大纲跟随实时编辑
  useEffect(() => {
    if (!onDraftChange) return
    const t = window.setTimeout(() => onDraftChange(ensureLeadingH1(draft, editTitle)), 280)
    return () => window.clearTimeout(t)
  }, [draft, editTitle, onDraftChange])

  const onBodyChange = useCallback((v: string) => {
    setDraft(v)
    setDirty(true)
  }, [])

  const save = useCallback(async () => {
    setLocalSaving(true)
    try {
      const content = ensureLeadingH1(draft, editTitle)
      await onSave({
        contentMd: content,
        author: 'human',
        expectedVersion: contentVer,
        summary: dirty ? '手动编辑' : undefined,
      })
      lastExternal.current = draft
      setDirty(false)
      Toast.success('已保存')
      if (onTitleChange && editTitle !== title) onTitleChange(editTitle)
    } catch (e) {
      Toast.error(e instanceof Error ? e.message : '保存失败')
    } finally {
      setLocalSaving(false)
    }
  }, [draft, editTitle, contentVer, dirty, onSave, onTitleChange, title])

  // Ctrl/Cmd+S
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 's') {
        e.preventDefault()
        void save()
      }
    }
    window.addEventListener('keydown', h)
    return () => window.removeEventListener('keydown', h)
  }, [save])

  // 编辑态与修订态本质是同一个编辑器（差别在 Altas 的作业方式，不在界面）；
  // 只读态才切成渲染视图。产品里没有 Markdown 源码模式。
  const showBlock = mode === 'edit'
  const showRead = mode === 'read'

  return (
    <div className="md-editor">
      <div className="doc-header">
        {/* 标题可改才用输入框：资料（doc）的标题走资料夹改名，这里直接按标题渲染，
            否则 disabled 的 Input 会把标题灰掉，看起来像不可用。 */}
        {mode === 'edit' && onTitleChange ? (
          <Input
            borderless
            className="doc-title-input"
            value={editTitle}
            onChange={setEditTitle}
            placeholder="请输入标题"
          />
        ) : (
          <Title heading={2} className="doc-title-read">
            {editTitle || '无标题'}
          </Title>
        )}
        <div className="doc-meta">
          <Avatar size="extra-small" color="light-blue">
            {author.slice(0, 1)}
          </Avatar>
          <span className="doc-meta-author">{author}</span>
          {updatedAt && <span className="doc-meta-sep">|</span>}
          {updatedAt && <span className="doc-meta-time">{absoluteTime(updatedAt)} 修改</span>}
          <div className="doc-meta-spacer" />
          {/* 同步状态很轻：干净时只有一行小字，有改动才出现保存按钮 */}
          <Text type="tertiary" size="small" className="editor-sync-hint">
            v{contentVer}
            {dirty ? ' · 未保存' : saving || localSaving ? ' · 保存中…' : ' · 已同步'}
          </Text>
          {(dirty || localSaving || saving) && (
            <Button
              size="small"
              theme="solid"
              type="primary"
              icon={<IconSave />}
              loading={localSaving || saving}
              onClick={() => void save()}
            >
              保存
            </Button>
          )}
          {/* 「AI 共建」= 展开右侧栏 Altas 标签（顶栏「问 Altas」入口 2026-09-16 移除，入口收归右侧栏拉门） */}
          {actions}
        </div>
      </div>

      {aiWriting && (
        <Banner
          type="warning"
          icon={<IconEdit />}
          description="Altas 正在写入…完成后自动刷新"
          closeIcon={null}
          className="doc-writing-banner"
        />
      )}

      <div className={`editor-body mode-${mode}`}>
        {showBlock && (
          <BlockMarkdownEditor
            value={showLive && liveSafe != null ? liveSafe : draft}
            onChange={onBodyChange}
            readOnly={showLive}
            live={showLive}
            emptyHint={
              !showLive && draft.trim() === '' && onAiWrite ? (
                <>
                  <span>按 “/” 插入内容，或让</span>
                  <button type="button" className="ai-inline-pill" onClick={onAiWrite}>
                    <IconAIFilledLevel1 size="small" />
                    AI 帮我写
                  </button>
                </>
              ) : null
            }
          />
        )}
        {showRead && (
          <div className="editor-pane preview-pane" id="content-scroll">
            {(showLive && liveSafe != null ? liveSafe : draft).trim() ? (
              <MarkdownPreview
                markdown={showLive && liveSafe != null ? liveSafe : draft}
                live={showLive}
              />
            ) : (
              <div className="preview-empty">暂无正文</div>
            )}
          </div>
        )}
      </div>
    </div>
  )
}

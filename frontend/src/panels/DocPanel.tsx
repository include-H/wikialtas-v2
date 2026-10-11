import { useEffect, useRef, useState } from 'react'
import { App as AntApp, Alert, Button, Empty, Space, Tag, Tooltip } from 'antd'
import { EditOutlined, EyeOutlined } from '@ant-design/icons'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { ApiError, api } from '../api'
import { normalizeMarkdown } from '../markdown'
import { joinDoc, splitDoc, type DocHeader } from '../epigraph'
import MarkdownEditor from '../editor/MarkdownEditor'
import Epigraph from './Epigraph'
import NoteBlock from './NoteBlock'
import Outline from './Outline'
import { useUi } from '../store'

export default function DocPanel({ nodeId }: { nodeId: string | null }) {
  if (!nodeId) {
    return (
      <div style={{ display: 'flex', height: '100%', alignItems: 'center', justifyContent: 'center' }}>
        <Empty description="左边选一个节点，或新建一个" />
      </div>
    )
  }
  // key：换节点就重建编辑器（Milkdown 只在挂载时吃一次初始值）
  return <DocEditor key={nodeId} nodeId={nodeId} />
}

const SAVE_DEBOUNCE_MS = 1500

function DocEditor({ nodeId }: { nodeId: string }) {
  const qc = useQueryClient()
  const { message } = AntApp.useApp()
  const mode = useUi((s) => s.docMode)
  const setDocMode = useUi((s) => s.setDocMode)
  const setAiOpen = useUi((s) => s.setAiOpen)
  const readOnly = mode === 'read'

  const detail = useQuery({ queryKey: ['node', nodeId], queryFn: () => api.node(nodeId) })

  const [body, setBody] = useState<string | null>(null) // null = 还没装载；存的是**不含文档头**的正文
  const [header, setHeader] = useState<DocHeader>({ titleLine: '', noteBlock: '', fence: '' })
  const [lastSaved, setLastSaved] = useState<string | null>(null) // 服务端那份（**完整**正文）
  const [baseVer, setBaseVer] = useState(0)
  const [savedAt, setSavedAt] = useState<string | null>(null)
  const [conflict, setConflict] = useState<{ baseVer: number; currentVer: number } | null>(null)
  const timer = useRef<number | null>(null)

  // 装载：定稿优先；有草稿（比如上次没定稿就关了）就用草稿。
  // 文档头（标题 / 说明块 / 题记）摘出来各自渲染，正文才进编辑器。
  useEffect(() => {
    if (!detail.data || body !== null) return
    const { content, draft } = detail.data
    const raw = draft?.body_md ?? content?.body_md ?? ''
    const { header: h, rest } = splitDoc(raw)
    setHeader(h)
    setBody(rest)
    setLastSaved(raw)
    setBaseVer(content?.ver ?? 0)
    setSavedAt(draft?.updated_at ?? null)
  }, [detail.data, body])

  /** 落库用的**完整**正文：把文档头原样接回正文前面。 */
  const fullBody = normalizeMarkdown(joinDoc(header, body ?? ''))

  const saveDraft = useMutation({
    mutationFn: ({ md, base }: { md: string; base: number }) => api.putDraft(nodeId, md, base),
    onSuccess: (d) => {
      setLastSaved(d.body_md)
      setSavedAt(d.updated_at)
    },
    onError: (e) => message.error(`草稿没存上：${e.message}`),
  })
  const saveDraftRef = useRef(saveDraft)
  saveDraftRef.current = saveDraft

  // 防抖存草稿：改了才存，停了 1.5 秒才发请求（题记改了也要存，所以 body/fence 都进依赖）
  useEffect(() => {
    if (body === null || fullBody === lastSaved) return
    if (timer.current) window.clearTimeout(timer.current)
    timer.current = window.setTimeout(() => {
      saveDraftRef.current.mutate({ md: fullBody, base: baseVer })
    }, SAVE_DEBOUNCE_MS)
    return () => {
      if (timer.current) window.clearTimeout(timer.current)
    }
    // lastSaved/baseVer 变化时不需要重跑：它们要么由装载设定，要么由保存结果设定
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [body, header])

  const commit = useMutation({
    mutationFn: ({ md, base }: { md: string; base: number }) => api.commit(nodeId, md, base, ''),
    onSuccess: (out) => {
      setBaseVer(out.ver)
      setConflict(null)
      setSavedAt(out.updated_at)
      setLastSaved(fullBody)
      qc.invalidateQueries({ queryKey: ['tree'] })
      qc.invalidateQueries({ queryKey: ['node', nodeId] })
      message.success(`已定稿 v${out.ver}`)
    },
    onError: (e) => {
      if (e instanceof ApiError && e.body?.error === 'version_conflict') {
        setConflict({ baseVer: e.body.baseVer ?? baseVer, currentVer: e.body.currentVer ?? 0 })
      } else {
        message.error(e.message)
      }
    },
  })

  // Ctrl/Cmd+S：定稿（v1 的 MarkdownEditor 用同一个快捷键保存）
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') {
        e.preventDefault()
        if (!readOnly && !conflict) commit.mutate({ md: fullBody, base: baseVer })
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [readOnly, conflict, fullBody, baseVer, commit])

  /** 冲突处理 A：载入最新，丢弃我这边的改动 */
  function reloadLatest() {
    setConflict(null)
    setBody(null) // 触发重新装载
    qc.invalidateQueries({ queryKey: ['node', nodeId] })
  }

  /** 冲突处理 B：以我的为准——先拿最新版本号，再明确地盖上去 */
  async function overwriteWithMine() {
    const fresh = await api.node(nodeId)
    commit.mutate({ md: fullBody, base: fresh.content?.ver ?? 0 })
  }

  if (detail.isLoading || body === null) {
    return <div style={{ padding: 24, color: 'var(--ant-color-text-tertiary)' }}>载入中…</div>
  }
  if (detail.isError) {
    return (
      <div style={{ padding: 24 }}>
        <Alert type="error" showIcon message="这个节点打不开" description={detail.error.message} />
      </div>
    )
  }

  const node = detail.data!.node
  const savedVer = detail.data!.content?.ver ?? 0
  const dirty = fullBody !== lastSaved

  return (
    <div className="doc-work">
      <Outline bodyMd={body} />

      <div className="doc">
        <div className="doc-head">
          <h1 className="doc-title">{node.title}</h1>
        </div>

        {/* meta 行（v1 的 .doc-meta）：状态 + 版本 ｜ 右侧动作 */}
        <div className="doc-bar">
          <Space size={8} wrap>
            <Tag style={{ marginInlineEnd: 0 }}>{node.kind}</Tag>
            {node.medium && (
              <Tag style={{ marginInlineEnd: 0 }} color="blue">
                {node.medium}
              </Tag>
            )}
            <span>{savedVer > 0 ? `定稿 v${savedVer}` : '还没定稿'}</span>
            {!readOnly && (
              <>
                <span>
                  {saveDraft.isPending
                    ? '正在存草稿…'
                    : savedAt
                      ? `草稿已存 ${fmtTime(savedAt)}`
                      : '无草稿'}
                </span>
                <span style={{ color: dirty ? 'var(--ant-color-warning)' : undefined }}>
                  {dirty ? '有改动（停 1.5 秒自动存草稿）' : '已同步'}
                </span>
                <span style={{ color: 'var(--ant-color-text-quaternary)' }}>
                  {conflict ? '' : `基于 v${baseVer}`}
                </span>
              </>
            )}
          </Space>
          <span className="doc-meta-spacer" />
          <Tooltip title={readOnly ? '切回编辑' : '切到阅读（只读）'}>
            <Button
              type="text"
              size="small"
              aria-label={readOnly ? '切回编辑' : '阅读'}
              icon={readOnly ? <EditOutlined /> : <EyeOutlined />}
              onClick={() => setDocMode(readOnly ? 'edit' : 'read')}
            />
          </Tooltip>
          {!readOnly && (
            <Tooltip title={conflict ? '先处理版本冲突' : '把当前内容落成新定稿'}>
              <Button
                type="primary"
                size="small"
                loading={commit.isPending}
                disabled={conflict !== null}
                onClick={() => commit.mutate({ md: fullBody, base: baseVer })}
              >
                定稿
              </Button>
            </Tooltip>
          )}
        </div>

        {conflict && !readOnly && (
          <Alert
            type="warning"
            showIcon
            style={{ margin: '10px 32px 0' }}
            message={`正文已被别人改过：你的草稿基于 v${conflict.baseVer}，库里已是 v${conflict.currentVer}`}
            description={
              <Space>
                <Button size="small" onClick={reloadLatest}>
                  载入最新（丢弃我的改动）
                </Button>
                <Button size="small" danger onClick={overwriteWithMine}>
                  以我的为准覆盖
                </Button>
              </Space>
            }
          />
        )}

        <div className="doc-body">
          {/* 文档头（主标题 → 说明块 → 题记）各按自己的形态渲染，**源码不进编辑器**；
              保存时 joinDoc 原样接回去（验收脚本守这条往返）。 */}
          {header.noteBlock && (
            <NoteBlock
              source={header.noteBlock}
              readOnly={readOnly}
              onChange={(next) => setHeader((h) => ({ ...h, noteBlock: next }))}
            />
          )}
          {header.fence && (
            <Epigraph
              fence={header.fence}
              readOnly={readOnly}
              onChange={(next) => setHeader((h) => ({ ...h, fence: next }))}
            />
          )}
          <MarkdownEditor
            initial={body}
            readOnly={readOnly}
            onAiWrite={() => setAiOpen(true)}
            onChange={(md) => setBody(normalizeMarkdown(md))}
            // 粘贴整篇文档时走与装载同一条路：文档头（主标题 / 说明块 / 题记）交上面
            // 的书页组件渲染，编辑器只吃正文。只在**空正文**里这么做——粘进已有正文的
            // 中间时，开头的 `#` 只是普通标题，不能抢成文档头。
            transformPastedText={(text) => {
              if ((body ?? '').trim() !== '') return text
              if (header.titleLine || header.noteBlock || header.fence) return text
              const { header: h, rest } = splitDoc(text)
              if (!h.titleLine && !h.noteBlock && !h.fence) return text
              setHeader(h)
              return rest
            }}
          />
        </div>
      </div>
    </div>
  )
}

function fmtTime(iso: string): string {
  const d = new Date(iso)
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}:${String(
    d.getSeconds(),
  ).padStart(2, '0')}`
}

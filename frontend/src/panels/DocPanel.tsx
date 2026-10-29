import { useEffect, useMemo, useRef, useState } from 'react'
import {
  App as AntApp,
  Alert,
  Button,
  Divider,
  Drawer,
  Dropdown,
  Empty,
  Input,
  List,
  Modal,
  Select,
  Space,
  Spin,
  Tooltip,
  Typography,
} from 'antd'
import {
  ApartmentOutlined,
  CopyOutlined,
  DeleteOutlined,
  HistoryOutlined,
  InfoCircleOutlined,
  LinkOutlined,
  MoreOutlined,
  PlusOutlined,
  SyncOutlined,
  ThunderboltOutlined,
  UndoOutlined,
} from '@ant-design/icons'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import {
  ApiError,
  api,
  type Node,
  type NodeContent,
  type NodeLink,
  type SynthesisType,
} from '../api'
import { normalizeMarkdown, sameDoc } from '../markdown'
import { joinDoc, splitDoc, type DocHeader } from '../epigraph'
import MarkdownEditor from '../editor/MarkdownEditor'
import { MEDIUM_LABEL, KIND_LABEL, statusClass } from '../nodeVisual'
import { SOURCE_LABEL as SOURCE_NAME } from './pool/util'
import Epigraph from './Epigraph'
import LiveMarkdown from './LiveMarkdown'
import NoteBlock from './NoteBlock'
import Outline from './Outline'
import { useUi } from '../store'

export default function DocPanel({ nodeId }: { nodeId: string | null }) {
  if (!nodeId) {
    return (
      <div style={{ display: 'flex', height: '100%', alignItems: 'center', justifyContent: 'center' }}>
        <Typography.Text type="secondary">左边选一个节点，或新建一个</Typography.Text>
      </div>
    )
  }
  // key：换节点就重建编辑器（Milkdown 只在挂载时吃一次初始值）
  return <DocEditor key={nodeId} nodeId={nodeId} />
}

const SAVE_DEBOUNCE_MS = 1500

function DocEditor({ nodeId }: { nodeId: string }) {
  const qc = useQueryClient()
  const { message, modal } = AntApp.useApp()
  const mode = useUi((s) => s.docMode)
  const prefillAi = useUi((s) => s.prefillAi)
  const live = useUi((s) => s.liveWrite)
  const select = useUi((s) => s.select)
  const readOnly = mode === 'read'
  const [relinkLink, setRelinkLink] = useState<NodeLink | null>(null)
  const [relationsOpen, setRelationsOpen] = useState(false)

  const detail = useQuery({ queryKey: ['node', nodeId], queryFn: () => api.node(nodeId) })
  const links = useQuery({
    queryKey: ['node-links', nodeId],
    queryFn: () => api.nodeLinks(nodeId),
    refetchInterval: (q) =>
      q.state.data?.some((l) => l.sync_targets.some((t) => t.status === 'pending')) ? 1500 : false,
  })

  const [body, setBody] = useState<string | null>(null) // null = 还没装载；存的是**不含文档头**的正文
  const [header, setHeader] = useState<DocHeader>({ titleLine: '', noteBlock: '', fence: '' })
  const [lastSaved, setLastSaved] = useState<string | null>(null) // 服务端那份（**完整**正文）
  const [baseVer, setBaseVer] = useState(0)
  const [savedAt, setSavedAt] = useState<string | null>(null)
  const [conflict, setConflict] = useState<{ baseVer: number; currentVer: number } | null>(null)
  const [versionsOpen, setVersionsOpen] = useState(false)
  const [preview, setPreview] = useState<{ ver: number; body_md: string } | null>(null)
  const [infoOpen, setInfoOpen] = useState(false)
  const timer = useRef<number | null>(null)

  // 实时写入（`design.md §6.3`）：Atlas 正在写这篇时，正文区切只读实时视图
  const liveFor = live && live.nodeId === nodeId ? live : null
  const wasLive = useRef(false)

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

  // 实时写入结束 → 从服务端草稿重新基线（D46）
  useEffect(() => {
    if (liveFor) {
      wasLive.current = true
      return
    }
    if (wasLive.current) {
      wasLive.current = false
      setBody(null)
      qc.invalidateQueries({ queryKey: ['node', nodeId] })
    }
  }, [liveFor, nodeId, qc])

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
    if (body === null || sameDoc(fullBody, lastSaved)) return
    if (timer.current) window.clearTimeout(timer.current)
    timer.current = window.setTimeout(() => {
      saveDraftRef.current.mutate({ md: fullBody, base: baseVer })
    }, SAVE_DEBOUNCE_MS)
    return () => {
      if (timer.current) window.clearTimeout(timer.current)
    }
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
      qc.invalidateQueries({ queryKey: ['overview'] })
      qc.invalidateQueries({ queryKey: ['versions', nodeId] })
      qc.invalidateQueries({ queryKey: ['node-links', nodeId] })
      message.success(out.sync_job_id ? `已定稿 v${out.ver}，正在反哺媒体库` : `已定稿 v${out.ver}`)
    },
    onError: (e) => {
      if (e instanceof ApiError && e.body?.error === 'version_conflict') {
        setConflict({ baseVer: e.body.baseVer ?? baseVer, currentVer: e.body.currentVer ?? 0 })
      } else {
        message.error(e.message)
      }
    },
  })

  /** 丢弃草稿 = v2 的「拒绝修订」：接受是定稿，拒绝是回到最近定稿。 */
  const discardDraft = useMutation({
    mutationFn: () => api.discardDraft(nodeId),
    onSuccess: (out) => {
      if (!out.discarded) {
        message.info('没有待丢弃的草稿')
        return
      }
      // 先把缓存里的草稿清掉，再让装载 effect 从定稿重新基线。
      // 只 setBody(null) 不行：effect 会先用旧缓存（还带着 draft）把草稿装回来。
      if (timer.current) window.clearTimeout(timer.current)
      qc.setQueryData<NodeContent | undefined>(['node', nodeId], (old) =>
        old ? { ...old, draft: null } : old,
      )
      setBody(null)
      setSavedAt(null)
      qc.invalidateQueries({ queryKey: ['node', nodeId] })
      message.success(
        savedVer > 0 ? `已丢弃草稿，回到定稿 v${savedVer}` : '已丢弃草稿',
      )
    },
    onError: (e) => message.error(`丢弃草稿失败：${e.message}`),
  })

  const sync = useMutation({
    mutationFn: (linkId: string) => api.resyncLink(linkId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['node-links', nodeId] })
      message.success('已把最新定稿加入反哺队列')
    },
    onError: (e) => message.error(`重新推送失败：${e.message}`),
  })

  const archive = useMutation({
    mutationFn: () => api.archiveNode(nodeId),
    onSuccess: () => {
      const parentId = detail.data?.node.parent_id ?? null
      select(parentId)
      qc.invalidateQueries({ queryKey: ['tree'] })
      qc.invalidateQueries({ queryKey: ['overview'] })
      qc.invalidateQueries({ queryKey: ['node', nodeId] })
      message.success('已从知识库删除；正文与版本记录仍保留')
    },
    onError: (e) => message.error(`删除失败：${e.message}`),
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
    setBody(null)
    qc.invalidateQueries({ queryKey: ['node', nodeId] })
  }

  /** 冲突处理 B：以我的为准——先拿最新版本号，再明确地盖上去 */
  async function overwriteWithMine() {
    const fresh = await api.node(nodeId)
    commit.mutate({ md: fullBody, base: fresh.content?.ver ?? 0 })
  }

  /** 把某一版快照恢复成草稿（版本历史抽屉里）。 */
  async function restoreVersion(ver: number) {
    const p = preview?.ver === ver ? preview : await api.versionBody(nodeId, ver)
    await api.putDraft(nodeId, p.body_md, baseVer)
    setVersionsOpen(false)
    setBody(null)
    qc.invalidateQueries({ queryKey: ['node', nodeId] })
    message.success(`已把 v${ver} 恢复成草稿`)
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
  const dirty = !sameDoc(fullBody, lastSaved)
  const activeLink = links.data?.find((l) => l.state === 'active')
  const staleLink = links.data?.find((l) => l.state === 'stale')
  const syncTarget = activeLink?.sync_targets[0]
  const canResync =
    activeLink && (activeLink.source === 'gameatlas' || activeLink.source === 'komga')

  const menuItems = [
    { key: 'relations', icon: <ApartmentOutlined />, label: '关系与综合页' },
    { key: 'versions', icon: <HistoryOutlined />, label: '版本历史' },
    { key: 'info', icon: <InfoCircleOutlined />, label: '节点信息' },
    { key: 'copy', icon: <CopyOutlined />, label: '复制 Markdown' },
    ...(detail.data?.draft && !readOnly
      ? [
          { key: 'discard', icon: <UndoOutlined />, label: '丢弃草稿', danger: true },
        ]
      : []),
    { type: 'divider' as const },
    {
      key: 'archive',
      icon: <DeleteOutlined />,
      label: '删除文章',
      danger: true,
    },
  ]

  const onMenu = ({ key }: { key: string }) => {
    if (key === 'relations') setRelationsOpen(true)
    else if (key === 'versions') {
      setPreview(null)
      setVersionsOpen(true)
      qc.invalidateQueries({ queryKey: ['versions', nodeId] })
    } else if (key === 'info') setInfoOpen(true)
    else if (key === 'copy') {
      void navigator.clipboard?.writeText(fullBody).then(
        () => message.success('已复制'),
        () => message.error('复制失败'),
      )
    } else if (key === 'discard') {
      modal.confirm({
        title: '丢弃草稿？',
        content:
          savedVer > 0
            ? `未定稿的改动会丢失，正文回到定稿 v${savedVer}。`
            : '当前还没有定稿，丢弃后正文为空。',
        okText: '丢弃',
        cancelText: '取消',
        okButtonProps: { danger: true },
        onOk: () => discardDraft.mutateAsync().catch(() => undefined),
      })
    } else if (key === 'archive') {
      modal.confirm({
        title: `删除《${node.title}》？`,
        content: '这篇文档及其子节点会从知识库移出，正文、版本和关系记录仍会保留。',
        okText: '删除',
        cancelText: '取消',
        okButtonProps: { danger: true },
        onOk: () => archive.mutateAsync().catch(() => undefined),
      })
    }
  }

  return (
    <div className="doc-work">
      <Outline bodyMd={body} />

      <div className="doc">
        <div className="doc-head">
          <h1 className="doc-title">{node.title}</h1>
        </div>

        {/* meta 行（`design.md §6.3`）：中文标签 ｜ 状态 ｜ 保存态 ｜ 右侧动作 */}
        <div className="doc-bar">
          <div className="doc-meta">
            <span className="doc-owner">
              <span className="doc-owner-avatar">W</span>
              我
            </span>
            <span className="doc-meta-sep">·</span>
            <span className={`doc-status ${statusClass(node.status)}`}>
              {node.medium ? MEDIUM_LABEL[node.medium] : KIND_LABEL[node.kind]} · {KIND_LABEL[node.kind]}
            </span>
            <span className="doc-meta-sep">｜</span>
            <span className="doc-ver">{savedVer > 0 ? `定稿 v${savedVer}` : '草稿'}</span>
            <span className="doc-meta-sep">｜</span>
            <span className="doc-saved">
              {readOnly
                ? '阅读模式'
                : saveDraft.isPending
                  ? '保存中…'
                  : dirty
                    ? '未保存'
                    : savedAt
                      ? `已保存 · ${fmtTime(savedAt)}`
                      : '已保存'}
            </span>
          </div>
          <span style={{ flex: 1 }} />
          <div className="doc-actions">
            <Button
              type="text"
              size="small"
              className="ai-accent"
              icon={<ThunderboltOutlined />}
              onClick={() => prefillAi(`为《${node.title}》写一篇完整 wiki 正文。`)}
            >
              让 Atlas 写
            </Button>
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
            <Dropdown menu={{ items: menuItems, onClick: onMenu }} trigger={['click']}>
              <Button type="text" size="small" aria-label="更多" icon={<MoreOutlined />} />
            </Dropdown>
          </div>
        </div>

        {/* 来源条（`design.md §6.3`） */}
        {activeLink && (
          <div className="doc-source">
            <LinkOutlined />
            <span>
              {SOURCE_NAME[activeLink.source]}《{activeLink.title_hint}》
            </span>
            <span className="doc-source-role">{activeLink.role === 'work' ? '正片' : '混杂位'}</span>
            {syncTarget && (
              <Tooltip title={syncTarget.message || undefined}>
                <span className={`doc-sync is-${syncTarget.status}`}>
                  {syncLabel(syncTarget.status, syncTarget.synced_ver)}
                  {syncTarget.message ? ` · ${syncTarget.message}` : ''}
                </span>
              </Tooltip>
            )}
            {canResync && (
              <Button
                type="link"
                size="small"
                icon={<SyncOutlined />}
                loading={sync.isPending}
                onClick={() => sync.mutate(activeLink.id)}
              >
                重新推送
              </Button>
            )}
          </div>
        )}
        {staleLink && (
          <Alert
            type="warning"
            showIcon
            style={{ margin: '0 32px 8px' }}
            message={`来源已失效：${SOURCE_NAME[staleLink.source]}《${staleLink.title_hint}》`}
            description="文档、正文和版本都还在；重新挂载只会替换来源链。"
            action={
              <Button size="small" onClick={() => setRelinkLink(staleLink)}>
                重新挂载
              </Button>
            }
          />
        )}

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
          <div className="doc-body-inner">
            {liveFor ? (
              <div className="doc-live" data-phase={live?.phase ?? ''}>
                <div className="doc-live-bar">
                  <ThunderboltOutlined />
                  <span>Atlas 正在写入{live?.phase ? ` · ${live.phase}` : ''}</span>
                </div>
                <LiveMarkdown body={liveFor.bodyMd} />
              </div>
            ) : (
              <>
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
                  onAiWrite={() => prefillAi(`为《${node.title}》写一篇完整 wiki 正文。`)}
                  onChange={(md) => setBody(normalizeMarkdown(md))}
                  transformPastedText={(text, { wholeDocSelected }) => {
                    const { header: h, rest } = splitDoc(text)
                    const hasDocHeader = h.titleLine || h.noteBlock || h.fence
                    if (!hasDocHeader) return text
                    // 空正文或 Ctrl+A 全选时，粘贴的是一整篇文章：文档头也一起换。
                    // 光标在正文中普通粘贴时不劫持，避免把局部内容误当成新文档头。
                    if (!wholeDocSelected && (body ?? '').trim() !== '') return text
                    setHeader(h)
                    return rest
                  }}
                />
              </>
            )}
          </div>
        </div>
      </div>

      <VersionsDrawer
        nodeId={nodeId}
        open={versionsOpen}
        preview={preview}
        onPreview={setPreview}
        onRestore={restoreVersion}
        onClose={() => setVersionsOpen(false)}
      />

      <Modal
        title="节点信息"
        open={infoOpen}
        onCancel={() => setInfoOpen(false)}
        footer={null}
      >
        <NodeInfo node={node} baseVer={baseVer} />
      </Modal>

      <RelinkModal
        nodeId={nodeId}
        link={relinkLink}
        onClose={() => setRelinkLink(null)}
      />
      <RelationsDrawer
        node={node}
        open={relationsOpen}
        onClose={() => setRelationsOpen(false)}
      />
    </div>
  )
}

function syncLabel(status: 'pending' | 'ok' | 'failed' | 'skipped', ver: number | null): string {
  if (status === 'ok') return ver ? `已反哺 v${ver}` : '已反哺'
  if (status === 'failed') return '反哺失败'
  if (status === 'skipped') return '已是最新'
  return '等待反哺'
}

function RelinkModal({
  nodeId,
  link,
  onClose,
}: {
  nodeId: string
  link: NodeLink | null
  onClose: () => void
}) {
  const qc = useQueryClient()
  const { message } = AntApp.useApp()
  const candidates = useQuery({
    queryKey: ['relink-candidates', link?.id],
    queryFn: () => api.relinkCandidates(link!.id),
    enabled: link !== null,
  })
  const relink = useMutation({
    mutationFn: (externalId: string) => api.relink(link!.id, externalId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['node-links', nodeId] })
      qc.invalidateQueries({ queryKey: ['node', nodeId] })
      qc.invalidateQueries({ queryKey: ['library-pool'] })
      qc.invalidateQueries({ queryKey: ['overview'] })
      message.success('已重新挂载，文档与正文未改动')
      onClose()
    },
    onError: (e) => message.error(`重挂失败：${e.message}`),
  })

  return (
    <Modal
      title={link ? `重新挂载《${link.title_hint}》` : '重新挂载'}
      open={link !== null}
      onCancel={onClose}
      footer={null}
      width={640}
    >
      <p className="relink-tip">
        候选来自当前清单，按原标题与别名的模糊相似度排序；确认后旧链保留为已替换。
      </p>
      {candidates.isLoading ? (
        <div className="relink-loading">
          <Spin />
        </div>
      ) : candidates.isError ? (
        <Alert type="error" showIcon message="候选没拉到" description={candidates.error.message} />
      ) : (candidates.data?.length ?? 0) === 0 ? (
        <Empty description="没找到相似条目。先重新扫描媒体库，或手动从池子建档。" />
      ) : (
        <div className="relink-list">
          {candidates.data!.map((c) => (
            <div className="relink-row" key={`${c.source}:${c.external_id}`}>
              <div className="relink-main">
                <div className="relink-title">
                  {c.title}
                  {c.year ? <span className="relink-year"> · {c.year}</span> : null}
                </div>
                <div className="relink-meta">
                  {c.container_title || '无系列'}
                  {c.aliases.length ? ` · 别名：${c.aliases.join('、')}` : ''}
                </div>
              </div>
              <span className="relink-score">相似度 {Math.round(c.score * 100)}%</span>
              <Button
                type="primary"
                size="small"
                loading={relink.isPending && relink.variables === c.external_id}
                onClick={() => relink.mutate(c.external_id)}
              >
                挂到这里
              </Button>
            </div>
          ))}
        </div>
      )}
    </Modal>
  )
}

const SYNTH_LABEL: Record<SynthesisType, string> = {
  role: '角色',
  worldbuilding: '世界观',
  timeline: '时间线',
  theme: '主题',
  creator: '创作者',
}

function RelationsDrawer({
  node,
  open,
  onClose,
}: {
  node: Node
  open: boolean
  onClose: () => void
}) {
  const qc = useQueryClient()
  const { message } = AntApp.useApp()
  const select = useUi((s) => s.select)
  const [target, setTarget] = useState<string>()
  const [relationType, setRelationType] = useState<string>()
  const [synthesisOpen, setSynthesisOpen] = useState(false)
  const [synthTitle, setSynthTitle] = useState('')
  const [synthType, setSynthType] = useState<SynthesisType>('role')
  const [synthParent, setSynthParent] = useState<string>()
  const [workIds, setWorkIds] = useState<string[]>([])

  const nodes = useQuery({ queryKey: ['tree'], queryFn: api.tree, enabled: open })
  const types = useQuery({ queryKey: ['relation-types'], queryFn: api.relationTypes, enabled: open })
  const relations = useQuery({
    queryKey: ['relations', node.id],
    queryFn: () => api.relations(node.id),
    enabled: open,
  })
  const synthesis = useQuery({ queryKey: ['synthesis'], queryFn: api.listSynthesis, enabled: open })

  const create = useMutation({
    mutationFn: () => api.createRelation(node.id, target!, relationType!),
    onSuccess: () => {
      setTarget(undefined)
      qc.invalidateQueries({ queryKey: ['relations', node.id] })
      message.success('已建立关系')
    },
    onError: (e) => message.error(`关系没建成：${e.message}`),
  })
  const remove = useMutation({
    mutationFn: (id: string) => api.deleteRelation(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['relations', node.id] })
      message.success('关系已删除（节点不受影响）')
    },
    onError: (e) => message.error(`删除失败：${e.message}`),
  })
  const makeSynthesis = useMutation({
    mutationFn: () =>
      api.createSynthesis({
        parent_id: synthParent ?? null,
        title: synthTitle.trim(),
        doc_type: synthType,
        work_ids: workIds,
      }),
    onSuccess: (n) => {
      qc.invalidateQueries({ queryKey: ['tree'] })
      qc.invalidateQueries({ queryKey: ['synthesis'] })
      setSynthesisOpen(false)
      setSynthTitle('')
      setWorkIds([])
      onClose()
      select(n.id)
      message.success(`已建综合页《${n.title}》`)
    },
    onError: (e) => message.error(`综合页没建成：${e.message}`),
  })
  const makeIndex = useMutation({
    mutationFn: () => api.syncIndex(node.id),
    onSuccess: (n) => {
      qc.invalidateQueries({ queryKey: ['tree'] })
      onClose()
      select(n.id)
      message.success('索引页已按当前子树更新')
    },
    onError: (e) => message.error(`索引更新失败：${e.message}`),
  })

  const incoming = relations.data?.filter((r) => r.direction === 'in') ?? []
  const outgoing = relations.data?.filter((r) => r.direction === 'out') ?? []
  const allNodes = nodes.data ?? []

  return (
    <Drawer
      title={`关系与综合 · ${node.title}`}
      width={680}
      open={open}
      onClose={onClose}
      extra={
        <Space>
          {node.kind === 'series' || node.kind === 'universe' ? (
            <Button loading={makeIndex.isPending} onClick={() => makeIndex.mutate()}>
              更新索引页
            </Button>
          ) : null}
          <Button type="primary" icon={<PlusOutlined />} onClick={() => setSynthesisOpen(true)}>
            新建综合页
          </Button>
        </Space>
      }
    >
      <div className="relation-graph">
        <div className="relation-col">
          <div className="relation-col-head">指向本页</div>
          {incoming.length === 0 ? (
            <span className="relation-empty">暂无</span>
          ) : (
            incoming.map((r) => (
              <button key={r.id} className="relation-node" onClick={() => jumpTo(r.other.id)}>
                <span>{r.label}</span>
                {r.other.title}
              </button>
            ))
          )}
        </div>
        <div className="relation-center">{node.title}</div>
        <div className="relation-col">
          <div className="relation-col-head">本页指向</div>
          {outgoing.length === 0 ? (
            <span className="relation-empty">暂无</span>
          ) : (
            outgoing.map((r) => (
              <button key={r.id} className="relation-node" onClick={() => jumpTo(r.other.id)}>
                <span>{r.label}</span>
                {r.other.title}
              </button>
            ))
          )}
        </div>
      </div>

      <Divider>添加关系</Divider>
      <div className="relation-add">
        <Select
          showSearch
          optionFilterProp="label"
          value={target}
          onChange={setTarget}
          placeholder="选另一端节点"
          options={allNodes
            .filter((n) => n.id !== node.id && n.status !== 'archived')
            .map((n) => ({ value: n.id, label: `${n.title}（${KIND_LABEL[n.kind]}）` }))}
        />
        <Select
          value={relationType}
          onChange={setRelationType}
          placeholder="关系类型"
          options={(types.data ?? []).map((t) => ({
            value: t.type,
            label: `${t.from_label} → ${t.to_label}`,
          }))}
        />
        <Button
          type="primary"
          disabled={!target || !relationType}
          loading={create.isPending}
          onClick={() => create.mutate()}
        >
          建立
        </Button>
      </div>

      <Divider>全部关系</Divider>
      <List
        size="small"
        loading={relations.isLoading}
        dataSource={relations.data ?? []}
        locale={{ emptyText: '还没有关系' }}
        renderItem={(r) => (
          <List.Item
            actions={[
              <Button
                key="del"
                type="text"
                size="small"
                danger
                icon={<DeleteOutlined />}
                onClick={() => remove.mutate(r.id)}
              />,
            ]}
          >
            <List.Item.Meta
              title={
                <span>
                  <span className="relation-label">{r.label}</span>
                  <a onClick={() => jumpTo(r.other.id)}>{r.other.title}</a>
                </span>
              }
              description={`${r.direction === 'out' ? '本页 → 对方' : '对方 → 本页'} · ${r.type}`}
            />
          </List.Item>
        )}
      />

      <Divider>综合页</Divider>
      <List
        size="small"
        loading={synthesis.isLoading}
        dataSource={synthesis.data ?? []}
        locale={{ emptyText: '还没有综合页' }}
        renderItem={(n) => (
          <List.Item onClick={() => jumpTo(n.id)} className="relation-clickable">
            <span>{n.title}</span>
            <span className="relation-empty">{n.tags.filter((t) => t !== '综合').join(' · ')}</span>
          </List.Item>
        )}
      />

      <Modal
        title="新建综合页"
        open={synthesisOpen}
        onCancel={() => setSynthesisOpen(false)}
        onOk={() => makeSynthesis.mutate()}
        okText="建立"
        confirmLoading={makeSynthesis.isPending}
        okButtonProps={{ disabled: !synthTitle.trim() || workIds.length === 0 }}
      >
        <div className="synthesis-form">
          <Input
            value={synthTitle}
            onChange={(e) => setSynthTitle(e.target.value)}
            placeholder="例：李逍遥、世界观：魂系、系列时间线"
          />
          <Select
            value={synthType}
            onChange={setSynthType}
            options={(Object.keys(SYNTH_LABEL) as SynthesisType[]).map((k) => ({
              value: k,
              label: SYNTH_LABEL[k],
            }))}
          />
          <Select
            allowClear
            value={synthParent}
            onChange={setSynthParent}
            placeholder="挂在哪个系列 / 宇宙（可选）"
            options={allNodes
              .filter((n) => n.kind === 'series' || n.kind === 'universe')
              .map((n) => ({ value: n.id, label: n.title }))}
          />
          <Select
            mode="multiple"
            showSearch
            optionFilterProp="label"
            value={workIds}
            onChange={setWorkIds}
            placeholder="引用哪些作品（可多选）"
            options={allNodes
              .filter((n) => n.kind === 'work')
              .map((n) => ({ value: n.id, label: n.title }))}
          />
          <div className="relation-empty">
            综合页会落在资料层，并自动建立到所选作品的引用关系。
          </div>
        </div>
      </Modal>
    </Drawer>
  )

  function jumpTo(id: string) {
    onClose()
    select(id)
  }
}

function VersionsDrawer({
  nodeId,
  open,
  preview,
  onPreview,
  onRestore,
  onClose,
}: {
  nodeId: string
  open: boolean
  preview: { ver: number; body_md: string } | null
  onPreview: (p: { ver: number; body_md: string } | null) => void
  onRestore: (ver: number) => void
  onClose: () => void
}) {
  const versions = useQuery({
    queryKey: ['versions', nodeId],
    queryFn: () => api.versions(nodeId),
    enabled: open,
  })
  const openPreview = async (ver: number) => {
    onPreview(await api.versionBody(nodeId, ver))
  }
  return (
    <Drawer title="版本历史" placement="right" width={520} open={open} onClose={onClose}>
      <List
        size="small"
        dataSource={versions.data ?? []}
        locale={{ emptyText: '还没有定稿快照' }}
        renderItem={(v) => (
          <List.Item
            actions={[
              <Button key="p" type="link" size="small" onClick={() => void openPreview(v.ver)}>
                预览
              </Button>,
              <Button key="r" type="link" size="small" onClick={() => onRestore(v.ver)}>
                恢复为草稿
              </Button>,
            ]}
          >
            <List.Item.Meta
              title={`v${v.ver}`}
              description={`${fmtTime(v.created_at)}${v.summary ? ` · ${v.summary}` : ''}`}
            />
          </List.Item>
        )}
      />
      {preview && (
        <div className="version-preview">
          <div className="version-preview-head">v{preview.ver} 预览</div>
          <pre>{preview.body_md}</pre>
        </div>
      )}
    </Drawer>
  )
}

function NodeInfo({ node, baseVer }: { node: import('../api').Node; baseVer: number }) {
  const rows = useMemo(
    () => [
      ['类型', KIND_LABEL[node.kind]],
      ['介质', node.medium ? MEDIUM_LABEL[node.medium] : '—'],
      ['状态', node.status === 'archived' ? '已归档' : node.status === 'ready' ? '定稿' : '草稿'],
      ['可见性', node.visibility === 'public' ? '公开' : '私有'],
      ['标签', node.tags.length ? node.tags.join('、') : '—'],
      ['别名', node.aliases.length ? node.aliases.join('、') : '—'],
      ['当前版本', `v${baseVer}`],
      ['节点 id', node.id],
    ],
    [node, baseVer],
  )
  return (
    <table className="node-info">
      <tbody>
        {rows.map(([k, v]) => (
          <tr key={k}>
            <td>{k}</td>
            <td>{v}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

function fmtTime(iso: string): string {
  const d = new Date(iso)
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

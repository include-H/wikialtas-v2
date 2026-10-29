import { useEffect, useState } from 'react'
import {
  App as AntApp,
  Alert,
  Button,
  Empty,
  Input,
  List,
  Modal,
  Skeleton,
  Tag,
} from 'antd'
import {
  CheckOutlined,
  CloseOutlined,
  ReloadOutlined,
  SafetyCertificateOutlined,
} from '@ant-design/icons'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import {
  api,
  type HealthFinding,
  type Suggestion,
  type SuggestionKind,
} from '../api'
import { useUi } from '../store'

const SUGGESTION_LABEL: Record<SuggestionKind, string> = {
  create: '建议建档',
  link: '建议关联',
  merge: '建议合并',
  research: '建议研究',
  ignore: '建议忽略',
  split: '建议拆页',
}

export default function AuditPanel() {
  const qc = useQueryClient()
  const { message } = AntApp.useApp()
  const select = useUi((s) => s.select)
  const setView = useUi((s) => s.setView)
  const [watching, setWatching] = useState(false)
  const [rejecting, setRejecting] = useState<Suggestion | null>(null)
  const [reason, setReason] = useState('')

  const suggestions = useQuery({
    queryKey: ['suggestions', 'pending'],
    queryFn: () => api.suggestions('pending'),
  })
  const findings = useQuery({
    queryKey: ['health-findings', 'open'],
    queryFn: () => api.healthFindings('open'),
    refetchInterval: watching ? 1200 : false,
  })

  useEffect(() => {
    if (!watching) return
    const timer = window.setTimeout(() => setWatching(false), 20_000)
    return () => window.clearTimeout(timer)
  }, [watching])

  const run = useMutation({
    mutationFn: api.runAudit,
    onSuccess: () => {
      setWatching(true)
      message.success('体检已入队')
    },
    onError: (e) => message.error(`体检没跑起来：${e.message}`),
  })
  const accept = useMutation({
    mutationFn: (s: Suggestion) => api.acceptSuggestion(s.id),
    onSuccess: (out) => {
      qc.invalidateQueries({ queryKey: ['suggestions'] })
      qc.invalidateQueries({ queryKey: ['tree'] })
      qc.invalidateQueries({ queryKey: ['health-findings'] })
      message.success(actionLabel(out.action))
    },
    onError: (e) => message.error(`接受失败：${e.message}`),
  })
  const reject = useMutation({
    mutationFn: ({ id, reason }: { id: string; reason: string }) =>
      api.rejectSuggestion(id, reason),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['suggestions'] })
      setRejecting(null)
      setReason('')
      message.success('已拒绝，理由会保留给后续判断')
    },
    onError: (e) => message.error(`拒绝失败：${e.message}`),
  })

  function openNode(id: string | null) {
    if (!id) return
    setView('doc')
    select(id)
  }

  return (
    <div className="audit">
      <div className="audit-inner">
        <div className="audit-head">
          <div>
            <h1>体检与 Review</h1>
            <p>确定性检查负责发现问题；有结构影响的动作留在这里由你裁决。</p>
          </div>
          <Button
            type="primary"
            icon={<SafetyCertificateOutlined />}
            loading={run.isPending}
            onClick={() => run.mutate()}
          >
            运行体检
          </Button>
        </div>

        <Alert
          type="info"
          showIcon
          message="体检只读扫描，不调用模型，也不会自动合并或删除任何节点。"
          style={{ marginBottom: 18 }}
        />

        <section className="audit-section">
          <div className="audit-section-head">
            <div>
              <h2>待裁决</h2>
              <span>{suggestions.data?.length ?? 0} 条建议</span>
            </div>
            <Button
              type="text"
              size="small"
              icon={<ReloadOutlined />}
              onClick={() => qc.invalidateQueries({ queryKey: ['suggestions'] })}
            />
          </div>
          {suggestions.isLoading ? (
            <Skeleton active />
          ) : suggestions.data?.length ? (
            <div className="review-list">
              {suggestions.data.map((s) => (
                <div className="review-row" key={s.id}>
                  <div className="review-kind">
                    <Tag>{SUGGESTION_LABEL[s.kind]}</Tag>
                    <span>{new Date(s.created_at).toLocaleString()}</span>
                  </div>
                  <div className="review-body">
                    <div className="review-title">{suggestionTitle(s)}</div>
                    <div className="review-why">{suggestionWhy(s)}</div>
                  </div>
                  <div className="review-actions">
                    {s.node_id ? (
                      <Button type="link" size="small" onClick={() => openNode(s.node_id)}>
                        打开节点
                      </Button>
                    ) : null}
                    <Button
                      size="small"
                      icon={<CheckOutlined />}
                      loading={accept.isPending && accept.variables?.id === s.id}
                      onClick={() => accept.mutate(s)}
                    >
                      接受
                    </Button>
                    <Button
                      size="small"
                      danger
                      icon={<CloseOutlined />}
                      onClick={() => {
                        setRejecting(s)
                        setReason('')
                      }}
                    >
                      拒绝
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <Empty description="没有待裁决建议" />
          )}
        </section>

        <section className="audit-section">
          <div className="audit-section-head">
            <div>
              <h2>体检发现</h2>
              <span>{findings.data?.length ?? 0} 项未解决；{watching ? '等待 worker…' : '当前结果'}</span>
            </div>
          </div>
          {findings.isLoading ? (
            <Skeleton active />
          ) : findings.data?.length ? (
            <List
              className="finding-list"
              dataSource={findings.data}
              renderItem={(f) => <FindingRow f={f} onOpen={openNode} />}
            />
          ) : (
            <Empty description={watching ? '正在体检…' : '还没有体检结果，点右上角运行一次'} />
          )}
        </section>
      </div>

      <Modal
        title="拒绝建议"
        open={rejecting !== null}
        onCancel={() => setRejecting(null)}
        onOk={() => reject.mutate({ id: rejecting!.id, reason })}
        okText="确认拒绝"
        okButtonProps={{ danger: true }}
        confirmLoading={reject.isPending}
      >
        <p className="review-why">拒绝理由会保留，后续同类判断会参考它。</p>
        <Input.TextArea
          rows={3}
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          placeholder="例如：来源不够硬 / 这不是同一部作品 / 已经有更合适的节点"
        />
      </Modal>
    </div>
  )
}

function FindingRow({
  f,
  onOpen,
}: {
  f: HealthFinding
  onOpen: (id: string | null) => void
}) {
  return (
    <List.Item
      className={`finding-row is-${f.severity}`}
      actions={
        f.node_id
          ? [<Button key="open" type="link" size="small" onClick={() => onOpen(f.node_id)}>打开</Button>]
          : undefined
      }
    >
      <List.Item.Meta
        title={
          <span>
            <Tag color={severityColor(f.severity)}>{severityLabel(f.severity)}</Tag>
            {findingLabel(f.kind)}
          </span>
        }
        description={findingDetail(f)}
      />
    </List.Item>
  )
}

function suggestionTitle(s: Suggestion): string {
  const title = s.payload.title
  if (typeof title === 'string') return title
  if (s.external_id) return s.external_id
  if (s.node_id) return `节点 ${s.node_id.slice(0, 8)}`
  return SUGGESTION_LABEL[s.kind]
}

function suggestionWhy(s: Suggestion): string {
  const reason = s.payload.reason ?? s.payload.why ?? s.payload.how
  if (typeof reason === 'string' && reason.trim()) return reason
  return `由 ${s.created_by} 提出；接受后会执行对应动作。`
}

function actionLabel(action: string): string {
  if (action.startsWith('created:')) return '已接受并创建节点'
  if (action === 'linked') return '已接受并挂上媒体链接'
  if (action === 'ignored') return '已接受并加入忽略列表'
  return '已接受；需要人工完成结构操作'
}

function severityLabel(s: HealthFinding['severity']): string {
  return s === 'error' ? '错误' : s === 'warn' ? '提醒' : '信息'
}

function severityColor(s: HealthFinding['severity']): string {
  return s === 'error' ? 'error' : s === 'warn' ? 'warning' : 'default'
}

function findingLabel(kind: string): string {
  const labels: Record<string, string> = {
    stale_link: '来源链失效',
    sync_not_latest: '定稿未反哺到最新版',
    ready_without_content: '标记定稿但没有正文',
    isolated_node: '孤立节点',
    duplicate_title: '重复标题',
    stale_draft: '草稿放置过久',
    unlinked_library: '库里有未建档条目',
  }
  return labels[kind] ?? kind
}

function findingDetail(f: HealthFinding): string {
  const d = f.detail
  if (f.kind === 'stale_link') return `《${String(d.title ?? '')}》已从清单消失，可重新挂载或解链。`
  if (f.kind === 'sync_not_latest') {
    return `${String(d.source ?? '')} 当前定稿 v${String(d.current_ver ?? '?')}，已反哺 v${String(d.synced_ver ?? '—')}（${String(d.status ?? '')}）。`
  }
  if (f.kind === 'ready_without_content') return '节点状态是定稿，但正文为空。'
  if (f.kind === 'isolated_node') return `《${String(d.title ?? '')}》没有任何关系边。`
  if (f.kind === 'duplicate_title') return `归一化标题「${String(d.title_norm ?? '')}」有 ${String(d.count ?? '')} 个节点。`
  if (f.kind === 'stale_draft') return `《${String(d.title ?? '')}》的草稿超过 24 小时未定稿。`
  if (f.kind === 'unlinked_library') return `${String(d.source ?? '')} 有 ${String(d.count ?? '')} 条尚未建档。`
  return JSON.stringify(d)
}

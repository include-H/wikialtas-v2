import { useState } from 'react'
import { Button, Dropdown, Input, Modal, Tag, TextArea, Typography } from '@douyinfe/semi-ui'
import { IconMore } from '@douyinfe/semi-icons'
import {
  FORMAT_LABEL,
  KIND_LABEL,
  ROLE_LABEL,
  SOURCE_LABEL,
  confColor,
} from '../../lib/mediaTags'
import type { NodePendingItem } from '../../types'
import type { LibraryPanelProps } from './RelatedMediaPanel'

const { Text } = Typography

/** 检索入口:空白 = 按节点标题检索;填词 = 附加检索词(预告/OST/原名…)。 */
function SearchControl({ busy, primaryLabel, onSubmit }: { busy: boolean; primaryLabel: string; onSubmit: (term: string) => void }) {
  const [term, setTerm] = useState('')
  const submit = () => {
    if (!busy) onSubmit(term)
  }
  return (
    <span className="lm-search">
      <Input
        size="small"
        className="lm-search-input"
        placeholder="检索词(可选)"
        value={term}
        disabled={busy}
        onChange={v => setTerm(typeof v === 'string' ? v : '')}
        onEnterPress={submit}
        showClear
      />
      <Button size="small" loading={busy} onClick={submit}>
        {busy ? '检索中' : primaryLabel}
      </Button>
    </span>
  )
}

/**
 * ③ 待确认:扫库 / 「AI 对一遍」产出的建议(LLM 只建议,人点确认才挂链)。
 * 有内容时整段浮到面板最上(写完看结果的主场景);空时收成一条细条,
 * 保留「检索媒体库」入口与源未配置提示。
 *
 * 行动词表随建议动作:
 *   link —— 挂到本节点(按钮「关联」)
 *   archive —— 条目属于同系列另一部作品:在本节点下建独立条目条挂它(「建档」)
 * 忽略是 per-node 的(不再向这个节点提起),与池子的全局忽略互不影响。
 */
export default function LibraryPending(p: LibraryPanelProps) {
  const { data, busyKey, watching, reload } = p
  const pending = data.pending
  const dismissed = data.dismissed
  const [showDismissed, setShowDismissed] = useState(false)
  // 忽略 = 「不再向这个节点提起」，可附理由（有当儿的会作为反馈喂给「AI 对一遍」）。
  const [dismissTarget, setDismissTarget] = useState<NodePendingItem | null>(null)
  const [dismissReason, setDismissReason] = useState('')
  const [dismissSending, setDismissSending] = useState(false)

  const submitDismiss = async () => {
    if (!dismissTarget || dismissSending) return
    setDismissSending(true)
    try {
      await p.dismissPending(dismissTarget, dismissReason.trim())
      setDismissTarget(null)
      setDismissReason('')
    } finally {
      setDismissSending(false)
    }
  }

  const scannning = busyKey === '__rescan'
  const highConf = pending.filter((s) => s.action === 'link' && s.confidence >= 0.8)

  const linkAllHigh = async () => {
    if (!highConf.length) return
    let ok = 0
    for (const s of highConf) {
      try {
        await p.linkPending(s)
        ok++
      } catch {
        // 单条失败不影响其余;逐条 toast 已在 linkPending 里
      }
    }
    if (ok > 0) void reload()
  }

  const unconfigured = data.sources.filter((s) => s.status !== 'ok' && s.message)

  // —— 细条态:没待确认、没忽略记录 ——
  if (pending.length === 0 && dismissed.length === 0) {
    return (
      <div className="lm-slim">
        <Text type="tertiary" size="small">
          {watching ? '写完啦,正在媒体库里找它的条目…' : '没有待确认的库条目'}
        </Text>
        <SearchControl busy={scannning} primaryLabel="检索媒体库" onSubmit={t => void p.rescan(t)} />
      </div>
    )
  }

  return (
    <div className="lm-card">
      <div className="lm-card-head">
        <Text strong>待确认</Text>
        <span className="lm-count">{pending.length}</span>
        <Text type="tertiary" size="small" style={{ flex: 1 }}>
          {scannning
            ? '正在检索媒体库…'
            : watching
              ? '写完啦,正在媒体库里找它的条目…'
              : `'${data.work.title}'在媒体库里的条目(确认后才关联)`}
        </Text>
        <SearchControl busy={scannning} primaryLabel="检索" onSubmit={t => void p.rescan(t)} />
        {highConf.length > 1 && (
          <Button size="small" theme="solid" type="primary" onClick={() => void linkAllHigh()}>
            关联全部高置信
          </Button>
        )}
      </div>
      {unconfigured.length > 0 && (
        <div className="lm-hints">
          {unconfigured.map((s) => (
            <Text key={s.source} type="warning" size="small">
              {SOURCE_LABEL[s.source] ?? s.source}:{s.message}
            </Text>
          ))}
        </div>
      )}
      <div className="lm-list">
        {pending.map((s) => (
          <PendingRow key={s.key} s={s} busy={busyKey === s.key} onDismissRequest={setDismissTarget} {...p} />
        ))}
      </div>
      {dismissed.length > 0 && (
        <div className="lm-dismissed">
          <a className="lm-toggle" onClick={() => setShowDismissed((v) => !v)}>
            已忽略 {dismissed.length} 条{showDismissed ? '(收起)' : ''}
          </a>
          {showDismissed &&
            dismissed.map((d) => (
              <span className="lm-dismissed-row" key={d.key}>
                <Text type="tertiary" size="small">
                  {d.title || d.key}
                  {d.reason ? ` · 「${d.reason}」` : ''}
                </Text>
                <a className="lm-toggle" onClick={() => void p.restoreDismissed(d.key)}>
                  {busyKey === d.key ? '恢复中…' : '恢复'}
                </a>
              </span>
            ))}
        </div>
      )}
      <Modal
        title="忽略这条建议"
        visible={!!dismissTarget}
        onOk={() => void submitDismiss()}
        onCancel={() => setDismissTarget(null)}
        okText="忽略"
        cancelText="取消"
        okButtonProps={{ loading: dismissSending }}
        width={480}
      >
        <div className="reject-modal">
          <Text type="tertiary" size="small">
            不再向「{p.data.work.title}」提起「{dismissTarget?.title ?? ''}」。可以顺便写下理由——有当儿的会作为反馈，
            让「AI 对一遍」下次换别的归处再提。
          </Text>
          <TextArea
            value={dismissReason}
            onChange={(v) => setDismissReason(typeof v === 'string' ? v : '')}
            autosize={{ minRows: 2, maxRows: 4 }}
            maxLength={200}
            showCounter
            placeholder="可选，例如：这不是本作的解析，是别的作品的"
            style={{ marginTop: 8 }}
          />
        </div>
      </Modal>
    </div>
  )

}

function PendingRow({
  s,
  busy,
  linkPending,
  archivePending,
  onDismissRequest,
}: {
  s: NodePendingItem
  busy: boolean
  linkPending: (s: NodePendingItem) => void
  archivePending: (s: NodePendingItem) => void
  onDismissRequest: (s: NodePendingItem) => void
}) {
  return (
    <div className="lm-row lm-row-pending">
      <span className="lm-row-cover">
        {s.coverImage ? <img src={s.coverImage} alt="" loading="lazy" /> : null}
      </span>
      <span className="lm-row-meta">
        <span className="lm-row-title" title={s.title}>
          {s.title}
        </span>
        <span className="lm-row-sub">
          <Text type="tertiary" size="small">
            <Tag size="small" color={confColor(s.confidence)} style={{ marginRight: 6 }}>
              {Math.round(s.confidence * 100)}%
            </Tag>
            <Tag size="small" color={s.role === 'mix' ? 'grey' : 'yellow'} style={{ marginRight: 6 }}>
              {ROLE_LABEL[s.role] ?? s.role}
            </Tag>
            <Tag size="small" style={{ marginRight: 6 }}>
              {SOURCE_LABEL[s.source] ?? s.source}
            </Tag>
            {s.kind ? (
              <Tag size="small" color="blue" style={{ marginRight: 6 }}>
                {KIND_LABEL[s.kind] ?? s.kind}
              </Tag>
            ) : null}
            {s.format ? (
              <Tag size="small" color="cyan" style={{ marginRight: 6 }}>
                {FORMAT_LABEL[s.format] ?? s.format}
              </Tag>
            ) : null}
            {s.reason}
            {s.extra ? ` · ${s.extra}` : ''}
          </Text>
        </span>
      </span>
      <Dropdown
        position="bottomRight"
        trigger="click"
        render={
          <Dropdown.Menu>
            {s.url ? (
              <Dropdown.Item onClick={() => window.open(s.url, '_blank', 'noreferrer')}>
                在库中打开
              </Dropdown.Item>
            ) : null}
            <Dropdown.Item onClick={() => onDismissRequest(s)}>忽略</Dropdown.Item>
          </Dropdown.Menu>
        }
      >
        <Button theme="borderless" icon={<IconMore />} className="lm-row-more" type="tertiary" title="更多操作" />
      </Dropdown>
      {s.action === 'archive' ? (
        <Button
          size="small"
          theme="solid"
          type="primary"
          loading={busy}
          onClick={() => void archivePending(s)}
        >
          建档
        </Button>
      ) : (
        <Button
          size="small"
          theme="solid"
          type="primary"
          loading={busy}
          onClick={() => void linkPending(s)}
        >
          关联
        </Button>
      )}
    </div>
  )
}

import { useEffect, useRef, useState } from 'react'
import { App as AntApp, Button, Input, Tag, Typography } from 'antd'
import { PlusOutlined, SendOutlined, ThunderboltOutlined } from '@ant-design/icons'
import { useQuery, useQueryClient } from '@tanstack/react-query'

import { api, ApiError } from '../api'
import { useUi } from '../store'

/**
 * 右栏 = Altas（v1 的 480px 侧栏，`archive/v1/VISUAL_SPEC.md` §1.5）。
 *
 * M3：接上真 run——`POST /api/runs` 入队，`GET /api/runs/{id}/events` 收 SSE。
 * **正文流只做展示**：真正的草稿在服务端（`content_drafts`），`content.delta` 只是加速通道
 * （`docs/api.md §4.3`：断线一律从服务端草稿重新基线）。所以流一结束就刷节点。
 */
export default function AiPanel() {
  const qc = useQueryClient()
  const { message } = AntApp.useApp()
  const selectedId = useUi((s) => s.selectedId)
  const tree = useQuery({ queryKey: ['tree'], queryFn: api.tree })
  const context = (tree.data ?? []).find((n) => n.id === selectedId)?.title ?? ''

  const [prompt, setPrompt] = useState('')
  const [busy, setBusy] = useState(false)
  const [live, setLive] = useState('')
  const [say, setSay] = useState('')
  const [status, setStatus] = useState<string>('')
  const esRef = useRef<EventSource | null>(null)

  // 换节点就把这一轮的展示清掉（草稿归服务端管，这里只是"这一轮"）
  useEffect(() => {
    setLive('')
    setSay('')
    setStatus('')
  }, [selectedId])

  useEffect(() => () => esRef.current?.close(), [])

  const send = async () => {
    if (!selectedId || !prompt.trim() || busy) return
    setBusy(true)
    setLive('')
    setSay('')
    setStatus('排队中…')
    try {
      const { runId } = await api.createRun({ node_id: selectedId, prompt: prompt.trim() })
      setPrompt('')
      const es = new EventSource(`/api/runs/${encodeURIComponent(runId)}/events`)
      esRef.current?.close()
      esRef.current = es
      es.onmessage = () => {}
      // 按事件名订阅（后端把 type 同时写进 event 与 payload.type）
      const on = (name: string, fn: (d: any) => void) =>
        es.addEventListener(name, (e) => fn(JSON.parse((e as MessageEvent).data)))
      on('wikiatlas.content.delta', (d) => setLive(d.payload?.body_md ?? ''))
      on('response.output_text.delta', (d) => setSay((s) => s + (d.payload?.delta ?? '')))
      on('wikiatlas.tool.result', (d) => setStatus(d.payload?.ok ? '正文已落草稿' : `写入被拒：${d.payload?.reason ?? ''}`))
      on('wikiatlas.run.end', () => {
        setStatus('完成')
        setBusy(false)
        es.close()
        qc.invalidateQueries({ queryKey: ['node', selectedId] })
      })
      es.onerror = () => {
        // 服务端收工会关流；真错就提示一次（重连交给浏览器）
        if (es.readyState === EventSource.CLOSED) {
          setBusy(false)
          qc.invalidateQueries({ queryKey: ['node', selectedId] })
        }
      }
    } catch (e) {
      setBusy(false)
      setStatus('')
      message.error(e instanceof ApiError ? e.message : '起工单失败')
    }
  }

  return (
    <div className="ai-panel">
      <div className="ai-head">
        <span className="ai-mark">
          <ThunderboltOutlined />
        </span>
        <span className="ai-title">Altas</span>
        {context && <span className="ai-context">{context}</span>}
        <span style={{ flex: 1 }} />
        {status && <Tag color={busy ? 'processing' : 'default'}>{status}</Tag>}
        <Button type="text" size="small" icon={<PlusOutlined />} disabled aria-label="新对话" />
      </div>

      <div className="ai-body">
        {!selectedId && (
          <div className="ai-empty">
            <Typography.Text type="secondary">先在左边选一个节点</Typography.Text>
          </div>
        )}
        {selectedId && !live && !say && !busy && (
          <div className="ai-empty">
            <Typography.Text type="secondary">今天有什么工作需要处理？</Typography.Text>
            <div style={{ marginTop: 6, fontSize: 12, color: 'var(--ant-color-text-quaternary)' }}>
              让它写正文、改某一节、或者问它要资料
            </div>
          </div>
        )}
        {say && <div className="ai-say">{say}</div>}
        {live && (
          <div className="ai-live">
            <div className="ai-live-head">正在写（草稿实时落库）</div>
            <pre className="ai-live-body">{live}</pre>
          </div>
        )}
      </div>

      <div className="ai-foot">
        <Input.TextArea
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          autoSize={{ minRows: 2, maxRows: 6 }}
          placeholder="让它写正文 / 改某一节 / 问它要资料…（Enter 发送，Shift+Enter 换行）"
          disabled={!selectedId || busy}
          onPressEnter={(e) => {
            if (!e.shiftKey) {
              e.preventDefault()
              void send()
            }
          }}
        />
        <div className="ai-foot-actions">
          <span className="ai-foot-hint">{busy ? 'Altas 正在干活…' : '技能 / 资料随后接'}</span>
          <Button
            type="primary"
            size="small"
            icon={<SendOutlined />}
            loading={busy}
            disabled={!selectedId || !prompt.trim()}
            onClick={() => void send()}
          >
            发送
          </Button>
        </div>
      </div>
    </div>
  )
}

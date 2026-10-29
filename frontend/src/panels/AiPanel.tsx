import { useEffect, useMemo, useRef, useState } from 'react'
import { App as AntApp, Button, Dropdown, Input, Modal, Select, Tag, Tooltip } from 'antd'
import {
  BookOutlined,
  BulbOutlined,
  CloseOutlined,
  EditOutlined,
  FileSearchOutlined,
  HistoryOutlined,
  PlusOutlined,
  SaveOutlined,
  SearchOutlined,
  SendOutlined,
  ThunderboltOutlined,
  ToolOutlined,
} from '@ant-design/icons'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { api, ApiError } from '../api'
import { useUi } from '../store'
import LiveMarkdown from './LiveMarkdown'

/**
 * 右栏 = Atlas（v1 的 480px 侧栏，`archive/v1/VISUAL_SPEC.md` §1.5）。
 *
 * **一个通用 harness**（D71）：只有一个输入框、一套工具面。用户在里头发一句话，
 * Atlas 自己判断是查库回答、还是动手写——没有「写 / 问」开关（对齐 OpenCode / Codex / dsh）。
 * 前端只做展示：工具行、阶段、账本/质量门、回答卡片；正文的真相仍在服务端草稿（D46）。
 */
interface AnswerRef {
  node_id: string
  title: string
  kind: string
}

export default function AiPanel({ onClose }: { onClose?: () => void } = {}) {
  const qc = useQueryClient()
  const { message } = AntApp.useApp()
  const selectedId = useUi((s) => s.selectedId)
  const docMode = useUi((s) => s.docMode)
  const select = useUi((s) => s.select)
  const setView = useUi((s) => s.setView)
  const prefill = useUi((s) => s.aiPrefill)
  const clearPrefill = useUi((s) => s.clearAiPrefill)
  const setLiveWrite = useUi((s) => s.setLiveWrite)
  const tree = useQuery({ queryKey: ['tree'], queryFn: api.tree })
  const context = (tree.data ?? []).find((n) => n.id === selectedId)?.title ?? ''

  const [prompt, setPrompt] = useState('')
  const [busy, setBusy] = useState(false)
  const [live, setLive] = useState('')
  const [say, setSay] = useState('')
  const [status, setStatus] = useState<string>('')
  const [phase, setPhase] = useState('')
  const [ledger, setLedger] = useState<{ coverage: number; missing: string[] } | null>(null)
  const [quality, setQuality] = useState<string>('')
  const [reasoning, setReasoning] = useState('')
  const [reasoningUnavailable, setReasoningUnavailable] = useState(false)
  const [thinkingOpen, setThinkingOpen] = useState(true)
  const [answer, setAnswer] = useState<{ text: string; refs: AnswerRef[] } | null>(null)
  const [saving, setSaving] = useState(false)
  const [saveTitle, setSaveTitle] = useState('')
  const [steps, setSteps] = useState<string[]>([])
  const [loadedSkill, setLoadedSkill] = useState(
    () => localStorage.getItem('atlas.skill') ?? 'atlas',
  )
  const [sessionId, setSessionId] = useState<string | null>(null)
  const [sentPrompt, setSentPrompt] = useState('')
  const esRef = useRef<EventSource | null>(null)
  const targetRef = useRef<string | null>(null)
  const reasoningRef = useRef('')
  const sessions = useQuery({
    queryKey: ['sessions', selectedId],
    queryFn: () => api.sessions(selectedId!),
    enabled: !!selectedId,
  })
  const sessionDetail = useQuery({
    queryKey: ['session-messages', sessionId],
    queryFn: () => api.sessionMessages(sessionId!),
    enabled: !!sessionId,
  })
  const skills = useQuery({ queryKey: ['skills'], queryFn: api.skills })
  const skillOptions = useMemo(() => {
    const groups = new Map<string, { value: string; label: string; title: string }[]>()
    for (const skill of skills.data?.skills ?? []) {
      const rows = groups.get(skill.group) ?? []
      rows.push({
        value: skill.name,
        label: skill.name,
        title: `${skill.name} ${skill.description}`,
      })
      groups.set(skill.group, rows)
    }
    return [...groups].map(([label, options]) => ({ label, options }))
  }, [skills.data])

  useEffect(() => {
    if (!loadedSkill || !skills.data) return
    if (!skills.data.skills.some((s) => s.name === loadedSkill)) {
      const fallback = skills.data.skills.find((s) => s.layer === 0)?.name ?? ''
      setLoadedSkill(fallback)
      if (fallback) localStorage.setItem('atlas.skill', fallback)
      else localStorage.removeItem('atlas.skill')
    }
  }, [loadedSkill, skills.data])

  // 「让 Atlas 写」把一句话投进来（`design.md §6.3`）
  useEffect(() => {
    if (prefill) {
      setPrompt(prefill)
      clearPrefill()
    }
  }, [prefill, clearPrefill])

  // 换节点就把这一轮的展示清掉（草稿归服务端管，这里只是"这一轮"）
  useEffect(() => {
    setLive('')
    setSay('')
    setStatus('')
    setPhase('')
    setLedger(null)
    setQuality('')
    setReasoning('')
    setReasoningUnavailable(false)
    reasoningRef.current = ''
    setAnswer(null)
    setSteps([])
    setSentPrompt('')
    setLiveWrite(null)
  }, [selectedId, setLiveWrite])

  useEffect(() => {
    if (!selectedId || !sessions.data) {
      if (!selectedId) setSessionId(null)
      return
    }
    const key = `wikiatlas.session.${selectedId}`
    const saved = localStorage.getItem(key)
    const next =
      sessions.data.find((s) => s.id === saved)?.id ??
      sessions.data[0]?.id ??
      null
    setSessionId(next)
    if (next) localStorage.setItem(key, next)
  }, [selectedId, sessions.data])

  useEffect(() => {
    setAnswer(null)
    setSay('')
    setPhase('')
    setLedger(null)
    setQuality('')
    setSteps([])
  }, [sessionId])

  useEffect(() => () => esRef.current?.close(), [])

  const reset = () => {
    setLive('')
    setSay('')
    setStatus('')
    setPhase('')
    setLedger(null)
    setQuality('')
    setReasoning('')
    setReasoningUnavailable(false)
    reasoningRef.current = ''
    setAnswer(null)
    setSteps([])
    setThinkingOpen(true)
  }

  const send = async (text?: string) => {
    const content = (text ?? prompt).trim()
    if (!selectedId || !content || busy) return
    targetRef.current = selectedId
    setBusy(true)
    reset()
    setStatus('排队中…')
    setSentPrompt(content)
    try {
      const { runId, sessionId: returnedSession } = await api.createRun({
        node_id: selectedId,
        prompt: content,
        session_id: sessionId ?? undefined,
        context: {
          doc_mode: docMode,
          one_shot: false,
          skills: loadedSkill ? [loadedSkill] : [],
        },
      })
      if (returnedSession) {
        setSessionId(returnedSession)
        localStorage.setItem(`wikiatlas.session.${selectedId}`, returnedSession)
      }
      setPrompt('')
      const es = new EventSource(`/api/runs/${encodeURIComponent(runId)}/events`)
      esRef.current?.close()
      esRef.current = es
      const on = (name: string, fn: (d: any) => void) =>
        es.addEventListener(name, (e) => fn(JSON.parse((e as MessageEvent).data)))
      on('wikiatlas.content.delta', (d) => {
        const md = d.payload?.body_md ?? ''
        setLive(md)
        if (targetRef.current) setLiveWrite({ nodeId: targetRef.current, bodyMd: md, phase: '' })
      })
      on('response.output_text.delta', (d) => setSay((s) => s + (d.payload?.delta ?? '')))
      on('response.reasoning_summary_text.delta', (d) => {
        const delta = d.payload?.delta ?? ''
        reasoningRef.current += delta
        setReasoning(reasoningRef.current)
        if (delta) setReasoningUnavailable(false)
      })
      on('wikiatlas.phase', (d) => {
        const label = d.payload?.label ?? d.payload?.phase ?? ''
        setPhase(label)
        const cur = useUi.getState().liveWrite
        if (cur && label) setLiveWrite({ ...cur, phase: label })
      })
      on('wikiatlas.ledger', (d) =>
        setLedger({ coverage: d.payload?.coverage ?? 0, missing: d.payload?.missing ?? [] }),
      )
      on('wikiatlas.quality', (d) => {
        const issues = (d.payload?.issues ?? []).filter((i: any) => i.hard)
        setQuality(issues.length === 0 ? '质量门：通过' : `质量门：${issues.length} 项待改`)
      })
      on('wikiatlas.tool.result', (d) => {
        setStatus(d.payload?.ok ? '' : `工具被拒：${d.payload?.reason ?? ''}`)
        const s = d.payload?.summary ?? d.payload?.name
        if (s) setSteps((t) => [...t, s])
      })
      on('wikiatlas.skill.loaded', (d) => {
        const names = (d.payload?.skills ?? []).filter(Boolean)
        if (names.length) setSteps((t) => [...t, `加载 Skill · ${names.join('、')}`])
      })
      on('wikiatlas.answer', (d) => {
        setAnswer({ text: d.payload?.text ?? '', refs: d.payload?.refs ?? [] })
        setSay('')
        setBusy(false)
      })
      on('wikiatlas.notice', (d) => setSay((s) => (s ? s + '\n' : '') + (d.payload?.text ?? '')))
      on('wikiatlas.run.end', () => {
        setReasoningUnavailable(reasoningRef.current.length === 0)
        setStatus('完成')
        setBusy(false)
        setSentPrompt('')
        setAnswer(null)
        setSay('')
        es.close()
        setLiveWrite(null)
        qc.invalidateQueries({ queryKey: ['node', selectedId] })
        qc.invalidateQueries({ queryKey: ['tree'] })
        qc.invalidateQueries({ queryKey: ['overview'] })
        qc.invalidateQueries({ queryKey: ['sessions', selectedId] })
        if (returnedSession) {
          qc.invalidateQueries({ queryKey: ['session-messages', returnedSession] })
        }
      })
      es.onerror = () => {
        if (es.readyState === EventSource.CLOSED) {
          setBusy(false)
          setLiveWrite(null)
          qc.invalidateQueries({ queryKey: ['node', selectedId] })
          qc.invalidateQueries({ queryKey: ['tree'] })
        }
      }
    } catch (e) {
      setBusy(false)
      setStatus('')
      message.error(e instanceof ApiError ? e.message : '起工单失败')
    }
  }

  const saveAnswer = useMutation({
    mutationFn: () =>
      api.saveAnswer({
        parent_id: selectedId,
        title: saveTitle.trim(),
        body_md: answer!.text,
        ref_ids: answer!.refs.map((r) => r.node_id),
      }),
    onSuccess: (n) => {
      setSaving(false)
      qc.invalidateQueries({ queryKey: ['tree'] })
      qc.invalidateQueries({ queryKey: ['overview'] })
      message.success(`已存为资料《${n.title}》`)
    },
    onError: (e) => message.error(`没存上：${e.message}`),
  })

  const modeMeta = {
    edit: { label: '编辑模式', hint: '可以问，也可以直接让它改' },
    proofread: { label: '校对模式', hint: '默认找错，明确要求才修改' },
    read: { label: '只读模式', hint: '只查资料和回答，不写库' },
  }[docMode]

  const starters = [
    {
      icon: <EditOutlined />,
      label: '写这篇的 Wiki',
      desc: '研究资料、搭骨架、分章落草稿',
      text: '为这篇写一篇完整 wiki 正文。',
    },
    {
      icon: <FileSearchOutlined />,
      label: '这部讲什么？',
      desc: '只依据库内内容，概括作品与看点',
      text: '这部作品讲的是什么？用几句话概括。',
    },
    {
      icon: <SearchOutlined />,
      label: '校对这篇文档',
      desc: '检查事实、结构、措辞和格式问题',
      text: '认真校对这篇文档，先列出需要修改的问题。',
    },
    {
      icon: <BookOutlined />,
      label: '库里有哪些作品？',
      desc: '浏览当前文档库与最近更新',
      text: '资料库里现在有哪些作品？列一下。',
    },
  ]

  const jump = (id: string) => {
    select(id)
    setView('doc')
  }

  const historyMessages = sessionDetail.data?.messages ?? []
  const hasHistory = historyMessages.length > 0

  async function newSession() {
    if (!selectedId || busy) return
    const s = await api.createSession(selectedId)
    localStorage.setItem(`wikiatlas.session.${selectedId}`, s.id)
    setSessionId(s.id)
    qc.invalidateQueries({ queryKey: ['sessions', selectedId] })
    reset()
  }

  const historyMenu = {
    items: [
      {
        key: 'new',
        icon: <PlusOutlined />,
        label: '新对话',
        disabled: busy,
      },
      { type: 'divider' as const },
      ...(sessions.data ?? []).map((s) => ({
        key: s.id,
        label: (
          <span className="ai-session-option">
            <strong>{s.title}</strong>
            <small>{s.preview || `${s.message_count} 条消息`}</small>
          </span>
        ),
      })),
    ],
    onClick: ({ key }: { key: string }) => {
      if (key === 'new') {
        void newSession()
        return
      }
      localStorage.setItem(`wikiatlas.session.${selectedId}`, key)
      setSessionId(key)
      reset()
    },
  }

  return (
    <div className="ai-panel">
      <div className="ai-head">
        <span className="ai-mark">
          <ThunderboltOutlined />
        </span>
        <span className="ai-head-main">
          <span className="ai-title">Atlas</span>
          {context && <span className="ai-context">{context}</span>}
        </span>
        <span style={{ flex: 1 }} />
        {status && <Tag color={busy ? 'processing' : 'default'}>{status}</Tag>}
        <Dropdown menu={historyMenu} trigger={['click']} disabled={!selectedId}>
          <Tooltip title="会话">
            <Button
              type="text"
              size="small"
              aria-label="会话"
              icon={<HistoryOutlined />}
              disabled={!selectedId}
            />
          </Tooltip>
        </Dropdown>
        {onClose && (
          <Button
            type="text"
            size="small"
            aria-label="关闭 Atlas"
            icon={<CloseOutlined />}
            onClick={onClose}
          />
        )}
      </div>

      <div className="ai-body">
        {!selectedId && (
          <div className="ai-empty">
            <span className="ai-empty-orb">
              <ThunderboltOutlined />
            </span>
            <span className="ai-empty-kicker">ATLAS · LIBRARY AGENT</span>
            <h2>先从左边选一篇文档</h2>
            <p>选好后可以问它、查库、校对，或直接让它修改正文。</p>
          </div>
        )}
        {selectedId && !live && !say && !busy && !answer && !hasHistory && (
          <div className="ai-empty">
            <span className="ai-empty-orb">
              <ThunderboltOutlined />
            </span>
            <span className="ai-empty-kicker">ATLAS · {modeMeta.label}</span>
            <h2>今天想让 Atlas 做什么？</h2>
            <p>{modeMeta.hint}。不用切换问答或撰写，直接说你想要什么。</p>
            <div className="ai-starters">
              {starters.map((s) => (
                <button
                  key={s.label}
                  type="button"
                  className="ai-starter-card"
                  onClick={() => void send(s.text)}
                >
                  <span className="ai-starter-icon">{s.icon}</span>
                  <span className="ai-starter-copy">
                    <strong>{s.label}</strong>
                    <small>{s.desc}</small>
                  </span>
                  <span className="ai-starter-arrow">↗</span>
                </button>
              ))}
            </div>
            <div className="ai-context-hint">
              <span>当前文档</span>
              <strong>{context}</strong>
            </div>
          </div>
        )}
        {historyMessages.length > 0 && (
          <div className="ai-thread">
            {historyMessages.map((m) => (
              <div key={m.id} className={`ai-thread-message is-${m.role}`}>
                <span className="ai-thread-role">{m.role === 'user' ? '我' : 'Atlas'}</span>
                <div className="ai-thread-bubble">
                  {m.role === 'assistant' ? (
                    <LiveMarkdown body={m.content} live={false} />
                  ) : (
                    m.content
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
        {sentPrompt && (
          <div className="ai-thread-message is-user is-current">
            <span className="ai-thread-role">我</span>
            <div className="ai-thread-bubble">{sentPrompt}</div>
          </div>
        )}
        {phase && (
          <div className="ai-phase" data-phase={phase}>
            <span className="ai-phase-dot" />
            <span>{phase}</span>
            {busy && <span className="ai-phase-hint">进行中…</span>}
          </div>
        )}
        {(reasoning || busy || reasoningUnavailable) && (
          <div className={`ai-thinking${thinkingOpen ? ' is-open' : ''}`}>
            <button
              type="button"
              className="ai-thinking-head"
              onClick={() => setThinkingOpen((v) => !v)}
            >
              <BulbOutlined />
              <span>{reasoning ? '思考' : '执行过程'}</span>
              <span className="ai-thinking-hint">
                {busy
                  ? reasoning
                    ? '进行中'
                    : '等待模型摘要…'
                  : reasoning
                    ? `${reasoning.length} 字`
                    : '模型未提供摘要'}
              </span>
              <span className="ai-thinking-chevron">{thinkingOpen ? '⌃' : '⌄'}</span>
            </button>
            {thinkingOpen && (
              <div className={`ai-thinking-body${reasoning ? '' : ' is-note'}`}>
                {reasoning ||
                  (busy
                    ? '正在等待模型返回思考摘要；检索、工具调用和写入进度会同步显示。'
                    : '当前模型或网关没有返回可展示的思考摘要。Atlas 的检索、工具调用和写入进度仍会实时显示。')}
              </div>
            )}
          </div>
        )}
        {ledger && (
          <div className="ai-ledger">
            <span>事实账本：覆盖率 {ledger.coverage}%</span>
            {ledger.missing.length > 0 && (
              <span className="ai-ledger-missing">缺 {ledger.missing.join('、')}</span>
            )}
          </div>
        )}
        {quality && <div className="ai-quality">{quality}</div>}
        {steps.length > 0 && (
          <div className="ai-steps">
            <span className="ai-steps-head">
              <ToolOutlined />
              Atlas 执行了 {steps.length} 个动作
            </span>
            <div className="ai-step-chips">
              {steps.map((s, i) => (
                <span key={`${s}-${i}`}>{s}</span>
              ))}
            </div>
          </div>
        )}
        {answer ? (
          <div className="ai-answer">
            <div className="ai-answer-head">
              <span className="ai-answer-mark">
                <ThunderboltOutlined />
              </span>
              <strong>Atlas 的回答</strong>
            </div>
            <LiveMarkdown body={answer.text} live={false} />
            {answer.refs.length > 0 && (
              <div className="ai-refs">
                {answer.refs.map((r) => (
                  <Button key={r.node_id} size="small" type="text" onClick={() => jump(r.node_id)}>
                    {r.title}
                  </Button>
                ))}
              </div>
            )}
            <div className="ai-answer-actions">
              <Button
                size="small"
                icon={<SaveOutlined />}
                onClick={() => {
                  setSaveTitle(`问答：${context || '资料'}`)
                  setSaving(true)
                }}
              >
                存为资料
              </Button>
            </div>
          </div>
        ) : (
          say && <div className="ai-say">{say}</div>
        )}
        {live && (
          <div className="ai-live">
            <div className="ai-live-head">正在写（草稿实时落库）</div>
            <pre className="ai-live-body">{live}</pre>
          </div>
        )}
      </div>

      <div className="ai-foot">
        <div className="ai-composer">
          <Input.TextArea
            variant="borderless"
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            autoSize={{ minRows: 2, maxRows: 7 }}
            placeholder="问问题，或直接让 Atlas 改这篇文档…"
            disabled={!selectedId || busy}
            onPressEnter={(e) => {
              if (!e.shiftKey) {
                e.preventDefault()
                void send()
              }
            }}
          />
          <div className="ai-composer-bar">
            <span className={`ai-composer-mode is-${docMode}`}>{modeMeta.label}</span>
            <span className="ai-skill-load" title="从项目 skills/ 目录加载一份完整技能正文">
              <ToolOutlined />
              <Select
                variant="borderless"
                size="small"
                allowClear
                showSearch
                optionFilterProp="title"
                value={loadedSkill || undefined}
                placeholder="加载 Skill"
                className="ai-skill-select"
                popupMatchSelectWidth={360}
                options={skillOptions}
                optionRender={(option) => {
                  const skill = skills.data?.skills.find((s) => s.name === option.value)
                  return (
                    <span className="ai-skill-option">
                      <strong>
                        {option.value}
                        <span className="ai-skill-layer">L{skill?.layer ?? 1}</span>
                      </strong>
                      <small>{skill?.description ?? ''}</small>
                    </span>
                  )
                }}
                onChange={(value) => {
                  const next = value ?? ''
                  setLoadedSkill(next)
                  if (next) localStorage.setItem('atlas.skill', next)
                  else localStorage.removeItem('atlas.skill')
                }}
              />
            </span>
            <span className="ai-foot-hint">
              {busy ? 'Atlas 正在处理…' : 'Enter 发送 · Shift+Enter 换行'}
            </span>
            <Button
              className="ai-send"
              type="primary"
              shape="circle"
              icon={<SendOutlined />}
              loading={busy}
              disabled={!selectedId || !prompt.trim()}
              onClick={() => void send()}
            />
          </div>
        </div>
      </div>

      <Modal
        title="把这条回答存成资料"
        open={saving}
        onCancel={() => setSaving(false)}
        onOk={() => saveAnswer.mutate()}
        okText="保存"
        confirmLoading={saveAnswer.isPending}
        okButtonProps={{ disabled: !saveTitle.trim() }}
      >
        <p style={{ color: 'var(--ant-color-text-tertiary)' }}>
          会新建一篇 doc 资料，保留可点引用；不会改动当前作品正文。
        </p>
        <Input
          value={saveTitle}
          onChange={(e) => setSaveTitle(e.target.value)}
          placeholder="资料标题"
        />
      </Modal>
    </div>
  )
}

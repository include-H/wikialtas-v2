import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react'
import type { ReactNode } from 'react'
import type { Me, Run, WorkSummary } from '../types'
import { getMe, getTree, listRuns } from '../lib/api'

export type DocMode = 'edit' | 'revision' | 'read'

/**
 * 右侧边栏标签：ai = Altas（LLM 模块）；library = 关联与媒体（仅作品页有效）。
 * 2026-09-16 起合并为单标签（宪法 §1.5「Altas｜关联与媒体」）：旧值 relations/media
 * 读出来归一成 library。
 */
export type RightTab = 'ai' | 'library'

const RIGHT_TAB_KEY = 'wikiatlas.rightTab'
/** 右侧边栏展开/收起：全局唯一收放机制（旧的悬浮窗圆球 + 竖向文字拉手两个机制均退役），默认收起。 */
const RIGHT_SIDE_KEY = 'wikiatlas.rightSidebar'

function readRightTab(): RightTab {
  try {
    const v = localStorage.getItem(RIGHT_TAB_KEY)
    return v === 'relations' || v === 'media' || v === 'library' ? 'library' : 'ai'
  } catch {
    return 'ai'
  }
}

function readRightSide(): boolean {
  try {
    return localStorage.getItem(RIGHT_SIDE_KEY) === '1'
  } catch {
    return false
  }
}

interface AppStore {
  nodes: WorkSummary[]
  treeLoading: boolean
  treeError: string | null
  refreshTree: () => Promise<void>
  /** 简易用户系统：访客=false，只能读公开文档 */
  me: Me
  /** /api/auth/me 是否已经回来过（用于避免"还没设密码"横幅一闪） */
  meLoaded: boolean
  refreshMe: () => Promise<void>
  /** 右侧边栏展开/收起：open = 展开并聚焦 Altas 标签；false = 收起。状态 localStorage 记忆（沿用旧名，语义升级为"右栏展开"） */
  aiPanelOpen: boolean
  setAiPanelOpen: (open: boolean) => void
  /** 右侧边栏当前标签（localStorage 记忆）；library 只在作品页有效，非作品页 RightSidebar 兜底回 ai */
  rightTab: RightTab
  setRightTab: (t: RightTab) => void
  /** 当前作品页待确认的库建议数（关联与媒体面板上报，拉门角标用；非作品页 0）。 */
  pendingCount: number
  setPendingCount: (n: number) => void
  /** 展开并落到指定标签（拉门图标走这里）。必须原子：先展开再落标签的顺序里
      若复用 setAiPanelOpen(true)，它内部会强制 tab='ai'，盖掉刚设的 relations——
      两类入口（"问 Altas" vs "拉门某图标"）的意图不同，各自一个动作。 */
  openRightSide: (t: RightTab) => void
  activeRuns: Run[]
  refreshRuns: () => Promise<void>
  /** bump when AI committed content so views can refresh */
  contentStamp: number
  notifyContentCommitted: (targetId: string, version: number) => void
  lastCommitted: { targetId: string; version: number } | null
  /** Altas 正在写入的正文（wikiatlas.content.staging 带过来的，用于"正在写入"标记） */
  staging: { targetId: string; targetType: string } | null
  setStaging: (v: { targetId: string; targetType: string } | null) => void
  /**
   * 文档模式（对齐飞书顶栏「编辑 ▾」下拉）：
   * edit = 所见即所得编辑；revision = Markdown 修订；read = 只读。
   * 编辑器内部不再放模式按钮。
   */
  docMode: DocMode
  setDocMode: (mode: DocMode) => void
  /** 正文选区（正文工具栏与 AI 面板共用：Altas 要知道"改的是哪一段"） */
  docSelection: string
  setDocSelection: (text: string) => void
  /**
   * 正文工具栏发起的一次提问，由 AI 面板消费：
   * prompt 为空 = 只把选段交给面板等用户提问；autoSend = 直接发出去（如「翻译这段」）。
   * intent = 'answer' 时走问答工单：任务框定为问答（翻译/解释这类一次性输出）。
   * oneShot = true 是**能力契约**：显式的一次性入口（工具栏按钮），运行时会剥掉
   * 写工具——只输出、不落盘。「翻译写回正文」事故的结构性兜底就靠它。
   */
  ask: { prompt: string; autoSend: boolean; intent?: 'answer'; oneShot?: boolean } | null
  askSelection: (opts: {
    selection: string
    prompt?: string
    autoSend?: boolean
    intent?: 'answer'
    oneShot?: boolean
  }) => void
  clearAsk: () => void
  sidebarCollapsed: boolean
  setSidebarCollapsed: (v: boolean) => void
  /** 当前正在看的批次（批量建档）；面板顶部显示批次卡 */
  activeBatchId: string | null
  setActiveBatchId: (id: string | null) => void
}

const Ctx = createContext<AppStore | null>(null)

export function AppProvider({ children }: { children: ReactNode }) {
  const [nodes, setNodes] = useState<WorkSummary[]>([])
  const [me, setMe] = useState<Me>({
    authed: false,
    username: '',
    adminPasswordConfigured: false,
  })
  const [meLoaded, setMeLoaded] = useState(false)
  const [treeLoading, setTreeLoading] = useState(true)
  const [treeError, setTreeError] = useState<string | null>(null)
  const [aiPanelOpen, setAiPanelOpenState] = useState<boolean>(readRightSide)
  const [rightTab, setRightTabState] = useState<RightTab>(readRightTab)
  const [pendingCount, setPendingCount] = useState(0)
  const [activeRuns, setActiveRuns] = useState<Run[]>([])
  const [contentStamp, setContentStamp] = useState(0)
  const [lastCommitted, setLastCommitted] = useState<{ targetId: string; version: number } | null>(
    null,
  )
  const [staging, setStaging] = useState<{ targetId: string; targetType: string } | null>(null)
  const [docMode, setDocMode] = useState<DocMode>('edit')
  const [docSelection, setDocSelection] = useState('')
  const [ask, setAsk] = useState<{
    prompt: string
    autoSend: boolean
    intent?: 'answer'
    oneShot?: boolean
  } | null>(null)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const [activeBatchId, setActiveBatchIdState] = useState<string | null>(() =>
    localStorage.getItem('wikiatlas.batch'),
  )

  const setActiveBatchId = useCallback((id: string | null) => {
    setActiveBatchIdState(id)
    try {
      if (id) localStorage.setItem('wikiatlas.batch', id)
      else localStorage.removeItem('wikiatlas.batch')
    } catch {
      // storage disabled — 批次卡降级为会话内状态
    }
  }, [])

  const askSelection = useCallback(
    ({
      selection,
      prompt = '',
      autoSend = false,
      intent,
      oneShot = false,
    }: {
      selection: string
      prompt?: string
      autoSend?: boolean
      intent?: 'answer'
      oneShot?: boolean
    }) => {
      setDocSelection(selection)
      setAsk({ prompt, autoSend, intent, oneShot })
      setAiPanelOpen(true)
    },
    [],
  )

  const clearAsk = useCallback(() => setAsk(null), [])

  const setRightTab = useCallback((t: RightTab) => {
    setRightTabState(t)
    try {
      localStorage.setItem(RIGHT_TAB_KEY, t)
    } catch {
      // 存不了就只在本次会话生效
    }
  }, [])

  const setAiPanelOpen = useCallback((open: boolean) => {
    setAiPanelOpenState(open)
    // 打开 = 展开并回到 Altas 标签（侧栏/工具栏「AI 共建」/askSelection/?ai=1 全走这个语义）；
    // 收起只管开合，不动标签——展开回来时用户还在原来的标签上
    if (open) setRightTabState('ai')
    try {
      localStorage.setItem(RIGHT_SIDE_KEY, open ? '1' : '0')
    } catch {
      // 存不了就只在本次会话生效
    }
  }, [])

  const openRightSide = useCallback((t: RightTab) => {
    setAiPanelOpenState(true)
    setRightTabState(t)
    try {
      localStorage.setItem(RIGHT_SIDE_KEY, '1')
      localStorage.setItem(RIGHT_TAB_KEY, t)
    } catch {
      // 存不了就只在本次会话生效
    }
  }, [])

  const refreshTree = useCallback(async () => {
    setTreeLoading(true)
    setTreeError(null)
    try {
      const res = await getTree()
      setNodes(res.nodes ?? [])
    } catch (e) {
      setTreeError(e instanceof Error ? e.message : '加载树失败')
      setNodes([])
    } finally {
      setTreeLoading(false)
    }
  }, [])

  const refreshRuns = useCallback(async () => {
    try {
      const res = await listRuns()
      const runs = res.runs ?? []
      setActiveRuns(runs.filter((r) => r.status === 'running' || r.status === 'interrupted'))
    } catch {
      setActiveRuns([])
    }
  }, [])

  const refreshMe = useCallback(async () => {
    try {
      setMe(await getMe())
    } catch {
      setMe({ authed: false, username: '', adminPasswordConfigured: false })
    } finally {
      setMeLoaded(true)
    }
  }, [])

  const notifyContentCommitted = useCallback((targetId: string, version: number) => {
    setLastCommitted({ targetId, version })
    setContentStamp((n) => n + 1)
    void refreshTree()
  }, [refreshTree])

  useEffect(() => {
    void refreshMe()
    void refreshTree()
  }, [refreshMe, refreshTree])

  // 工单是管理态专属：访客不请求 /api/runs（否则每轮轮询都吃 401）
  useEffect(() => {
    if (me.authed) void refreshRuns()
    else setActiveRuns([])
  }, [me.authed, refreshRuns])

  const value = useMemo<AppStore>(
    () => ({
      nodes,
      treeLoading,
      treeError,
      refreshTree,
      me,
      meLoaded,
      refreshMe,
      aiPanelOpen,
      setAiPanelOpen,
      rightTab,
      setRightTab,
      pendingCount,
      setPendingCount,
      openRightSide,
      activeRuns,
      refreshRuns,
      contentStamp,
      notifyContentCommitted,
      lastCommitted,
      staging,
      setStaging,
      docMode,
      setDocMode,
      docSelection,
      setDocSelection,
      ask,
      askSelection,
      clearAsk,
      sidebarCollapsed,
      setSidebarCollapsed,
      activeBatchId,
      setActiveBatchId,
    }),
    [
      nodes,
      treeLoading,
      treeError,
      refreshTree,
      me,
      meLoaded,
      refreshMe,
      aiPanelOpen,
      rightTab,
      pendingCount,
      activeRuns,
      refreshRuns,
      contentStamp,
      notifyContentCommitted,
      lastCommitted,
      staging,
      docMode,
      docSelection,
      ask,
      askSelection,
      clearAsk,
      sidebarCollapsed,
      activeBatchId,
      setActiveBatchId,
    ],
  )

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>
}

export function useAppStore(): AppStore {
  const ctx = useContext(Ctx)
  if (!ctx) throw new Error('useAppStore outside AppProvider')
  return ctx
}

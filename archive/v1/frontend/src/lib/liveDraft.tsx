import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import type { ContentDeltaPayload } from '../types'

/**
 * Altas 正在流式写的那份正文草稿（wikiatlas.content.delta 累积出来的）。
 *
 * 后端把写正文类工具（write_content / patch_section）的流式参数投影成"段"，
 * 在这里累积成一份草稿，编辑器拿它把正文实时长出来：
 *   · write_content 段的就是成品正文（整篇替换）；
 *   · patch_section 段的是某一节的新正文（由消费方按 heading 与当前正文拼接）。
 *
 * **收尾有两种语义**（编辑器据此决定草稿的最后一帧怎么处理）：
 *   · commit() —— 正文已落库：编辑器把末稿留在原地，等正式内容回来无缝对齐
 *     （不垫的话会先闪回旧正文再换上新的）；
 *   · abort() —— 工单中断/失败：正文没有变，编辑器直接退回真实内容。
 *
 * **为什么单独开一个 Provider，而不是放全局 store**：这是条高频通道
 * （~5 段/秒，写一篇正文持续好几分钟），进全局 store 会让每个 useAppStore
 * 的消费者跟着重渲染；这里把"值"和"写入口"拆成两个 context——读值的只有
 * 正在显示草稿的编辑器视图，写入口（push / commit / abort）引用恒定，
 * 生产者不因它重渲染。
 */
export interface LiveDraft {
  callId: string
  targetId: string
  targetType: string
  /** 触发它的工具名（write_content | patch_section） */
  name: string
  /** patch_section 的目标章节 */
  heading: string
  md: string
  /** patch_sections 波次里已完成的各节（当前 heading/md 只持活跃节） */
  sections?: { heading: string; md: string }[]
}

export interface LiveDraftState {
  draft: LiveDraft | null
  /** 最近一次收尾是中断（true）还是提交（false）——只在 draft 刚变 null 时有意义 */
  aborted: boolean
}

interface LiveDraftApi {
  push: (p: ContentDeltaPayload) => void
  /** 提交收尾：正文已落库，末稿留在编辑器里等正式内容对齐 */
  commit: () => void
  /** 中断收尾：正文没变，编辑器退回真实内容 */
  abort: () => void
}

const ValueCtx = createContext<LiveDraftState>({ draft: null, aborted: false })
const ApiCtx = createContext<LiveDraftApi | null>(null)

export function LiveDraftProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<LiveDraftState>({ draft: null, aborted: false })
  const ref = useRef<LiveDraft | null>(null)

  // rAF 合流：正文段 ~5/秒，直接每段 setState 会让「读草稿的视图」每段重渲一次。
  // 改成 delta 累进 ref、每动画帧最多 setState 一次（状态按序落、渲染按帧合并，
  // 对齐 dsh 的 publication: 'animation-frame'）。commit/abort 会取消待处理的帧。
  const rafRef = useRef<number | null>(null)
  const scheduleFlush = useCallback(() => {
    if (rafRef.current != null) return
    rafRef.current = requestAnimationFrame(() => {
      rafRef.current = null
      const cur = ref.current
      setState({ draft: cur ? { ...cur } : null, aborted: false })
    })
  }, [])
  const cancelFlush = useCallback(() => {
    if (rafRef.current != null) {
      cancelAnimationFrame(rafRef.current)
      rafRef.current = null
    }
  }, [])
  // unmount 清理：别在卸载后还 setState
  useEffect(() => cancelFlush, [cancelFlush])

  const push = useCallback(
    (p: ContentDeltaPayload) => {
      if (!p.callId) return
      let l = ref.current
      if (!l || l.callId !== p.callId) {
        l = { callId: p.callId, targetId: '', targetType: 'work', name: '', heading: '', md: '' }
        ref.current = l
      }
      // 目标信息可能后到（参数流里 targetId 在正文之后才完整）：持续跟着更新
      if (p.name) l.name = p.name
      if (p.targetId) l.targetId = p.targetId
      if (p.targetType) l.targetType = p.targetType
      if (p.name === 'patch_sections' && p.heading && l.heading && l.heading !== p.heading) {
        // 波次切到下一节：上一节锁进 sections，当前槽只持活跃节
        if (l.md) l.sections = [...(l.sections ?? []), { heading: l.heading, md: l.md }]
        l.md = ''
      }
      if (p.heading) l.heading = p.heading
      l.md = p.reset ? (p.text ?? '') : l.md + (p.delta ?? '')
      scheduleFlush()
    },
    [scheduleFlush],
  )

  const commit = useCallback(() => {
    cancelFlush()
    ref.current = null
    setState({ draft: null, aborted: false })
  }, [cancelFlush])

  const abort = useCallback(() => {
    cancelFlush()
    ref.current = null
    setState({ draft: null, aborted: true })
  }, [cancelFlush])

  const api = useMemo<LiveDraftApi>(() => ({ push, commit, abort }), [push, commit, abort])

  return (
    <ApiCtx.Provider value={api}>
      <ValueCtx.Provider value={state}>{children}</ValueCtx.Provider>
    </ApiCtx.Provider>
  )
}

/** 读当前草稿状态（只有真正要显示它的视图才订阅：每次正文段都会触发重渲染）。 */
export function useLiveDraft(): LiveDraftState {
  return useContext(ValueCtx)
}

/** 写入口（push / commit / abort）：引用恒定，用它不会因正文流重渲染。 */
export function useLiveDraftApi(): LiveDraftApi {
  const api = useContext(ApiCtx)
  if (!api) throw new Error('useLiveDraftApi outside LiveDraftProvider')
  return api
}

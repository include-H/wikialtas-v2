import { useEffect, useRef, useState } from 'react'
import { streamingResponseToMessage } from '@douyinfe/semi-ui'
import type { RunEvent } from '../types'
import {
  dedupeEvents,
  isResponseEvent,
  normalizeAssistant,
  responsesChunks,
  type DialogueMessage,
  type ResponseChunk,
} from './responses'

/**
 * 当前工单的助手消息，用 Semi 归约器的**增量**能力算出来。
 *
 * 为什么不用 `runToMessage`（每帧全量重算）：
 *   `runToMessage` 每次把整个事件流 `.sort()` + 重编号 + 从空 state 归约一遍，
 *   O(n log n)/帧，累积 O(n²)——一个 7k 字工单几千条 delta 时前端卡死。
 *   归约器原生就是增量的（返回 { message, nextState }，processedSeq 按 seq 去重、
 *   outputs 按 output_index 增量 append），本 hook 跨帧持有 prevState，每帧只喂
 *   「events 尾部新增的 response.* 事件」，O(1)/帧。
 *
 * 一次性全量（历史加载 / 发新单时冻结当前单）仍走 `runMessages`，那条是低频、
 * 不是每帧，全量重算没有性能问题且保证幂等。
 *
 * **序号空间必须跨帧稳定**（归约器靠 sequence_number 去重 + 连续推进）：
 *   序号 = 「该事件是第几个 response.* 事件」的 0-index，全量与增量同一口径
 *   （全量 0..n-1；增量从 n 继续）。流式中 SSE 按 seq 递增到达，新增事件的
 *   全局位置恒为 n + 相对序，和全量重编号结果一致。
 */

/** 归约器 prevState 的类型：从 semi 的函数签名推导，避免手抄一条 import 路径。 */
type PrevState = NonNullable<Parameters<typeof streamingResponseToMessage>[1]>


export function useReducedAssistant(
  run: { id?: string | null } | null,
  events: RunEvent[],
  /**
   * SSE rebaseline（重拉窗口回补）计数：变化时把投影从日志**全量重建**一次
   * （对齐 DSH 的 baseline 语义）。回补的事件可能乱序/带洞，增量路径的序号
   * 假设会被打破，全量重算最稳——幂等、低频，代价可忽略。
   */
  resetNonce?: number,
): DialogueMessage | null {
  const runId = run?.id ?? null
  const [msg, setMsg] = useState<DialogueMessage | null>(null)
  const stateRef = useRef<PrevState | null>(null)
  const runIdRef = useRef<string | null>(null)
  /** 已归约的 response.* 事件数 = 增量序号基线（全量后 = n，之后只增）。 */
  const seqBaseRef = useRef(0)
  /** 已处理到的 events 数组索引（mergeEvent 追加在末尾，增量从这里切）。 */
  const evIdxRef = useRef(0)
  const msgRef = useRef<DialogueMessage | null>(null)
  /** 已见过的 resetNonce：变化 = SSE 回补过窗口，需要全量重建投影。 */
  const nonceRef = useRef(0)

  useEffect(() => {
    if (!runId) {
      runIdRef.current = null
      stateRef.current = null
      seqBaseRef.current = 0
      evIdxRef.current = 0
      nonceRef.current = 0
      if (msgRef.current !== null) {
        msgRef.current = null
        setMsg(null)
      }
      return
    }

    /** 全量归约一次（新 run / 异常重置）：0..n-1 连续序号，交给归约器容错空洞。 */
    const doFull = () => {
      const chunks = responsesChunks(dedupeEvents(events))
      const out = streamingResponseToMessage(chunks)
      seqBaseRef.current = chunks.length
      stateRef.current = (out?.nextState as PrevState | null) ?? null
      const m = normalizeAssistant(out?.message)
      msgRef.current = m
      setMsg(m)
    }

    // run 身份变化（含 null→id，覆盖 send 里 SSE 先于 setRun 的时序）：全量，不丢先到的事件
    if (runIdRef.current !== runId) {
      runIdRef.current = runId
      evIdxRef.current = events.length
      nonceRef.current = resetNonce ?? 0
      doFull()
      return
    }

    // SSE rebaseline（回补过窗口）：从日志全量重建投影。
    if (resetNonce != null && resetNonce !== nonceRef.current) {
      nonceRef.current = resetNonce
      evIdxRef.current = events.length
      doFull()
      return
    }

    const from = evIdxRef.current
    if (events.length < from) {
      // 事件数变少了却没换 runId（罕见的全量替换）：退化全量，保证正确
      evIdxRef.current = events.length
      doFull()
      return
    }
    if (events.length === from) return
    evIdxRef.current = events.length

    const newResp = events
      .slice(from)
      .filter((ev) => isResponseEvent(ev.type))
      .sort((a, b) => a.seq - b.seq)
    if (newResp.length === 0) return // 只来了 wikiatlas.* 旁路，对话没变

    const chunks: ResponseChunk[] = newResp.map((ev, i) => {
      const p = (ev.payload ?? {}) as ResponseChunk
      return { ...p, type: ev.type, sequence_number: seqBaseRef.current + i }
    })
    seqBaseRef.current += newResp.length

    const out = streamingResponseToMessage(chunks, stateRef.current ?? undefined)
    stateRef.current = (out?.nextState as PrevState | null) ?? null
    const m = normalizeAssistant(out?.message)
    if (m !== msgRef.current) {
      msgRef.current = m
      setMsg(m)
    }
  }, [runId, events, resetNonce])

  return msg
}

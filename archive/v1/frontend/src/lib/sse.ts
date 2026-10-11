import type { RunEvent, RunEventType } from '../types'
import { RUN_EVENT_TYPES } from './responses'

// Run 事件流客户端：后端 SSE 的 `data:` 只带事件对象本身，事件类型在 `event:`
// 字段上；这里统一还原成完整 RunEvent 信封。
//
// 类型清单的 owner 是 `lib/responses.ts`（与后端 responses.go 一一对应）——
// EventSource 只派发**显式订阅过**的命名事件，所以这份清单必须整份订阅，
// 漏一个那一类就静默收不到（不报错、不提示，只是界面少了一块）。
//
// 韧性（对齐 DSH 的 transient/durable 分离）：
//   · 游标只在**成功交付**后推进——畸形帧不推进，否则"推进了却没处理"= 这条
//     seq 永久丢失（续传会从它之后开始，回补不到）。
//   · seq 空洞（有序连接上跳号）= 中间丢了事件 → rebaseline：按 lastSeq 重拉
//     窗口，后端回放**权威事件**（流式 delta 不落库、回放本就跳过，丢了是
//     设计——权威 done 事件带全文，归约器能重建）。
//   · rebaseline 一次性抑制：回放合法跳过 delta 造成的"空洞"不是丢帧，别追着
//     无限重连；真正连续丢帧靠 onerror 的指数退避兜底。

export type SseHandler = (event: RunEvent) => void

export interface SseHandle {
  close: () => void
}

const NAMED_EVENTS: readonly RunEventType[] = RUN_EVENT_TYPES

export interface SseOptions {
  lastEventId?: string
  onOpen?: () => void
  onError?: (e: Event) => void
  /** 观测点：丢帧 / 空洞计数，便于排查"界面少了一块"。 */
  onDiagnostic?: (kind: 'malformed' | 'gap', detail: string) => void
  /** 触发过 rebaseline（重拉窗口回补）——前端据此把投影从日志全量重建一次。 */
  onRebaseline?: () => void
}

export function streamRunEvents(
  runId: string,
  onEvent: SseHandler,
  opts?: SseOptions,
): SseHandle {
  let closed = false
  let lastEventId = opts?.lastEventId ?? ''
  let lastSeq = lastEventId ? Number(lastEventId) || 0 : 0
  let es: EventSource | null = null
  let retryTimer: ReturnType<typeof setTimeout> | null = null
  let retries = 0
  /** 本轮已 rebaseline 过一次：回放跳 delta 的"空洞"别再追，直接交付防死循环。 */
  let rebaselined = false
  let malformedCount = 0

  const deliver = (id: string, seq: number, name: RunEventType, payload: Record<string, unknown>) => {
    lastEventId = id
    lastSeq = seq
    rebaselined = false // 连贯推进 = 恢复正常
    onEvent({
      id: id ? `${runId}:${id}` : `${runId}:${name}`,
      runId,
      seq,
      type: name,
      payload,
      createdAt: new Date().toISOString(),
    })
  }

  /** 跳过一条无法使用的事件（持续畸形）：只推进游标，不 onEvent，防卡死。 */
  const skipTo = (id: string, seq: number) => {
    lastEventId = id
    lastSeq = seq
  }

  const rebaseline = (_reason: string) => {
    rebaselined = true
    opts?.onRebaseline?.()
    es?.close()
    es = null
    retryTimer = setTimeout(connect, 0)
  }

  const connect = () => {
    if (closed) return
    const url = lastEventId
      ? `/api/runs/${runId}/events/stream?lastEventId=${encodeURIComponent(lastEventId)}`
      : `/api/runs/${runId}/events/stream`

    es = new EventSource(url)
    es.onopen = () => {
      retries = 0
      opts?.onOpen?.()
    }

    for (const name of NAMED_EVENTS) {
      es.addEventListener(name, (msg) => {
        const me = msg as MessageEvent
        const id = me.lastEventId || ''
        const seq = Number(id) || 0

        let payload: Record<string, unknown>
        try {
          payload = JSON.parse(me.data) as Record<string, unknown>
        } catch (err) {
          // 畸形帧：上报 + 计数；**不推进游标**。重拉一次（可能只是网络抖动）；
          // 若重拉后仍畸形（rebaselined 已置位），跳过这条防卡死。
          malformedCount += 1
          console.warn(`[sse] ${runId} 畸形 ${name} 帧 id=${id}（累计 ${malformedCount}）:`, err)
          opts?.onDiagnostic?.('malformed', `${name} id=${id} #${malformedCount}`)
          if (!rebaselined) {
            rebaseline(`畸形帧 ${name} id=${id}`)
          } else {
            skipTo(id, seq)
          }
          return
        }

        // seq 空洞：有序连接上 seq 应连续；跳号 = 中间丢了事件 → 回补。
        // 重拉后仍跳号（replay 合法跳过 delta）→ 直接交付，不无限重连。
        if (lastSeq > 0 && seq > lastSeq + 1) {
          if (!rebaselined) {
            opts?.onDiagnostic?.('gap', `seq ${lastSeq}→${seq}`)
            rebaseline(`seq 空洞 ${lastSeq}→${seq}`)
            return
          }
          opts?.onDiagnostic?.('gap', `seq ${lastSeq}→${seq}（跳过回补，直接交付）`)
          deliver(id, seq, name, payload)
          return
        }

        deliver(id, seq, name, payload)
      })
    }

    es.onerror = (e) => {
      opts?.onError?.(e)
      es?.close()
      es = null
      if (closed) return
      retries += 1
      retryTimer = setTimeout(connect, Math.min(1000 * 2 ** retries, 15000))
    }
  }

  connect()

  return {
    close: () => {
      closed = true
      if (retryTimer) clearTimeout(retryTimer)
      es?.close()
    },
  }
}

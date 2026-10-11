import { useEffect, useMemo, useState } from 'react'
import { Typography } from '@douyinfe/semi-ui'
import type { RunEvent } from '../../../types'
import { formatDuration, runStatusOf } from '../../../lib/runView'

const { Text } = Typography

// 工单状态行（时间线的尾）：进行中显示"正在工作 + 已处理多久"，终态显示结果与总时长。
//
// 终态取自事件流里的 `wikiatlas.run.end`——**这就是加那个事件的原因**：以前
// interrupted / 进程重启的工单在流里没有任何结尾，界面只能靠 run.status 猜（还得
// 多查一次接口）。现在流自己说完了。

const TERMINAL: Record<string, { icon: string; text: string }> = {
  completed: { icon: '✓', text: '已完成' },
  failed: { icon: '✕', text: '工单失败' },
  interrupted: { icon: '⏹', text: '工单已中断' },
  expired: { icon: '·', text: '工单已归档' },
}

export default function RunStatusRow({
  events,
  run,
}: {
  events: RunEvent[]
  run?: { status?: string; startedAt?: string | null } | null
}) {
  const info = useMemo(() => runStatusOf(events, run), [events, run])
  const [now, setNow] = useState(() => Date.now())

  // 只有进行中才需要秒级心跳（"已处理 4m 12s"要动）；终态一次算完就停。
  useEffect(() => {
    if (info.status !== 'running' || !info.startedAt) return
    const t = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(t)
  }, [info.status, info.startedAt])

  if (info.status === 'running') {
    const elapsed = info.startedAt ? ` · 已处理 ${formatDuration(now - info.startedAt)}` : ''
    return (
      <div className="ai-working-row" role="status" aria-live="polite">
        <span className="ai-working-dot" aria-hidden />
        <Text type="tertiary" size="small">
          Altas 正在工作…{elapsed}
        </Text>
      </div>
    )
  }

  const t = TERMINAL[info.status]
  if (!t) return null
  const dur = info.durationMs != null ? ` · ${formatDuration(info.durationMs)}` : ''
  const reason = info.status === 'completed' ? '' : info.reason || ''

  return (
    <div className={`ai-run-status is-${info.status}`} role="status">
      <span className="ai-run-status-icon" aria-hidden>
        {t.icon}
      </span>
      <Text type="tertiary" size="small">
        {t.text}
        {dur}
        {reason ? ` · ${reason}` : ''}
      </Text>
    </div>
  )
}

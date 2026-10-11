import { useState } from 'react'
import { Typography } from '@douyinfe/semi-ui'
import {
  IconBriefStroked,
  IconChevronDown,
  IconChevronRight,
  IconCodeStroked,
  IconLoading,
  IconSearchStroked,
  IconStoryStroked,
} from '@douyinfe/semi-icons'
import type { ToolGroupItem } from '../../../lib/runItems'
import {
  detailOf,
  diffOf,
  durationOf,
  hasDetail,
  labelOf,
  summarizeGroup,
  toolMeta,
  toolStateOf,
  type ToolIconKind,
} from '../../../lib/toolFamilies'
import ToolDetail from './ToolDetail'

// 工具组折叠条（VISUAL_SPEC §1.5/§4）：**弱化的一行**——族图标 + 人话措辞 +
// 一行摘要，展开才是逐条工具行与产物。
//
// 为什么成组：一时段里连着调十个工具，摊开来是"一面工具墙"，读不出这轮干了什么；
// 合成一条「已读取 2 处 · 已检索 3 次」才看得出来。中间夹了叙事句/思考就被切断
// （分组在 runView.groupToolCalls 做，渲染层只看得到已经成组的项）。
//
// 复用 `.tool-card*` 那套样式（弱化背景、nowrap 摘要、悬停高亮），组只是"一行 + 多块详情"。

const ICONS: Record<ToolIconKind, React.ReactNode> = {
  read: <IconBriefStroked />,
  search: <IconSearchStroked />,
  write: <IconCodeStroked />,
  plan: <IconStoryStroked />,
  tool: <IconBriefStroked />,
}

export default function ToolGroupChip({ item }: { item: ToolGroupItem }) {
  const [open, setOpen] = useState(false)
  const single = item.items.length === 1
  const first = item.items[0]
  const running = item.items.some((it) => toolStateOf(it).running)
  const failed = item.items.some((it) => toolStateOf(it).failed)
  const title = summarizeGroup(item.items)
  const detail = single ? detailOf(first) : ''
  const expandable = item.items.some(hasDetail)
  const { additions, deletions } = diffOf(item.items)
  const ms = durationOf(item.items)

  const cls = ['tool-card', running ? 'is-running' : '', failed ? 'is-failed' : ''].filter(Boolean).join(' ')

  return (
    <div className={cls}>
      <button
        type="button"
        className="tool-card-head"
        aria-expanded={expandable ? open : undefined}
        onClick={() => expandable && setOpen((v) => !v)}
      >
        <span className="tool-card-icon" aria-hidden>
          {running ? <IconLoading /> : ICONS[toolMeta(first.name).icon]}
        </span>
        <span className="tool-card-label">{title}</span>
        {detail && (
          <Typography.Text type="tertiary" size="small" ellipsis={{ showTooltip: true }} className="tool-card-detail">
            {detail}
          </Typography.Text>
        )}
        {(additions > 0 || deletions > 0) && (
          <span className="tool-card-diff" aria-label="改动行数">
            {additions > 0 && <span className="tool-card-diff-add">+{additions}</span>}
            {deletions > 0 && <span className="tool-card-diff-del">−{deletions}</span>}
          </span>
        )}
        {ms >= 1000 && !running && (
          <Typography.Text type="tertiary" size="small" className="tool-card-time">
            {(ms / 1000).toFixed(1)}s
          </Typography.Text>
        )}
        {expandable && (
          <span className="tool-card-caret" aria-hidden>
            {open ? <IconChevronDown /> : <IconChevronRight />}
          </span>
        )}
      </button>

      {open && (
        <div className="tool-card-body">
          {item.items.map((it, i) => (
            <div className="tool-group-item" key={it.id ?? i}>
              {/* 单条时折叠头已经是它的措辞，不再重复一行 */}
              {!single && <div className="tool-group-item-head">{labelOf(it)}</div>}
              <ToolDetail item={it} />
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

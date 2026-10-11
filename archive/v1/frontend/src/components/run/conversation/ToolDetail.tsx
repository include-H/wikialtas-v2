import { AIChatDialogue } from '@douyinfe/semi-ui'
import type { FunctionCallItem } from '../../../lib/runItems'
import { fullDetailOf, toolStateOf } from '../../../lib/toolFamilies'

// 工具调用的**展开区**：摘要全文 + 产物（来源列表 / 代码卡）+ 截断说明。
//
// 卡片上的信息全部来自**项本身**：结果摘要、耗时、diff、产物挂在 `item.wikiatlas`
// 下（后端 itemExtraKey）。也就是说展开一张卡片不需要去别的事件里检索——项是自足的。

/** Semi 官方代码卡片（带语言标签 + 复制键）——读取类产物的展示容器。 */
const CodeCard = AIChatDialogue.defaultComponents.code as unknown as React.ComponentType<{
  className?: string
  children?: React.ReactNode
}>

/** 检索类产物：编号来源列表（可点标题）。 */
function SourcesList({ lines }: { lines: string[] }) {
  const items = lines
    .map((line) => {
      const idx = line.lastIndexOf(' — ')
      if (idx > 0) return { title: line.slice(0, idx).trim(), tail: line.slice(idx + 3).trim() }
      return { title: line.trim(), tail: '' }
    })
    .filter((it) => it.title)
    .slice(0, 10)
  if (!items.length) return null
  return (
    <ol className="ai-sources">
      {items.map((it, i) => (
        <li key={i}>
          {it.tail.startsWith('http') ? (
            <a href={it.tail} target="_blank" rel="noreferrer">
              {it.title}
            </a>
          ) : (
            it.title
          )}
        </li>
      ))}
    </ol>
  )
}

export default function ToolDetail({ item }: { item: FunctionCallItem }) {
  const extra = item.wikiatlas ?? {}
  const art = extra.artifact
  // 折起那行是截断过的，展开给摘要全文（正常卡通常也就一行，不显多余）
  const full = fullDetailOf(item)
  const failed = toolStateOf(item).failed

  return (
    <div className="tool-detail">
      {full && <div className={failed ? 'tool-card-error' : 'tool-card-note'}>{full}</div>}
      {art?.lines?.length ? (
        art.kind === 'list' ? (
          <SourcesList lines={art.lines} />
        ) : (
          <div className="ai-artifact">
            <CodeCard className="language-md">{art.lines.join('\n')}</CodeCard>
          </div>
        )
      ) : null}
      {art?.truncated && (
        <div className="ai-artifact-note">
          已截断 · 仅显示前 {art.lines?.length ?? 0} 行
          {art.totalLines ? ` · 共 ${art.totalLines} 行` : ''}
        </div>
      )}
    </div>
  )
}

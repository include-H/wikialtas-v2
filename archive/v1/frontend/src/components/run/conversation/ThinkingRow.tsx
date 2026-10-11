import { useState } from 'react'
import { MarkdownRender } from '@douyinfe/semi-ui'
import { IconAISearchLevel2, IconChevronDown, IconChevronUp } from '@douyinfe/semi-icons'
import { reasoningText, type ReasoningItem } from '../../../lib/runItems'

/**
 * 思考行。经 AIChatDialogue 的官方扩展点 `renderDialogueContentItem.reasoning`
 * 注册（Semi 对内容项先查自定义键，未命中才走内置 ReasoningWidget）。
 *
 * 为什么用自研而不是内置 ReasoningWidget：
 *   内置把思考正文放进 Collapsible——class 组件 + ResizeObserver 量内容高 +
 *   overflow:hidden 动态裁剪，长思考高频 delta 时量高滞后就把正文裁在旧高度里
 *   （观感"思考卡住/被截断"，已用同版本复现装置与真机事件流验证）。这里没有那层
 *   测量，正文按内容自然撑开，结构上裁不掉。
 *
 * 默认态（VISUAL_SPEC §4）：**默认收起**；进行中在折叠头显最后一句（当进度条用），
 * 完成后显「已思考 · N 字」；用户点开过就一直尊重。三家母本（DSH / 豆包 / 我们）
 * 默认都是收起的——一屏思考墙谁都不想读。
 */
export default function ThinkingRow({ item }: { item: ReasoningItem }) {
  const [open, setOpen] = useState(false)
  const thinking = (item.status ?? 'in_progress') !== 'completed'

  // 归约器原地 mutate（summary 数组跨帧同引用），依赖引用做 memo 会永远错过新文本
  // ——必须渲染时现读，与内置 getText 同一契约。
  const text = reasoningText(item)
  const chars = text.length
  const tail = thinking ? lastLine(text) : ''

  const toggle = () => setOpen((v) => !v)

  return (
    <div className={`ai-thinking${thinking ? ' is-thinking' : ''}${open ? ' is-open' : ''}`}>
      <div
        className="ai-thinking-head"
        role="button"
        tabIndex={0}
        aria-expanded={open}
        onClick={toggle}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault()
            toggle()
          }
        }}
      >
        <IconAISearchLevel2 className="ai-thinking-icon" />
        <span className="ai-thinking-title">{thinking ? '正在思考' : '思考'}</span>
        {!open && (
          <span className="ai-thinking-hint">{thinking ? tail : `${chars} 字 · 点击展开`}</span>
        )}
        {open ? <IconChevronUp className="ai-thinking-chevron" /> : <IconChevronDown className="ai-thinking-chevron" />}
      </div>
      {open && (
        <div className="ai-thinking-body">
          <MarkdownRender format="md" className="ai-md ai-thinking-md" raw={text} />
        </div>
      )}
    </div>
  )
}

/** 折叠头那一行进度：取最后一句非空文本，短截断。 */
function lastLine(text: string): string {
  const lines = text.split('\n').map((l) => l.trim()).filter(Boolean)
  const last = lines[lines.length - 1] ?? ''
  return last.length > 32 ? `${last.slice(0, 32)}…` : last
}

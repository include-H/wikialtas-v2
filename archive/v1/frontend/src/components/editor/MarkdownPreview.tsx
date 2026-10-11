import DOMPurify from 'dompurify'
import { marked } from 'marked'
import { useMemo, useRef } from 'react'
import { slugifyHeading, transformEpigraph, uniqueSlug } from '../../lib/mdOutline'

marked.setOptions({ gfm: true, breaks: true })

const SANITIZE_ATTRS = ['target', 'id', 'class'] as const

interface Props {
  markdown: string
  className?: string
  /**
   * 正在接收流式草稿帧（正文只在末尾增长 / 单节原位拼接）。
   * 当前段（末尾块）挂「加载态」：压暗表示还没写完；段写完、新块挂上后
   * 摘类，CSS 过渡把它渐显回正文。关掉时对静态内容无副作用。
   */
  live?: boolean
}

interface BlockView {
  key: string
  html: string
  tail: boolean
}

/**
 * Markdown 预览（流式草稿的只读表面）。
 *
 * 渲染策略：`marked.lexer` 把整篇折成顶层 token（每个 = 一个块），逐块出
 * HTML、sanitize，交给 React 按 key 调和。跨帧 diff：token `raw` 没变的块
 * 直接复用上一帧的 HTML（不重 parse/sanitize），只有增长中的当前段重算。
 *
 * 为什么不是整篇 `dangerouslySetInnerHTML`：整篇重挂会把每个块的 DOM 全
 * 销毁/重建/重排，长文每帧都卡、锚点跳。keyed 渲染下流式只在末尾长——
 * 前缀块 key 稳定保 DOM、当前段 key 稳定只更新 innerHTML、段写完新块挂上
 * → 旧尾摘 `live-tail` 渐显。
 *
 * 标题 id 仍按出现序给 h2/h3 配 uniqueSlug（与 parseOutline 的 slug 规则
 * 一致），在渲染后注入——marked 不认 `{#id}` 语法，塞进源码会被当正文。
 */
export default function MarkdownPreview({ markdown, className, live }: Props) {
  const prevRef = useRef<{ raws: string[]; htmls: string[] } | null>(null)

  const blocks = useMemo<BlockView[]>(() => {
    if (!markdown?.trim()) {
      prevRef.current = null
      return []
    }
    const tokens = marked.lexer(transformEpigraph(markdown)).filter((t) => t.type !== 'space')
    const prev = prevRef.current
    const n = tokens.length
    const raws: string[] = new Array(n)
    const htmls: string[] = new Array(n)
    const used = new Map<string, number>()
    let headingIdx = 0
    for (let i = 0; i < n; i++) {
      const tok = tokens[i]
      raws[i] = tok.raw
      // 标题每帧重算（id 依赖出现序，复用会把旧 id 叠上去）；非标题块 raw 没变就复用。
      let html =
        prev && prev.raws[i] === tok.raw && tok.type !== 'heading' ? prev.htmls[i] : ''
      if (html === '') {
        html = DOMPurify.sanitize(marked.parser([tok]) as string, {
          ADD_ATTR: [...SANITIZE_ATTRS],
        })
      }
      if (tok.type === 'heading' && (tok.depth === 2 || tok.depth === 3)) {
        const text = tok.text.replace(/[#*`_]+/g, '').trim()
        const id = uniqueSlug(slugifyHeading(text, headingIdx), used)
        headingIdx += 1
        html = html.replace(
          /^<h([23])([^>]*)>/i,
          (_m, lv: string, attrs: string) => `<h${lv} id="${id}"${attrs}>`,
        )
      }
      htmls[i] = html
    }
    prevRef.current = { raws, htmls }
    return htmls.map((html, i) => ({
      // key = 块序:类型。流式只在末尾增长 → 前缀块与当前段的 key 跨帧稳定。
      key: `${i}:${tokens[i].type}`,
      html,
      tail: i === n - 1,
    }))
  }, [markdown])

  if (blocks.length === 0) return null

  return (
    <div className={`md-preview ${className ?? ''}`}>
      {blocks.map((b) => (
        <div
          key={b.key}
          className={`md-preview-block${b.tail && live ? ' live-tail' : ''}`}
          // eslint-disable-next-line react/no-danger
          dangerouslySetInnerHTML={{ __html: b.html }}
        />
      ))}
    </div>
  )
}

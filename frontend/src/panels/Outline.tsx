import { useEffect, useState } from 'react'
import { UnorderedListOutlined } from '@ant-design/icons'

/**
 * 文档大纲栏（v1 的 `lib/mdOutline.ts` + `OutlinePane`）：从正文的 `##` / `###` 解析出
 * 编号目录，点击滚到那一章，滚动时高亮当前章。
 *
 * **编号是 v1 的规矩**：正文里已经写了「1. 作品概览」就沿用，否则按层级自动补
 * `1.` / `2.1`，保证大纲永远有编号。跳转按**标题在正文里的出现序**定位（不是按文本），
 * 同名章节才不会滚错地方。
 */
interface Item {
  level: 2 | 3
  text: string
}

function parseOutline(md: string): Item[] {
  const out: Item[] = []
  let inFence = false
  let chapter = 0
  let section = 0
  for (const line of md.split('\n')) {
    if (line.trimStart().startsWith('```') || line.trimStart().startsWith('~~~')) {
      inFence = !inFence
      continue
    }
    if (inFence) continue
    if (line.trimStart().startsWith(':::')) continue
    const m = /^(##|###)\s+(.+?)\s*$/.exec(line)
    if (!m) continue
    const level = m[1].length as 2 | 3
    const rawTitle = m[2].replace(/[#*`_]+/g, '').trim()
    if (!rawTitle) continue

    // 正文里已经写了编号就沿用，否则按层级自动编号（与 v1 parseOutline 一致）
    const numbered = /^(\d+(?:\.\d+)*)[.、]?\s+(.*)$/.exec(rawTitle)
    let number: string
    let text: string
    if (numbered) {
      number = numbered[1].includes('.') ? numbered[1] : `${numbered[1]}.`
      text = numbered[2].trim()
      const parts = numbered[1].split('.')
      if (parts.length > 1) {
        chapter = Number(parts[0]) || chapter
        section = Number(parts[1]) || section
      } else {
        chapter = Number(parts[0]) || chapter + 1
        section = 0
      }
    } else if (level === 2) {
      chapter += 1
      section = 0
      number = `${chapter}.`
      text = rawTitle
    } else {
      section += 1
      number = chapter > 0 ? `${chapter}.${section}` : `${section}`
      text = rawTitle
    }
    out.push({ level, text: `${number} ${text}` })
  }
  return out
}

export default function Outline({ bodyMd }: { bodyMd: string }) {
  const items = parseOutline(bodyMd)
  const [active, setActive] = useState(-1)

  // 正文滚动时高亮当前章（按标题元素离顶部的距离算，最上面那个就是当前章）
  useEffect(() => {
    const scroller = document.querySelector('.doc-body')
    if (!scroller) return
    const onScroll = () => {
      const heads = [...scroller.querySelectorAll('h2, h3')]
      let current = -1
      for (let i = 0; i < heads.length; i += 1) {
        if (heads[i].getBoundingClientRect().top - scroller.getBoundingClientRect().top <= 24) {
          current = i
        }
      }
      setActive(current)
    }
    onScroll()
    scroller.addEventListener('scroll', onScroll, { passive: true })
    return () => scroller.removeEventListener('scroll', onScroll)
  }, [bodyMd])

  /** 按出现序跳转——同名章节才不会滚到第一处。 */
  function jumpTo(index: number) {
    const scroller = document.querySelector('.doc-body')
    const heads = scroller?.querySelectorAll('h2, h3')
    heads?.[index]?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }

  if (items.length === 0) {
    return (
      <aside className="outline">
        <div className="outline-title">文档大纲</div>
        <div className="outline-empty">
          <UnorderedListOutlined />
          <span>还没有章节</span>
          <small>写上 `## 1. 作品概览` 后，目录会自动出现。</small>
        </div>
      </aside>
    )
  }

  return (
    <aside className="outline">
      <div className="outline-title">文档大纲</div>
      {items.map((it, i) => (
        <button
          key={`${it.level}-${i}-${it.text}`}
          type="button"
          className={`outline-item${it.level === 3 ? ' lv-3' : ''}${i === active ? ' is-active' : ''}`}
          onClick={() => jumpTo(i)}
          title={it.text}
        >
          {it.text}
        </button>
      ))}
    </aside>
  )
}

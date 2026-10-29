import type { ReactNode } from 'react'

/**
 * 实时写入用的**块级 Markdown 渲染**（`docs/design.md §6.3 §7` D64）：
 * 只认我们正文里真正会出现的那几种块（标题 / 说明 / 题记 / 列表 / 表格 / 分隔线 / 段落），
 * 自己写、不引新依赖。末块挂 `live-tail`（压暗 + 紫色脉动光标）。
 */
type Block =
  | { kind: 'h'; level: number; text: string }
  | { kind: 'note'; text: string }
  | { kind: 'epigraph'; lines: string[] }
  | { kind: 'list'; items: string[] }
  | { kind: 'table'; rows: string[] }
  | { kind: 'hr' }
  | { kind: 'p'; text: string }

function parse(body: string): Block[] {
  const out: Block[] = []
  const lines = body.split('\n')
  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i]
    const t = raw.trim()
    if (t === '') continue
    if (t.startsWith(':::epigraph')) {
      const body0: string[] = []
      i++
      while (i < lines.length && lines[i].trim() !== ':::') {
        body0.push(lines[i])
        i++
      }
      out.push({ kind: 'epigraph', lines: body0 })
      continue
    }
    if (t === '---') {
      out.push({ kind: 'hr' })
      continue
    }
    if (t.startsWith('#')) {
      const m = /^(#{1,6})\s+(.*)$/.exec(t)
      if (m) {
        out.push({ kind: 'h', level: m[1].length, text: m[2] })
        continue
      }
    }
    if (t.startsWith('> ')) {
      out.push({ kind: 'note', text: t.slice(2) })
      continue
    }
    if (t.startsWith('|')) {
      const rows: string[] = []
      while (i < lines.length && lines[i].trim().startsWith('|')) {
        rows.push(lines[i].trim())
        i++
      }
      i--
      out.push({ kind: 'table', rows })
      continue
    }
    if (t.startsWith('- ') || /^\d+\.\s/.test(t)) {
      const items: string[] = []
      while (i < lines.length && /^\s*([-*]|\d+\.)\s/.test(lines[i])) {
        items.push(lines[i].trim().replace(/^([-*]|\d+\.)\s+/, ''))
        i++
      }
      i--
      out.push({ kind: 'list', items })
      continue
    }
    out.push({ kind: 'p', text: t })
  }
  return out
}

function inline(text: string): ReactNode {
  // 只处理粗体与行内代码，够实时预览用
  const parts = text.split(/(\*\*[^*]+\*\*|`[^`]+`)/g)
  return parts.map((p, i) => {
    if (p.startsWith('**') && p.endsWith('**')) return <strong key={i}>{p.slice(2, -2)}</strong>
    if (p.startsWith('`') && p.endsWith('`')) return <code key={i}>{p.slice(1, -1)}</code>
    return p
  })
}

export default function LiveMarkdown({ body, live = true }: { body: string; live?: boolean }) {
  const blocks = parse(body)
  return (
    <div className="doc-live-md">
      {blocks.map((b, i) => {
        const tail = live && i === blocks.length - 1 ? ' live-tail' : ''
        switch (b.kind) {
          case 'h': {
            const Tag = (`h${Math.min(b.level, 6)}`) as 'h1'
            return (
              <Tag key={i} className={`live-h live-h${b.level}`}>
                {b.text}
              </Tag>
            )
          }
          case 'note':
            return (
              <div key={i} className="live-note">
                {inline(b.text)}
              </div>
            )
          case 'epigraph':
            return (
              <blockquote key={i} className="live-epigraph">
                {b.lines.map((l, j) => (
                  <div key={j}>{l}</div>
                ))}
              </blockquote>
            )
          case 'hr':
            return <hr key={i} className="live-hr" />
          case 'list':
            return (
              <ul key={i} className={`live-list${tail}`}>
                {b.items.map((it, j) => (
                  <li key={j}>{inline(it)}</li>
                ))}
              </ul>
            )
          case 'table':
            return (
              <table key={i} className={`live-table${tail}`}>
                <tbody>
                  {b.rows.map((r, j) => (
                    <tr key={j}>
                      {r
                        .replace(/^\||\|$/g, '')
                        .split('|')
                        .map((c, k) => (
                          <td key={k}>{c.trim()}</td>
                        ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            )
          default:
            return (
              <p key={i} className={`live-p${tail}`}>
                {inline(b.text)}
              </p>
            )
        }
      })}
    </div>
  )
}

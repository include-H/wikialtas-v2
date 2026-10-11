/**
 * 正文规范化。**规则与 Rust 侧 `domain::text::normalize_markdown` 一致，改一处要改两处。**
 *
 * 为什么前端也要一份：编辑器把「空段落」序列化成一行 `<br />`，如果只让服务端清，
 * 前端手里那份就永远和「已保存的那份」不相等——状态条会一直显示"有改动"。
 *
 * 保留行内的硬换行（`上<br />下`）。
 */
export function normalizeMarkdown(md: string): string {
  // 与 Rust 的 `str::lines()` 对齐：末尾换行不算一行，否则两边规范形不一致
  const lines = md.split('\n')
  if (lines.length > 0 && lines[lines.length - 1] === '') lines.pop()

  const out: string[] = []
  let blankRun = 0
  for (const line of lines) {
    const t = line.trim()
    if (t === '<br />' || t === '<br/>' || t === '<br>') continue
    if (t === '') {
      blankRun += 1
      if (blankRun > 1) continue
    } else {
      blankRun = 0
    }
    out.push(line.replace(/\s+$/, ''))
  }
  return out.map((l) => l + '\n').join('')
}

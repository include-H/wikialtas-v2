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

/**
 * 比较用的规范形（**只用于判断"有没有改"，不影响存什么**）。
 *
 * 编辑器（Milkdown）会把列表符号统一成 `*`，而服务端存的是原文（可能是 `-`）。
 * 于是「刚装载、没动过」也会被判成 dirty → 1.5 秒后凭空落一条草稿。
 * 这里把三种列表符号都归一到 `-` 再比，装完就不算改动。
 */
export function normalizeForCompare(md: string): string {
  return normalizeMarkdown(md).replace(/^(\s*)[-*+]\s+/gm, '$1- ')
}

/** 两份 markdown 是不是"同一篇"（忽略编辑器归一化的序列化差异）。 */
export function sameDoc(a: string | null, b: string | null): boolean {
  if (a === null || b === null) return a === b
  return normalizeForCompare(a) === normalizeForCompare(b)
}

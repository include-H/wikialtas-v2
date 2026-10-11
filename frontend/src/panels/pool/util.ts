import type { LibrarySource, PoolEntry } from '../../api'

export const SOURCE_LABEL: Record<LibrarySource, string> = {
  gameatlas: 'GameAtlas',
  emby: 'Emby',
  komga: 'Komga',
}

export const KIND_LABEL: Record<string, string> = {
  game: '游戏',
  movie: '电影',
  tv: '剧集',
  book: '书',
  collection: '合集',
}

export const SOURCE_TAG_COLOR: Record<LibrarySource, string> = {
  gameatlas: 'blue',
  emby: 'geekblue',
  komga: 'green',
}

export interface Group {
  key: string
  source: LibrarySource
  container_title: string
  container_kind: string
  role: string
  entries: PoolEntry[]
}

/** 按「源 → 容器」分组；keyword 只过滤标题（本地过滤，清单已在手）。 */
export function groupByContainer(rows: PoolEntry[], keyword: string): Group[] {
  const kw = keyword.trim().toLowerCase()
  const kept = kw
    ? rows.filter(
        (p) =>
          p.entry.title.toLowerCase().includes(kw) ||
          p.entry.aliases.some((a) => a.toLowerCase().includes(kw)),
      )
    : rows
  const map = new Map<string, Group>()
  for (const p of kept) {
    const key = `${p.entry.source}|${p.entry.container_key}`
    let g = map.get(key)
    if (!g) {
      g = {
        key,
        source: p.entry.source,
        container_title: p.entry.container_title,
        container_kind: p.entry.container_kind,
        role: p.role,
        entries: [],
      }
      map.set(key, g)
    }
    g.entries.push(p)
  }
  return [...map.values()].sort((a, b) => b.entries.length - a.entries.length)
}

export function relative(iso: string): string {
  const min = Math.round((Date.now() - new Date(iso).getTime()) / 60_000)
  if (min < 1) return '刚刚'
  if (min < 60) return `${min} 分钟前`
  const h = Math.round(min / 60)
  return h < 24 ? `${h} 小时前` : `${Math.round(h / 24)} 天前`
}

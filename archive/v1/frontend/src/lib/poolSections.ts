import type { PoolEntry, PoolGroup, WorkSummary } from '../types'
import { SOURCE_ORDER } from './mediaTags'

/** 规范化：去全部空白（含全角空格）、全角字母数字转半角、大小写归一。 */
function norm(s: string): string {
  return s
    .replace(/[\s\u3000]/g, '')
    .replace(/[Ａ-Ｚａ-ｚ０-９]/g, (c) => String.fromCharCode(c.charCodeAt(0) - 0xfee0))
    .toLowerCase()
}

/** 同名时优先当"容器"的节点类型：系列/宇宙 > 合集 > 单作。 */
const KIND_RANK: Record<string, number> = { universe: 0, series: 1, collection: 2, work: 3 }
const rank = (n: WorkSummary) => KIND_RANK[n.kind] ?? 9

/**
 * 标题 → 该作品在 Wiki 里已有的节点：**标题或别名**规范化后精确同名。
 *
 * 只做精确匹配：不做模糊、也不问模型。理由两条——
 *  ① 模糊匹配的误配代价是"一次建错 N 个节点"；
 *  ② "这条值不值得写"本来就不该由模型替你判断（树只有几十个节点，库里上千条未挂链，
 *     "这条该挂到哪"在大多数条目上根本问不出来）。
 * 译名对不上时的正确修法是**给节点补别名**，一次修、永久生效——不是每次判读让模型猜。
 */
export function indexWikiNodes(nodes: WorkSummary[]): Map<string, WorkSummary> {
  const m = new Map<string, WorkSummary>()
  for (const n of nodes) {
    for (const t of [n.title, ...(n.aliases ?? [])]) {
      const k = norm(t ?? '')
      if (!k) continue
      const cur = m.get(k)
      if (!cur || rank(n) < rank(cur)) m.set(k, n)
    }
  }
  return m
}

export function matchNode(
  title: string | null | undefined,
  index: Map<string, WorkSummary>,
): WorkSummary | undefined {
  if (!title) return undefined
  return index.get(norm(title))
}

/**
 * 是不是"占位容器"：扫描层给 GameAtlas 里**没有系列的散游戏**造了一个假分组
 * （`library_scan.go`：containerKey `series:none` / 标题 `（无系列）`）。
 * 它只是分组用的显示名，**不能拿去当系列节点名**——否则会建出一个叫「（无系列）」的节点。
 */
export function isPlaceholderContainer(title: string): boolean {
  const t = title.trim()
  return t === '' || /^（.*）$/.test(t)
}

export interface PoolSection {
  group: PoolGroup
  items: PoolEntry[]
  /** 容器名命中的节点：GA/Komga 的 series 容器 → 一键把整组建到/挂到它下面。 */
  matched?: WorkSummary
  /** 条目各自命中的节点（key → node）：这些在 Wiki 已有条目，挂上去就行，不用建档。 */
  hitByKey: Map<string, WorkSummary>
  hits: number
  /** 容器是不是"作品系列"（GA/Komga=series，且不是"（无系列）"那种占位）。
   *  Emby 的容器是**库**（电影/番剧），拿库名去建节点毫无意义。 */
  seriesLike: boolean
  /** 占位容器（散游戏）：没有系列可用 → 整组按"不归属"直接建节点。 */
  standalone: boolean
  /** ① 有现成归处（容器同名或组内有命中）→ 展开；② 其余 → 收进"其余"折叠 */
  tier: 1 | 2
}

/**
 * 把容器分成"有现成归处的"与"其余"：
 *   档 1 = 容器名对上节点（一键），**或**组里有条目在 Wiki 已有节点（挂上就行）；
 *   档 2 = 其余（整组都是"要新开的"），按未挂链数降序，默认折叠。
 */
export function splitPoolSections(
  groups: PoolGroup[],
  nodes: WorkSummary[],
  visible: (g: PoolGroup) => PoolEntry[],
): { featured: PoolSection[]; restBySource: { source: string; sections: PoolSection[]; count: number }[] } {
  const index = indexWikiNodes(nodes)
  const featured: PoolSection[] = []
  const rest: PoolSection[] = []

  for (const g of groups) {
    const items = visible(g)
    if (items.length === 0) continue
    const hitByKey = new Map<string, WorkSummary>()
    for (const e of items) {
      const n = matchNode(e.title, index)
      if (n) hitByKey.set(`${e.source}:${e.publicId}`, n)
    }
    const section: PoolSection = {
      group: g,
      items,
      matched: matchNode(g.containerTitle, index),
      hitByKey,
      hits: hitByKey.size,
      seriesLike: g.containerKind === 'series' && !isPlaceholderContainer(g.containerTitle),
      standalone: isPlaceholderContainer(g.containerTitle),
      tier: 2,
    }
    if (section.matched || section.hits > 0) {
      section.tier = 1
      featured.push(section)
    } else {
      rest.push(section)
    }
  }

  // 排序：容器名对上节点的排最前（一个按钮清一整组），再按组内"Wiki 已有"的条数，
  // 最后按体量。不是按体量排——88 条的 Emby 库看着大，但一条都挂不上。
  featured.sort((a, b) => {
    const am = a.matched ? 1 : 0
    const bm = b.matched ? 1 : 0
    return bm - am || b.hits - a.hits || b.items.length - a.items.length
  })
  const bySize = (a: PoolSection, b: PoolSection) => b.items.length - a.items.length
  const restBySource = SOURCE_ORDER.map((src) => {
    const sections = rest.filter((s) => s.group.source === src).sort(bySize)
    return { source: src as string, sections, count: sections.reduce((n, s) => n + s.items.length, 0) }
  }).filter((x) => x.sections.length > 0)

  return { featured, restBySource }
}

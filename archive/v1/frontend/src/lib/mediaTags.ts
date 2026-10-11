// 媒体库组件共用的标签词表与配色（来源 / 类别 / 形态 / 可信度）。
//
// 配色纪律（VISUAL_SPEC §2）：紫色是 AI 唯一强调色，媒体库这些展示性标签
// 一律不许用紫系（purple / violet / indigo）——kind=blue、format=cyan、
// source=grey（中性，不跟类别色抢视线）、可信度=绿/橙/灰三档。
export const SOURCE_LABEL: Record<string, string> = {
  gameatlas: 'GameAtlas',
  emby: 'Emby',
  komga: 'Komga',
}

/** 介质 → 正片位所在库：一部作品只有一个载体，正片条目只可能落在对应库。
 * 电影/剧集→Emby、游戏→GameAtlas、书/漫画→Komga；介质不明 → undefined（三源皆可）。 */
const MEDIUM_SOURCE: Record<string, string> = {
  game: 'gameatlas',
  movie: 'emby',
  tv: 'emby',
  manga: 'komga',
  book: 'komga',
}

export function homeSourceOf(medium: string | null | undefined): string | undefined {
  return medium ? MEDIUM_SOURCE[medium] : undefined
}

/** 分栏顺序：与后端 sourceOrder 一致。 */
export const SOURCE_ORDER = ['gameatlas', 'emby', 'komga'] as const

export const KIND_LABEL: Record<string, string> = {
  game: '游戏',
  tv: '剧集',
  series: '剧集', // Emby 搜索结果的 kind 值
  movie: '电影',
  book: '书籍',
  collection: '合集',
  album: '专辑',
}

/** 作品节点的介质（medium）：待写正文列表等处要显示成人话。 */
export const MEDIUM_LABEL: Record<string, string> = {
  game: '游戏',
  movie: '电影',
  tv: '剧集',
  manga: '漫画',
  book: '书籍',
  other: '其他',
}

export const FORMAT_LABEL: Record<string, string> = {
  comic: '漫画',
  novel: '小说',
}

/** 角色标签：work=正片级 / mix=混杂级。 */
export const ROLE_LABEL: Record<string, string> = {
  work: '正片级',
  mix: '混杂',
}

/** 挂链出处：谁建了这条链。 */
export const LINKED_BY_LABEL: Record<string, string> = {
  sweep: 'Altas 建议确认',
  pool: 'AI 对一遍确认',
  manual: '手动关联',
  archive: '建档时挂上',
  legacy: '历史关联',
}

/** 可信度 → Tag 颜色：≥0.85 绿 / ≥0.6 橙 / 其余灰。 */
export function confColor(confidence: number): 'green' | 'orange' | 'grey' {
  if (confidence >= 0.85) return 'green'
  if (confidence >= 0.6) return 'orange'
  return 'grey'
}


/**
 * 题记排版：移植自 v1（`archive/v1/frontend/src/lib/epigraph.ts`，其源头是 GameManager
 * 的 `buildEpigraphHtml`）——见 `archive/v1/VISUAL_SPEC.md` §3：
 * **无底色、无边框**，只有一对对角装饰引号、居中排布、分行排版、署名右对齐。
 *
 * 核心观感：**外文原文小字、中文译文大字**，逐行交替。原文可能是日文、韩文、西里尔……
 * 不只拉丁——所以判据是「这一行是不是中文」，不是「是不是拉丁」。
 */

export type EpigraphLineType = 'display' | 'latin'

/** 拉丁字母（含拉丁扩展 A/B、重音字符）。 */
const LATIN_SCRIPT = /[A-Za-zÀ-ɏḀ-ỿ]/

/** 汉字（含扩展 A、兼容汉字）。这是「中文行」的判据。 */
const HAN_SCRIPT = /[㐀-䶿一-鿿豈-﫿]/

/** 明确的非中文书写系统：假名、韩文、西里尔、希腊、希伯来、阿拉伯、天城文、泰文。
 *  优先级高于汉字——日文行夹汉字，必须先靠假名认成日文，才能降到小字。 */
const NON_CHINESE_SCRIPT =
  /[぀-ヿㇰ-ㇿᄀ-ᇿ㄰-㆏가-힯Ѐ-ӿͰ-Ͽ֐-׿؀-ۿऀ-ॿ฀-๿]/

/** 分行/分段的句读标点（中文、日文、拉丁）。 */
const PUNCTUATION = '，。！？；：、,.!?;:｡､'

function classifyLine(line: string): EpigraphLineType {
  if (NON_CHINESE_SCRIPT.test(line)) return 'latin'
  if (HAN_SCRIPT.test(line)) return 'display'
  if (LATIN_SCRIPT.test(line)) return 'latin'
  return 'display' // 纯数字/符号行按大字处理（与原始实现一致）
}

/** 中文行按标点切成「不换行小段」，让居中排版在换行处更整齐。 */
function splitCnSegments(line: string): { text: string; punctuation: string }[] {
  const normalized = line.replace(/\s+/g, '')
  const segments = normalized
    .split(new RegExp(`(?<=[${PUNCTUATION}])`, 'u'))
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
  const parts = segments.length > 0 ? segments : [normalized]
  return parts.map((part) => {
    const m = part.match(new RegExp(`^(.*?)([${PUNCTUATION}]+)?$`, 'u'))
    return { text: m?.[1] ?? part, punctuation: m?.[2] ?? '' }
  })
}

/** 按内容长度选字号/字距（原实现的三档规则）。 */
function cnTypography(line: string): { fontSize: string; letterSpacing: string } {
  const contentLength = line
    .replace(/[\s，。！？；：、｡､、“”‘’「」『』（）()《》〈〉【】〔〕,.!?;:]/gu, '')
    .length
  if (contentLength >= 42) {
    return { fontSize: 'clamp(0.92rem, 0.72vw + 0.68rem, 1.08rem)', letterSpacing: '0.03em' }
  }
  if (contentLength >= 24) {
    return { fontSize: 'clamp(0.98rem, 0.9vw + 0.66rem, 1.2rem)', letterSpacing: '0.05em' }
  }
  return { fontSize: 'clamp(1.04rem, 1.15vw + 0.68rem, 1.32rem)', letterSpacing: '0.07em' }
}

export interface EpigraphLine {
  type: EpigraphLineType
  text: string
  segments: { text: string; punctuation: string }[]
  style: { fontSize: string; letterSpacing: string }
}

export interface Epigraph {
  lines: EpigraphLine[]
  author: string
}

const AUTHOR_LINE = /^(?:--|---|——)\s*(.+)$/

/** 题记围栏内容 → 排版模型（最后一行以 ——/--/--- 开头时视为署名）。 */
export function parseEpigraph(text: string): Epigraph {
  const lines = text
    .split('\n')
    .map((l) => l.trim())
    // 围栏标记本身（`:::epigraph` / `:::`）不是内容——不管调用方有没有先剥掉，
    // 这里再兜一层：它漏出来过一次（题记上方凭空多一行 `:::epigraph`）。
    .filter((l) => Boolean(l) && !l.startsWith(':::'))
  let author = ''
  const last = lines[lines.length - 1]
  if (last) {
    const m = last.match(AUTHOR_LINE)
    if (m) {
      author = m[1].trim()
      lines.pop()
    }
  }
  return {
    author,
    lines: lines.map((line) => {
      const type = classifyLine(line)
      return { type, text: line, segments: type === 'display' ? splitCnSegments(line) : [], style: cnTypography(line) }
    }),
  }
}

/* ================================================================ 文档头 */

/**
 * 文档头 = **主标题 → 说明块 → 题记**（`archive/v1/VISUAL_SPEC.md` §1.4 的三样），
 * 它们各有渲染形态，所以不进正文编辑器；**原文逐字留着**，保存时原样接回去。
 *
 * 注意：题记**不在正文第一行**——骨架是「`# 标题` → `> 说明：…` → `:::epigraph`」，
 * 所以这里按「块」扫，而不是只认开头那一处。
 */
export interface DocHeader {
  /** `# 标题` 那一行（含换行） */
  titleLine: string
  /** `> 说明：…` 那一段（含换行） */
  noteBlock: string
  /** `:::epigraph … :::`（含换行） */
  fence: string
}

export interface SplitDoc {
  header: DocHeader
  /** 交给编辑器的正文（编号章节那段） */
  rest: string
}

const FENCE_BLOCK = /^:::epigraph[ \t]*\n[\s\S]*?\n:::[ \t]*(\n|$)/

/** 把正文切成「文档头」+「正文」。切不出头也能用（header 全空、rest = 原文）。 */
export function splitDoc(body: string): SplitDoc {
  const header: DocHeader = { titleLine: '', noteBlock: '', fence: '' }
  let rest = body

  // 吃掉开头的连续空行，但**原样留着**——丢了往返就对不上（实测差的就是这几行）
  const takeBlanks = (): string => {
    const m = /^[ \t]*\n(?:[ \t]*\n)*/.exec(rest)
    if (!m) return ''
    rest = rest.slice(m[0].length)
    return m[0]
  }
  const absorb = (tail: string) => {
    if (!tail) return
    if (header.fence) header.fence += tail
    else if (header.noteBlock) header.noteBlock += tail
    else if (header.titleLine) header.titleLine += tail
    else rest = tail + rest
  }

  let lead = takeBlanks()

  // 1) 主标题
  const title = /^#[ \t]+[^\n]*\n/.exec(rest)
  if (title) {
    header.titleLine = lead + title[0]
    lead = ''
    rest = rest.slice(title[0].length)
  }

  // 2) 说明块（`> …`）。顺序固定：说明在前、题记在后（19 篇基准都是这个序；
  //    万一是倒的，题记就留在编辑器里——不丢东西，只是没渲染成书页样式）。
  const noteGap = takeBlanks()
  const note = /^(?:>[^\n]*\n)+/.exec(rest)
  if (note) {
    header.noteBlock = noteGap + note[0]
    rest = rest.slice(note[0].length)
  } else {
    rest = noteGap + rest
  }

  // 3) 题记围栏
  const fenceGap = takeBlanks()
  const fence = FENCE_BLOCK.exec(rest)
  if (fence) {
    header.fence = fenceGap + fence[0]
    rest = rest.slice(fence[0].length)
  } else {
    rest = fenceGap + rest
  }

  // 4) 文档头之后的空行：归到最后一个块，别丢给编辑器（编辑器会把它规整掉）
  absorb(takeBlanks())
  if (lead) rest = lead + rest

  return { header, rest }
}

/** 把文档头接回正文前面（顺序：标题 → 说明 → 题记）。丢了哪块就不补哪块。 */
export function joinDoc(header: DocHeader, rest: string): string {
  return `${header.titleLine}${header.noteBlock}${header.fence}${rest}`
}

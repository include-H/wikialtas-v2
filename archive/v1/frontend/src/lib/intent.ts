import type { RunIntent } from '../types'

/**
 * 一条消息是不是在让我**产出/改动内容**。
 *
 * 这是唯一的一维判断（"有没有在派活"），不是按话题分档——所以规则只有一条，
 * 而不是"哪些话题算写哪些算聊"的一堆例外。
 *
 * 两个容易划偏的地方，都在这条正则里：
 *   - `写` 后跟「得/的」是**评价**不是派活（"这句写得怎么样"）；
 *   - `补`/`改` 光带宾语就是派活（"补一下""改一句"），不能只认"补充/改写"。
 *
 * 媒体库指令（"把 X 挂到 Y""关联上"）也是派活：动的是结构不是正文，
 * 但同样需要写工具面（attach_library_link），判成聊天会让它拿不到工具。
 */
const WRITE_VERBS =
  /(写(?!得|的)|新建|建个|建立|建档|创建|新增|补(?!丁)|扩写|扩充|改写|重写|整理|完善|润色|修订|继续|接着|生成|做一?篇|来一?篇|起稿|落笔|加一?段|加一?章|调整|改|挂到|挂上|挂进|关联到|关联上|绑到)/

/**
 * 像不像"在提问/要一次性输出"（对齐工具栏的翻译/解释动作，判成 answer）。
 *
 * 注意别把祈使句卷进来："你**可以**直接去搜资料"里的"可以"不是疑问词，
 * 所以这里不收录裸"可以/能不能"之外的模糊词；只认疑问句式和明确的"要一段输出"。
 */
const ASK_MARKERS =
  /[?？]|(吗|呢)[。！!？?]?\s*$|什么|怎么|为什么|如何|是否|多少|哪[个些里儿]|谁|几[个部年代岁]|介绍|讲讲|说说|解释|翻译|总结/

export interface IntentContext {
  docMode?: string
  workId?: string | null
  docId?: string | null
}

/**
 * 从上下文推工单意图。意图只影响**任务框定与 skill 预载**（写不写由模型按
 * 用户消息判断，见系统提示的 This task 段）——**不决定能力面**：写工具只被两种
 * 用户显式选择剥掉（只读模式、oneShot 一次性入口）。"问答不落盘"靠 oneShot
 * （结构性）+ 提示（一次性输出不落盘）+ 可回滚修订兜底，不靠意图标签。
 *
 * 判定顺序：
 *   1. 只读模式 → answer（用户显式选的，不是猜测）；
 *   2. 有写类动词 → 写作意图；
 *   3. 像提问 → answer；
 *   4. 作品/资料页上，既没写动词也不像提问 → **按继续做事处理**。
 *      单字打错（"谢"："写"）、写作中途的纠偏语（"重来，直接去搜资料"）都在这。
 *      以前这一步兜底成聊天：一个错字整单就拿不到写工具，只能重说一句——
 *      猜错的代价其实不对称。
 *   5. 没有页面上下文（如设置页）→ 保守，answer。
 */
export function inferIntent(goal: string, ctx: IntentContext): RunIntent {
  if (ctx.docMode === 'read') return 'answer'
  if (WRITE_VERBS.test(goal)) {
    if (ctx.docId) return 'write_doc'
    if (ctx.workId) return 'continue_wiki'
    return 'create_wiki'
  }
  if (ASK_MARKERS.test(goal)) return 'answer'
  if (ctx.docId) return 'write_doc'
  if (ctx.workId) return 'continue_wiki'
  return 'answer'
}

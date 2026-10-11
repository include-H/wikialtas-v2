# SSE × Semi 原生对齐：追踪（2026-09-14 起）

> **定位**：给"流式对话 + 正文流"这一坨做**原生对齐**的执行追踪。
> 姊妹篇：`docs/HARNESS_DISPLAY.md`（dsh 展示模型对照）、`DESIGN_V2.md` §4.10 / §6.2（为什么归约交给组件库、事件表与 sequence_number 硬约束）。
> 本文件是**活的 todo**，做完一项勾一项并回填"实测"。

## 0. 一句话

用"**原生**"当尺子量了三层（协议 / SSE 语义 / 组件消费）。协议层是原生的（甚至比裸用更好）；
组件消费层有 **3 处偏离原生**，外加 1 处是 **Semi 归约器没跟上 OpenAI 规范**的 gap。
感知到的"卡 / 闪回 / 手搓打字机 / 正文整篇重渲"几乎都能落到这几处，**不是 SSE 天生和前端不合**。

- "原生" = 每条线都用**它该用的那份契约**，不手搓平行体系。
- 对话（气泡）应 100% 走 OpenAI Response 协议 + Semi 归约器 + Semi 渲染。
- 正文（编辑器）**不是对话**，是文档：目的地是可编辑文档（题记块 / 大纲 / 修订回滚），
  走独立通道**本身是对的**，别硬塞进 Semi 对话气泡；问题只在这条通道实现得不够原生。

---

## 1. 坐实的发现（每条带证据，改前别忘回看）

### F1 A 线全量归约 = O(n²)　【要改 · 最高 ROI】
`frontend/src/lib/responses.ts` 的 `runToMessage` 每次 `streamingResponseToMessage(chunks)`
**不传 `prevState`**，且 `responsesChunks` 每次 `.sort()` + **重编号** `sequence_number: i`。
`AiPanel.tsx` 的 `chats = useMemo([history, events, run])`，`events` 每来一个 SSE 事件就变
（`mergeEvent` 追加）→ **每帧一次 O(n log n) 全量重归约，累积 O(n²)**。

- Semi 归约器**原生就是增量**：返回 `{ message, nextState }`，`processedSeq: Set<seq>` 去重、
  `outputs: Map<index, ContentItem>` 按位 append（源码 `dataAdapter/streamingResponseToMessage.ts`）。
  我们把它最该用的 `prevState` 丢了，换来"幂等全量重算"的方便，代价是长工单卡死。
- **因果链**：F1 卡死 → 几千条 `function_call_arguments.delta` 不能转发 → 后端**被迫**把正文
  参数从工具调用里抠出来走旁路（F3）→ 正文失去 Semi 原生表达。**F1 是因，F3 是果。**

### F2 reasoning 是"假流式"，打字机是它的补偿　【要决策】
`backend/internal/run/responses.go` 的 `reasoningDelta` 发
`response.reasoning_summary_text.delta`（payload `{output_index, summary_index, delta}`）。
但 Semi `processChunk` 的 switch **只认** `reasoning_summary_part.added/done` 和
`reasoning_text.delta/done` —— **不认** `reasoning_summary_text.delta`，落 `default` 被静默忽略。

- 对比：对话正文 `textDelta` 发 `response.output_text.delta`，Semi **认**（`text += delta`）→ **真流式逐字长**。
- 结果：reasoning 的流式 delta 一条没进归约器，直到 `closeReasoning` → `output_item.done`
  **整段**塞入 → `frontend/src/lib/smoothing.ts` 的 `useSmoothedChats` 打字机把它"摊开"。
- **`response.reasoning_summary_text.delta` 恰恰是 OpenAI Responses 规范里 summary 流式的正确事件名。**
  所以**后端是原生的，是 Semi 归约器没实现这个 case**（详见 §3 决策）。
- `useSmoothedChats` 注释把锅甩给"网关偶尔砸 430 字"，但 reasoning 那条主因是 delta 被吞。
> **更正（2026-09-16，semi_mcp 源码 + 396 条真实事件 node 回放验证）**：当前 `2.89.x` 的
> `streamingResponseToMessage` **认** `response.reasoning_summary_text.delta`（填
> `item.summary[summary_index].text`），reasoning 本来就是真流式，不是"整段一次性"。
> 上面"归约器没实现这个 case"只对旧版本成立；以现装版本为准，F2 的"假流式"结论作废，
> 打字机（`useSmoothedChats`）删除的决定不受影响——真流式之下更没有存在的必要。

### F3 正文旁路 + `MarkdownPreview` 整篇重渲　【要改 · 但"走独立通道"本身对】
`contentArgsStream`（responses.go:447）把 `write_content/patch_section` 参数从
`function_call_arguments` 里抠出，走 `wikiatlas.content.delta` + 200ms 节流；前端
`lib/liveDraft.tsx` 的 `push` **每段一次 `setState`**；`MarkdownPreview.tsx` 每次
`marked.parse` + `DOMPurify` + 全文正则补 id + `dangerouslySetInnerHTML` **整篇重挂**。
- "正文走独立编辑器通道"是对的（它不是对话）；不够原生的是"每段 setState + 整篇重渲"。
- 该做：**rAF 合流**（delta 累进 ref，每动画帧一次 setState）+ **增量块渲染**（见 T5）。

### F4 `delete msg.output_text`　【不用改 · 记录清楚即可】
Semi `dialogueContent.tsx`：`const textContent = ... : message.output_text; if (textContent) 只渲染整段 else if (Array.isArray(content)) 逐项渲染`。
只要 `output_text` 非空，`content[]`（reasoning/tool/正文）**整个被跳过**；而归约器**必然**拼出
`output_text`。所以 `runToMessage` 的 `delete msg.output_text` 是**要展示结构化 content[] 的唯一
正确适配**（`renderDialogueContentItem.default` 也只拿得到文本字符串、拿不到 content[]，救不了）。
**保留，注释已充分，别再当 bug 改。**

### F5 SSE `catch {}` 静默丢帧 + 无 gap 检测 + 游标推进 bug　【已修 T2】
`frontend/src/lib/sse.ts` 三处偏离原生韧性：
1. `catch { /* ignore malformed */ }` 静默丢帧，无上报。
2. 无 seq 空洞检测——断线靠 `Last-Event-ID` 续传（对），但中间丢的帧**永久没了**。
3. **游标推进 bug（真正确性缺陷）**：原代码在 `try` **之前**就 `lastEventId = me.lastEventId`——
   畸形帧也推进游标，于是"推进了却没处理"= 这条 seq 永久丢失（续传从它之后开始，回补不到）。
原生韧性 = 游标只在成功交付后推进 + gap 检测 → rebaseline（重拉窗口回补权威事件）。

### F6 narrative/answer 工具 6 单 0 调用：模型"闷头苦干"　【要改后端提示词】
用户观察（用 Codex 自己举例）："模型思考完/调工具不汇报想干嘛，一股劲闷头干"。
查证（近 6 单 `run_events` 全量统计）：
- **narrative 调用 = 0，answer 调用 = 0**（全 6 单）。樱花大战4 单 1176 条事件、19 个
  function_call（search_web×16 / fetch_url / write_content / edit×4 / upsert×2），
  **没有一条 narrative**；最后的完工总结是直接吐 `message` output item，也没走 answer 工具。
- 基础设施**全在**：`narrative` 工具存在（`tools/librarian.go:1167`，Desc 写了
  "stating only what you are about to do"）；`executor.go:231` 把 narrative 翻成 message 项；
  前端能渲染。缺的不是通道，是**"何时开口"的行为触发**。
- 提示词（`prompt.go:112`）只有一句**否定式**约束："Speak to the user only through the
  narrative tool; never dump chain-of-thought"——声明了通道，但**没有肯定式**"动手前先说一句"。
  skill（wiki-writing）里零相关引导。模型是 Qwen。
- **结论**：这是提示词 gap（不是渲染问题）。修法 = 在系统提示加一条**原则**（讲道理，不列个例，
  符合 AGENTS.md 维护纪律）："用户看不到你的思考和工具调用，run 进行中 narrative 是你唯一的
  进度信号；每个新阶段（检索/读/写）开始前先说一句"。属宿主/流程，进 prompt.go 不进 skill。
- **与"思考空白"收敛**：narrative 修复是天花板（模型主动开口），"工作中"指示器（T8）是地板
  （模型不开口也不显得死）。两层都原生（一个用现成 narrative 通道，一个用 run 状态）。

> **后续更正（本次）**：`narrative` / `answer` 两个交付工具**已整体退役**。F6 的诊断
> （"模型不开口"）是对的，但根因比"缺肯定式原则"更深一层：**是提示词里"只能走
> narrative/answer"这条通道限制**把模型自己的 `message` 项堵死了。真跑实测（同一网关、
> 同一问题）：把通道限制换成"你的回复就是用户读到的话"之后，模型立刻在 content 里自己
> 说话——`commentary`（带工具调用时）+ `final_answer`（不带工具调用时）天然成立，那正是
> Responses 规范的原生区分位。随之删掉的还有一整套脚手架（controlText 投影 / lastNarrative
> 去重 / "带工具调用时正文超 300 字丢弃" / answer 交付注记与宽限轮 / 语言判官 replyNonChinese），
> 它们都是为绕开那个堵点而建的。所以 T9 的结论（"加一条肯定式原则"）**已不适用**：
> 不需要原则，只需要别禁。

### F7 "思考卡住/被截断"：数据与管线是好路，问题在渲染层观感　【已修 T10】
现象：用户实测"LLM 会回答了（message 流正常），可思考还是卡住"；且 tool call 后的思考
应读作**全新的一块**，不应混在一起/看不出来。逐层取证（全部真实数据/源码/实测，非猜测）：

1. **数据层完整**：TEST 单 `01a09f93` 两个 reasoning——`rs_0`（output_index=0）26 个 delta
   共 500 字、`rs_3`（tool call 之后）207 个 delta 共 3500 字，`output_item.done` 时
   summary 全文落库、status=completed；樱花大战 4 单同理（4 块全完整）。
2. **归约器正确**：396 条 response.* 事件逐帧喂 `streamingResponseToMessage`（node 回放）
   → 两个独立块（不混）、文本单调增长、每帧新 message 引用。
3. **Semi 渲染层复现**（同版本 harness，vite build + 无头 Chromium，2 条/60ms 重放全流）：
   内置 `ReasoningWidget`（`Collapsible` 量高裁剪）渲染正常、块高 1:1 跟随、
   头部"正在思考中…→已思考完成"正常切换 → **常规节奏下内置实现不出问题**。
4. **真实 App 活体**（登录态 Playwright + 真实工单）：短思考逐字流式正常。

所以"卡住"是**三重观感因素叠加**，不是丢帧/丢数据：
- **a. Collapsible 量高裁剪**：思考正文在 class 组件 + ResizeObserver 量出的动态高度 +
  `overflow:hidden` 里；长思考 × 每 delta 整段 MDX 重编时，量高滞后/竞态就把正文裁在
  旧高度里——重负载/后台标签/弱设备上出现即"截断"。结构性的裁剪层=结构性的风险。
- **b. 完成后自动折叠且零提示**：`defaultOpen = status!=='completed'`——刷新/终态后
  每个思考块只剩一行"已思考完成"，用户要花好的思考不可见，读作"被截断/卡住"。
- **c. 工具长时静默**：`write_content` 期间正文在编辑器长、对话零输出（设计如此，
  AGENTS.md 正文流）——分钟级对话空白即"闷头苦干"。地板层=T8 工作中指示器；
  天花板=T9 narrative 提示词。

修法见 T10（官方扩展点换思考块渲染器，拆掉量高裁剪层 + 完成折叠带字数提示）。

## 2. reasoning 的"原生冲突"（决策点，动手 T4 前必须拍板）

"既尊重 OpenAI 规范、又想 Semi 原生"在 reasoning 流式上**对不齐**（Semi 没实现那个 delta case）。三选一：

- **A. 前端事件名归一**：`chunkOf` 把 `reasoning_summary_text.delta` 映射成 Semi 认的形式
  → reasoning 真流式逐字。代价：`payload 就是规范事件对象` 的纯净性破一点。
- **B. 接受 reasoning 整段，删打字机**（推荐）：reasoning 本来就不必逐字（"思考中…→ 整段"是
  常见产品形态）。最省事、最"尊重规范"、少一层手搓。output_text 那条是真流式，不依赖打字机。
- **C. 后端改 reasoning 命名**：不尊重 OpenAI 规范，**不选**。

> 决策：**B**（已定，2026-09-14）。output_text 本就真流式逐字，reasoning 一次性显示可接受；打字机是多余的补偿，删。

---

## 3. TODO（按 ROI 排；全朝"原生"收敛）

### 已拍板（2026-09-16，不再复议）
| 项 | 决定 | 依据 |
| ---- | ---- | ---- |
| 灌节点批量（`upsert_works`） | ❌ 不做，维持 single-step | single-step 提示行已入 golden（判定面要求逐步消化），批量建节点把多个决定合成一次裁决 |
| A2 收尾通道语义 | ⏸ 维持现状 | 非本线放大器，无增量需求 |
| 质量门噪声检测（混语 token 密度 → 低质标） | ⏸ **待用户拍板** | "越修越脏"的闭环已被 T11 三原则在入口掐掉；质量门是独立的原则级增补（哈尔事件选项 3） |

### [x] T1 A 线增量归约（F1）—— 先做，最高 ROI、风险最低
- 目标：`events → assistant message` 从"每帧 O(n log n) 全量"降到"每帧 O(1) 增量"。
- 设计（新文件 `frontend/src/lib/reduceRun.ts`，别把 AiPanel / responses.ts 撑更长）：
  - useRef 持有归约器 `prevState` + `reducedUpToSeq` 高水位。
  - 新 run（`run.id` 变）/ 刷新接管：reset，**一次性全量**归约（seq 基本连续，空洞靠归约器 MAX_GAP=10 容错），随后转增量。
  - 流式中：`useEffect([events])` 取 `seq > reducedUpToSeq` 的 `response.*` 事件，按 seq 排序，
    逐个喂 `streamingResponseToMessage([chunk], prevState)`（**用稳定 sequence_number = seq-1，不重编号**），
    存回 `nextState`，`setState(assistantMessage)`。
  - **关键**：喂归约器的 seq 必须**稳定**（跨帧一致），否则 `processedSeq` 去重失效、重复 append。
    流式中 seq 连续无空洞 → 不需重编号；只有全量接管那一次可能需要（交给归约器容错）。
  - `runMessages` 拆成"归约（增量 state）+ 组合（history + user + assistant + boundary notices，保持 useMemo 派生）"。
- 涉及：`frontend/src/lib/reduceRun.ts`（新）、`responses.ts`（runMessages 拆）、`AiPanel.tsx`（chats 依赖换增量 state）。
- 验证：`npm run build`；联调 mock（`WIKIATLAS_MOCK_WRITE_DELAY_MS`）观察长工单对话不再卡；确认断线续传 / 刷新接管后消息不重不丢。
- 实现：新 `frontend/src/lib/reduceRun.ts`（`useReducedAssistant` hook，跨帧持 prevState，只喂 events 尾部新增的 response.*）；
  `responses.ts` 抽 `normalizeAssistant` 供 hook 与 `runToMessage` 共用（避免循环依赖）；`AiPanel` 当前单改增量、
  历史冻结路径的 `runMessages` 保留（一次性全量，幂等）。
- 状态：✅ 代码完成，`tsc -b && vite build` 通过；⏳ 待联调 mock 长工单确认「不卡 / 断线续传 / 刷新接管」实测

### [x] T2 SSE 韧性（F5）—— 独立小改
- `sse.ts`：游标只在**成功交付**后推进（修 F5-3 永久丢帧 bug）；畸形帧上报（console.warn + 计数
  + `onDiagnostic`）；**seq gap 检测** → rebaseline（按 lastSeq 重拉窗口，后端回放权威事件）。
- **一次性抑制**（`rebaselined` 标志）：replay 合法跳过流式 delta（不落库）造成的"空洞"不是丢帧，
  别追着无限重连——rebaseline 一次后仍跳号就直接交付防死循环；真连续丢帧靠 onerror 指数退避兜底。
- **投影重建**：rebaseline 回补的事件可能乱序/带洞，增量序号假设会被打破 → `sse.ts` 发
  `onRebaseline` → `AiPanel` 的 `rebaselineNonce` +1 → `useReducedAssistant(resetNonce)` 从日志
  **全量重建**投影（对齐 DSH baseline 语义，幂等低频）。
- 涉及：`frontend/src/lib/sse.ts`（重写）、`lib/reduceRun.ts`（+resetNonce 参数）、`AiPanel.tsx`（接线 nonce）。
- 验证：`tsc -b && vite build` 通过；⏳ 待 mock 制造乱序/丢帧确认自愈（rebaseline 触发、投影不重不丢）。
- 状态：✅ 代码完成

### [x] T3 正文 rAF 合流（F3 上半）—— 独立、安全
- `liveDraft.push` 改 rAF 合流：delta 累进 ref，`requestAnimationFrame` 每帧一次 `setState`
  （DSH 的 `publication: 'animation-frame'`：状态按序落、渲染按帧合并）。
- 涉及：`frontend/src/lib/liveDraft.tsx`（push 内加 rAF 缓冲；注意 unmount / commit / abort 时 flush）。
- 验证：构建；mock 长正文，"几百次 rerender" 压到"每帧一次"，编辑器不卡。
- 实现：`liveDraft.tsx` 的 `push` 已改 rAF 合流（delta 累进 ref、`requestAnimationFrame` 每帧一次
  setState；commit/abort 取消待处理帧防竞态）。
- 状态：✅ 代码完成，`tsc -b && vite build` 通过；⏳ 待 mock 长正文实测

### [x] T4 打字机处置（F2）—— 依赖 §2 决策
- 已定 B：删 `useSmoothedChats` + `frontend/src/lib/smoothing.ts`（整文件删除）；`AiPanel` 的 `chatsShown` 接线删除、
  `AIChatDialogue` 直接吃 `chats`。reasoning 走 `output_item.done` 一次性整段显示（可接受），output_text 仍逐字。
- 涉及：`AiPanel.tsx`（删 import/接线）、`smoothing.ts`（删）。
- 状态：✅ 已删，`tsc -b && vite build` 通过。
- 更正：reasoning 同样真流式（见 F2 更正），output_text 与 reasoning 都逐字，无打字机层。

### [x] T5 正文增量块渲染（F3 下半）—— 两个表面都增量
- **BlockNote 编辑面（mode-block，主 live 表面）：已在**（上一轮落地，文档漏回填）——
  `BlockMarkdownEditor.tsx` 的 `firstDivergentBlock`（按块指纹找分歧点）+ `replaceBlocks`（分歧
  尾部原地替换）/ `insertBlocks`（纯追加，删除集不能为空）分流 + `.live-tail` 加载态摘挂（写完
  渐显）+ `lastLoaded` 基准取"替换后实际序列化出来的样子"（AGENTS.md 正文流雷区全踩点已处理）。
- **MarkdownPreview 读面（mode-read）：本轮改 keyed 增量**——`marked.lexer` 折顶层 token（每个=
  一块），逐块 `marked.parser([token])` + sanitize，React 按 `key=块序:类型` 调和。跨帧 diff：
  token `raw` 没变的块**复用上一帧 HTML**（不重 parse/sanitize），只有增长中的当前段重算。
  前缀块 key 稳定保 DOM、当前段 key 稳定只更 innerHTML、段写完新块挂上→旧尾摘 `.live-tail` 渐显。
  标题 id 仍按出现序给 h2/h3 配 uniqueSlug（渲染后注入，marked 不认 `{#id}`）。
  CSS 加 `.md-preview .md-preview-block` 作用域 live-tail（读面无 `.bn-inline-content` 结构，整块压暗）。
- **context7 已核 BlockNote 增量原语**（v0.54.0 档，项目 0.53.0）：`replaceBlocks(blocksToRemove,
  blocksToInsert)` / `insertBlocks(blocks, referenceBlock, placement)` 语义与代码用法一致。
- 涉及：`MarkdownPreview.tsx`（重写）、`MarkdownEditor.tsx`（+live prop）、`index.css`。
- 验证：`tsc -b && vite build` 通过；⏳ 待 playwright 无头真跑截图（读面 live 不整篇重挂、尾段渐显）。
- 状态：✅ 代码完成
- **正文粒度（回答用户"逐章 or 逐行"）**：`wikiatlas.content.delta` 是后端 200ms 时间窗节流成段
  （~5 段/秒），非逐章非逐行；"写完一章显示一章"是 LLM 顺序写的自然结果（T5 不改语义只优化渲染）。

### [x] T6 `delete output_text`（F4）—— 结论：保留，非 bug
- 见 F4。`renderDialogueContentItem.default` 救不了，delete 是唯一正确适配。不改，仅记录。

### [x] T8 "思考卡住/空白"地板层：工作中指示器
- 根因：reasoning delta 被归约器吞（F2 协议 gap）+ 工具长耗时阶段对话无可见输出 → 234 秒纯空白，
  用户观感"卡住/思考被截断"。**不是死锁**（后端心跳 15s 活着、SSE 实时推了 delta 只是被吞）。
- 地板层（前端-only，本轮做）：`run.status==='running'` 时对话底部常亮"Altas 正在工作…"（AI 紫
  脉动点 + tertiary 文字，全 token 无 hex，`prefers-reduced-motion` 降级）。终态自动消失。
  作用：模型哪怕一句不说，界面也不显得死（对齐 Codex 自己"工具活动可见所以闷头也不像卡死"）。
- 天花板层（后续，依赖决策）：
  - **F2 根治** = reasoning 真流式。选项 A（前端 `chunkOf` 把 `reasoning_summary_text.delta` 归一成
    Semi 认的形式）或升级/patch Semi 归约器。有协议纯净性代价，待拍板。
  - **narrative 修复**（T9）= 模型主动开口。
- 涉及：`AiPanel.tsx`（+working row）、`index.css`（+ai-working-row/dot）。
- 验证：`tsc -b && vite build` 通过；⏳ 待真跑确认 running 时常亮、终态消失、不挡交互。
- 状态：✅ 地板层代码完成

### [x] T10 "思考卡住/截断"观测层：流式思考块（F7）
- 经 **`renderDialogueContentItem.reasoning` 官方扩展点**注册自研 `ThinkingBlock`
  （`src/components/run/ThinkingBlock.tsx`，与 ToolCard 同套路；Semi content 项渲染先查
  自定义键、未命中才走内置 ReasoningWidget——`dialogueContent.tsx` 源码确认，无 `.semi-*`
  反向覆盖、不改 node_modules）。
- 契约：**思考中恒展开**（无测量/裁剪层，长思考结构上裁不掉）；完成瞬间默认折叠，头部
  "思考 · N 字 · 点击展开"（点过就永远尊重用户）；`key=item.id`（rs_0/rs_3…，
  不随边界行位置漂移）；正文 Semi `MarkdownRender`（官方导出组件）；渲染时现读文本
  （归约器原地 mutate，summary 跨帧同引用，依赖引用 memo 会永远错过新文本）。
- CSS 全 token（紫=AI 专属，思考中图标脉动 `@keyframes`）。
- 状态：✅ `tsc -b && vite build` 绿；✅ 活体 e2e（UI 输入发送→面板实时拾起→思考块
  恒展开流式 2→281 字→完成自动折叠"282 字·点击展开"→点击展开见全文；历史 16 块全带
  字数提示；`builtInReasoning=0`、0 pageerror）。

### [x] T9 "模型不汇报"天花板层：narrative 肯定式原则（后端提示词）
- 诊断见 F6：6 单 0 次 narrative/answer，提示词只有否定式约束，缺"何时开口"触发。
- 修法（原则，不讲个例）：`prompt.go` identitySection 加肯定式——
  "The user cannot see your thinking or your tool calls: while the run is in progress,
  narrative is your only progress signal — say one short line of what you are about to do
  before each new phase (search, read, write). A long run without narrative looks frozen."
- 落地：identitySection 新增句；golden ×4 重录；`go test ./...` 绿。
- 状态：✅

---

### [x] T11 哈尔事件契约层：生成单位三原则（提示面）
- 背景：哈尔条目 ~9k 字整篇单次生成写入解码漂移（音素噪声 / 跨语言插入 / 复读），
  且每轮修复整页回读把脏文反复喂回窗口——"越修越脏"是**结构性的放大器**，不是提示误导。
- 三处，全在原则层（一个事实一个 owner，不讲个例；个例证据进 examples/ 不进运行时规则）：
  1. **修坏点只回读定点**（probeSection）："read back only the neighbourhood of it
     (a `section` around it), never the whole page"——掐断"脏文回喂窗口"这个放大器；
  2. **整篇重写是 LAST RESORT**（intentSection hasWrites）：repair = 重写坏掉的那部分即可
     （chapter 级单元让一次坏写的修复代价小）；真要整篇重写，先 narrative 说一句；
  3. **write_content 契约收窄**（tools/librarian.go desc）：只用于新条目**骨架** /
     整篇重写后的**新骨架**；"never emit a whole long entry in a single call"
     ——长单次生成正是漂移与垃圾 token 出处。patch_sections desc 同位补
     "建新的长条目：先 write_content 铺骨架，再 patch_sections 分波填充（每波 ≤8 章）；
     整篇重写长文是多波 patch_sections，不是 write_content 整篇换"。
- 落地：golden ×4 重录；`go test ./...` 绿。
- 状态：✅

### [x] T12 `patch_sections`：波次灌章（自评①"单块写入"的真缺口）
- 问题：patch_section 一章一回、write_content 整页一回。批量导入多章世界 =
  N 份整页修订快照（页逐章变长，总存储 O(n²)）+ N 条 live-draft 流 + N 个回滚点——
  20 章导入的后患（"累人"）就出在回滚粒度上。
- 修法：新工具 `patch_sections`（`sections: [{heading, newMarkdown}]`，≤8 节/波）：
  - **原子波**：加载页一次 → 波内顺序真跑 ReplaceSection → commit 一次
    （一个修订 / 一个回滚点——"整体导入"的用户语义）；
  - **预检**：工作副本上任一节定位不到 → 整波不落盘，照实报现有章节清单（不造空中章节）;
  - **守卫扩展**（executor）：逐 heading 计 sectionRewrites，任一超 2 次 → 整波拦下；
    空节拒绝；≤8 上限；
  - **正文流波次投影**（responses.go `streamWave`）：按 `newMarkdown` 出现次序给每节建槽，
    每节独立累积 / 独立 flush；delta 形状与 patch_section 完全一致
    （`{callId, name:'patch_sections', targetType, targetId, heading, delta}`）。
    heading 取该节 newMarkdown 之前**最后一个已闭合**值；模型反着写（正文先行）就取本节
    对象内第一个已闭合值——**未闭合则 buffer 按住不发**（半截标题的 delta 前端找不到落点，
    本回合修掉的活 bug：`flushContent` 曾把 heading 未解析的节直接发出），收尾（callArgsDone
    force）时未解析也照发；
  - **前端**（liveDraft / WorkView / DocView / ToolCard）：当前槽只持活跃节，完成的节 push
    进 `sections[]` 锁定；liveMd 按"已锁定各节按序原位拼接 + 当前活跃节"渲染
    （活跃节为空不拼，防章节闪空）；`spliceSection`（lib/mdOutline.ts）**镜像后端
    ReplaceSection 匹配规则**——改一边必须同步另一边（AGENTS.md 正文流纪律）。
- 测试：`librarian_test.go` 三个波次测试（恰 1 修订 / 预检 0 落盘 / 守卫拒绝）+
  `responses_content_test.go` 两个波次投影测试（逐节配对 + heading 后到配对）全绿；
  前端 `npm run build` 绿。
- 明确不动：单章工具保留（单章改动仍走 patch_section，路径不变）；read_work 30k 上限不抬——
  截断后给模型"读什么"靠返回里恒带 `chapters`（## 标题清单）+ `relations`（本节点现有边，
  建/删边前先看，配合 `delete_relation` 按三元组删）——该两条随并行线（librarian.go /
  library.go）同轮落地。
- 状态：✅ 后端 + 前端全落地；⏳ 待起一单多章波次联调：看编辑器原位拼接观感与版本号闪动

---

### [x] T7 对话气泡：AI + 用户都出气泡
- 需求：用户明确要 AI 和用户消息都用气泡。Semi 原生 `mode="bubble"` 即"所有消息套 `-bubble` 类"
  （`dialogueContent.js`：`bubble = mode==='bubble'` → 所有 role 都有背景框）；`userBubble` 则只有用户出气泡。
- 改动：`AiPanel` 的 `AIChatDialogue` `mode="userBubble"` → `mode="bubble"`。
  气泡样式全走 Semi token（背景 `--semi-color-fill-0`、圆角 large、`width: fit-content` + `max-width:100%`、padding 12/16），
  非 hex、不反写 `.semi-*`。现有 `.ai-panel .semi-ai-chat-dialogue-*{min-width:0}` 宽度收口对 bubble 同样生效。
- 视觉注记：Semi 原生 bubble 模式下 AI 与用户气泡**同色**（都 `--semi-color-fill-0` 淡灰）。
  若要区分（如用户气泡用主色），后续在 `.ai-panel` 作用域用 token 微调，别写 hex。
- 状态：✅ 已改，`tsc -b && vite build` 通过；⏳ 待联调截图确认观感


## 4. 验证口径（AGENTS.md）
```bash
# 后端
wsl -d Ubuntu -- bash -lc 'export PATH=/usr/local/go/bin:$PATH; export GOCACHE=/root/.cache/wikiatlas-go-build; cd /root/WikiAltas/backend && go test ./...'
# 前端
wsl -d Ubuntu -- bash -lc 'export PATH=/root/.nvm/versions/node/v24.20.0/bin:$PATH; cd /root/WikiAltas/frontend && npm run build'
# 联调 mock（正文流）
#  第二套后端：WIKIATLAS_MOCK_WRITE_DELAY_MS=24000 go run ./cmd/wikiatlas -db /tmp/x.db -addr 127.0.0.1:8090 -seed -skill …
#  前端：WIKIATLAS_DEV_PORT=5200 WIKIATLAS_API_TARGET=http://127.0.0.1:8090 npx vite
```

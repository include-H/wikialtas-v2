# Atlas v2 · 追踪文件（续工入口）

> **这个文件是唯一的"续工入口"。** 新对话开始时**只加载它**，agent 就能按 §1 的恢复协议接手，不需要产品方再指方向。
> **它是一份"活文档"**：每完成一件事，就更新 §2 快照、§3 看板、§6 进度日志。改了要改的，别另起炉灶。
> **最后更新**：2026-10-29 · 状态：**文档已冻结（实现方代拍板），代码 M0–M9 全部 ✅；发布前免费回归全绿（真实媒体库写回待产品方显式批准）**

---

## 0. 载体说明（这是什么、怎么用）

- **这套文档 = 规范**，代码照它写。冲突时**以 `docs/README.md §3 决策冻结表`为准**，其次本文 §3 看板。
- **本文件 = 状态**：做到哪、下一步做什么、哪些坑不能踩。**任何新决定先追加到 `README.md §3`，再回填本文。**
- **给新对话的第一句**（模板，直接复制）：

  > 读 `docs/TRACKING.md`，按它的 §1 恢复协议继续开发 Atlas v2；当前里程碑以它的 §2 为准。

---

## 1. 恢复协议（新对话 / 新 agent 照着做，别跳步）

1. **读本文件**（尤其 §2 当前快照 + §3 看板 + §5 风险）。
2. **读 `docs/README.md`**：拿决策冻结表（D1–D74）与里程碑定义。
3. **按你负责的层读对应文档**：
   - 后端 / agent → `behavior-notes.md`（**写 AI 前必读**）· `backend.md` · `database.md` · `api.md` · `tools.md` · `prompts.md`
   - 前端 → `frontend.md` · `frontend-migration.md` · `api.md §4`
   - 媒体库 → `library-sync.md`
   - 技能 / 提示词 → `skills.md` · `eval-cases.md` · `prompts.md`
4. **只做 §2「下一步」那一个里程碑**（不要提前做后面的；依赖是有序的）。
5. **完成标准 = §3 看板里该里程碑的「验收」那一列真跑通过**（不是"编译过了"）。
6. **收尾三件事**（必做，否则等于没做完）：
   - 更新 §3 看板状态（⬜→🟡→✅）；
   - 在 §6 进度日志追加一行（日期 · 里程碑 · 做了什么 · 证据）；
   - 更新 §2 当前快照的"下一步"。
7. 遇到与文档冲突 / 文档没覆盖的情况：**先按 `README.md §3` 的裁决原则做最保守的决定**，把决定追加进决策表，并在 §5 记录；不要停下来问产品方（已委托）。

---

## 2. 当前快照

| 项 | 状态 |
|---|---|
| 文档冻结 | ✅ 全部 16 篇已冻结（决策 D1–D74 无"待定"） |
| 代码基线 | ✅ M0–M9：树/内容 · 三源扫描与建档 · 通用 Atlas harness · 三段式写作 · 关系/综合 · Review/体检 · 反哺/重挂 · 检索/存资料 · MCP/设置/用量/导出导入 |
| 产出基准 | **规则** = 项目内 `skills/`（14 技能 + `samples/` 18 篇）；**质量目标** = `/root/Game_Wiki` 真产出（D3） |
| 当前里程碑 | **M0–M9 全部 ✅**；路线图内无未实现里程碑 |
| 下一步 | **发布前验收**：① 经你明确批准后，用真实 GameAtlas/Komga 写回一条测试文档（**唯一需要你点头的外部写**）；② 维持 `/root/Game_Wiki` 产出档评测；③ 需要时再接可选「AI 对一遍」与 pgvector。免费回归已于 2026-10-29 全绿 |
| 阻塞项 | 无 |
| 需要产品方做的事 | **无**（已委托；一切按文档与本文裁决） |
| 仓库状态 | V2 实现已提交到本地（`feat(v2): M4.5–M9 …`），**尚未推送**；工作树仅剩两张本地截图 `1.png` / `2.png` 未跟踪 |

---

## 3. 里程碑看板（**唯一进度真相**）

图例：⬜ 未开始 · 🟡 进行中 · ✅ 完成（= 该行验收真跑通过）

| 里程碑 | 状态 | 依据文档 | 验收（真跑） |
|---|---|---|---|
| **M0 基线** | ✅ | `backend.md §11` | `cargo test --workspace` + `scripts/e2e-ui.sh` 全过 |
| **M1 媒体库进料** | ✅ | `library-sync.md §1–3` | **真跑过**：真库（192.168.1.4）扫三源 → 252(GA)+1523(Emby)+113(Komga) 条落 `library_manifest` → 池子列出「有/无文档」。见 `scripts/e2e-library.sh`（15/15） |
| **M2 建档与关联** | ✅ | `library-sync.md §2 §4` | **真跑过**：池子点「建档」→ 树出系列节点 + work 文档 + 链 active；与父同名 → 加介质后缀（`SPY×FAMILY間諜家家酒（漫画）`）；解链只删链、文档还在。见 `scripts/e2e-link.sh`（17/17） |
| **M3 Agent 内核**（尖刺 B） | ✅ | `backend.md §5 §6` · `behavior-notes.md` | **真跑过（真模型）**：Atlas 里发一句话 → `function_call_arguments.delta` → 14 段 `content.delta` → 写工具执行 → **草稿落库** → 浏览器实时看到正文。见 `scripts/e2e-run.sh`（11/11） |
| **M4 写作与质量门** | ✅ | `ai.md §4` · `prompts.md §2.3 §3` · `tools.md §2` · **D3 D55–D68** | **真跑过（真模型 · 规则档）**：经通用 harness 的 `write_wiki` 工具跑完整三段 → 过**正文门 + 骨架门 + 引用门**、账本覆盖率 **83%** → 定稿 → 快照。见 `scripts/e2e-write.sh`（**27/27**）。**已落**：三段式（`checkpoint.phase`）· 事实账本 · `web_fetch` · 按章 `patch_section` · 三闸 + 骨架兜底 · **章名从代码注入**（修 R13）。**待**：① 研究/骨架走便宜档（D20）② 修订类小任务跳过前两段（D55 §2.1，随 `write_content`/`edit` 补） |
| **M4.5 通用 Atlas Harness** | ✅ | `ai.md §1 §4` · `tools.md §1 §3` · **D70 D71** | **真跑过（真模型）**：一个循环 + 一整套工具面，**模型自己判断答还是写**（没有 UI 开关）。**问**：读取库内容 → `answer`（带可点 `refs`）→ 浏览器渲染，无写工具、不落草稿（`scripts/e2e-query.sh` **16/16**）。**写**：模型按需调 `write_wiki` → 三段管线真跑出 **15.8k 字**草稿（引用门 passed · 账本覆盖 83%）；已加「一轮只写一次」护栏（质量门待 M4 调，见 R13）。见 `scripts/e2e-write.sh` |
| **M5 关系与综合** | ✅ | `features.md §6.5` · `tools.md §2` | 综合页引用两部作品 → 关系抽屉/图谱展示 → 系列索引由宿主重算。见 `scripts/e2e-relations.sh`（**12/12**） |
| **M6 Review 与体检** | ✅ | `ai.md §9` · `features.md §6` | worker 查出孤立节点/缺失正文/失效链 → 面板可见 → Review 接受后真建节点。见 `scripts/e2e-audit.sh`（**9/9**） |
| **M7 反哺与失效重挂** | ✅ | `library-sync.md §5 §6` | 定稿事务入队 → GameAtlas 全文+摘要 / Komga 摘要 → 条目消失 stale → UI 候选重挂 → 旧链 replaced、正文不动。见 `scripts/e2e-sync.sh`（**17/17**，隔离临时库 + 本地真 HTTP 仿真） |
| **M8 检索与问答** | ✅ | `database.md §3` · `ai.md §7` | 正文独有事实可检索（trigram + bigram）→ 结果可点回文档 → 回答可存成资料且不改作品正文。见 `scripts/e2e-search.sh`（**11/11**）；真模型问答主链已在 M4.5 验收 |
| **M9 对外与运维** | ✅ | `api.md` · `features.md §9` | MCP stdio 只读检索 → 设置不回显密钥 → token/成本可见 → JSON 导出/空库导入 → `pg_dump/pg_restore` → 可选访问密码拦截/登录。见 `scripts/e2e-ops.sh`（**17/17**） |

### 3.1 前端编辑器待迁项（v1 有、v2 尚未落——随对应里程碑补）

来源：`frontend.md §5.5`（v1 编辑器规格）。这些**不是可选项**，是 M3/M4/M6 的组成部分：

- [x] **实时草稿只读显示**（`liveMd`；服务端草稿为真相，`content.delta` 仅加速）——右栏 `.ai-live` + 正文区切**只读实时视图**（`.doc-live` + 自写块级 `LiveMarkdown`，`design.md §6.3`）；收工后从服务端草稿重新基线（D46）
- [~] **落块动画**（`live-tail`：压暗 + 紫色脉动条 + 写完渐显）——**末块 `live-tail` 脉动已做**；「前缀块复用 DOM、只换末块」的块级 diff 未做（当前每次 delta 整体重渲，规模可控）
- [x] **「…」更多菜单**：版本历史（抽屉：列表 + 预览 + 「恢复为草稿」）/ 节点信息 / 复制 Markdown / 归档（灰，M6 前不可用）——`design.md §6.3`
- [x] **修订接受 / 拒绝条**（v1 `PendingRevisions`）——v2 由草稿模型替代（**D74**）：接受 = 定稿，拒绝 = 丢弃草稿（`DELETE /api/nodes/{id}/draft`，回到最近定稿、不产生版本）
- [x] **工具栏 / 斜杠菜单 / 块手柄的视觉样式**：按 `VISUAL_SPEC` 的飞书语法补齐（浮层卡片 + 紫色 AI 区）；并修了三个真问题——①浮层挂在 `document.body` 上**取不到 AntD token**（`--ant-*`/`--ai` 全空 → 没底色、AI 不是紫）→ 改挂 `.shell` 内；②隐藏用 `display:none` 导致 floating-ui 量到 0 宽 → 块手柄压住正文 → 改 `visibility`；③`Inner` 每帧重渲染给编辑器再派发事务的**忙循环**（~56 次/秒）把 `/` 菜单的 200ms 防抖卡死 → 去掉多余的 `useEditorTick()`。回归见 `scripts/editor-check.sh`（14 项）

### 3.2 每个里程碑的铁律（复述，别忘）

- 验收 = **真跑**（副本库 + 无头浏览器 + 回服务端核对），不是"构建通过"。
- AI 相关：改技能/提示词必跑基准回归 + 触发纪律；**不过就简化，不叠加新规则**。
- **真模型调用要显式确认**；测试默认 mock / 副本库。
- 一切多步写在一个事务；`domain` 零 IO；单文件 ≤300 行（`agent::runtime`/`tools` 例外，见 D41）。

---

## 4. 决策与文档冻结状态

| 项 | 状态 |
|---|---|
| 决策冻结表（D1–D74） | ✅ 冻结，`docs/README.md §3` |
| 各文档"待定"章节 | ✅ 全部收敛为"已拍板"（grep 无残留） |
| 文档清单（16 篇） | ✅ README · features · v2-from-v1 · behavior-notes · ai · tools · prompts · skills · eval-cases · library-sync · backend · database · api · frontend · frontend-migration · testing |
| 新决定往哪写 | `docs/README.md §3` 追加一行（含理由），再回填本文 §3/§5 |

---

## 5. 开放项 / 风险台账

> 现在**无未决决定**。以下是"实现期要用实测确认、无需产品方介入"的项，验证后把结论回填对应文档。

| # | 项 | 现在的默认（已冻结） | 何时确认 | 归属 |
|---|---|---|---|---|
| R1 | Komga 摘要字段约束 | 首段 / 去 markdown / ≤500 字 / 超 `…` | M7 实测；更严取更短 | `library-sync.md §5.4` |
| R2 | 上下文预算具体比例 | 系统15/索引10/目标40/历史20/余量15 | M3 按真实模型调；只调数值不改结构 | `ai.md §2` |
| R3 | 分析步默认档位 | `low`（可配） | M4 试 | `ai.md §13` |
| R9 | **研究 / 骨架段目前共用主模型与 `llm.effort`**（尚未按 D20 拆「分析步便宜档」） | 保持一个模型配置，待 M4 真跑评估后再决定拆不拆（拆的话在 `runs.checkpoint` 或 `secrets` 加 `llm.research_*`） | M4 真跑后 | `ai.md §3 §6` · D20 |
| R10 | **验收脚本曾 `TRUNCATE nodes CASCADE` 真库，误删用户手写节点**（`e2e-ui.sh` / `layout-check.sh` 各一处；已改：只建/删自己命名的测试节点、断言按 `node_id` 圈定）· 新增 `scripts/backup.sh`（`pg_dump`，`_baseline`/`.backups` 已 gitignore） | 已修（不再有全库破坏操作） | `README.md §5.8` · `docs/database.md §6` |
| R11 | **打开已定稿文档会凭空落一条草稿**（Milkdown 把列表符号 `-` 归一成 `*`，`fullBody` 与 `lastSaved` 恒不相等 → 判成"有改动" → 1.5s 后写草稿；**不是用户改的**） | 已修：`frontend/src/markdown.ts` 的 `sameDoc` 比较规范形（**只影响"有没有改"的判断，不改存什么、不动写路径**） | M4.5 问答验收时发现（E2E 里断言"问答不落草稿"暴露） | `frontend/src/markdown.ts` · `DocPanel.tsx` |
| R12 | **`/` 斜杠菜单偶发停在 (0,0)、不跟随光标**：Milkdown `SlashProvider` 的 `#onUpdate` 带 **200ms 防抖**，会把「输入 `/` 的那次事务」和随后的 meta 事务塌成一次；末次 `prevState` 与当前 doc/selection 相同 → `isSame` **早退** → 从头没调过 `computePosition`。表现：输入快时菜单不出来 / 贴左上角 | 已修：给 provider 加 `debounce: 0`（去掉会吞掉关键事务的防抖）；`scripts/editor-check.sh` 连跑 4 次 **14/14** | M4.5 复跑 `editor-check.sh` 时暴露（此前偶发） | `frontend/src/editor/MarkdownEditor.tsx` |
| R13 | **M4 规则档真跑：正文门未过**——真模型经 `write_wiki` 产出草稿，但**正文门挂在「章名按介质映射」**（模型把第 5 章写成「玩法」而不是「玩法与结构特色」）；且模型看到 `quality=failed` 会整篇重调 `write_wiki`（烧额度）；另发现生成段提示词**没注入 `node_id`** → 每章 `patch_section` 被拒 | **已修**：① 章名**唯一来源** `skeleton::canonical_chapters`（骨架/正文/两处提示词都认它），骨架段提示词**注入标准章名**；② `write_wiki` 动作层「一轮只写一次」护栏（带单测）；③ 生成段注入目标 `node_id`。**复跑 `scripts/e2e-write.sh` 27/27**（正文门/骨架门/引用门 passed · 账本 83%），M4 ✅ | 真跑暴露并修复 | `design.md §4` · `skeleton.rs` · `phases.rs` · `policy.rs` · D71 |
| R14 | **M7 自动化没有写真实 GameAtlas/Komga**：真实写回会改变你的媒体库内容，验收脚本改为**临时库 + 本地真 HTTP 仿真**；协议/worker/UI/状态机都真跑，但生产源连接仍需一次显式批准 | 保持“不擅自写外部媒体库”；批准后跑一条自己命名的测试文档 | 发布前 | `scripts/e2e-sync.sh` · D32 |
| R15 | **本轮没有重新调用付费模型**：M7–M9 没改写作提示词主链，只把 `search_works` 扩到正文；既有 M4/M4.5 真模型证据保留。后续改 prompt/技能仍需按纪律显式确认后跑付费回归 | 默认 mock/隔离验收 | 每次 AI 行为变更 | `README.md §5.7` |
| R16 | **搜索预算与上下文**：聚合搜索一次默认融合 20 条、provider 侧 20–100 候选；默认研究 24 搜 / 16 抓 / 48 轮，可设置。真正无限会耗尽窗口与费用，所以保留高安全上限 | D72 | 按真实产出与覆盖率继续调数值 | `tools.md §3.1` · `websearch.rs` |
| R4 | 中文检索召回 | 归一化 + `pg_trgm` + bigram | M8 已落机制；真实语料召回继续按使用情况观察，暂不新增分词依赖 | `database.md §3` |
| R5 | Rust 退出条件 | 保留（M3 后若改 prompt 往返明显慢于 v1 才回 Go） | M3 结束时评估 | `backend.md §9` |
| R6 | Komga 判型（分卷 / 抽屉）M1 只跑**确定性启发式**（`sources::komga::book_is_volume`），脏命名会判歪 | 保持零 LLM 扫描；判歪的进池子也只是多几条候选 | M5/M7 做「AI 对一遍」（`library-sync.md §7`）时收敛；真库实测：`为美好的生活献上祝福`（库名把「世界」写成了「生活」）被拆成 33 个单册，就是这类脏数据 | `library-sync.md §1 §7` |
| R7 | `scripts/md-check.sh` 曾长期红：① 样本路径仍指向旧技能目录（`skills/samples`）②「页面标题用节点标题」断言与测试节点名不一致 ③ Milkdown 的 GFM 序列化会规范化 `-`→`*`、`---`→`***`、给表格补对齐空格、给链接 URL 的 `()`/`&` 加反斜杠——**内容没丢** | **已修**：① 路径改为 `skills/atlas/references/samples`；② 断言用实际节点标题；③ 往返比较改为**规范形等价**（列表符号 / 分隔线 / 表格空格 / 反斜杠转义统一），不删断言、不静默忽略内容差异。`scripts/md-check.sh` **18/18** | 2026-10-29 发布前回归 | `scripts/md-check.sh` |
| R8 | 编辑器浮层宿主（工具栏/斜杠/块手柄）曾是 body 里的静态 div，撑出 46px 页面滚动 | M1 已修：`.editor-portal{position:fixed}` + `[data-show=false]{display:none}` | 已完成（`layout-check.sh` 32/32） | `frontend/src/editor/editor.css` |

**已知头号风险**：**行为知识会丢** → 缓解 = `behavior-notes.md`（**写 AI 前必读、逐条对照**）。

---

## 6. 进度日志（append-only，最新在上）

| 日期 | 里程碑 | 做了什么 | 证据 |
|---|---|---|---|
| 2026-10-29 | **发布前回归 + 修订闭环（D74）** | 逐项复跑 M0–M9 验收并修真回归：① `e2e-audit.sh` 按侧栏序号点「体检」→ 新增「全文检索」后点错页 → 改为按文案点击（**9/9**）；② `e2e-write-mock.sh` 的 mock 仍用旧判据（system 含「当前运行约束」）把根 Atlas 写请求当成工具面验收 → 改为测试口令 + 新增根循环 `write_wiki` 分支（**27/27**）；③ `md-check.sh` 旧路径/过期断言 → 规范化等价比较（**18/18**，R7 关闭）；④ 补 **D74**：接受 = 定稿、拒绝 = 丢弃草稿，新增 `DELETE /api/nodes/{id}/draft`、文档菜单「丢弃草稿」、store 单测 | `e2e-sync 17/17` · `e2e-harness-tools 15/15` · `e2e-search 11/11` · `e2e-relations 12/12` · `e2e-audit 9/9` · `e2e-ops 17/17` · `e2e-ui 35/35`（含丢弃草稿）· `e2e-write-mock 27/27` · `md-check 18/18` · `editor-check 22/22` · `layout-check 39/39`；`cargo test --workspace` **171** 通过，前端构建通过 |
| 2026-10-17 | **Harness 工具面补齐 + 聚合搜索** | 按 DSH 风格重写 Atlas 提示（身份/约束/工作方式/写库纪律/交付）；通用 Harness 真正补齐节点、正文局部编辑、批量章节、关系、媒体链、资料、归档、todo、会话回忆等工具；`read`/`oneShot` 从 run context 进入 schema 与 `policy` 双层裁剪。联网搜索统一为 `search_web`：Exa + Brave + DeepSeek 并行、URL 去重、RRF 融合、失败降级；DeepSeek Fetch 优先、本地抽取兜底。默认一次融合 20 条（max 50），研究预算 24/16/48 可配 | `harness_tools` 单测/DB 工具真跑；`websearch` 三家 fake HTTP + RRF 测试；`scripts/e2e-harness-tools.sh` **7/7**（真 PG/API/worker + mock 模型：edit/upsert/todo 真执行，read 模式误调 edit 被拒）；`cargo test -p wikiatlas-agent` **91/91** |
| 2026-10-17 | **M9 ✅ 对外与运维** | 新增 `crates/mcp`：stdio JSON-RPC MCP server，只暴露 `search_works` / `read_node` / `get_tree`，没有写工具；新增设置页与非密钥 `app_settings` 配置、`PUT /api/secrets`（只回显“已配置”）；用量从 `response.completed.usage` 按模型聚合，配置单价后算成本；项目 JSON 导出/空库导入（不含密钥/runs/媒体缓存）；验证 `pg_dump/pg_restore`；补可选单密码访问控制（未设置时本地开放，设置后 HttpOnly Cookie 校验） | `scripts/e2e-ops.sh` **17/17**（MCP 真子进程检索、导出导入、恢复库正文、设置页/用量、401/登录）；`cargo test --workspace` 全绿，`npm run build` 通过 |
| 2026-10-17 | **M8 ✅ 检索与问答** | 新增 `store::search`：标题/别名/正文联合检索，归一化 + `pg_trgm`，应用侧字符 bigram 兜底；新增全文检索页与可点结果；`search_works` 工具同步升级；新增 `POST /api/answers`，好回答存成 doc + 快照 + `doc_refs` + references 边，不改作品正文；Atlas 回答卡加「存为资料」 | `scripts/e2e-search.sh` **11/11**；`search_answer_flow` 3 项；`cargo test` + tsc 通过 |
| 2026-10-17 | **M6 ✅ Review 与体检** | `store::review`（建议入队、接受/拒绝、拒绝理由保留；create/link/ignore 会真执行，merge/split/research 标记人工跟进）；`store::audit` 检查失效链、未反哺、定稿空正文、孤立节点、重名、陈旧草稿、未建档库存；worker `audit`；前端「体检与 Review」页支持运行、查看、裁决 | `scripts/e2e-audit.sh` **9/9**；`review_flow` 3 项；全仓测试/tsc 通过 |
| 2026-10-17 | **M5 ✅ 关系与综合** | `relation_types` 读契约 + 关系出/入边查询/建/删 + 同类型环校验；综合页 = doc + `doc_refs` + references 边；宿主索引页按系列/宇宙子树确定性重算；文档页「关系与综合」抽屉提供关系图、建关系、新建综合页、更新索引 | `scripts/e2e-relations.sh` **12/12**；`relation_flow` 4 项；全仓测试/tsc 通过 |
| 2026-10-17 | **M7 ✅ 反哺与失效重挂** | 定稿事务同时入队 sync；worker 写回 GameAtlas 全文+摘要、Komga 摘要并落 `sync_targets`；单源空扫描也会正确 stale，条目恢复自动 active；重挂候选按标题/别名 trigram 排序，旧链 `replaced` + `replaced_by`、新链 active、文档不动；文档来源条显示反哺状态和手动重推 | `scripts/e2e-sync.sh` **17/17**（隔离 PG + API/worker/UI + 本地 GameAtlas/Komga HTTP）；`sync_flow` 4 项；客户端 HTTP 测试 2 项 |
| 2026-10-13 | **M4 ✅（规则档真跑通过）** | 修 R13，让真模型写作过正文门：① **章名唯一来源** `skeleton::canonical_chapters`（骨架门 / 正文门 / 骨架段提示词 / 生成段提示词都认它，删掉三处各写一份的重复 + 死代码 `prompt::writing_system_prompt`），骨架段提示词**注入标准章名**（D24「数字与状态从代码注入」）；② 生成段注入目标 `node_id`（此前不注入 → 模型瞎填 → 每章 `patch_section` 被拒，mock 从不填 id 所以没抓到）；③ `write_wiki` 动作层「一轮只写一次」护栏 + 单测。收尾：`policy` 单测、`phases` 章名注入单测 | **真模型** `scripts/e2e-write.sh` **27/27**（正文门 + 骨架门 + 引用门 passed · 账本覆盖 **83%** · 章数 1–9 · 定稿 → 快照）；**M4 规则档达成** |
| 2026-10-13 | **M4.5（通用 Harness，重做 D71）** | 读完 `archive/v1/deepseek-harness`（agent-loop / tools / system-prompt / user-approval / plan-mode）、`archive/v1/codex`（instructions + permissions）、OpenCode 二进制里的提示词后，**推翻 D69 的「写/问 双引擎 + UI 开关」**：Atlas 改成**一个通用 harness**——单循环 + 一整套工具面，**模型自己判断答还是写**；三段式写作收成 **`write_wiki` 工具**（模型按需调，D55 的结构修复保留）。新增 `crates/agent/src/runtime/atlas.rs`（通用循环）、`policy.rs`（动作层许可）；**`write_wiki` 一轮只做一次**（实测模型看到 `quality=failed` 会整篇重来）；**修生成段真 bug**：提示词没注入 `node_id` → 模型瞎填 → 每章 `patch_section` 被拒（mock 从不填 id，所以 M4 免费自测没抓到）；`read_node` 去掉 `outline_only`（实测模型只看大纲就答「库里没内容」）。事件分层 `wikiatlas.write.end` / `run.end`。前端去掉 `Segmented`「写/问」开关，单输入框 | **真模型**：问路径 `scripts/e2e-query.sh` **16/16**（回答引用「只有库里有」的事实 + refs 回指、无写工具、不落草稿）；写路径 `scripts/e2e-write.sh` 经 `write_wiki` 产出 **15.8k 字**草稿（引用门 passed · 账本 83%），正文门待 M4 调（R13）；`cargo test` agent 83 · tsc 通过 |
| 2026-10-13 | **M4.5（通用 Harness + 问答）** | run 按 `intent` 路由（**D69**）：`问答` → 通用**只读**工具循环（新 `crates/agent/src/runtime/query.rs`），`写正文` → 三段流水线不变（**D70** 走用户指示，先于 D62 的 M7）。新增读工具面 `crates/agent/src/read_tools.rs`（`get_tree` / `list_works` / `read_node` / `read_doc` / `search_works` / `read_ledger`）+ 控制 `answer`（`text` + `refs`，宿主机校验节点存在、补标题）/ `finish`；`phases::query_prompt`（只读纪律）+ `is_query_intent`。右栏加「写 / 问」切换（AntD `Segmented`）+ 回答卡片（复用 `LiveMarkdown`，`live=false` 关掉脉动）+ 可点引用（点开跳节点）。store 新增 `nodes::search`（归一化子串 + pg_trgm）/ `nodes::list_works`。**顺手修 R11**：打开已定稿文档会凭空落草稿（`-`→`*` 序列化差异）→ `markdown.ts::sameDoc` 比较规范形。**顺手修 R12**：`/` 斜杠菜单偶发停在 (0,0) → `SlashProvider` 去掉 200ms 防抖（`debounce: 0`） | `cargo test --workspace` 全绿（agent **84** · store **23**，含 `search_flow` 3）；`frontend` tsc 通过；**真模型** `scripts/e2e-query.sh` **17/17**（回答引用了只有库里有的事实「青羽测试」+ refs 回指节点；无写工具、不落草稿）；回归 `editor-check.sh` 14/14（连跑 4 次）· `e2e-ui.sh` 31/31 · `layout-check.sh` 32/32 · 免费 mock 写作链 27/27 |
| 2026-10-13 | **修复（数据安全事故）** | 查明「写的节点/正文丢失」不是重启导致（迁移不删数据），而是**我运行的验收脚本对真库执行了 `TRUNCATE nodes CASCADE`**（`e2e-ui.sh:42`、`layout-check.sh:38`）＋调试时跑过一次 `delete from content_drafts`。**改法**：两个脚本不再全库删除，只建/删**自己命名**的测试节点（`e2e-ui·测试作品` / `layout-check·测试节点`），所有断言按 `node_id` 圈定；新增 `scripts/backup.sh`（`pg_dump`，`.backups/` gitignore）；纪律写进 `README §5.8`。**用户被删的手写内容无备份、不可恢复**（已在磁盘找过，无 pg_dump） | `grep` 确认脚本内已无 `TRUNCATE`/全表删；`e2e-ui.sh` **31/31** · `layout-check.sh` **32/32**（改完仍过，且是在有用户数据的库上跑的）；`scripts/backup.sh` 产出 3.4M dump |
| 2026-10-13 | **M4**（编辑器浮层 + 题记修正） | 补 `Toolbar`/`SlashMenu`/`BlockHandle` 的外观（`VISUAL_SPEC` 飞书语法）· 修**浮层取不到 AntD token**（改挂 `.shell` 内）· 修**块手柄压字**（`display:none`→`visibility`，先量尺寸再定位）· 修**编辑器忙循环**（`Inner` 每帧重渲染再派发事务，~56 次/秒，把 `/` 菜单的防抖卡死）· 修**题记围栏外漏**（`header.fence` 带前导空行时 `^:::epigraph` 剥不掉，`parseEpigraph` 再兜一层）· 空文档提示**跟随光标**（v1 `placeHint`）、空文档不出块手柄 | `scripts/editor-check.sh` **14/14**（题记不漏围栏 · 工具栏卡片 + AI 独立色 · `/` 菜单挂到光标 · 手柄透明图标且在正文左侧）· `e2e-ui.sh` **31/31** · `layout-check.sh` **32/32** · `frontend` tsc 通过 |
| 2026-10-13 | **M4**（前端视觉收口，`design.md §6` D63） | **首页**（问候 + 四张统计卡「未建档→池子」+ 最近更新）· 左栏导航加图标 + 池子未建档徽标、**树节点按种类/介质给图标 + 草稿橙点/归档灰字** · 文档页**统一 760px 阅读栏**（标题/meta/说明/题记/正文一条线）+ meta 行中文化（`游戏 · 作品｜定稿 vN｜已保存 · 时间`）+ **⋯ 菜单**（版本历史抽屉含预览与「恢复为草稿」/ 节点信息 / 复制 Markdown / 归档灰）+ **来源条**（含失效黄条）· **正文区实时只读视图**（自写块级 `LiveMarkdown` + 末块 `live-tail` 脉动，收工后从服务端草稿重新基线）· Atlas 右栏**起手式按钮 + 收尾** · 排版收口（列表紧凑 / 表格斑马纹 / `---` 弱化 / `##` 细线）· 新增只读端点 `GET /api/overview`、`/api/nodes/{id}/links`、`/api/nodes/{id}/versions/{ver}` | `cargo test --workspace` 全绿（agent 78 · domain 27 · sources 2 · store 20）· `frontend` tsc 通过 · `layout-check.sh` **32/32 不回退** · `e2e-ui.sh` **31/31** · 免费 mock 端到端（**含浏览器**）**28/28** · 截图 `_baseline/visual-{home,doc,doc-menu,versions,atlas}.png` · 新增脚本 `scripts/visual-shots.sh`（未新增 hex；紫色只在 `.ai-*`/`--ai`） |
| 2026-10-13 | **M4**（三段式） | 按 `design.md §2` 把写作拆成**一个工单三个阶段**（研究 → 骨架 → 生成，`checkpoint.phase` + `wikiatlas.phase`）· **事实账本**一等公民（`kind=doc + tags=['账本']`，固定「断言表 + 来源表」；`ledger_put` 增量记、`store::ledger::save` 幂等落库；覆盖率达 80% 收工、来源 URL ≥5）· 研究段加 `web_fetch`（超 12k 截断并说话）· 骨架段结构化 JSON + **骨架门** + 兜底 · 生成段**按章 `patch_section` 波次**（生成段无检索，从结构上不可能"只查不写"）· 新增**账本门 / 骨架门 / 引用门**（全代码）· `ResearchRunner` 抽象（默认内置）· Atlas 右栏加阶段/账本/质量门时间线 · 决策 D65–D68 | `cargo test --workspace` 全绿（agent **78** · domain 27 · sources 2 · store 20，新增 `ledger_flow` 3 项）；`frontend` tsc 通过；**免费 mock 端到端自测 27/27**（真 PG/API/worker + `scripts/mock-llm.py`，走完「研究→骨架→生成→过三闸→定稿→快照」）；真模型验收 `scripts/e2e-write.sh` 待跑（花钱，需确认） |
| 2026-10-12 | **M4**（联网+超时） | `agent::client` 加**流空闲超时**（90s 无事件即 Err 收尾——长文可以写很久，但不许挂着不动）· `web_search` 工具（Exa，secrets `exa.api_key`；返回 title/url/snippet 当事实账本）· 系统提示加检索预算（≤4 次）+ "检索完必须落笔" · `MAX_ITERS` 6→12 | 真跑实测：`web_search` 7 次全 ok；但**模型只检索不落笔**（两次都这样）→ 结论写进下一步：按 `ai.md §4` 拆两步式 |
| 2026-10-12 | **M4**（基础） | `agent::quality`（质量门：标题/说明行/题记围栏与块内空行/1–7 章齐全与跳号/第 5 章按介质/套话黑名单/字数下限；名场面 `；` 只提醒不硬拦）· `agent::prompt`（扫 `skills/*/SKILL.md` 解析 front matter → **L0 目录只放 name+description+何时用/何时不用**，正文靠 `read_skill`；写作系统提示带骨架/硬规则/质量门靶子）· runtime 接上 `read_skill` + 写完后跑门、不过回模型改一次仍不过则留草稿待人工 | `cargo test -p wikiatlas-agent` **34/34**（quality 9 · prompt 4） |
| 2026-10-11 | **M3** | 收官：`agent::client`（自写 Responses API 客户端：SSE 拆帧 + 事件映射，实测 `output_item.added`/`function_call_arguments.delta/done`/`completed`）· `agent::runtime`（run 执行器：事件落 `run_events`、正文边流边落草稿、工具循环上限 6、会话日志 append-only）· `store::runs` · worker 认领 `run` · API `POST /api/runs` + `GET /api/runs/{id}/events`（SSE 从 `run_events` 轮询，api 与 worker 不共享内存）· 前端 Atlas 真接上（发送 → SSE → 实时正文 → 刷节点） | `scripts/e2e-run.sh` **11/11**（真模型，草稿落库 + 浏览器看到流）；`cargo test --workspace` agent 21 · domain 27 · store 17 |
| 2026-10-11 | **M3**（起步） | 动 AI 前通读 `behavior-notes.md`（§5 流式投影 / §3 护栏 / §7 模型行为）。落两块**纯逻辑 + 重点单测**的地基：`agent::stream`（从不完整 `function_call_arguments` JSON 里增量抽正文：转义断半不吐字、代理对等高代理、只认键位置、200ms 节流）· `agent::guard`（空写拒绝 / 截断守卫 `>200→<50` / 归档拒写 / JSON 错不猜 / 重复调用 >3 拦、`todo_write` 豁免） | `cargo test -p wikiatlas-agent` **16/16**（stream 9 · guard 7） |
| 2026-10-11 | **M2** | `domain::place`（落位规则：介质推定 / 撞名后缀 / 复用判据，从 v1 `library_place.go` 搬）· `store::library::{build_doc,unlink,add_ignore,remove_ignore,list_ignores}`（建档三件事一个事务）· `nodes::{create_in_tx,get_in_tx}`（把建节点抽出来共用）· API `POST /api/library/links` + `DELETE /api/library/links/{id}` + `/api/library/ignores` CRUD · 池子行加「建档 / 忽略（单条·整组）/ 解链」 | `scripts/e2e-link.sh` **17/17**（建档那步走真界面点）；`cargo test --workspace`：domain 27 · store 17 |
| 2026-10-11 | **M1** | `crates/sources`（GA/Emby/Komga 三源客户端 + 清单构建 + 扫描比对 + `scan_runs`）、`crates/worker`（jobs 队列 + `FOR UPDATE SKIP LOCKED` + 租约）、`store::library`（清单 upsert / 池子 / 扫描记账）、`store::{jobs,secrets}`、迁移 `0002_library_manifest`、API `GET /api/library/{pool,manifest,status,scan-runs}` + `POST /api/library/scan`、前端「媒体库池子」视图 | `scripts/e2e-library.sh` **15/15**；`cargo test --workspace` 全绿（domain 19 · sources 2 · store 9）；真库扫描 252+1523+113 条 |
| 2026-10-11 | 修复 | 修了三个**挡路的旧伤**（都不是 M1 引入，但让 M0 验收回不来）：① 皮肤上下文未 inject → 编辑器根本挂不上（`ctx.set` → `ctx.inject`）② `tick` 在 ProseMirror 事务里同步通知 React → React 19 无限重渲染（改 rAF 合并）③ 题记围栏尾随空行导致 `:::` 漏成正文、署名认不出 | `scripts/e2e-ui.sh` **31/31** · `scripts/layout-check.sh` **32/32** |
| 2026-10-11 | 仓库 | 删除旧 `.git` 并重新 init：排除外部参考仓库与运行时数据后做首次提交（376 文件 / ~5.3MB）；`origin` 指回 Gitea | `git log` 单一提交；工作树干净 |
| 2026-10-11 | 基准 | 确立两层产出基准（D3）：规则层 `skills/samples`（18 篇）、**产出层 `/root/Game_Wiki` 真产出（36 篇）**；写作流程镜像 `WRITING-BRIEF.md`（`prompts.md §2.3`）；纠正技能命名（保持现状） | `README.md §3 D3` · `skills.md §5` |
| 2026-10-11 | 文档 | 冻结全部决策（D1–D50，收敛所有"待定"）；产出全套 16 篇开发文档；从 v1 与 `nashsu/llm_wiki` 提炼行为知识清单；确认前端"展示照 v1、实现换 AntD/Milkdown"，补全首页与编辑器（`frontend.md §2.2 §5.5`）；建立本追踪文件 | `docs/` 全部文件；本文件 |
| 2026-10 | M0 | 已有：`domain`/`store`/`api` 三层 + 前端骨架 + 尖刺 A（人的编辑与存储） | `backend.md §11` |

# Atlas v2 · 照着 v1 说「去 / 留 / 改」

> **这份文档回答一个问题**：v1（`archive/v1`）里哪些**留**、哪些**去**、哪些**改**，以及 llm_wiki 的方法论内核怎么并进来。
> **关联**：决策冻结表在 `README.md §3`；行为证据在 `behavior-notes.md`；功能面在 `features.md`。
> **"v1"指谁**：`archive/v1/` 里那个 Go + SQLite + React/Semi 的完整应用（它内部的设计文档叫 `DESIGN_V2.md`——别被名字绕晕，那是**归档应用的**冻结设计，本文一律称"v1"）。

---

## 0. 两份地图，先分清楚

### 0.1 Atlas v1（归档应用）是什么

- 面向**个人媒体库**的中文百科创作工具：把 Emby / Komga / GameAtlas 的收藏整理成「宇宙 → 系列 → 单作」的树，交给 LLM 馆员 **Atlas** 按 `skills/wiki-writing` 写条目。
- 技术：Go（net/http）+ SQLite + React + Semi Design；LLM 走 **OpenAI Responses** 协议，SSE 直出到界面。
- 它**已经把这些做对了**（v2 全部继承）：正文唯一真相（一篇 Markdown）、会话日志作为上下文事实源、Responses 原生事件 + `wikiatlas.*` 旁路、工具面纪律、护栏、流式正文投影、媒体库三源 + "AI 只建议不挂链"、判定口径版本化、视觉宪法（紫色只给 AI）。
- 它**有这些病根**（v2 要治）：
  - `schema_migrations` 写死 version=1（真实状态是"列存不存在"）；
  - 建/改节点是多条裸 Exec（非事务）；`updated_at` 手写；
  - 到处硬删，删节点连带毁修订历史；
  - 规则写进 `httpapi`，没有测试基建；
  - 强加载技能（`SKILL.md` + `core.md` + `media-*.md` 每个工单都塞进 system）；
  - 进程内队列 / 内存标志（`staging` 只在 SSE 里，崩了没痕迹）；
  - 前端单文件 962 行 / `index.css` 3730 行。

### 0.2 llm_wiki 是什么

- 一个 Kata（Karpathy llm-wiki 模式）的**完整落地**：LLM 增量编译、维护一份**持久化的 wiki**（markdown 页 + `[[wikilink]]` + frontmatter），而不是每次现挖 RAG。
- 三层：原始资料（不可变）→ wiki（LLM 生成）→ schema（规则）。
- 三操作：**Ingest / Query / Lint**。
- 技术：Tauri v2（Rust 后端）+ React + shadcn/ui；`src-tauri/src/agent/` 是一个 5800 行的 Rust agent（工具循环 / 流式 / skills / 会话）。

**核心差异（必须记住）**：

| | llm_wiki | Atlas |
|---|---|---|
| 一个"来源"产出 | **多篇**页面（entity/concept/synthesis/query） | **一篇**文档（一个库条目 ↔ 一篇） |
| 主干结构 | 页面**图**（wikilink + frontmatter `sources[]`） | 归属**树**（宇宙/系列/单作）+ 关系边 |
| "锚点" | 文档内容本身 | **外部媒体库身份** `source+external_id` |
| 原料 | 用户丢进来的任意文档（PDF/网页/…） | **媒体库清单**（三源）+ 基准语料 |

**结论**：llm_wiki 的**方法论内核**要并进来；它的**通用知识库实现**不要照搬。

---

## 1. 边界：Atlas 从 llm_wiki 只取内核

**要（内核）**：知识库是被持续维护的产物 · 增量编译 · 矛盾显式标注 · schema/purpose 分层 · 人策展 + LLM 维护 · **宿主确定性维护导航** · 可校验的产出结构。

**不要（实现）**：多格式解析（PDF/Office/EPUB/MOBI/MinerU）· 向量语义检索（LanceDB/pgvector 主路径）· 四信号图谱 + Louvain + 图谱洞察 · 去重队列 · Chrome 扩展 · 深度研究主路径 · Obsidian vault 兼容 · 桌面端打包。

**理由**：Atlas 的原料是**结构化清单**（媒体库条目）而不是"一坨任意文档"，所以 llm_wiki 里最重的那些"把野文档变成可检索知识"的机器，对 Atlas 是**成本 >> 收益**。

**但要补一个缺口**：llm_wiki 最值钱的东西是**页面之间的网 + 跨来源综合**。v1 的树只是"归属"，`relations` 只是"展示"。所以 v2 **必须**把「关系网 + 综合页」提成一等公民（见 §3.2 与 `features.md §6.5`）。

---

## 2. 逐层「去 / 留 / 改」

### 2.1 定位层

| | 内容 | 理由 |
|---|---|---|
| **留** | "媒体库文档层 + 一位有全权工具、没有手的馆员"；核心链（扫描→池子→建档→写正文→反哺→失效重挂） | 这是产品存在的理由，v1 已验证 |
| **改** | 把 llm_wiki 的 **Lint（体检）** 提成一等公民（v1 把它当附属） | "持续维护"的复利全靠它，否则 wiki 会烂 |
| **去** | 通用知识库幻觉（"什么都能装"） | Atlas 服务的是个人媒体库 |

### 2.2 结构层

| | 内容 | 理由 |
|---|---|---|
| **留** | 单 MD 真相（`content_md`/`body_md` 一篇到底）；树 `宇宙 → 系列 → 单作`；`doc`（资料）挂节点、不进树叶子 | v1 §3.1 已冻结，是对的 |
| **留** | **类型化有向关系边**（冻结词典，禁止自由文本谓词） | v1 §3.4 |
| **留** | 文档耐久、链可替换；拆链保留文档 | v1 §3.5 + llm_wiki 的 `sources[]` 共享实体保护同源 |
| **改** | 关系从"只展示"→ **进主结构**：关系要能被查询、被综合页引用、被体检发现缺口 | 见 §3 |
| **改** | 综合页 = `kind='doc'` 的跨作品页（角色/世界观/时间线/主题/创作者），可挂关系边 | 补上 llm_wiki 的"网" |
| **改** | 导航/索引/概览页由**宿主确定性维护**，不让模型整篇重写 | 借 llm_wiki：大库永远不被模型整篇重写 |
| **去** | `PageBlock` + `Entity/Fact/Relation` 三套真相 | v1 §2 的病根 |
| **去** | `Proposal` 多级审批 | 改为"写完即 commit + revision 回滚" |
| **去** | `Thread/Turn/Rollout` 双栈 | 改为"一个会话日志 + Run" |
| **去** | `links_json`（引用藏在 JSON、Go 里遍历） | 改真表 `doc_refs` |

### 2.3 Agent 层

| | 内容 | 理由 |
|---|---|---|
| **留** | **会话日志 = 上下文事实源**（append-only + `shadowed_by`/`replaces_seq`）；前缀稳定 + 重建一致两条不变量 | v1 §4.8，设计得最好的一块 |
| **留** | 提示词**分段组装**（`section{name,order,text}`）+ golden 快照 | v1 §4.9 |
| **留** | Responses 原生事件 + `wikiatlas.*` 旁路；`sequence_number` 只给 `response.*` | v1 §4.10 |
| **留** | **流式正文投影**（从 `function_call_arguments.delta` 抽正文，~200ms 节流，按章节分槽） | v1 的实时感来源 |
| **留** | 工具面纪律：只按用户显式选择剥写工具、意图不决定能力面、运行时二次校验 | `behavior-notes.md §4` |
| **留** | 护栏全套（空写/截断/归档/JSON/重复/联网/截断说话/破坏性写） | `behavior-notes.md §3` |
| **留** | 批量工具"波次 + 预检 + all-or-nothing + 上限" | 同上 |
| **留** | 上下文窗口=硬预算、可配、`×0.75` 压缩、压完拒发 | `behavior-notes.md §2` |
| **改** | 技能从"强加载整包"→ **L0 目录常驻 + `read_skill` 按需** | v1 的噪音/过拟合/耦合病 |
| **改** | 机械项从提示词/技能 → **质量门（代码）** | 同一病 |
| **改** | 队列从进程内 → **PG `jobs` + 租约（`SKIP LOCKED`）** | v1 崩了就没了 |
| **改** | 重启恢复：v1 "把 running 全标 interrupted 且不续跑" → **租约过期自动回收** | 同上 |
| **补** | 两步式（分析→生成）落地成两步 **run 阶段**（v1 intent 里有，但没有独立的便宜档分析步设计） | llm_wiki 的看家本领 |
| **补** | Review 队列（异步裁决 + 预定义动作 + 预生成搜索词） | llm_wiki；v1 的"建议池"只覆盖媒体库 |
| **去** | 自造投影层（`runProjection.ts` / `PlanSteps` / 自定义 `plan`/`tasks` 内容类型） | 交给 X runtime |
| **去** | `wroteContent` 靠内存标志 | 改 `write_events` 表（写入事实） |

### 2.4 媒体库层

| | 内容 | 理由 |
|---|---|---|
| **留** | 三源客户端（GA cookie / Emby token 只读 / Komga api key） | v1 已验证 |
| **留** | **AI 只建议不建库不挂链**；宿主 node-sweep 做写后关联 | v1 AGENTS |
| **留** | 三种"不要"语义分开（池子全局忽略 / per-node dismiss / 建议拒绝带理由回喂） | v1 AGENTS，逻辑很细，别简化 |
| **留** | 判定口径进数据（`judge_cache`），改口径自动重判 | v1 `komgaJudgeVersion` 的泛化 |
| **留** | 扫描低频、确定性、稳态零 LLM | v1 |
| **改** | 清单从一坨 KV → **真表** `library_manifest` | v1 病根 |
| **改** | `library_links` 加状态机（`active｜stale｜replaced` + `missing_since` + `replaced_by`） | v2 新增「失效与重挂」 |
| **改** | 新增 `sync_targets`（每条链 × 每个可写目标的同步状态） | v2 新增「反哺状态可查」 |
| **去** | 把 Emby `description` 当正文 | 正文只由 wiki 侧写 |
| **去** | 靠人记得给判定口径 +1 | 改指纹自动失效 |

### 2.5 数据层

| | 内容 | 理由 |
|---|---|---|
| **留** | v1 已验证的表形状（`runs`/`run_events`/`sessions`/`session_messages`）**原样搬** | append-only 日志是最好的设计 |
| **留** | `revisions`（快照）语义；UUIDv7；节点可见性 public/private | v1 |
| **改** | SQLite → **PostgreSQL** | 并发、`SKIP LOCKED`、`pg_trgm`、类型系统 |
| **改** | `schema_migrations` 写死版本 → **`sqlx migrate`（`NNNN_*.sql`）** | v1 病根 |
| **改** | 非事务多步写 → **一切多步写在一个事务**；时间戳 DB `now()` | v1 病根 |
| **改** | 到处硬删 → **默认归档，物理删只给人** | v1 病根 |
| **改** | `settings` 一坨 KV → 拆 `app_settings` / `secrets` / `library_manifest` / `library_ignores` | v1 病根 |
| **改** | 闭集枚举加 CHECK（新增取值走迁移，摩擦是有意的） | v2 |
| **改** | 排序键：整数 `sort_order` → **可插值 `sort_key`**（同级唯一，支持重排） | v2 |
| **补** | `write_events`（写入事实，取代内存 `wroteContent`） | v2 |
| **补** | `health_findings`（体检结果）、`suggestions`（Review）、`scan_runs` | v2 |
| **后置** | `embeddings`（语义检索）先建表、功能后置 | `database.md §3` |

### 2.6 前端层

| | 内容 | 理由 |
|---|---|---|
| **留** | 三面板布局（左树 / 中文档 / 右 AI）；飞书视觉语言；**紫色只给 AI** | v1 VISUAL_SPEC |
| **留** | 默认编辑态；编辑器顶栏切阅读 | v1 |
| **留** | 题记排版模型（外文原文小字 / 中译大字逐行交替，署名右对齐，无底色无边框） | v1 `lib/epigraph.ts` 直接移植 |
| **留** | `liveDraft` 值/写入口**双 context** 的做法 | v1 对，防全应用重渲染 |
| **改** | Semi Design → **AntD v6 + @ant-design/x** | 生态与 X 组件 |
| **改** | BlockNote → **Milkdown**（markdown 无转换层） | 避免 Tiptap/ProseMirror JSON 往返 |
| **改** | 手写 `fetch+useState` + 手动 refresh → **TanStack Query** | v1 同一份真相散落 8 处 |
| **改** | 2s/5s 轮询 → **事件驱动**（`sync.*`/`link.*`/`content.committed`）+ Query 失效 | v1 |
| **改** | `index.css` 3730 行全局样式 → **AntD token + CSS Modules**，无全局大文件 | v1 病根 |
| **改** | 手抄事件类型三处 → **后端生成 TS 类型**（ts-rs/typeshare） | v1 病根 |
| **改** | `AiPanel.tsx` 962 行 → 拆 transport / session / Conversation / HostWidgets | v1 病根 |
| **去** | 置顶功能（2026-09-17 已拍板移除） | v1 |

### 2.7 工程 / 运维层

| | 内容 | 理由 |
|---|---|---|
| **留** | 分层纪律（`cmd→httpapi→domain→store` → v2 为 `domain/store/sources/agent/api/worker`） | v1 AGENTS |
| **留** | 单文件 ≤300 行；真跑验收（无头浏览器截图 + 行为断言） | v1 AGENTS |
| **留** | SQL 全参数化；错误上抛不吞 | v1 |
| **留** | golden 快照守提示词 | v1 |
| **改** | Docker 一坨自建 dev 脚本 → 本机原生 PG + `scripts/`；compose 后置 | `database.md §6` |
| **补** | vitest / playwright / eslint+tsc（v1 测试基建为零） | `frontend.md §7` |
| **补** | 备份 pg_dump 定时；迁移前后各一次 | `database.md §6` |

---

## 3. 三条"改"的重点展开

### 3.1 关系网 + 综合页（补 llm_wiki 的"网"）

v1 的 `relations` 表只服务"作品关系"展示。v2 要让它**干活**：

- **关系是可查询、可体检、可被工具操作的对象**：`upsert_relation` / `detach_relation` 进工具面；体检能发现"孤立节点 / 关系缺口"。
- **综合页**（`kind='doc'`，跨作品）：角色页 / 世界观页 / 时间线页 / 主题页 / 创作者页。它们引用多个作品节点，是 llm_wiki "synthesis" 的对位物。
- **导航由宿主维护**：首页概览、索引页、系列页的目录骨架由代码确定性更新（新增节点就追加一行），模型只写正文，不重写整篇导航。
- **判据**：任何"把多个作品连起来看"的产出，都归综合页；任何"单部作品内部"的产出，都归它的 work 文档。

### 3.2 技能与质量门的分工（治 v1 的过拟合）

- **机械项 → 代码**：章节齐全 / 编号连续 / 章名按介质映射 / 字段行格式 / `:::epigraph` 唯一 / 参考资料条数 / `待核实` 用法 / 禁用表达 / 全篇字数下限 / 引用指向存在的节点。（清单见 `prompts.md §3`）
- **判断项 → 技能**（按需读）：教判断，不教长相；每条规则要能回答"删了它会坏什么"。
- **L0 只放目录**：`name` + 一句话 `description`（含"何时用/何时不用"）；正文 `read_skill` 按需读。
- **收益是"一致性 + 可校验 + 可迭代"**，不是"单篇更长更全"——判据按这个用（`eval-cases.md`）。

### 3.3 从"进程内状态"到"PG 唯一真相"（治 v1 的崩了就丢）

v1 的 `staging`（生成中的正文）只存在于 SSE 事件里、队列在进程内、`wroteContent` 在内存、判定口径靠人记得 +1。v2 一律**落库**：

- 生成中正文 → **草稿行**（`content_drafts`，节流写；SSE 只是加速）；
- 队列 → **`jobs` + 租约**（`FOR UPDATE SKIP LOCKED`，租约过期自动回收）；
- 写入事实 → **`write_events`**；
- 判定口径 → **`judge_cache.fingerprint`**（改 prompt/换模型自动重判）。

---

## 4. 明确**不**采纳 llm_wiki 的哪些（附理由）

| llm_wiki 能力 | 不采纳的理由 |
|---|---|
| 多格式文档解析（PDF/Office/EPUB/MOBI + MinerU） | Atlas 的原料是媒体库清单，不是野文档；需要时用 workspace 里的技能样例 |
| 向量语义检索（LanceDB）+ 去重队列 | 一个条目一篇文档的规模，bigram/trigram 够用；向量后置 |
| 四信号图谱 + Louvain 社区 + 图谱洞察 | 通用知识库的图谱分析，对媒体库收益有限；只保留"关系图可视化"（可后置） |
| Chrome 网页剪藏 | 与"媒体库文档层"无关 |
| 深度研究（Tavily/SerpApi/SearXNG）主路径 | 保留为**默认关**的能力，接口先设计、实现后置 |
| Obsidian vault 兼容 | 明确不做（D4） |
| Tauri 桌面端 | v2 是 web（api + worker）；但"agent core 在后端、前端只渲染 UI 状态"这条纪律**要抄** |
| 本地 HTTP API 的"双向写" | v2 对外**只读优先**（MCP server），写操作留给人 |

---

## 5. 迁移风险（提前说清）

1. **行为知识会丢** → 已落 `behavior-notes.md`；**写 M3 前必读**，逐条对照。这是头号风险。
2. **流式投影难** → M3 里独立成模块 + 重点单测，和其余部分隔离；它是"实时感"的唯一来源。
3. **Rust 迭代慢** → 已确认走 Rust，但有**退出条件**：M3 做完若"改一次 prompt 往返明显慢于 v1"，回 Go（PG schema / 前端 / 功能设计不受影响，见 `backend.md §9`）。
4. **技能重构可能退化产出** → 逻辑不是"删技能"，是"机械项搬质量门 + 判断项按需读"；18 篇基准是唯一对齐依据，改技能必跑回归。
5. **前端重写丢细节** → v1 的题记排版、`liveDraft` 双 context、粘贴文档头拆分（`transformPastedText`/`splitDoc`）都是踩出来的，`frontend-migration.md` 逐条列了，别漏。

# WikiAtlas v2 · 开发文档总纲

> **本文是整套文档的入口与最高裁决处。** 任何具体文档之间、或文档与代码之间冲突，一律以本文件的「决策冻结表」为准；表里没有的，按「文档地图」里的分工判定。
> 本套文档的**唯一目标**：让后续开发可以**完全照着文档进行**，不再需要产品方逐条拍板。
> 因此：本文写完之后，**不再有"待定"章节**——每个开放问题都已收敛成一个决定；新问题一旦出现，追加到本文件的决策表，而不是散落在各文档。

---

## 0. 一句话定位

**WikiAtlas = 你的媒体库书架 + 一位有全权工具、但没有手的馆员。**

- **它是「媒体库的文档层」**：GameAtlas / Emby / Komga 里的**一个条目对应一篇 wiki 文档**；WikiAtlas 是文档真源，把最新定稿**反哺**回库。
- **方法论基准是 `nashsu/llm_wiki`（Karpathy 的 llm-wiki 模式）**：知识库是**被持续维护的产物**（持续编译），不是每次查询现挖 RAG。
- **它不复制 llm_wiki 的"通用知识库"实现**（多格式解析 / 向量检索 / 图谱算法 / 深度研究 / 桌面端），只取它的**方法论内核**：增量编译 · 矛盾显式 · schema 分层 · 人策展 + LLM 维护。详见 `docs/v2-from-v1.md`。

**核心链**（应用存在的理由，缺一不可）：

```
① 扫描        三源 → 条目清单（宿主，零 LLM，纯确定性）
② 池子        列出"库里有哪些条目 + 哪些还没有文档"
③ 建立文档    人点一次 → 原子产出【树里的节点】+【条目↔节点 的链】
④ 写正文      模型写 → 草稿落库 → 定稿（草稿/定稿/快照）
⑤ 反哺        定稿 → 推回 GameAtlas / Komga（Emby 只读，不参与）
⑥ 失效与重挂  库里条目没了 → 链标记失效 +「重新挂载」；文档永不动
```

**三条硬规则（不可违背）**：

1. **扫描与建档不需要模型。** 模型的位置在 ③ 之后（写正文、维护），不在"该不该建"这个判断上。
2. **关联靠身份，不靠标题**：`source + external_id`。标题会变、会重名、会分版本。
3. **文档是耐久物，链是可替换的指针。** 链失效只标记，不删链、不删文档。

---

## 1. 技术基座（冻结）

| 维度 | 决定 |
|---|---|
| 功能基准 | `nashsu/llm_wiki`（持续维护一份 wiki） |
| 布局蓝本 | 飞书云文档三面板（左树 / 中文档 / 右 AI 助手）—— **布局**，不是视觉母本 |
| 视觉母本 | v1 的 `archive/v1/VISUAL_SPEC.md`（颜色只用 token、紫色只给 AI） |
| 前端 | React 19 + Vite + TS(strict) + **Ant Design v6** + **@ant-design/x** + TanStack Query + Zustand + Milkdown |
| 后端 | **Rust**（axum + sqlx + tokio），进程形态 `api` + `worker` |
| 数据库 | **PostgreSQL 18**（`pg_trgm` 必须；`pgvector` 后置） |
| 编辑器 | Milkdown（存储格式 = `body_md`，无转换层） |
| 事件协议 | OpenAI **Responses** 形状（`response.*`）+ 宿主旁路（`wikiatlas.*`） |
| 对外 | 先做 **MCP server**；本地 HTTP API 同期；client 后议 |

---

## 2. 文档地图（读哪本、管什么）

| 文档 | 管什么 | 主要读者 |
|---|---|---|
| **`docs/TRACKING.md`** | **续工入口：当前状态 / 里程碑看板 / 进度日志**（新对话第一步读它） | 所有人，最先读 |
| `docs/README.md` | **总纲 · 决策冻结 · 路线图** | 所有人，先读 |
| `docs/features.md` | 功能清单（做什么 / 不做什么） | 所有人 |
| `docs/v2-from-v1.md` | **v1 → v2 的「去 / 留 / 改」**（为什么这么定） | 所有人，接手前必读 |
| `docs/behavior-notes.md` | **行为知识清单**（v1 血泪 + llm_wiki 教训；写 agent 层前必读） | 后端 / prompt / 技能维护者 |
| `docs/ai.md` | AI 设计（能力面 / 两步式 / 落点 / 护栏 / 成本） | 后端 / prompt |
| `docs/tools.md` | **工具面契约**（工具清单 · schema · kind · 护栏 · 批量） | 后端 / agent |
| `docs/prompts.md` | **提示词分层 + 质量门契约**（分段表 · golden · 机械项清单） | prompt / 后端 |
| `docs/skills.md` | 技能体系（L0/L1/L2/L3 · 写法纪律 · 评测） | 技能维护者 |
| `docs/eval-cases.md` | 评测用例与触发纪律 | 技能 / prompt |
| `docs/library-sync.md` | **媒体库子系统**（三源 · 扫描 · 建档 · 反哺 · 失效重挂） | 后端 / 前端 |
| `docs/backend.md` | 后端设计（进程 · crate · 队列 · 流式投影） | 后端 |
| `docs/database.md` | 数据库设计（表 · 索引 · 迁移 · 检索） | 后端 |
| `docs/api.md` | **HTTP API + 事件契约 + SSE 纪律** | 后端 / 前端 |
| `docs/frontend.md` | 前端设计（基座 · 三面板 · 适配层 · 状态模型） | 前端 |
| `docs/frontend-migration.md` | v1 前端 → v2 的迁移清单与批次 | 前端 |
| `docs/testing.md` | 测试与验收（单测 / 集成 / 真跑 / 截图） | 所有人 |
| `docs/refs/trae-skill-best-practice.md` | 外部参考（技能写法） | 技能维护者 |

**阅读顺序（新人 / 新会话）**：**`TRACKING`（先读它，拿状态与下一步）** → `README`（本文，拿决策）→ `features` → `v2-from-v1` → 按你负责的层读 `backend/database/api` 或 `frontend`；写 AI 之前必读 `behavior-notes` + `ai` + `tools` + `prompts`。

---

## 3. 决策冻结表（唯一定论处）

> 表里每条都能追到详细文档。**「裁决」列的措辞即最终决定**，实现不得偏离；若实现中发现问题，改这里 + 对应文档，再改代码。

### 3.1 产品与定位

| # | 裁决 | 详 |
|---|---|---|
| D1 | WikiAtlas = 媒体库文档层；**一个库条目 ↔ 一篇文档**；WikiAtlas 是真源，定稿反哺回库 | `features.md §0` |
| D2 | **不迁 v1 数据**，v2 从空库起；种子数据用 `skills/samples/` 的 18 篇基准 | `backend.md §10` |
| D3 | 「写得好」分两层：**规则层** = `skills/samples/`（18 篇手写基准，与技能**双向对齐**）；**产出层** = **`/root/Game_Wiki` 的真产出**（Altas 按本技能集生成、随生产增长）。**V2 的 Altas 产出必须达到 Game_Wiki 这一档**，Game_Wiki 兼作产出回归语料 | `skills.md §5` · `eval-cases.md` |
| D4 | 明确不做：多租户 / 权限组 / 发布工作流 / IM 机器人 / 桌面打包 / Obsidian 兼容 / 移动端 / 多语言界面 | `features.md §10` |
| D5 | 访问控制 = 单一访问密码 + 节点级 public/private（沿用 v1 语义）；**默认 private** | `database.md §2` |

### 3.2 结构与内容

| # | 裁决 | 详 |
|---|---|---|
| D6 | 树 = `universe｜series｜work｜doc`，靠 `parent_id`；**doc（资料）不进主树叶子**，挂节点 | `database.md §2` |
| D7 | 关系 = **类型化有向边**，方向语义唯一定义在 `relation_types` 表；自指 CHECK、悬空 FK、成环应用层 | `database.md §2.1` |
| D8 | **关系网 + 综合页进 v2 主结构**：综合页 = `kind='doc'` 的跨作品页（角色/世界观/时间线/主题/创作者）；系列主文 = `series` 节点正文 | `features.md §6.5` |
| D9 | 聚合页（首页概览 / 索引）由**宿主确定性维护**，不交给模型整篇重写（借 llm_wiki 做法） | `features.md §6.5` |
| D10 | 文档耐久、链可替换：链失效只标 `stale`，**不删链、不删文档**；重挂产 `replaced` | `library-sync.md §6` |

### 3.3 AI / Agent

| # | 裁决 | 详 |
|---|---|---|
| D11 | 三个操作：**Ingest（写正文）/ Query（问答）/ Lint（体检）**；写正文走**两步式**（分析→生成），小任务跳过 | `ai.md §4` |
| D12 | 建档**零 LLM、人点**；AI 只能**建议**建档（进 Review），但能**直接建关系**（受护栏） | `ai.md §3` |
| D13 | 未建档库条目**只能分析，不能写正文**（先建档） | `ai.md §2` |
| D14 | 工具面**只按用户显式选择**剥写工具（`docMode=read` / `oneShot`）；**意图不决定能力面**；运行时二次校验 | `tools.md §3` |
| D15 | 批量工具一律**波次 + 预检在前 + 全成或全不成 + 上限** | `tools.md §4` |
| D16 | 护栏（全保留）：空写拒绝 · 截断守卫 `>200→<50` 拒 · 归档拒写 · 参数 JSON 错报错 · 重复调用分级 · 联网预算 · 截断必须说话 · 破坏性写只许自有可回滚 | `behavior-notes.md §3` |
| D17 | 质量门 = **代码**（机械项）；技能 = **判断**；**L0 目录常驻 + `read_skill` 按需** | `prompts.md §3` / `skills.md §1` |
| D18 | 产出落点三档（固定）：**草稿正文**（模型写、永不直接定稿）/ **结构操作**（直接做、受护栏）/ **建议**（进 Review、人裁决） | `ai.md §2` |
| D19 | 问答产物只能**存成「资料」**，不直接改正文（正文改动走修订，才有版本可追溯） | `ai.md §3` |
| D20 | 模型路由**可分别配**（写正文/分析/看图/研究）；分析步默认**便宜档** | `backend.md §6` |
| D21 | **看图、研究默认关**，用户开了才跑 | `ai.md §3` |
| D22 | **会话日志 = 上下文的事实源**（append-only + `shadowed_by` / `replaces_seq`）；两条不变量：**前缀稳定 + 重建一致** | `behavior-notes.md §1` |
| D23 | 上下文窗口**硬预算**、按模型可配（4K–1M）；按比例分配；`×0.75` 压缩一次，压完仍放不下就**拒发并说明** | `behavior-notes.md §2` |
| D24 | 提示词 = **分段组装**（`section{name, order, text}`）；稳定文本用 **golden 快照**钉住 | `prompts.md §1` |
| D25 | Review 动作枚举（防幻觉）：**建档 / 关联 / 合并 / 研究 / 忽略**；每条带"为什么 + 怎么做 + 预生成搜索词" | `ai.md §9` |

### 3.4 媒体库

| # | 裁决 | 详 |
|---|---|---|
| D26 | 三源：GameAtlas（管理员会话 cookie）· Emby（`X-Emby-Token`，**只读**）· Komga（`X-API-Key`，读写） | `library-sync.md §1` |
| D27 | 身份 = `source + external_id`（GA public_id / Emby item id / Komga series id）；**一个库条目只能挂一篇文档** | `library-sync.md §2` |
| D28 | 扫描**确定性、零 LLM**；与链比对产出三事件：新增 / 消失（→stale）/ 变更（→体检项） | `library-sync.md §3` |
| D29 | 三种"不要"语义分开：池子全局忽略（条目没价值）/ 节点 per-node dismiss（别向此节点提）/ 建议拒绝（带理由，回喂二次判断） | `library-sync.md §4` |
| D30 | 判定口径进数据：`judge_cache.fingerprint = hash(prompt_version, model, input_signature)`，改口径自动重判 | `database.md §2` |
| D31 | 容器语义两类：GA/Komga 的 series = **作品系列**（可当父节点）；Emby 的 library = **库**（拿库名建节点无意义） | `library-sync.md §2` |
| D32 | 反哺时机 = **定稿自动推** + 手动重推兜底；语义 = WikiAtlas 独占字段、以最新定稿覆盖；幂等（同 `synced_ver` 不重推） | `library-sync.md §5` |
| D33 | 重挂 = **宿主建议（归一化标题 + trigram 模糊，零 LLM）+ 手动兜底**；旧链标 `replaced`，文档不动 | `library-sync.md §6` |
| D34 | Komga 摘要适配规则：取正文**首个非说明非题记段落**，去 markdown 标记，**截断上限 500 字**、超出加 `…`（实现期以 Komga 实测字段约束为准，若更严则取更短，规则此处冻结） | `library-sync.md §5` |

### 3.5 数据与后端

| # | 裁决 | 详 |
|---|---|---|
| D35 | **PG 是唯一真相**；迁移有版本（`sqlx migrate`）；一切多步写在一个事务；时间戳由 DB `now()`；删除默认归档 | `database.md §0` |
| D36 | 中文检索：**先做归一化精确匹配 + `pg_trgm` 模糊/trigram**；**不上 zhparser/pg_jieba**；bigram 在应用侧兜底；`pgvector` 后置但预留 `embeddings` 表 | `database.md §3` |
| D37 | `runs.id` / `sessions.id` 用 **text**（v1 是自由串）；时间戳一律 `timestamptz`；闭集枚举加 CHECK | `database.md §2` |
| D38 | **Rust 确认**（llm_wiki 的 agent core 已是 Rust：`runtime.rs` 5816 行 / `tools.rs` 3888 行，跑得起来）；退路保留但不再是默认 | `backend.md §9` |
| D39 | 两进程 `api` + `worker`，同一 PG；**LLM 调用永远在事务外** | `backend.md §1` |
| D40 | 分层 `domain/store/sources/agent/api/worker`；`domain` 零 IO；业务规则不许进 handler | `backend.md §3` |
| D41 | 单文件 ≤300 行；**例外**：`agent::runtime` / `agent::tools` 允许更高上限，但必须按子模块拆分、并在文件头写清拆分理由 | `backend.md §3` |

### 3.6 前端

| # | 裁决 | 详 |
|---|---|---|
| D42 | AntD v6 + @ant-design/x；**服务端状态**归 TanStack Query，**UI 状态**归 Zustand（不攒草稿） | `frontend.md §1` |
| D43 | 编辑器 = Milkdown（markdown ↔ `body_md` 无转换层）；必带 `listener` + `clipboard` + `history` | `frontend.md §1` |
| D44 | 三面板（飞书布局）；视觉照 v1 `VISUAL_SPEC`：颜色只用 AntD token、**紫色只给 AI**、不反向覆盖 `.ant-*` | `frontend.md §2 §6` |
| D45 | 事件两条线：`response.*` → X runtime；`wikiatlas.*` → 宿主部件；**适配层单独模块**；**类型由后端生成** | `api.md §4` |
| D46 | 草稿**只读**（服务端草稿行是真相，`content.delta` 只是加速通道）；重连从服务端草稿重新基线 | `frontend.md §5` |
| D47 | 人与模型**共用同一条写路径**（防抖写草稿 → 定稿事务）；乐观冲突给「载入最新」/「以我的为准覆盖」 | `frontend.md §5` |

### 3.7 运维与对外

| # | 裁决 | 详 |
|---|---|---|
| D48 | 对外先做 **MCP server + 本地 HTTP API**；**只读优先**，明确触发词约束（不响应泛泛的"搜我的笔记"） | `features.md §9` / `api.md` |
| D49 | 备份：`pg_dump` 定时 + 迁移前后各一次；导出/导入项目整个树 | `database.md §6` |
| D50 | 成本可见：每次工单 token/成本可见；批量上限；定稿反哺幂等避免重复成本 | `ai.md §11` |

### 3.8 实现期补充（M1 落地时拍板，追加不改前面的）

| # | 裁决 | 详 |
|---|---|---|
| D51 | **Komga 卷/抽屉判型在扫描里只用确定性启发式**（零 LLM）；判歪的照进池子，等 M5/M7 的「AI 对一遍」（可选、产出建议、人确认）收敛 | `library-sync.md §1 §7` |
| D52 | **Emby 混杂库**用 `app_settings.library.emby_mixed_views`（view id 或库名数组）标定，命中 → `container_kind=mixed`，条目落链 `role=mix`。**Emby/Komga 封面 M1 不落清单**（缩略图要带 token/api-key，落进清单就泄漏给前端）→ 封面代理端点随设置页（M9） | `library-sync.md §1 §2.4` |
| D54 | run 的目标节点由**宿主注入提示词**（模型不猜 id），写工具再二次校验；**写类工单第一轮 `tool_choice=required`**（实测模型爱"把正文写在回复里"而不调工具），之后交回 `auto`。模型配置（base_url / key / model / effort / **context_window**）走 `secrets` 表 `llm.*`，窗口只有一个来源 | `backend.md §5 §6` · `behavior-notes.md §2 §5` |
| D53 | 源密钥**唯一真相是 `secrets` 表**（`<source>.base_url` / `gameatlas.password` / `emby.token` / `komga.api_key`）；**M1 允许 `WIKIATLAS_*` 环境变量兜底**（设置页还没做），设置页落地后删掉兜底那一半。密钥值**绝不进任何 API DTO** | `database.md §4` / `library-sync.md §1` |

---

## 4. 开发路线图（里程碑 · 每步可独立验收）

> 原则：**每个里程碑都要有一个"真跑"的验收**（不是"编译通过"），且都从**空库**起跑。
> 依赖关系：M1→M2 是核心链前半；M3 是 Agent 内核（最关键的不确定性）；M4 起进入写作；M5–M8 是维护与产出层；M9 收口。
> 每个里程碑的详细规格在其"依据"列所指文档。

| 里程碑 | 产出 | 依据 | 验收（真跑） |
|---|---|---|---|
| **M0 基线** ✅ | `domain`/`store`/`api` 三层 + 前端骨架 + 尖刺 A（人的编辑与存储） | `backend.md §11` | 已有：`cargo test` + `scripts/e2e-ui.sh` |
| **M1 媒体库进料** | `sources` crate：三源客户端 + 清单构建 + `scan_runs`；池子 API + 前端池子面板 | `library-sync.md §1–3` | 真库（192.168.1.4）扫三源 → 清单进 `library_manifest` → 池子列出"有/无文档" |
| **M2 建档与关联** | 落位规则（domain）+ 原子建节点+链（store 事务）+ 池子建档/忽略 + 链管理 | `library-sync.md §2 §4` | 池子点一条 → 树出现节点 + 链 active；解链保留文档；撞名加介质后缀 |
| **M3 Agent 内核**（尖刺 B） | `worker` + `jobs`（租约）+ run 执行器 + Responses 客户端 + 会话日志 + SSE + 流式正文投影 | `backend.md §5 §6` + `behavior-notes.md` | 真跑一次 run：流式正文投影 + 一个写工具 → SSE 到浏览器 → 草稿落库 |
| **M4 写作与质量门** | 技能加载（L0+按需）+ 两步式 + `write_content`/`patch_section` + 质量门 + 草稿/定稿/快照 | `ai.md §4` + `prompts.md §3` + `tools.md §2` | 空库跑一篇完整 18 篇基准同构的正文 → 过质量门 → 定稿 → 快照生成 |
| **M5 关系与综合** | 关系工具 + 综合页（doc 型）+ 系列主文 + 聚合页宿主维护 | `features.md §6.5` + `tools.md §2` | 建一篇综合页并挂关系 → 图谱/关联展示 → 索引页由宿主更新 |
| **M6 Review 与体检** | `suggestions`（Review 队列）+ `health_findings` + 体检面板 + 确定性检查 | `ai.md §9` + `features.md §6` | 制造孤立节点/失效链/定稿未反哺 → 体检列出 → Review 可裁决 |
| **M7 反哺与失效重挂** | `sync_targets` + 定稿触发 + GA/Komga 写回 + 失效检测 + 重挂建议 | `library-sync.md §5 §6` | 定稿 → GA/Komga 收到；停掉一个库条目 → 链 stale + 重挂候选 |
| **M8 检索与问答** | bigram/trigram 检索 + 问答（引用到节点）+ 存为资料 | `database.md §3` + `ai.md §7` | 就库内容提问 → 回答带可点节点引用；好答案存成资料 |
| **M9 对外与运维** | MCP server + 本地 API + 备份/导出 + 用量成本 + 设置收口 | `api.md` + `features.md §9` | 外部 agent 通过 MCP 只读检索；pg_dump 恢复；成本可见 |

**关键风险与前置**（见 `backend.md §9`）：
1. **行为知识会丢** → 先写 `behavior-notes.md`（**已完成**，写 M3 前必读）。
2. **流式正文投影**（M3）难度高、是体验核心 → 独立模块 + 重点单测，与 M3 其余部分隔离着打。
3. **Rust 迭代速度** → 已确认走 Rust；若 M3 做完但"改一次 prompt 往返明显慢于 v1"，才触发回 Go 的退出条件（届时 PG schema / 前端 / 功能设计不受影响）。

---

## 5. 开发纪律（照做即可）

1. **文档即规范**：先读对应文档，再写代码；文档与代码不一致时，先改文档（或先确认本文决策表），再改代码。
2. **单文件 ≤300 行**（D41 的例外除外）；**颜色只用 token**；**紫色只给 AI**（`frontend.md §6`）。
3. **分层纪律**：`domain` 零 IO；`api`/`worker` 只编排；业务规则进 `domain` 或 store，不进 handler。
4. **一切多步写在一个事务**；时间戳由 DB `now()` 出；空列表给 `[]` 不给 `null`。
5. **验收必须真跑**：无头浏览器截图 + 行为断言 + 回服务端核对；不许只凭"构建通过"。
6. **改 AI 行为**（技能 / 提示词 / 工具）→ 跑基准回归 + 触发纪律评估（`eval-cases.md`）；不过就**优先简化，不叠加新规则**。
7. **真模型调用要显式确认**（历史上误花过额度）；测试默认用 mock / 副本库。
8. **密钥永不进代码/文档/日志**。

---

## 6. 文档维护纪律

- 本文件的**决策冻结表**是最高裁决处；新增决定追加到表里，不散落。
- 每个文档顶部的 `> 依据 / 关联` 交叉引用要维护，别留悬空指针。
- 数字与状态（窗口、预算、版本）**从代码注入，不手抄进提示词/技能**；提示词稳定文本用 golden 快照钉住。
- 技能里的规则要能回答"删了它会坏什么"；答不出的删掉（`skills.md §3`）。

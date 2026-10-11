# WikiAtlas v2 · 追踪文件（续工入口）

> **这个文件是唯一的"续工入口"。** 新对话开始时**只加载它**，agent 就能按 §1 的恢复协议接手，不需要产品方再指方向。
> **它是一份"活文档"**：每完成一件事，就更新 §2 快照、§3 看板、§6 进度日志。改了要改的，别另起炉灶。
> **最后更新**：2026-10-11 · 状态：**文档已冻结（实现方代拍板），代码 M0 ✅ / M1 ✅ / M2 ✅，下一步 M3**

---

## 0. 载体说明（这是什么、怎么用）

- **这套文档 = 规范**，代码照它写。冲突时**以 `docs/README.md §3 决策冻结表`为准**，其次本文 §3 看板。
- **本文件 = 状态**：做到哪、下一步做什么、哪些坑不能踩。**任何新决定先追加到 `README.md §3`，再回填本文。**
- **给新对话的第一句**（模板，直接复制）：

  > 读 `docs/TRACKING.md`，按它的 §1 恢复协议继续开发 WikiAtlas v2；当前里程碑以它的 §2 为准。

---

## 1. 恢复协议（新对话 / 新 agent 照着做，别跳步）

1. **读本文件**（尤其 §2 当前快照 + §3 看板 + §5 风险）。
2. **读 `docs/README.md`**：拿决策冻结表（D1–D50）与里程碑定义。
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
| 文档冻结 | ✅ 全部 16 篇已冻结（决策 D1–D50 无"待定"） |
| 代码基线 | ✅ M0（`crates/{domain,store,api}` + `frontend/` 骨架 + 尖刺 A）· ✅ M1（`crates/{sources,worker}` + 池子 API/UI + 0002 迁移） |
| 产出基准 | **规则** = `/root/WikiAltas/skills`（14 技能 + `samples/` 18 篇）；**质量目标** = `/root/Game_Wiki` 真产出（D3） |
| 当前里程碑 | **M3 Agent 内核（尖刺 B）** |
| 下一步（唯一） | M3：run 执行器 + Responses 形状事件流 + SSE 投影 + 一个写工具落草稿（`docs/backend.md` §5 §6 · **先读 `behavior-notes.md`** · 尖刺 B） |
| 阻塞项 | 无 |
| 需要产品方做的事 | **无**（已委托；一切按文档与本文裁决） |
| 仓库状态 | ✅ 已**重新起仓**（旧 `.git` 备份在 `/tmp/wikialtas-git-backup-*.tar.gz`）；`origin`=Gitea（**新历史尚未推送**，首推需 `-f`）。M1 后**工作树待提交** |

---

## 3. 里程碑看板（**唯一进度真相**）

图例：⬜ 未开始 · 🟡 进行中 · ✅ 完成（= 该行验收真跑通过）

| 里程碑 | 状态 | 依据文档 | 验收（真跑） |
|---|---|---|---|
| **M0 基线** | ✅ | `backend.md §11` | `cargo test --workspace` + `scripts/e2e-ui.sh` 全过 |
| **M1 媒体库进料** | ✅ | `library-sync.md §1–3` | **真跑过**：真库（192.168.1.4）扫三源 → 252(GA)+1523(Emby)+113(Komga) 条落 `library_manifest` → 池子列出「有/无文档」。见 `scripts/e2e-library.sh`（15/15） |
| **M2 建档与关联** | ✅ | `library-sync.md §2 §4` | **真跑过**：池子点「建档」→ 树出系列节点 + work 文档 + 链 active；与父同名 → 加介质后缀（`SPY×FAMILY間諜家家酒（漫画）`）；解链只删链、文档还在。见 `scripts/e2e-link.sh`（17/17） |
| **M3 Agent 内核**（尖刺 B） | ⬜ | `backend.md §5 §6` · `behavior-notes.md` | 真跑一次 run：流式正文投影 + 一个写工具 → SSE 到浏览器 → 草稿落库 |
| **M4 写作与质量门** | ⬜ | `ai.md §4` · `prompts.md §2.3 §3` · `tools.md §2` · **D3** | 空库跑一篇 → **达到 `/root/Game_Wiki` 质量档**（同骨架/密度/证据落点）→ 过质量门 → 定稿 → 快照 |
| **M5 关系与综合** | ⬜ | `features.md §6.5` · `tools.md §2` | 综合页挂关系 → 关联展示 → 索引页由宿主更新 |
| **M6 Review 与体检** | ⬜ | `ai.md §9` · `features.md §6` | 制造孤立节点/失效链/定稿未反哺 → 体检列出 → Review 可裁决 |
| **M7 反哺与失效重挂** | ⬜ | `library-sync.md §5 §6` | 定稿 → GA/Komga 收到；停一个库条目 → 链 stale + 重挂候选 |
| **M8 检索与问答** | ⬜ | `database.md §3` · `ai.md §7` | 就库内容提问 → 回答带可点引用；好答案存成资料 |
| **M9 对外与运维** | ⬜ | `api.md` · `features.md §9` | 外部 agent 走 MCP 只读检索；`pg_dump` 恢复；成本可见 |

### 3.1 前端编辑器待迁项（v1 有、v2 尚未落——随对应里程碑补）

来源：`frontend.md §5.5`（v1 编辑器规格）。这些**不是可选项**，是 M3/M4/M6 的组成部分：

- [ ] **实时草稿只读显示**（`liveMd`；服务端草稿为真相，`content.delta` 仅加速）——随 **M3/M4**
- [ ] **落块动画**（`live-tail`：压暗 + 紫色脉动条 + 写完渐显；增量替换、不能整篇重建）——随 **M4**
- [ ] **「…」更多菜单**：版本历史 / 回滚 / 归档 / 删除 / 节点信息 / 媒体关联——随 **M2/M4**（`GET /versions` 已有）
- [ ] **修订接受 / 拒绝条**（v1 `PendingRevisions`）——并入 **M6** 的 Review 队列
- [ ] **工具栏 / 斜杠菜单 / 块手柄的视觉样式**（`.feishu-toolbar` `.slash-menu` `.block-handle`：
      现在**只有宿主容器**（`.editor-portal`，M1 补的，避免页面多一条滚动），组件本身没有 CSS，
      是「能用但没打扮」的状态）——随 **M3/M4** 按 v1 `VISUAL_SPEC.md` 补回

### 3.2 每个里程碑的铁律（复述，别忘）

- 验收 = **真跑**（副本库 + 无头浏览器 + 回服务端核对），不是"构建通过"。
- AI 相关：改技能/提示词必跑基准回归 + 触发纪律；**不过就简化，不叠加新规则**。
- **真模型调用要显式确认**；测试默认 mock / 副本库。
- 一切多步写在一个事务；`domain` 零 IO；单文件 ≤300 行（`agent::runtime`/`tools` 例外，见 D41）。

---

## 4. 决策与文档冻结状态

| 项 | 状态 |
|---|---|
| 决策冻结表（D1–D50） | ✅ 冻结，`docs/README.md §3` |
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
| R4 | 中文检索召回 | 归一化 + `pg_trgm` + bigram | M8 用真语料看召回，决定要不要加列 | `database.md §3` |
| R5 | Rust 退出条件 | 保留（M3 后若改 prompt 往返明显慢于 v1 才回 Go） | M3 结束时评估 | `backend.md §9` |
| R6 | Komga 判型（分卷 / 抽屉）M1 只跑**确定性启发式**（`sources::komga::book_is_volume`），脏命名会判歪 | 保持零 LLM 扫描；判歪的进池子也只是多几条候选 | M5/M7 做「AI 对一遍」（`library-sync.md §7`）时收敛；真库实测：`为美好的生活献上祝福`（库名把「世界」写成了「生活」）被拆成 33 个单册，就是这类脏数据 | `library-sync.md §1 §7` |
| R7 | `scripts/md-check.sh` 有 3 项红（非 M0/M1 验收门）：①「页面标题用节点标题」断言过期（样本 H1 = `《魔卡少女樱》`，脚本却期望节点标题）②③ Milkdown 的 GFM 序列化**会给表格单元格补空格**（`\| 卷    \| 卷名   \|`），所以 md5 不是逐字节、"\| 卷 \| 卷名 \|" 字面量也搜不到——**内容没丢** | 不删断言、不静默改 | M3/M4 编辑器迁移时二选一：给序列化补 normalize，或把断言改成「去 `\|` 周围空白后等价」 | `scripts/md-check.sh` |
| R8 | 编辑器浮层宿主（工具栏/斜杠/块手柄）曾是 body 里的静态 div，撑出 46px 页面滚动 | M1 已修：`.editor-portal{position:fixed}` + `[data-show=false]{display:none}` | 已完成（`layout-check.sh` 32/32） | `frontend/src/editor/editor.css` |

**已知头号风险**：**行为知识会丢** → 缓解 = `behavior-notes.md`（**写 AI 前必读、逐条对照**）。

---

## 6. 进度日志（append-only，最新在上）

| 日期 | 里程碑 | 做了什么 | 证据 |
|---|---|---|---|
| 2026-10-11 | **M2** | `domain::place`（落位规则：介质推定 / 撞名后缀 / 复用判据，从 v1 `library_place.go` 搬）· `store::library::{build_doc,unlink,add_ignore,remove_ignore,list_ignores}`（建档三件事一个事务）· `nodes::{create_in_tx,get_in_tx}`（把建节点抽出来共用）· API `POST /api/library/links` + `DELETE /api/library/links/{id}` + `/api/library/ignores` CRUD · 池子行加「建档 / 忽略（单条·整组）/ 解链」 | `scripts/e2e-link.sh` **17/17**（建档那步走真界面点）；`cargo test --workspace`：domain 27 · store 17 |
| 2026-10-11 | **M1** | `crates/sources`（GA/Emby/Komga 三源客户端 + 清单构建 + 扫描比对 + `scan_runs`）、`crates/worker`（jobs 队列 + `FOR UPDATE SKIP LOCKED` + 租约）、`store::library`（清单 upsert / 池子 / 扫描记账）、`store::{jobs,secrets}`、迁移 `0002_library_manifest`、API `GET /api/library/{pool,manifest,status,scan-runs}` + `POST /api/library/scan`、前端「媒体库池子」视图 | `scripts/e2e-library.sh` **15/15**；`cargo test --workspace` 全绿（domain 19 · sources 2 · store 9）；真库扫描 252+1523+113 条 |
| 2026-10-11 | 修复 | 修了三个**挡路的旧伤**（都不是 M1 引入，但让 M0 验收回不来）：① 皮肤上下文未 inject → 编辑器根本挂不上（`ctx.set` → `ctx.inject`）② `tick` 在 ProseMirror 事务里同步通知 React → React 19 无限重渲染（改 rAF 合并）③ 题记围栏尾随空行导致 `:::` 漏成正文、署名认不出 | `scripts/e2e-ui.sh` **31/31** · `scripts/layout-check.sh` **32/32** |
| 2026-10-11 | 仓库 | 删除旧 `.git` 并重新 init：排除外部参考仓库与运行时数据后做首次提交（376 文件 / ~5.3MB）；`origin` 指回 Gitea | `git log` 单一提交；工作树干净 |
| 2026-10-11 | 基准 | 确立两层产出基准（D3）：规则层 `skills/samples`（18 篇）、**产出层 `/root/Game_Wiki` 真产出（36 篇）**；写作流程镜像 `WRITING-BRIEF.md`（`prompts.md §2.3`）；纠正技能命名（保持现状） | `README.md §3 D3` · `skills.md §5` |
| 2026-10-11 | 文档 | 冻结全部决策（D1–D50，收敛所有"待定"）；产出全套 16 篇开发文档；从 v1 与 `nashsu/llm_wiki` 提炼行为知识清单；确认前端"展示照 v1、实现换 AntD/Milkdown"，补全首页与编辑器（`frontend.md §2.2 §5.5`）；建立本追踪文件 | `docs/` 全部文件；本文件 |
| 2026-10 | M0 | 已有：`domain`/`store`/`api` 三层 + 前端骨架 + 尖刺 A（人的编辑与存储） | `backend.md §11` |

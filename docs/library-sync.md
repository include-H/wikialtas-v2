# WikiAtlas v2 · 媒体库子系统

> **管什么**：把 GameAtlas / Emby / Komga 的收藏变成清单、池子、链，再把定稿反哺回库；库里条目消失时标记失效、支持重挂。
> **关联**：功能面见 `features.md §1–§4`；表结构见 `database.md §2`；API 见 `api.md §1.4`；行为证据见 `behavior-notes.md §8`。
> **两条硬边界**：① 扫描与建档**零 LLM**；② **AI 永不主动建库/挂链**（只建议）。

---

## 0. 一句话

```
三源（GA/Emby/Komga）
  → 清单（library_manifest，确定性）
  → 池子（库里有哪些条目 + 哪些还没有文档）
  → 人点一次：原子建【树节点】+【条目↔节点 的链】（library_links）
  → 写正文 → 定稿
  → 反哺（GameAtlas 正文+摘要 / Komga 摘要；Emby 不参与）
  → 库里条目没了：链 stale +「重新挂载」（文档永不动）
```

---

## 1. 三源客户端

| 源 | 认证 | 载体 | 方向 | 客户端要点 |
|---|---|---|---|---|
| **GameAtlas** | 管理员会话 cookie（登录表单换取） | game | 双向（读清单 + 反哺正文） | 读系列成员；写 `PUT /api/games/{id}/wiki` |
| **Emby** | `X-Emby-Token` | movie / tv | **只读** | 需要给每个 library 标角色（正片 / 混杂）；读 `item_id` |
| **Komga** | `X-API-Key` | manga / book | 双向（读 series + 反哺摘要） | 卷判型 / 抽屉系列拆单册；写 `PATCH .../metadata` |

- 地址与密钥存在 **`secrets` 表**（独立表，写入不回显），配置入口在设置页。
- **库 vs 系列**：Emby 的 library 是"库"（拿库名建节点毫无意义）；GA/Komga 的 series 是**作品系列**（可当父节点）。
- **绝不联网抓正文**：正文只由 wiki 侧写；`library_manifest` 只存身份与元数据。

---

## 2. 清单模型与落位规则

### 2.1 清单字段（`library_manifest`）

身份（`source + external_id`）· 标题 / 副标题 / 别名 · 封面 · 年份 / 发行日 · 容器（库 / 系列）· `kind`（tv|movie|game|book）· `format`（comic|novel）· 条目 URL · extra（GA 系列成员等）。

### 2.2 身份 = 关联的锚（**核心规则**）

- 关联靠 **`source + external_id`**（GA `public_id` / Emby item id / Komga series id），**不靠标题**。标题会变、会重名、会分版本。
- `UNIQUE (source, external_id)`：**一个库条目只能挂一篇文档**。

### 2.3 落位规则（一个条目 = 一篇文档）

- 容器是**作品系列**（GA series / Komga series）→ 文档建在该系列节点下（系列不存在则建）；
- 容器是**库**（Emby library）→ 文档建在顶级（或用户指定的父节点）；
- 子标题默认 = 条目名；**只有与父节点同名、或与已有兄弟撞名时，才加介质后缀**（`《X》→《X（剧集）》`）；
- **原子性**：节点与链在**同一事务**里创建；失败不留孤儿节点。

### 2.4 链的属性（`library_links`）

| 字段 | 取值 | 说明 |
|---|---|---|
| `role` | `work`（正片/本体）｜`mix`（混杂：OST / 预告 / 解析） | 一个节点在一个源上只有一个 `work` 位 |
| `state` | `active`｜`stale`｜`replaced` | **不删链** |
| `missing_since` | timestamptz | 何时起在清单里找不到 |
| `replaced_by` | link id | 重挂后被谁取代 |
| `linked_by` | `manual`｜`pool`｜`archive`｜`scan` | 谁建的，可追溯 |

**部分唯一索引**：`(node_id, source) WHERE role='work' AND state<>'replaced'`。

> ⚠️ **实测要点**：条件里 `state <> 'replaced'` 意味着**失效链（stale）没让位之前不能插第二条 work 链**——重挂必须**先显式标 `replaced`（带 `replaced_by`）**。这是**有意为之**：逼出一次明确决定，而不是让两条链悄悄并存。（已在数据库冒烟测试里固化成反例。）

---

## 3. 扫描与比对

- **周期可关** + 手动扫描（`POST /api/library/scan` 入队 `scan` job）。
- **确定性、零 LLM**：拉清单 → 与上次比 → 落 `scan_runs` + 更新 `library_manifest`。
- **增量缓存**（借 llm_wiki `ingest-cache`）：源条目内容哈希未变则跳过。
- **与链比对**产出三事件（落 `link.*` 事件）：

| 事件 | 条件 | 结果 |
|---|---|---|
| **新增** | 清单里有、链上无 | 池子里出现（"可以建档"） |
| **消失** | 链上有、清单里无 | 链 `stale` + 记 `missing_since`；（再次出现 → **自动回 `active`**） |
| **变更** | 标题/年份/容器等元数据变了 | 体检项：文档里的信息要不要跟着改 |

---

## 4. 三种「不要」（**别混**）

| 语义 | 存哪 | 含义 | 可恢复 |
|---|---|---|---|
| **池子全局忽略** | `library_ignores`(scope=entry) | "这个条目本身没价值" | 可恢复 |
| **整组忽略** | `library_ignores`(scope=container) | "这一整组我都不想要文档" | 可恢复 |
| **节点 per-node dismiss** | `library_ignores`(scope=node) | "别向**这个节点**提起这个条目" | 可恢复，可附理由 |
| **建议拒绝** | KV / `suggestions`（Rejected） | "模型给的挂法不对"——**条目仍是一等公民** | 理由进 KV，下次「AI 对一遍」作为 `previousRejections` 喂回模型二次判断；模型复读被拒挂法时行不藏、挂「上次被拒」标注 |

> **API 形状纪律**：空列表一律 `[]` 不给 `null`；KV 多字段共存时别写"空列表=没有记录"的守卫（会吞掉同记录其他字段）。

---

## 5. 反哺（WikiAtlas 为真源）

### 5.1 能力矩阵

| 目标 | 通道 | 能写什么 | 方向 |
|---|---|---|---|
| **GameAtlas** | `PUT /api/games/{id}/wiki` | 正文 + 摘要 | 双向（我们写回去） |
| **Komga** | `PATCH .../metadata` | 条目 metadata 摘要 | 双向（我们写摘要回去） |
| **Emby** | —（只读） | 不可写 | 单向（只读它） |

### 5.2 时机与语义

- **定稿时自动推送**（定稿是唯一"内容变了"的确定性事件）+ 手动「重新推送」兜底。
- **语义**：这几个字段由 **WikiAtlas 独占写**，以**最新定稿**覆盖；**不合并、不做双向 diff**。
- **幂等**：同 `synced_ver` 不重复推（避免重复成本）。

### 5.3 同步状态（`sync_targets`，每条链 × 每个目标一行）

`synced_ver`（推的是哪一版）· `status`（`pending｜ok｜failed`）· `message`（失败原因）· `synced_at`。
→ 失败可见、可重试，且能回答"库那边是不是这篇的最新版"。

### 5.4 Komga 摘要适配（规则冻结）

取正文**首个非说明非题记段落**，去 markdown 标记，**截断上限 500 字**、超出加 `…`。
> 实现期以 Komga 实测字段约束为准：若更严则取更短。**规则在此冻结，实现不得自由发挥。**

---

## 6. 失效与重新挂载

```
扫描后发现链上的 source+external_id 不在新清单里
  → 链 state = stale，记 missing_since（不删链、不删文档）
  → 该文档显示「来源已失效，请重新挂载」+ 动作
  → 再次出现 ⇒ 自动恢复 active（库临时离线又回来，不该让人点两遍）
```

**重新挂载的两种方式**（**零 LLM**）：

1. **宿主建议**：用归一化标题（+ 别名 + trigram 模糊）在清单里找候选，列出"这条像不像原来那部"；
2. **手动挑**：直接从清单里选一条。

**重挂语义**：新链 `active`，**旧链标 `replaced`（带 `replaced_by`，保留可追溯）**。
**文档不动**：正文、版本、关系、资料全部保留。**失效只影响链，不影响文档。**

---

## 7. AI 建议（零 LLM 扫描之上的"对一遍"）

- "AI 对一遍"是**可选**动作（默认关），产出**建议**（KV / `suggestions`），**人点确认才执行**。
- **判定口径进数据**（`judge_cache`）：`fingerprint = hash(prompt_version, model, input_signature)`；改 prompt / 换模型 → 指纹变 → **自动重判**（取代 v1 靠人记得给 `komgaJudgeVersion` +1）。
- 机械判定（媒体库层级 / 题名变体 / 相关性判官）用**无思考档**直出（`behavior-notes.md §7`）。
- 判定失败**退回启发式，不允许挡扫描**。

---

## 8. 时序（定稿 → 反哺）

```
用户点「定稿」→ commit 事务（写 contents / 快照 / write_events）
  → 定稿事件 → 入队 sync job
     ├─ 对每条 active 链 × 每个可写目标：查 sync_targets
     ├─ 若 synced_ver == 当前 ver → 跳过（幂等）
     ├─ 否则写回（GA 正文+摘要 / Komga 摘要）
     └─ 更新 sync_targets(status/message/synced_at)
  → 前端收 sync.* 事件 → 状态条刷新
```

> **反哺永远在事务外、在 worker 里**（`backend.md §1`）。

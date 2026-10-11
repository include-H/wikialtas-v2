# WikiAtlas v2 · 工具面契约

> **工具是馆员的"手"。这份文档是工具清单、schema、能力面与护栏的唯一契约。**
> **关联**：AI 能力面见 `ai.md §6`；护栏的来龙去脉见 `behavior-notes.md §3 §4`；提示词分段见 `prompts.md §1`。
> **一句话纪律**：**工具怎么用 → 写进工具的 `description`，不写进提示词**（`behavior-notes.md §0`）。

---

## 0. 总则

```rust
pub enum ToolKind { Read, Write, Control }

pub trait Tool {
    fn name(&self) -> &'static str;
    fn schema(&self) -> serde_json::Value;   // 由 schemars 从入参类型生成，禁止手写
    fn kind(&self) -> ToolKind;
    async fn call(&self, ctx: &ToolCtx, args: serde_json::Value) -> Result<ToolOut>;
}
```

- **schema 从类型生成**（Rust `schemars`）：消灭"schema 与实现漂移"（v1 靠手写 + `schemas_test` 守，v2 更彻底）。
- **`description` 是"怎么用"的唯一 owner**：写清**何时用、何时不用、边界**。
- **工具名 / schema 一致性由测试守**：改了入参类型要能编译/测试失败，而不是运行时才发现。
- **新增工具前先问**："现有面是不是已覆盖它？"——与技能维护纪律同源，**别重复造**（v1 AGENTS）。

---

## 1. 工具清单

### 1.1 读工具（Read）

| 工具 | 作用 |
|---|---|
| `get_tree` | 取子树（含节点 kind/medium/status） |
| `list_works` | 列作品（可按 medium / 容器 / 状态过滤） |
| `read_node` | 读节点元数据 + 定稿正文（可只读大纲） |
| `read_doc` | 读资料正文 |
| `search_works` | 标题 / 别名 / 正文检索（bigram/trigram） |
| `list_library` | 列媒体库清单 / 池子 / 某条目的链与反哺状态 |
| `list_skills` | 列技能 L0 目录（name + description） |
| `read_skill` | 按需读技能正文（防路径穿越；大文件截断要说话） |
| `read_source` | 读工作区里的参考文件（如 `skills/samples/` 某节）；**必须写明只读哪一节** |
| `search_web` | 联网检索（**默认关**，有预算） |
| `fetch_url` | 读允许的搜索结果页 |
| `search_sessions` / `read_session` | 跨会话回忆；读到的内容一律当**不可信背景** |

### 1.2 写工具（Write；直接生效，进 revision / 写事件）

| 工具 | 作用 | 关键护栏 |
|---|---|---|
| `upsert_work` | 建/改节点（title/medium/parent/status/aliases/tags） | work 必带介质；doc 不带 |
| `write_content` | 整篇替换节点正文 | **空写拒绝 + 截断守卫**（`>200→<50`） |
| `patch_section` | 按 `##` 锚点局部替换 | 锚点必须唯一命中 |
| `patch_sections` | 批量章节替换 | **波次 + 预检 + all-or-nothing + 上限** |
| `edit` | 字面串局部替换（`oldString` 必须唯一匹配；题记/说明行等无 `##` 锚点处只此一路） | 唯一匹配，否则报错 |
| `upsert_relation` | 按冻结词典建/改关系 | type 必须在 `relation_types`；成环校验 |
| `upsert_relations` | 批量关系 | 波次 + all-or-nothing + 上限 |
| `detach_relation` | 解关系 | — |
| `attach_library_link` | 把库条目挂到节点 | 身份唯一 `(source,external_id)` |
| `detach_link` | 解链（**保留文档**） | — |
| `relink` | 重挂（旧链 `replaced`） | — |
| `create_doc` | 在节点下建资料 / 建综合页 | — |
| `archive_node` | 归档（默认删除策略） | 归档后所有写工具拒写 |
| `todo_write` | 整份提交任务清单 | **不受重复调用拦截**（本来就要反复整份提交） |

### 1.3 控制工具（Control）

| 工具 | 作用 |
|---|---|
| `answer` | 口头回答，可附跳转 `node_id`（**README §3 D19**：问答产物只能存成资料，不直接改正文） |
| `ask_user` | 结构化追问（尽量被技能声明的输入表单替代，见 `skills.md §4`） |
| `finish` | 收尾（把本次 run 标记完成） |

---

## 2. 关键工具契约（细则）

### 2.1 `write_content`

```
入参：{ node_id, content_md, summary? }
出参：{ ok, ver, wrote: true } | { ok:false, reason }
```
- **空写拒绝**：`content_md` 去空后为空 → 拒绝。
- **截断守卫**：若目标已有正文且 `旧长 > 200`、`新长 < 50` → 拒绝（防把整章写没了）。
- 落到**草稿**（`content_drafts`，节流）→ 过**质量门** → 才允许 `commit`。
- **归档节点拒写**。

### 2.2 `patch_section` / `patch_sections`

```
入参：{ node_id, anchor: "## 3. ...", new_markdown }   // 或 sections[]
```
- 锚点在当前正文里**必须唯一命中**，否则报错（别猜第几个）。
- **投影**：正文从 `function_call_arguments.delta` 增量抽出（`behavior-notes.md §5`）。

### 2.3 `edit`

- `old_string` 必须在正文里**唯一出现**；否则报错。
- 用途：改一句、修一处（**别为一句重写整章**——这条边界写进工具 description）。

### 2.4 链路工具（v1 缺，v2 必补）

| 工具 | 语义 |
|---|---|
| `detach_link` | 解链，**文档不动** |
| `relink` | 重挂到另一条目；旧链 `state=replaced`（带 `replaced_by`），新链 `active` |
| `rebuild_doc` | 从媒体库条目重建档（复用落位规则） |

> **为什么它们必须是工具**：v1 只能靠 API，导致"馆员有全权工具"其实有缺口（`backend.md §4`）。

### 2.5 综合页 / 系列主文

- `create_doc` 带 `doc_type`（角色 / 世界观 / 时间线 / 主题 / 创作者）时 → 建**综合页**（`kind='doc'`，跨作品，可挂关系）。
- 目标是 `series` / `universe` 节点时，走 `series-main` 技能（`skills.md §2`）。

---

## 3. 能力面纪律（**最重要的一条**）

> 来源：v1 AGENTS.md（`behavior-notes.md §4`）。

1. **工具面只按用户显式选择剥写工具**：
   - `docMode=read`（只读模式）→ 剥**所有**写工具；
   - `oneShot`（工具栏「翻译/解释」按钮的能力契约）→ 剥写工具；
   - **仅此两种**。意图（`answer` / `continue_wiki` …）**不决定能力面**。
2. **运行时二次校验**：被剥掉的写工具**即使被模型幻觉调用，也拒绝执行**（双层防护）。
3. **提示词段自动跟随工具面**：`text` 是函数，**工具不在场就返回空串**（`answer` 意图读不到写工具的规矩）。
4. **别按猜测锁工具**：猜错了（错字 / 纠偏 / 问句暗示干活）也要能干。护栏放在**动作层**（修订可回滚 / 写工具自带空写、残稿拒绝）与**提示**。

---

## 4. 批量工具契约（波次模式）

**所有批量工具一律**：

1. **预检全在前**：先校验全部入参（锚点存在性、唯一性、关系合法性、目标可写），**任一不过就整体拒绝**；
2. **all-or-nothing**：一个事务里全成或全不成；
3. **带上限**：如批量建档单次 ≤ 50 部、`limit` ≤ 200；
4. **波次**：大任务分波提交，每波可独立验收。

范本：`patch_sections` / `upsert_relations`。

---

## 5. 护栏汇总（工具侧，逐条对照 `behavior-notes.md §3`）

| 护栏 | 落点 |
|---|---|
| 空写拒绝 | `write_content` |
| 截断守卫 `>200→<50` | `write_content` |
| 归档拒写 | 所有写工具 |
| 参数 JSON 错 → 报错不猜 | 工具调用入口 |
| 重复调用分级（>3 次 `ok:false`，`todo_write` 除外） | 工具调度器 |
| 联网预算 | `search_web` / `fetch_url` |
| 截断必须说话 | 所有会截断的工具（含 `read_skill` / `read_source`） |
| 破坏性写只许自有可回滚 | `archive_node` / `detach_*` / `edit` |
| 路径穿越防护 | `read_skill` / `read_source` |

---

## 6. 与 v1 工具面的对照

| v1 | v2 |
|---|---|
| `search_works` / `read_work` / `read_doc` / `get_tree` / `read_skill` / `search_web` / `fetch_url` / `list_library` | 读面基本保留（`read_work` → `read_node`） |
| `upsert_work` / `write_content` / `patch_section` / `edit` / `upsert_relation` / `attach_library_link` / `create_doc` / `sync_library` | 写面保留 |
| `answer` | 保留（+ `ask_user` / `finish`） |
| ——（v1 缺） | **新增** `detach_link` / `relink` / `rebuild_doc` / `detach_relation` / `list_skills` / `archive_node` / `patch_sections` / `upsert_relations` |
| 工具名/描述散落 | schema 由 `schemars` 从类型生成 + 一致性测试守 |

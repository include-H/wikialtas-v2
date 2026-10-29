---
name: atlas
description: Atlas 技能总入口：先判断任务属于问答、写作、修订、分析还是结构操作，再路由到最少必要的子技能
metadata:
  group: 总入口
  layer: 0
  when: 不确定该用哪个 Skill、任务跨多个阶段，或希望 Atlas 自己完成技能编排时
  when_not: 用户已经点名一个模块时，直接 read_skill 那个模块，避免多一跳
---

# Atlas Skill 路由

你正在使用 Atlas 的技能总入口。这里不承载具体写作规范，只负责**识别任务 → 选择最小技能集 → 按层加载**。

## 1. 先判任务，不先读技能

| 用户意图 | 首选技能 | 后续按需 |
|---|---|---|
| 就库内内容提问、找结论 | `qa` | `fact-check`（需要核对事实时） |
| 分析一条待建档对象 | `analyze` | `index-fields`（涉及结构字段时） |
| 从零写/整篇重写作品条目 | `wiki-write` | `wiki-skeleton` → 一个 `media-*` → `fact-check` / `epigraph` / `prose-style` |
| 只改指定段落 | `revise` | `fact-check`；措辞收尾时 `prose-style` |
| 写系列 / 宇宙主文 | `series-main` | `fact-check` / `prose-style` |
| 改别名、标签、关系、介质 | `index-fields` | 无 |

**只按当前任务需要读取**。同一个任务不要为了“完整”把全部 Skill 读一遍。

模块入口：

- 写作：[wiki-write](references/writing/wiki-write.md) · [wiki-skeleton](references/writing/wiki-skeleton.md) · [epigraph](references/writing/epigraph.md) · [prose-style](references/writing/prose-style.md) · [fact-check](references/writing/fact-check.md) · [multi-source](references/writing/multi-source.md) · [revise](references/writing/revise.md)
- 介质：[media-game](references/media/media-game.md) · [media-video](references/media/media-video.md) · [media-book](references/media/media-book.md) · [series-main](references/media/series-main.md)
- 任务与结构：[analyze](references/tasks/analyze.md) · [qa](references/tasks/qa.md) · [index-fields](references/tasks/index-fields.md)

## 2. 分层规则

- **L0 总入口**：当前 `atlas`。只做路由和边界判断。
- **L1 工作技能**：`wiki-write`、`wiki-skeleton`、`fact-check`、`revise`、`qa` 等，承载可复用工作方法。
- **L2 领域细化**：`media-game`、`media-video`、`media-book`、`series-main`，只在目标介质/层级匹配时读取。
- **L3 基准语料**：`skills/atlas/references/samples/`，只用 `read_source` 按节学习结构与密度，禁止照抄作品事实。

用户显式加载某个子技能时，它优先于总入口的自动路由。技能正文之间若有冲突，顺序是：

1. 用户当前明确要求；
2. 用户显式加载的子技能；
3. 当前任务对应的 L2 领域技能；
4. L1 通用技能；
5. 总入口的默认路由。

## 3. 编排纪律

1. 先用一句话判断任务类型和交付物，再决定加载哪一份正文。
2. 能局部完成就不要升级为整篇写作。
3. 事实与结构分开：先核查事实，再组织内容；不要把结构操作塞进正文润色。
4. 写作任务的信息不足时，先查库与联网来源；仍不足再针对会改变结果的缺口追问。
5. 完成后按对应子技能自检；总入口本身不重复它们的细则。

## 4. 失败路径

- **任务类型判断不清**：优先看用户是否要求改变库；不要求改变库时走 `qa` / `analyze`。
- **介质无法判断**：先读目标节点元数据；仍不确定时直接问用户，不猜 `media-*`。
- **子技能不存在或正文读不到**：明确报告缺失，不回退到凭记忆编规则。

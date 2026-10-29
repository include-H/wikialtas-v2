# Atlas v2 · 项目交接摘要

> 给第一次接触本项目的开发者 / AI。只写仓库里确实存在的信息；不确定的地方会标注。

## 1. 项目一句话介绍

Atlas v2 是中文 ACG Wiki 库：把三源（GameAtlas / Emby / Komga）条目变成可写、可检索、可反哺的文档，AI 馆员 Atlas 协助查资料与写正文。

## 2. 技术栈

- 前端：React + TypeScript + Vite + Ant Design + Milkdown。
- 后端：Rust（axum / sqlx / tokio），crates 分为 `api` / `worker` / `agent` / `store` / `domain` / `sources` / `mcp`。
- 数据库：PostgreSQL（`pg_trgm`）。
- 模型：OpenAI Responses 兼容接口；地址、模型、effort 存 `secrets` 表，不写入代码。
- 运行：Linux 本地；api `127.0.0.1:8088`、前端 `5173`、worker 独立进程。
- 验收：真实 PG 隔离库 + `agent-browser`；免费链路默认用 mock 模型。

## 3. 目录结构

| 路径 | 作用 |
|---|---|
| `crates/agent/` | 单循环 harness、工具面、三段写作、质量 / 引用门 |
| `crates/api/` | HTTP + SSE 路由，只做编排 |
| `crates/worker/` | 消费 jobs（run / scan / sync / audit） |
| `crates/store/` | PG 仓储与迁移 |
| `crates/domain/` | 零 IO 领域模型与校验 |
| `crates/sources/` | GameAtlas / Emby / Komga 客户端与扫描 |
| `crates/mcp/` | 只读 MCP stdio server |
| `frontend/src/` | `panels/` 页面、`editor/` Milkdown 编辑器、`api.ts` |
| `skills/` | 写作技能与 18 篇基准样本 |
| `docs/` | 冻结规范；续工入口 `TRACKING.md` |
| `scripts/` | e2e、编辑器 / 布局 / Markdown 检查、mock 模型 / 媒体库 |

## 4. 核心功能与数据流

UI 建 run 入 `jobs`；worker 跑通用 Atlas 循环，正文只落草稿，事件写 `run_events` 并经 SSE 回前端。定稿是一个事务：版本 +1、快照、写入事实、清草稿，并同时入队反哺。媒体库扫描零 LLM：`library_manifest` → 池子 → 人点建档生成节点 + `library_links`；定稿后推送 GameAtlas / Komga，条目消失则链 `stale` 并可重挂。检索用 `pg_trgm` + bigram，MCP 只读。

## 5. 当前进度

- **已完成**：M0–M9（树 / 内容、媒体库进料与建档、通用 harness、三段写作 + 三闸、关系 / 综合、Review / 体检、反哺 / 重挂、检索 / 问答、MCP / 设置 / 用量 / 导出导入）。
- **P1 已完成**：取消运行（排队即取消、运行中协作收尾）、节点改名 / 别名 / 标签 / 移动、取消归档 +「已归档」入口。
- **半成品**：块级 diff 动画（仅末块脉动）、`resume`、版本 `revert` API、结构写审批。
- **未开始**：真实媒体库写回验收、pgvector、Komga「AI 对一遍」。

## 6. 已知问题与卡点

- R14：真实 GameAtlas / Komga 写回未验证，需显式批准，避免改用户库。
- R9：研究 / 骨架仍共用主模型与 effort，成本未优化。
- 无 `POST /runs/{id}/resume`；版本回滚只有 UI「恢复为草稿」，没有 `revert` API。
- 结构写只有 allow / deny，没有 per-call `ask` 审批。
- 改 prompt / 技能后必须重跑付费真模型回归（R15）；最近一轮未跑。
- 文档 R1–R3 未回填；`docs/api.md` 部分状态已更新，其余表可能滞后。
- 本地还有两张截图 `1.png` / `2.png` 未跟踪（未入库）。

## 7. 下一步计划

1. 发布前验收：经批准写回一条真实媒体库测试文档，并做 `/root/Game_Wiki` 产出档评测。
2. P2：研究 / 骨架便宜档、搜索预算调参、结构写审批。
3. 可选：pgvector、Komga「AI 对一遍」、块级 diff 动画。

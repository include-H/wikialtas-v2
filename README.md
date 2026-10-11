# WikiAtlas v2

**WikiAtlas = 你的媒体库书架 + 一位有全权工具、但没有手的 LLM 馆员。**
它是 GameAtlas / Emby / Komga 的**文档层**：库里一个条目对应一篇 wiki 文档；WikiAtlas 是文档真源，定稿反哺回库。

> **续工入口：新对话先加载 [`docs/TRACKING.md`](docs/TRACKING.md)**——它记录当前状态、里程碑看板与下一步，并按 `docs/README.md`（总纲 / **决策冻结表** / 路线图）继续开发，不再需要逐条拍板。

v1 已整棵归档到 [`archive/v1/`](archive/v1/)（代码、脚本、技能、旧文档）。v1 只作参考，不再改动；**它的真实数据库 `archive/v1/backend/data/wikiatlas.db` 留在本机、未入库（D2：不迁 v1 数据）**；它最值钱的"行为知识"已摘进 [`docs/behavior-notes.md`](docs/behavior-notes.md)。

## 已定基线

| 维度 | 决定 |
|---|---|
| 功能基准 | [`nashsu/llm_wiki`](https://github.com/nashsu/llm_wiki)（Karpathy 的 llm-wiki 模式：**持续维护一份 wiki**，而不是每次现挖 RAG） |
| 布局蓝本 | 飞书云文档三面板（左：知识库树 / 中：文档 / 右：AI 助手）—— 这是**布局**，不是视觉母本 |
| 前端 | React 19 + Vite + TS(strict) + **Ant Design v6** + **@ant-design/x** + TanStack Query + Zustand + Milkdown |
| 后端 | **Rust**（axum + sqlx + tokio；进程形态：`api` + `worker`） |
| 数据库 | **PostgreSQL 18**（`pg_trgm` 必须；`pgvector` 后置） |
| 事件协议 | OpenAI **Responses** 形状（`response.*`）+ 宿主旁路（`wikiatlas.*`） |

## 文档地图（全在 `docs/`）

- **续工入口（先读）**：`docs/TRACKING.md` · **总纲 / 决策 / 路线图**：`docs/README.md`
- 功能：`docs/features.md` · v1→v2 取舍：`docs/v2-from-v1.md` · 行为知识：`docs/behavior-notes.md`
- AI：`docs/ai.md` · 工具面：`docs/tools.md` · 提示词/质量门：`docs/prompts.md` · 技能：`docs/skills.md` · 评测：`docs/eval-cases.md`
- 后端：`docs/backend.md` · 数据库：`docs/database.md` · 接口/事件：`docs/api.md` · 媒体库：`docs/library-sync.md`
- 前端：`docs/frontend.md` · v1 前端迁移：`docs/frontend-migration.md`
- 测试：`docs/testing.md`

## 现状（M0 已完成）

**代码骨架已立**：`crates/{domain,store,api}`（Rust，含单测 + `sqlx::test` 集成测）、`frontend/`（AntD X 三面板骨架 + Milkdown 编辑器）；**尖刺 A「人的编辑与存储」已跑通**。

**下一步 = `docs/README.md §4` 的 M1（媒体库进料）**，一路到 M9。所有里程碑的产出、依据、验收都在那张表里。

```bash
# 后端（需 PostgreSQL 18，连接串在 .secrets/wikiatlas.pg.env）
set -a; . .secrets/wikiatlas.pg.env; set +a; export DATABASE_URL="$WIKIATLAS_DB_URL"
cargo run -p wikiatlas-api          # 127.0.0.1:8088，启动自动跑迁移
cargo test --workspace              # 单测 + 集成

# 前端
cd frontend && npm run dev          # 5173，/api 代理到 8088
```

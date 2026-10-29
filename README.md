# Atlas v2

**Atlas = 你的媒体库书架 + 一位有全权工具、但没有手的 LLM 馆员。**
它是 GameAtlas / Emby / Komga 的**文档层**：库里一个条目对应一篇 wiki 文档；Atlas 是文档真源，定稿反哺回库。

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

## 现状（M0–M9 已完成）

V2 已完成路线图里的全部里程碑：媒体库扫描/建档、通用 Atlas harness、三段式写作、关系与综合页、Review/体检、反哺与重挂、全文检索/存资料、MCP 与运维。

**验收方式**：后端单测 + `sqlx::test` 集成测 + 前端构建 + 隔离临时库/临时端口的端到端真跑。媒体库写回没有擅自写你的真实 GameAtlas/Komga；使用本地真 HTTP 仿真验证协议与状态机，真实库确认需一次显式批准。

```bash
# 后端（需 PostgreSQL 18，连接串在 .secrets/wikiatlas.pg.env）
set -a; . .secrets/wikiatlas.pg.env; set +a; export DATABASE_URL="$WIKIATLAS_DB_URL"
cargo run -p wikiatlas-api          # 127.0.0.1:8088，启动自动跑迁移
cargo test --workspace              # 单测 + 集成
WIKIATLAS_DB_URL="$WIKIATLAS_DB_URL" cargo run -p wikiatlas-mcp  # stdio MCP（只读）

# 前端
cd frontend && npm run dev          # 5173，/api 代理到 8088
```

隔离验收脚本（不碰现有库）：

```bash
bash scripts/e2e-sync.sh        # M7 反哺/失效/重挂
bash scripts/e2e-relations.sh   # M5 关系/综合/索引
bash scripts/e2e-audit.sh       # M6 Review/体检
bash scripts/e2e-search.sh      # M8 检索/存资料
bash scripts/e2e-ops.sh         # M9 MCP/设置/用量/导出恢复
bash scripts/e2e-harness-tools.sh  # Harness 全工具/只读裁剪（mock 模型）
```

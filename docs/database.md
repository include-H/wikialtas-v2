# Atlas v2 · 数据库设计（PostgreSQL）

> 功能见 `docs/features.md`；后端见 `docs/backend.md`；接口见 `docs/api.md`；媒体库表见 `docs/library-sync.md`。决策冻结见 `README.md §3`。

## 0. 五条原则（每条都针对 v1 踩过的坑）

| # | 原则 | 针对 v1 的什么 |
|---|---|---|
| 1 | **PG 是唯一真相**（内存/文件/进程内标志都不算） | v1 的 `staging` 只存在于 SSE 事件里，崩了没痕迹 |
| 2 | **迁移有版本**（`sqlx migrate`，`migrations/NNNN_*.sql`） | v1 的 `schema_migrations` 只写死 version=1，真实状态是"列存不存在" |
| 3 | **一切多步写在一个事务里**；时间戳由 DB `now()` 出 | v1 的 `CreateDoc` 三次裸 Exec、`PatchWork` 七条独立 UPDATE、Go 手写 `updated_at` |
| 4 | **删除有策略**：默认归档，物理删只给人 | v1 到处硬删，且删节点会连带毁掉修订历史 |
| 5 | **领域状态落在模型上**（同步状态、口径版本），不藏在流程里 | v1 靠人记得把 `komgaJudgeVersion` 常量 +1；换模型不会重判 |

## 1. 表清单

| 组 | 表 |
|---|---|
| 结构 | `nodes` · `relations` |
| 内容 | `contents`（定稿）· `content_versions`（快照）· `content_drafts`（草稿）· `write_events`（写入事实） |
| 媒体库 | `library_manifest`（清单）· `library_ignores` · `library_links`（链，含状态）· `sync_targets`（反哺状态）· `suggestions` · `judge_cache` · `scan_runs` |
| 工单/对话 | `runs` · `run_events` · `sessions` · `session_messages` · `jobs`（队列） |
| 运维 | `app_settings` · `secrets` · `health_findings`（体检） |

## 2. 关键表与设计点（DDL 草图）

```sql
-- 一棵树：universe | series | work | doc（资料是 kind='doc' 的叶子）
CREATE TABLE nodes (
  id           uuid PRIMARY KEY,               -- UUIDv7，应用侧生成（与 v1 一致）
  parent_id    uuid REFERENCES nodes(id) ON DELETE RESTRICT,
  kind         text NOT NULL,                  -- universe|series|work|doc
  medium       text,                           -- game|movie|tv|manga|book|other（doc 为空）
  title        text NOT NULL,
  title_norm   text NOT NULL,                  -- 归一化（去空白/全半角/大小写）→ 匹配唯一口径
  aliases      text[] NOT NULL DEFAULT '{}',
  tags         text[] NOT NULL DEFAULT '{}',
  status       text NOT NULL DEFAULT 'draft',  -- draft|ready|archived
  visibility   text NOT NULL DEFAULT 'private',-- 与 status 分轴：写好了没 ≠ 给人看没
  sort_key     text NOT NULL,                  -- 可插值排序键，同级唯一（v1 的整数 sort_order 无法重排）
  created_at   timestamptz NOT NULL DEFAULT now(),
  updated_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX ON nodes (parent_id, sort_key);
CREATE INDEX ON nodes (title_norm);
CREATE INDEX ON nodes USING gin (aliases);

-- 关系边：有向，**类型与方向是契约**（见 §2.1），一处定义、工具描述从它生成
CREATE TABLE relations (
  id          uuid PRIMARY KEY,
  from_id     uuid NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  to_id       uuid NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  type        text NOT NULL,                   -- 见 §2.1
  created_by  text NOT NULL,                   -- human:<id> | llm:<run_id> | host
  created_at  timestamptz NOT NULL DEFAULT now(),
  UNIQUE (from_id, to_id, type),
  CHECK (from_id <> to_id)                     -- 禁止自指（成环由应用层在写路径上校验）
);
CREATE INDEX ON relations (to_id);

-- 定稿正文：一节点一行
CREATE TABLE contents (
  node_id    uuid PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
  body_md    text NOT NULL DEFAULT '',
  ver        int  NOT NULL DEFAULT 0,
  updated_by text NOT NULL,                    -- human:<id> | llm:<run_id> | host
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- 草稿：一行，覆盖写（不产生快照）；lease 保证"一个节点同时只有一个写者"
CREATE TABLE content_drafts (
  node_id     uuid PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
  body_md     text NOT NULL DEFAULT '',
  base_ver    int  NOT NULL,                   -- 基于哪一版定稿；提交时不符 → 冲突报错
  by          text NOT NULL,
  run_id      uuid REFERENCES runs(id),
  lease_until timestamptz NOT NULL,
  updated_at  timestamptz NOT NULL DEFAULT now()
);

-- 快照：**只存定稿**（草稿不产生快照，避免 O(n²) 存储）
CREATE TABLE content_versions (
  id uuid PRIMARY KEY,
  node_id uuid NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  ver int NOT NULL, body_md text NOT NULL,
  source text NOT NULL,                        -- llm|human|host|import
  run_id uuid REFERENCES runs(id),
  summary text NOT NULL DEFAULT '',
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (node_id, ver)
);

-- 写入事实（取代 v1 内存里的 wroteContent 标志：patch_sections 漏标导致质量门/扫库钩子被跳过）
CREATE TABLE write_events (
  id bigserial PRIMARY KEY,
  node_id uuid NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  ver int NOT NULL, source text NOT NULL, run_id uuid, at timestamptz NOT NULL DEFAULT now()
);

-- 链：条目 ↔ 节点。**文档是耐久物，链是可替换的指针**
CREATE TABLE library_links (
  id uuid PRIMARY KEY,
  node_id uuid NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  source text NOT NULL,                        -- gameatlas|emby|komga
  external_id text NOT NULL,                   -- GA public_id / Emby item id / Komga series id
  url text, title_hint text,
  role text NOT NULL DEFAULT 'work',           -- work=正片位 | mix=混杂位
  state text NOT NULL DEFAULT 'active',        -- active|stale|replaced  ★ 不删链
  missing_since timestamptz,                   -- 何时起在清单里找不到
  replaced_by uuid REFERENCES library_links(id),
  linked_by text NOT NULL DEFAULT 'manual',    -- manual|pool|archive|scan
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (source, external_id)                 -- 一个库条目只能挂一篇文档（核心规则）
);
CREATE UNIQUE INDEX ON library_links (node_id, source) WHERE role = 'work' AND state <> 'replaced';

-- 反哺状态：每条链 × 每个可写目标（Emby 只读，不进来）
CREATE TABLE sync_targets (
  link_id uuid NOT NULL REFERENCES library_links(id) ON DELETE CASCADE,
  target text NOT NULL,                        -- gameatlas|komga
  synced_ver int, status text NOT NULL DEFAULT 'pending', -- pending|ok|failed
  message text NOT NULL DEFAULT '', synced_at timestamptz,
  PRIMARY KEY (link_id, target)
);

-- 队列 + 租约（取代 v1 的进程内队列与"重启全标 interrupted"）
CREATE TABLE jobs (
  id bigserial PRIMARY KEY,
  kind text NOT NULL,                          -- run|scan|sync|audit
  payload jsonb NOT NULL DEFAULT '{}',
  state text NOT NULL DEFAULT 'pending',       -- pending|running|done|failed
  attempts int NOT NULL DEFAULT 0,
  run_after timestamptz NOT NULL DEFAULT now(),
  locked_by text, locked_until timestamptz,    -- 租约过期 → 自动回收
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX ON jobs (state, run_after);

-- 判断口径进数据：改 prompt / 换模型 → 指纹变 → 自动重判
CREATE TABLE judge_cache (
  scope text NOT NULL, key text NOT NULL,
  fingerprint text NOT NULL,                   -- hash(prompt_version, model, input_signature)
  prompt_version int NOT NULL, model text NOT NULL,
  verdict jsonb NOT NULL, judged_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (scope, key)
);
```

### 2.1 关系方向语义（**唯一定义处**）

方向是契约：**技能里不重复**（`index-fields` 只说"按工具契约填"），工具描述与 UI 渲染都从这张表来。

| `type` | 方向（`from` → `to`） |
|---|---|
| `adaptation_of` | 改编作 → 原作 |
| `sequel_of` | 续作 → 前作 |
| `spin_off_of` | 衍生作 → 来源作 |
| `remake_of` | 重制 / 重拍 → 原作 |
| `inspired_by` | 受启发作 → 启发来源 |
| `references` | 引用 / 致敬方 → 被引用对象 |

规则落点（**机械项交给代码，判断留给技能**）：

- **自指** → DB `CHECK (from_id <> to_id)`；
- **悬空**（指向不存在的对象）→ 外键；
- **成环** → 应用层在写路径上校验（跨 type 的环可能合法，不做 DB 约束）；
- **「不要用模糊的『相关』代替具体关系」「不要为了让关系图好看而猜关系」「别名只收指向本对象的名称」** → 判断类，在 `skills/atlas/references/tasks/index-fields.md` 里。

其余表沿用 v1 已验证的形状（**不重设计**）：`runs` / `run_events` / `sessions` / `session_messages`（append-only 的日志是 v1 设计得最好的一块，直接重用）；v1 的 `settings` 一坨 KV 拆成 `app_settings`（UI/行为配置，jsonb）· `secrets`（密钥，独立表）· `library_manifest`（清单进表）· `library_ignores`（忽略分单条/整组）。

## 3. 扩展、索引与检索

- **必须装**：`pg_trgm`（标题/别名的模糊匹配，用于"重挂候选"与搜索兜底）。
- **中文全文检索（已拍板，D36）**：**先做归一化精确匹配 + `pg_trgm` 模糊/trigram**；应用侧 **bigram** 兜底（llm_wiki 的做法：二元组 + 单字兜底 + 标题匹配加分）。**不上 `zhparser` / `pg_jieba` / `pg_bigm`**——会被词典与安装环境绑死，且一个条目一篇文档的规模用不上。分词方案若将来确有需要，作为**新迁移**再加。
- **后置但先装**：`pgvector`（语义检索；表结构留 `embeddings(node_id, chunk_id, vec, model, ver)`，功能后置）。
- `contents` 上可加 `tsvector` 生成列 + GIN（决定分词方案后再加）。

## 4. v1 数据：**不导入**（2026-10 决定）

原先拟过一份「v1 → v2 逐表映射」和一套「迁移验收硬门槛」（逐表行数一致 · 正文 SHA256 抽样 · 孤儿出报告 · 幂等重跑）。**用户决定不搬 v1 数据**，这部分随之取消——v2 从空库开始，只保留 v1 的**设计教训**（已落在 §0 的五条原则与 §2 的注释里）。

v1 全量留档在 `archive/v1/`（含 `backend/data/wikiatlas.db` 真库与完整前后端源码），只作**查阅**用；将来若改主意要导，映射口径能从 git 历史里那份文档捞回来。

---

## 6. 运维

- **本机（当前开发环境）：Ubuntu 26.04 原生安装 PostgreSQL 18.6**（该机没有 Docker）· systemd 管集群 `18/main`（`127.0.0.1:5432`）· 库与角色都是 `wikiatlas` · 连接串在 `.secrets/wikiatlas.pg.env`（gitignored）。
- `docker compose`（`postgres` + `api` + `worker`）：v1 是一坨自建 dev 脚本；**本机用原生 PG**，compose 留给以后「一键起完整栈」时再说。
- 备份：`pg_dump` 定时 + 保留策略；**迁移前后各手动备份一次**。
- 时区统一 UTC（`timestamptz`）；连接池按进程分别配置。

## 7. 已落地（2026-10 · P1）

**环境**

| 项 | 值 |
|---|---|
| 主机 | Ubuntu 26.04 LTS（**无 Docker** → 原生安装） |
| PostgreSQL | **18.6**（`apt install postgresql postgresql-contrib`） |
| 集群 | `18/main` · `127.0.0.1:5432` · systemd 托管（`systemctl status postgresql`） |
| 角色 / 库 | `wikiatlas` / `wikiatlas`（UTF8，默认 collation `C.UTF-8`；ICU 的 `zh-*` 排序规则可用，需要时按列指定） |
| 连接串 | `.secrets/wikiatlas.pg.env` → `WIKIATLAS_DB_URL=postgres://wikiatlas:…@127.0.0.1:5432/wikiatlas`（`.gitignore` 已挡） |
| 扩展 | `pg_trgm` ✅（**pgvector 后置**，`embeddings` 表还没建） |

**迁移文件**（**已由 `crates/store` 用 `sqlx::migrate!` 跑**，库启动时自动应用；`_sqlx_migrations` 记版本）

- `crates/store/migrations/0001_init.sql` —— **23 张表**（§1 全部 + `relation_types` 契约表 + `doc_refs`）。
- `crates/store/tests/0001_init.smoke.sql` —— 约束冒烟：**3 条正例 + 17 条反例**（放在 `tests/` 而不是 `migrations/`，免得 sqlx 把非约定文件名当迁移），每条反例还校验**报错类型**（23514 CHECK / 23505 唯一 / 23503 外键），防止「因为别的原因失败」被当成通过。整体一个事务 + 末尾 `ROLLBACK`，不留数据：

  ```bash
  psql -h 127.0.0.1 -U wikiatlas -d wikiatlas -v ON_ERROR_STOP=1 -f crates/store/migrations/0001_init.smoke.sql
  ```

**落地清点**：47 个索引 · 29 个 CHECK · 18 个外键。

**照草图原样落的地方**：§2 的表名、列名、索引、`CHECK (from_id <> to_id)`、`UNIQUE (source, external_id)`、`library_links` 的部分唯一索引。

**与草图不同、以及为什么**

| 改动 | 原因 |
|---|---|
| `run_id` 用 **text**（草图写 `uuid`） | v1 的 `runs.id` 是 text（`sessions.id` 更是 `work:<uuid>` 这类自由串）→ 外键类型必须一致，导入时也不用解析 |
| 关系方向从「§2.1 的 markdown 表」变成 **`relation_types` 表 + `relations.type` 外键** | §2.1 自己写着「工具描述与 UI 渲染都**从这张表**来」——那就该是真表：方向语义改一处、全体跟着变 |
| `nodes` 同级唯一索引加 **`NULLS NOT DISTINCT`** | 草图说「同级唯一」，但 `parent_id` 为 NULL 的根节点在 PG 里默认互不相等 → 不加就等于根之间可以随便撞 |
| `nodes` 加 `CHECK (kind<>'doc' OR medium IS NULL)` 与 `(kind<>'work' OR medium IS NOT NULL)` | doc 不带介质、work 必带介质——把落位规则里最容易写错的一档交给 DB |
| 闭集枚举加 CHECK（kind / medium / status / visibility / role / state / linked_by / sync target / jobs.kind·state / severity / suggestions.kind / source） | 落实 §2.1「机械项交给代码」；**新增取值必须走迁移**，这道摩擦是有意的 |
| v1 沿用表的类型升级：`checkpoint` · `tool_cache` · `result_json` · `error_json` → **jsonb**；时间戳 TEXT → **timestamptz** | 形状不变，只换类型（v1 是 SQLite，没有原生类型） |
| `session_messages.session_id` **不加外键** | 与 v1 一致；孤儿按 §5 要**出报告**——加了外键会把报告变成硬失败 |
| `library_manifest` / `library_ignores` / `suggestions` / `scan_runs` / `health_findings` 的形状**按 §1 新定** | v1 里它们是 `settings` 的一坨 KV，没有形状可搬；将来大概率要改，改动永远新增迁移 |

**冒烟测试当场抓到的两处**（都是「以为对、其实错」，留档）：

1. **「正片位」包含 `stale`**：`library_links_work_uniq` 的条件是 `state <> 'replaced'` → 失效链**没让位之前不能插第二条 work 链**，重挂必须**先显式标 `replaced`（带 `replaced_by`）**。这是有意为之（逼出一次明确决定，而不是让两条链悄悄并存），已固化成反例。
2. **根节点的同级唯一必须靠 `NULLS NOT DISTINCT`**——否则根之间可以随便撞 `sort_key`。

**下一步**：① v1 数据导入（`archive/v1/backend/data/wikiatlas.db` → 按 §4 映射、按 §5 硬门槛出报告）；② Rust scaffold（`crates/store` 接 `sqlx::migrate!`，冒烟测试搬成 `sqlx::test`）；③ `pgvector` + `embeddings`（等语义检索有真实需求）。

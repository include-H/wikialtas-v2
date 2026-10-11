-- WikiAtlas v2 · 0001 初始 schema
--
-- 依据：docs/database.md §1（表清单）与 §2（DDL 草图）。
-- 沿用：v1 已有的形状（runs / run_events / sessions / session_messages）照 v1 **原样**搬，不重设计（docs/database.md §4）。
--
-- 约定
--   · 时间戳一律 timestamptz（UTC）；「一切多步写在一个事务里」由 store 层保证。
--   · 业务 id 由应用侧生成（UUIDv7）；**runs / sessions 的 id 沿用 v1 的 text**
--     （v1 的 sessions.id 是 `work:<uuid>` / `batch:<uuid>` 这类自由字符串，塞 uuid 会丢语义）。
--   · 空值语义：写「不知道」用 SQL NULL；「有意的空」用空串 / 空数组。
--
-- 迁移纪律：本文件为 0001，之后 0002…（sqlx migrate 按序执行）。**已应用的迁移不许改**，改动永远新增一版。

-- pg_trgm：标题 / 别名的模糊匹配（重挂候选、搜索兜底）。trusted extension，库 owner 可建。
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- ============================================================ 契约表

-- 关系类型与方向：**唯一定义处**（docs/database.md §2.1）。
-- 技能里不重复方向规则（index-fields 只说「按工具契约填」），工具描述与 UI 渲染都读这张表。
CREATE TABLE relation_types (
  type       text PRIMARY KEY,
  from_label text NOT NULL,           -- 例：续作
  to_label   text NOT NULL,           -- 例：前作
  note       text NOT NULL DEFAULT ''
);

INSERT INTO relation_types (type, from_label, to_label, note) VALUES
  ('adaptation_of', '改编作',      '原作',       '由原作改编而来'),
  ('sequel_of',     '续作',        '前作',       '故事上接续'),
  ('spin_off_of',   '衍生作',      '来源作',     '由来源作衍生'),
  ('remake_of',     '重制 / 重拍', '原作',       '同一作品的重做'),
  ('inspired_by',   '受启发作',    '启发来源',   '受其启发'),
  ('references',    '引用 / 致敬方', '被引用对象', '明确引用或致敬');

-- ============================================================ 结构：一棵树

CREATE TABLE nodes (
  id         uuid PRIMARY KEY,
  parent_id  uuid REFERENCES nodes(id) ON DELETE RESTRICT,
  kind       text NOT NULL CHECK (kind IN ('universe','series','work','doc')),
  medium     text CHECK (medium IN ('game','movie','tv','manga','book','other')),
  title      text NOT NULL,
  title_norm text NOT NULL,                                  -- 归一化（去空白/全半角/大小写）→ 匹配唯一口径
  aliases    text[] NOT NULL DEFAULT '{}',
  tags       text[] NOT NULL DEFAULT '{}',
  status     text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','ready','archived')),
  visibility text NOT NULL DEFAULT 'private' CHECK (visibility IN ('private','public')),
  sort_key   text NOT NULL,                                  -- 可插值排序键（v1 的整数 sort_order 无法重排）
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  CHECK (kind <> 'doc' OR medium IS NULL),                   -- 资料（doc）不带介质
  CHECK (kind <> 'work' OR medium IS NOT NULL)               -- 作品（work）必须带介质
);

CREATE INDEX nodes_parent_sort_idx ON nodes (parent_id, sort_key);
-- 同级唯一；NULLS NOT DISTINCT 让「根节点」也算同一级（否则根之间可撞 sort_key）
CREATE UNIQUE INDEX nodes_sibling_sort_uniq ON nodes (parent_id, sort_key) NULLS NOT DISTINCT;
CREATE INDEX nodes_title_norm_idx ON nodes (title_norm);
CREATE INDEX nodes_aliases_gin ON nodes USING gin (aliases);
CREATE INDEX nodes_title_trgm_idx ON nodes USING gin (title gin_trgm_ops);

-- 关系边：有向。type 的方向语义见 relation_types；
-- 自指由 CHECK 挡；悬空由外键挡；成环由应用层在写路径上校验（跨 type 的环可能合法）。
CREATE TABLE relations (
  id         uuid PRIMARY KEY,
  from_id    uuid NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  to_id      uuid NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  type       text NOT NULL REFERENCES relation_types(type),
  created_by text NOT NULL,                                  -- human:<id> | llm:<run_id> | host
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (from_id, to_id, type),
  CHECK (from_id <> to_id)
);
CREATE INDEX relations_to_idx ON relations (to_id);

-- 资料 ↔ 作品：v1 把引用藏在 docs.links_json 里、在 Go 里做祖先遍历（SQL 查不了）
CREATE TABLE doc_refs (
  doc_id     uuid NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  work_id    uuid NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (doc_id, work_id)
);
CREATE INDEX doc_refs_work_idx ON doc_refs (work_id);

-- ============================================================ 工单 / 对话（沿用 v1 形状）

CREATE TABLE runs (
  id           text PRIMARY KEY,
  workspace    text NOT NULL DEFAULT 'default',
  intent       text NOT NULL,
  goal         text NOT NULL,
  status       text NOT NULL,
  checkpoint   jsonb NOT NULL DEFAULT '{}',                  -- v1：TEXT 装 JSON
  tool_cache   jsonb NOT NULL DEFAULT '{}',
  result_json  jsonb,
  error_json   jsonb,
  model        text NOT NULL DEFAULT '',
  started_at   timestamptz NOT NULL,
  last_active  timestamptz NOT NULL,
  expires_at   timestamptz,
  completed_at timestamptz
);
CREATE INDEX runs_status_idx ON runs (status);

CREATE TABLE run_events (
  id         text PRIMARY KEY,
  run_id     text NOT NULL REFERENCES runs(id),
  seq        integer NOT NULL,
  type       text NOT NULL,
  payload    jsonb NOT NULL DEFAULT '{}',
  created_at timestamptz NOT NULL,
  UNIQUE (run_id, seq)
);

CREATE TABLE sessions (
  id         text PRIMARY KEY,                               -- `work:<uuid>` / `batch:<uuid>` / `~` 后缀 → 保持 text
  target     text NOT NULL DEFAULT '',
  title      text NOT NULL DEFAULT '',
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL
);
CREATE INDEX sessions_target_idx ON sessions (target, updated_at DESC);

-- append-only 的日志（v1 设计得最好的一块）。session_id 沿用 v1 **不加外键**：
-- 导入时孤儿要出报告，不静默丢（docs/database.md §5）。
CREATE TABLE session_messages (
  id              text PRIMARY KEY,
  session_id      text NOT NULL,
  seq             integer NOT NULL,
  replaces_seq    integer NOT NULL DEFAULT 0,
  run_id          text NOT NULL DEFAULT '',
  turn            integer NOT NULL DEFAULT 1,
  role            text NOT NULL,
  content         text NOT NULL DEFAULT '',
  tool_calls_json text NOT NULL DEFAULT '',
  tool_call_id    text NOT NULL DEFAULT '',
  tool_name       text NOT NULL DEFAULT '',
  header_hash     text NOT NULL DEFAULT '',
  header_reason   text NOT NULL DEFAULT '',
  shadowed_by     text NOT NULL DEFAULT '',
  created_at      timestamptz NOT NULL,
  UNIQUE (session_id, seq)
);

-- ============================================================ 内容：定稿 / 草稿 / 快照 / 写入事实

CREATE TABLE contents (
  node_id    uuid PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
  body_md    text NOT NULL DEFAULT '',
  ver        integer NOT NULL DEFAULT 0,
  updated_by text NOT NULL,                                  -- human:<id> | llm:<run_id> | host
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- 草稿：一行，覆盖写（不产生快照）；lease 保证「一个节点同时只有一个写者」
CREATE TABLE content_drafts (
  node_id     uuid PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
  body_md     text NOT NULL DEFAULT '',
  base_ver    integer NOT NULL CHECK (base_ver >= 0),        -- 基于哪一版定稿；提交时不符 → 冲突
  by          text NOT NULL,
  run_id      text REFERENCES runs(id),
  lease_until timestamptz NOT NULL,
  updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX content_drafts_lease_idx ON content_drafts (lease_until);

-- 快照：**只存定稿**（草稿不产生快照，避免 O(n²) 存储）
CREATE TABLE content_versions (
  id         uuid PRIMARY KEY,
  node_id    uuid NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  ver        integer NOT NULL,
  body_md    text NOT NULL,
  source     text NOT NULL CHECK (source IN ('llm','human','host','import')),
  run_id     text REFERENCES runs(id),
  summary    text NOT NULL DEFAULT '',
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (node_id, ver)
);

-- 写入事实：取代 v1 内存里的 wroteContent 标志（漏标会让质量门 / 扫库钩子被跳过）
CREATE TABLE write_events (
  id      bigserial PRIMARY KEY,
  node_id uuid NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  ver     integer NOT NULL,
  source  text NOT NULL CHECK (source IN ('llm','human','host','import')),
  run_id  text,
  at      timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX write_events_node_idx ON write_events (node_id, at DESC);

-- ============================================================ 媒体库

-- 清单：最近一次扫描看到的条目（链是「条目 ↔ 节点」，这里是「库里有什么」）
CREATE TABLE library_manifest (
  source        text NOT NULL CHECK (source IN ('gameatlas','emby','komga')),
  external_id   text NOT NULL,
  title         text NOT NULL,
  title_norm    text NOT NULL,
  year          integer,
  kind          text NOT NULL DEFAULT '',                    -- 库侧类型（如 movie / series / volume）
  raw           jsonb NOT NULL DEFAULT '{}',
  seen_at       timestamptz NOT NULL DEFAULT now(),
  missing_since timestamptz,                                 -- 何时起在清单里找不到（不删行）
  PRIMARY KEY (source, external_id)
);
CREATE INDEX library_manifest_title_norm_idx ON library_manifest (title_norm);
CREATE INDEX library_manifest_trgm_idx ON library_manifest USING gin (title gin_trgm_ops);

CREATE TABLE library_ignores (
  id          uuid PRIMARY KEY,
  source      text CHECK (source IN ('gameatlas','emby','komga')),
  external_id text,                                          -- 忽略单条
  group_key   text,                                          -- 或忽略整组（如某系列 / 某目录）
  reason      text NOT NULL DEFAULT '',
  created_by  text NOT NULL DEFAULT 'human',
  created_at  timestamptz NOT NULL DEFAULT now(),
  CHECK (external_id IS NOT NULL OR group_key IS NOT NULL)
);

-- 链：条目 ↔ 节点。**文档是耐久物，链是可替换的指针**
CREATE TABLE library_links (
  id            uuid PRIMARY KEY,
  node_id       uuid NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  source        text NOT NULL CHECK (source IN ('gameatlas','emby','komga')),
  external_id   text NOT NULL,                               -- GA public_id / Emby item id / Komga series id
  url           text,
  title_hint    text,
  role          text NOT NULL DEFAULT 'work' CHECK (role IN ('work','mix')),   -- work=正片位 | mix=混杂位
  state         text NOT NULL DEFAULT 'active' CHECK (state IN ('active','stale','replaced')),  -- ★ 不删链
  missing_since timestamptz,
  replaced_by   uuid REFERENCES library_links(id),
  linked_by     text NOT NULL DEFAULT 'manual' CHECK (linked_by IN ('manual','pool','archive','scan')),
  created_at    timestamptz NOT NULL DEFAULT now(),
  UNIQUE (source, external_id),                              -- 一个库条目只能挂一篇文档（核心规则）
  CHECK (state <> 'stale' OR missing_since IS NOT NULL)      -- stale 必须带失效时间
);
-- 正片位唯一：同一节点同一来源最多一条在用的 work 链
CREATE UNIQUE INDEX library_links_work_uniq ON library_links (node_id, source)
  WHERE role = 'work' AND state <> 'replaced';
CREATE INDEX library_links_node_idx ON library_links (node_id);

-- 反哺状态：每条链 × 每个可写目标（Emby 只读，不进来）
CREATE TABLE sync_targets (
  link_id    uuid NOT NULL REFERENCES library_links(id) ON DELETE CASCADE,
  target     text NOT NULL CHECK (target IN ('gameatlas','komga')),
  synced_ver integer,
  status     text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','ok','failed','skipped')),
  message    text NOT NULL DEFAULT '',
  synced_at  timestamptz,
  PRIMARY KEY (link_id, target)
);

-- 建议（建档 / 关联 / 合并 / 忽略…）：动作**枚举化**防幻觉；生命周期与「忽略」分开（v1 一个 blob 混三种）
CREATE TABLE suggestions (
  id          uuid PRIMARY KEY,
  kind        text NOT NULL CHECK (kind IN ('create','link','merge','research','ignore','split')),
  node_id     uuid REFERENCES nodes(id) ON DELETE CASCADE,
  source      text CHECK (source IN ('gameatlas','emby','komga')),
  external_id text,
  payload     jsonb NOT NULL DEFAULT '{}',
  state       text NOT NULL DEFAULT 'pending' CHECK (state IN ('pending','accepted','rejected','ignored')),
  created_by  text NOT NULL DEFAULT 'llm',
  created_at  timestamptz NOT NULL DEFAULT now(),
  decided_at  timestamptz
);
CREATE INDEX suggestions_state_idx ON suggestions (state, created_at DESC);

-- 判断口径进数据：改 prompt / 换模型 → 指纹变 → 自动重判
CREATE TABLE judge_cache (
  scope          text NOT NULL,
  key            text NOT NULL,
  fingerprint    text NOT NULL,                              -- hash(prompt_version, model, input_signature)
  prompt_version integer NOT NULL,
  model          text NOT NULL,
  verdict        jsonb NOT NULL,
  judged_at      timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (scope, key)
);

CREATE TABLE scan_runs (
  id          bigserial PRIMARY KEY,
  source      text NOT NULL CHECK (source IN ('gameatlas','emby','komga')),
  started_at  timestamptz NOT NULL DEFAULT now(),
  finished_at timestamptz,
  ok          boolean,
  stats       jsonb NOT NULL DEFAULT '{}',                   -- 新增 / 消失 / 变更 计数
  message     text NOT NULL DEFAULT ''
);
CREATE INDEX scan_runs_source_idx ON scan_runs (source, started_at DESC);

-- ============================================================ 运维

-- 队列 + 租约（取代 v1 的进程内队列与「重启全标 interrupted」）
CREATE TABLE jobs (
  id           bigserial PRIMARY KEY,
  kind         text NOT NULL CHECK (kind IN ('run','scan','sync','audit')),
  payload      jsonb NOT NULL DEFAULT '{}',
  state        text NOT NULL DEFAULT 'pending' CHECK (state IN ('pending','running','done','failed')),
  attempts     integer NOT NULL DEFAULT 0,
  run_after    timestamptz NOT NULL DEFAULT now(),
  locked_by    text,
  locked_until timestamptz,                                  -- 租约过期 → 自动回收
  created_at   timestamptz NOT NULL DEFAULT now(),
  CHECK (state <> 'running' OR locked_until IS NOT NULL)
);
CREATE INDEX jobs_state_run_after_idx ON jobs (state, run_after);

-- 配置与密钥分离：密钥绝不进 API DTO / 不进常规备份（docs/database.md §4）
CREATE TABLE app_settings (
  key        text PRIMARY KEY,
  value      jsonb NOT NULL,
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE secrets (
  key        text PRIMARY KEY,
  value      text NOT NULL,                                  -- 原文；本表不进任何 API DTO
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE health_findings (
  id          uuid PRIMARY KEY,
  kind        text NOT NULL,                                 -- 失效链 / 孤儿快照 / 缺关系 / 字段缺口 …
  node_id     uuid REFERENCES nodes(id) ON DELETE CASCADE,
  severity    text NOT NULL DEFAULT 'info' CHECK (severity IN ('info','warn','error')),
  detail      jsonb NOT NULL DEFAULT '{}',
  state       text NOT NULL DEFAULT 'open' CHECK (state IN ('open','resolved','ignored')),
  created_at  timestamptz NOT NULL DEFAULT now(),
  resolved_at timestamptz
);
CREATE INDEX health_findings_state_idx ON health_findings (state, severity);

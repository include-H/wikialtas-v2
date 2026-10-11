-- WikiAtlas v2 · 0002 清单契约字段（`docs/library-sync.md` §2.1）
--
-- 0001 的 library_manifest 只落了「身份 + 标题 + 年份 + 库侧 kind + raw」。
-- 清单契约要求封面 / 条目 URL / 容器（库 or 系列）/ format / 副标题 / 别名
-- 都能被池子直接查（而不是塞在 raw 里当二等公民），所以扩列。
--
-- 迁移纪律：0001 已应用，故一律走 ALTER（不许改老迁移）。

ALTER TABLE library_manifest
  ADD COLUMN subtitle       text,
  ADD COLUMN aliases        text[] NOT NULL DEFAULT '{}',
  ADD COLUMN cover_url      text,
  ADD COLUMN url            text,
  ADD COLUMN container_key   text NOT NULL DEFAULT '',   -- 稳定分组键（series:<id> / view:<id>）
  ADD COLUMN container_title text NOT NULL DEFAULT '',
  ADD COLUMN container_kind  text NOT NULL DEFAULT '',   -- series|library|mixed（空 = 未分类）
  ADD COLUMN format          text,                        -- comic|novel
  ADD COLUMN extra           text NOT NULL DEFAULT '',    -- 人读备注（「GA系列：…」「抽屉拆单册」）
  ADD COLUMN content_hash    text NOT NULL DEFAULT '',    -- 增量缓存指纹（llm_wiki ingest-cache）
  ADD COLUMN first_seen_at   timestamptz NOT NULL DEFAULT now();

ALTER TABLE library_manifest
  ADD CONSTRAINT library_manifest_container_kind_chk
  CHECK (container_kind IN ('', 'series', 'library', 'mixed'));

CREATE INDEX library_manifest_pool_idx
  ON library_manifest (source, missing_since, container_key);

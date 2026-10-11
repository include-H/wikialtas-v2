-- 0001 的约束冒烟测试
--
-- 期望：正例写入成功；反例被挡住，且**报错类型正确**（防止「因为别的原因失败」被当成通过）。
-- 跑法：psql -v ON_ERROR_STOP=1 -f 0001_init.smoke.sql
-- 整体在一个事务里，末尾 ROLLBACK，不留数据。SQLSTATE：23514=CHECK 23505=唯一 23503=外键 23502=非空

BEGIN;

DO $$
DECLARE
  n_series uuid; n_work uuid; n_doc uuid;
  l1 uuid; l2 uuid;
  c record; got text;
BEGIN
  -- ---------------------------------------------------------- 正例：能写
  INSERT INTO nodes (id, kind, medium, title, title_norm, sort_key)
    VALUES (gen_random_uuid(), 'series', 'game', '冒烟系列', '冒烟系列', 'a0') RETURNING id INTO n_series;
  INSERT INTO nodes (id, parent_id, kind, medium, title, title_norm, sort_key)
    VALUES (gen_random_uuid(), n_series, 'work', 'game', '冒烟作品', '冒烟作品', 'a1') RETURNING id INTO n_work;
  INSERT INTO nodes (id, kind, title, title_norm, sort_key)
    VALUES (gen_random_uuid(), 'doc', '冒烟资料', '冒烟资料', 'a2') RETURNING id INTO n_doc;

  INSERT INTO relations (id, from_id, to_id, type, created_by)
    VALUES (gen_random_uuid(), n_work, n_series, 'sequel_of', 'host');
  INSERT INTO contents (node_id, body_md, ver, updated_by) VALUES (n_work, '正文', 1, 'host');
  INSERT INTO content_versions (id, node_id, ver, body_md, source, summary)
    VALUES (gen_random_uuid(), n_work, 1, '正文', 'host', '首版');
  INSERT INTO write_events (node_id, ver, source) VALUES (n_work, 1, 'host');
  INSERT INTO doc_refs (doc_id, work_id) VALUES (n_doc, n_work);
  INSERT INTO jobs (kind, payload) VALUES ('scan', '{"source":"komga"}');
  INSERT INTO library_links (id, node_id, source, external_id, role)
    VALUES (gen_random_uuid(), n_work, 'komga', 'komga-1', 'work') RETURNING id INTO l1;
  INSERT INTO sync_targets (link_id, target) VALUES (l1, 'komga');
  RAISE NOTICE '✓ 正例：节点 / 关系 / 定稿 / 快照 / 写入事实 / 资料引用 / 任务 / 链 / 反哺目标 全部可写';

  -- 失效链不删，但「正片位」仍被占着：stale 的 work 链没让位前，插第二条 work 链必须被挡
  -- （这是有意的：重挂要逼出一次明确决定，而不是让两条链悄悄并存）
  UPDATE library_links SET state = 'stale', missing_since = now() WHERE id = l1;
  got := '';
  BEGIN
    INSERT INTO library_links (id, node_id, source, external_id, role)
      VALUES (gen_random_uuid(), n_work, 'komga', 'komga-1-new', 'work');
  EXCEPTION WHEN others THEN got := SQLSTATE;
  END;
  IF got <> '23505' THEN
    RAISE EXCEPTION '✗ stale 链应仍占正片位（期望 23505，得到 %）', coalesce(nullif(got, ''), '未报错');
  END IF;
  RAISE NOTICE '✓ 已挡住：stale 的 work 链仍占正片位（23505）';

  -- 重挂：先显式让位（replaced + replaced_by），再插新链 → 放行
  UPDATE library_links SET state = 'replaced' WHERE id = l1;
  INSERT INTO library_links (id, node_id, source, external_id, role, replaced_by)
    VALUES (gen_random_uuid(), n_work, 'komga', 'komga-1-new', 'work', l1) RETURNING id INTO l2;
  RAISE NOTICE '✓ 正例：重挂（旧链 replaced + 新链 active）可共存';

  -- ---------------------------------------------------------- 反例：必须被挡
  FOR c IN SELECT * FROM (VALUES
    ('自指关系',            format('INSERT INTO relations (id, from_id, to_id, type, created_by) VALUES (gen_random_uuid(), %L, %L, ''sequel_of'', ''host'')', n_work, n_work), '23514'),
    ('未知关系类型',        format('INSERT INTO relations (id, from_id, to_id, type, created_by) VALUES (gen_random_uuid(), %L, %L, ''related_to'', ''host'')', n_work, n_series), '23503'),
    ('同级 sort_key 重复（同一父）', format('INSERT INTO nodes (id, parent_id, kind, medium, title, title_norm, sort_key) VALUES (gen_random_uuid(), %L, ''work'', ''game'', ''撞名'', ''撞名'', ''a1'')', n_series), '23505'),
    ('同级 sort_key 重复（根，NULLS NOT DISTINCT）', format('INSERT INTO nodes (id, kind, medium, title, title_norm, sort_key) VALUES (gen_random_uuid(), ''series'', ''game'', ''撞根'', ''撞根'', ''a0'')'), '23505'),
    ('doc 带介质',          format('INSERT INTO nodes (id, kind, medium, title, title_norm, sort_key) VALUES (gen_random_uuid(), ''doc'', ''game'', ''坏资料'', ''坏资料'', ''a9'')'), '23514'),
    ('work 缺介质',         format('INSERT INTO nodes (id, kind, title, title_norm, sort_key) VALUES (gen_random_uuid(), ''work'', ''无介质'', ''无介质'', ''a8'')'), '23514'),
    ('同源同节点第二条 work 链', format('INSERT INTO library_links (id, node_id, source, external_id, role) VALUES (gen_random_uuid(), %L, ''komga'', ''komga-9'', ''work'')', n_work), '23505'),
    ('同名异物（source+external_id 撞）', format('INSERT INTO library_links (id, node_id, source, external_id, role) VALUES (gen_random_uuid(), %L, ''komga'', ''komga-1-new'', ''mix'')', n_doc), '23505'),
    ('stale 缺 missing_since', format('INSERT INTO library_links (id, node_id, source, external_id, role, state) VALUES (gen_random_uuid(), %L, ''emby'', ''emby-1'', ''mix'', ''stale'')', n_work), '23514'),
    ('未知介质',            format('INSERT INTO nodes (id, kind, medium, title, title_norm, sort_key) VALUES (gen_random_uuid(), ''work'', ''anime'', ''未知介质'', ''未知介质'', ''a7'')'), '23514'),
    ('未知 kind',           format('INSERT INTO nodes (id, kind, medium, title, title_norm, sort_key) VALUES (gen_random_uuid(), ''misc'', ''game'', ''未知类'', ''未知类'', ''a6'')'), '23514'),
    ('running 无租约',      format('INSERT INTO jobs (kind, state) VALUES (''run'', ''running'')'), '23514'),
    ('未知 sync target',    format('INSERT INTO sync_targets (link_id, target) VALUES (%L, ''emby'')', l2), '23514'),
    ('资料引用悬空',        format('INSERT INTO doc_refs (doc_id, work_id) VALUES (%L, gen_random_uuid())', n_doc), '23503'),
    ('快照版本重复',        format('INSERT INTO content_versions (id, node_id, ver, body_md, source) VALUES (gen_random_uuid(), %L, 1, ''另一个正文'', ''llm'')', n_work), '23505'),
    ('草稿 base_ver 为负',  format('INSERT INTO content_drafts (node_id, base_ver, by, lease_until) VALUES (%L, -1, ''host'', now() + interval ''1 hour'')', n_work), '23514')
  ) AS t(desc_, sql_, want) LOOP
    got := '';
    BEGIN
      EXECUTE c.sql_;
    EXCEPTION WHEN others THEN got := SQLSTATE;
    END;
    IF got = '' THEN RAISE EXCEPTION '✗ 反例未被挡住：%', c.desc_; END IF;
    IF got <> c.want THEN RAISE EXCEPTION '✗ 反例 % 报错类型不对：得到 %，期望 %', c.desc_, got, c.want; END IF;
    RAISE NOTICE '✓ 已挡住：%（%）', c.desc_, got;
  END LOOP;

  RAISE NOTICE '—— 全部通过 ——';
END $$;

ROLLBACK;

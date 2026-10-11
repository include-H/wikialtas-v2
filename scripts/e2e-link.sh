#!/bin/bash
# M2 验收：**池子点一条 → 树出节点 + 链 active**；撞名加介质后缀；解链保留文档。
#
# 前置：api 8088、worker、前端 5173、agent-browser 可用；媒体库扫过一次（脚本会自己确保）。
# 用法：bash scripts/e2e-link.sh
#
# 纪律（docs/backend.md §9）：验收必须真跑——建档那一步走**真界面点**，其余回服务端核对。
set -e
cd "$(dirname "$0")/.."

AB=agent-browser
API=http://127.0.0.1:8088
WEB=http://127.0.0.1:5173
PW=$(grep -oP '(?<=wikiatlas:)[^@]+' .secrets/wikiatlas.pg.env)
export PGPASSWORD="$PW"
PSQL="psql -h 127.0.0.1 -U wikiatlas -d wikiatlas -qtA"

ok=0; bad=0
chk() { if [ "$2" = "$3" ]; then ok=$((ok+1)); echo "  ✓ $1"; else bad=$((bad+1)); echo "  ✗ $1  得到[$2] 期望[$3]"; fi; }
jqget() { python3 -c "import sys,json;print(json.load(sys.stdin)$1)"; }

echo "=== 0. 等 API + 确保清单非空 ==="
curl -s --retry 30 --retry-connrefused --retry-delay 1 -o /dev/null "$API/api/healthz"
if [ "$($PSQL -c 'select count(*) > 0 from library_manifest')" != "t" ]; then
  curl -s -X POST "$API/api/library/scan" >/dev/null
  for _ in $(seq 1 90); do
    [ "$(curl -s "$API/api/library/status" | jqget '["queueDepth"]')" = "0" ] && break
    sleep 2
  done
fi
chk "清单非空" "$($PSQL -c 'select count(*) > 0 from library_manifest')" "t"

echo "=== 1. 挑一条「有系列容器、还没挂链」的 GA 条目 ==="
# EID=清单身份；CT=容器标题（该被建成系列节点）；T=条目标题
IFS='|' read -r EID CT T <<<"$($PSQL -c "
  select m.external_id, m.container_title, m.title from library_manifest m
  left join library_links l on l.source = m.source and l.external_id = m.external_id
  where m.source='gameatlas' and m.missing_since is null
    and m.container_kind='series' and m.container_key <> 'series:none' and l.id is null
    and m.title !~ '（'  -- 挑个没有后缀的，断言好写
  order by m.title limit 1")"
echo "  条目：$T（系列容器：$CT）"
chk "挑到了条目" "$([ -n "$EID" ] && echo yes || echo no)" "yes"
# 清理上一轮可能留下的同名节点（保证幂等重跑）
$PSQL -c "delete from nodes where title = '$CT' or title like '$T%'" >/dev/null 2>&1 || true

echo "=== 2. 真界面：池子 → 建档 ==="
$AB set viewport 1440 1000 >/dev/null
$AB open "$WEB" >/dev/null
$AB wait 1500 >/dev/null
$AB click ".side-links .side-link:nth-child(2)" >/dev/null   # 媒体库池子
$AB wait 1200 >/dev/null
$AB fill "input[placeholder='过滤标题']" "$T" >/dev/null
$AB wait 800 >/dev/null
chk "池子里过滤到这一条" "$($AB eval "[...document.querySelectorAll('.pool-row')].some(r=>r.innerText.includes('$T')) ? 'yes':'no'" 2>&1 | tail -1)" '"yes"'
# 点这一行的「建档」
$AB eval "(()=>{const r=[...document.querySelectorAll('.pool-row')].find(r=>r.innerText.includes('$T'));r.querySelector('button.ant-btn-primary').click();return 'ok'})()" >/dev/null
$AB wait 900 >/dev/null
chk "建档弹窗开了" "$($AB eval "document.querySelector('.ant-modal-title')?.innerText" 2>&1 | tail -1)" '"建档"'
$AB click ".ant-modal-footer .ant-btn-primary" >/dev/null
$AB wait 2000 >/dev/null

echo "=== 3. 回服务端核对：节点 + 链 ==="
chk "建出了系列节点（容器名）" "$($PSQL -c "select count(*) from nodes where kind='series' and title='$CT'")" "1"
chk "文档节点是 work + 介质=game" "$($PSQL -c "select kind||'/'||medium from nodes where title='$T'")" "work/game"
chk "文档挂在系列节点下" "$($PSQL -c "select p.kind from nodes n join nodes p on p.id=n.parent_id where n.title='$T'")" "series"
chk "链 active / work / linked_by=pool" "$($PSQL -c "select state||'/'||role||'/'||linked_by from library_links where source='gameatlas' and external_id='$EID'")" "active/work/pool"
chk "一个条目只挂一篇文档（唯一）" "$($PSQL -c "select count(*) from library_links where source='gameatlas' and external_id='$EID'")" "1"
chk "池子里这条变成「有文档」" \
  "$(curl -s "$API/api/library/pool?source=gameatlas&has_doc=true" | python3 -c "import sys,json;print(sum(1 for p in json.load(sys.stdin) if p['entry']['external_id']=='$EID'))")" "1"

echo "=== 4. 撞名加介质后缀（Komga：系列名 = 作品名）==="
IFS='|' read -r KEID KT <<<"$($PSQL -c "
  select m.external_id, m.title from library_manifest m
  left join library_links l on l.source = m.source and l.external_id = m.external_id
  where m.source='komga' and m.missing_since is null and m.container_title = m.title
    and m.container_kind='series' and m.format='comic' and l.id is null
  order by m.title limit 1")"
echo "  条目：$KT"
# 清理上一轮：系列节点 + 各种介质后缀的文档节点
$PSQL -c "delete from nodes where title = '$KT' or title like '$KT（%'" >/dev/null 2>&1 || true
curl -s -X POST "$API/api/library/links" -H 'Content-Type: application/json' \
  -d "{\"source\":\"komga\",\"external_id\":\"$KEID\"}" >/dev/null
chk "与父同名 → 加了介质后缀（漫画）" "$($PSQL -c "select title from nodes where title like '$KT%（漫画）'")" "$KT（漫画）"
chk "系列节点与文档节点不同" "$($PSQL -c "select count(*) from nodes where title in ('$KT','$KT（漫画）')")" "2"

echo "=== 5. 解链：只删链，文档不动 ==="
LID=$($PSQL -c "select id from library_links where source='komga' and external_id='$KEID'")
KNID=$($PSQL -c "select node_id from library_links where id='$LID'")
curl -s -X DELETE "$API/api/library/links/$LID" >/dev/null
chk "链没了" "$($PSQL -c "select count(*) from library_links where id='$LID'")" "0"
chk "文档还在" "$($PSQL -c "select count(*) from nodes where id='$KNID'")" "1"
chk "这条回到池子的「无文档」" \
  "$(curl -s "$API/api/library/pool?source=komga&has_doc=false" | python3 -c "import sys,json;print(sum(1 for p in json.load(sys.stdin) if p['entry']['external_id']=='$KEID'))")" "1"

echo "=== 6. 忽略（单条）后可恢复 ==="
GID=$(curl -s -X POST "$API/api/library/ignores" -H 'Content-Type: application/json' \
  -d "{\"source\":\"gameatlas\",\"external_id\":\"$EID\",\"reason\":\"e2e\"}" | jqget '["id"]')
chk "被忽略的不在池子里" \
  "$(curl -s "$API/api/library/pool?source=gameatlas" | python3 -c "import sys,json;print(sum(1 for p in json.load(sys.stdin) if p['entry']['external_id']=='$EID'))")" "0"
curl -s -X DELETE "$API/api/library/ignores/$GID" >/dev/null
chk "恢复后又回到池子" \
  "$(curl -s "$API/api/library/pool?source=gameatlas" | python3 -c "import sys,json;print(sum(1 for p in json.load(sys.stdin) if p['entry']['external_id']=='$EID'))")" "1"

echo
echo "=== 结果：通过 $ok 项，失败 $bad 项 ==="
[ "$bad" = "0" ]

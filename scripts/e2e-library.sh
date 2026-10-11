#!/bin/bash
# M1 验收：**真库**扫三源 → 清单进 library_manifest → 池子列出「有/无文档」。
#
# 前置：api 在 127.0.0.1:8088、worker 在跑、前端在 5173、agent-browser 可用。
#       （扫描本身在 worker 里跑，所以这个脚本跑之前必须先起 worker。）
#
# 用法：bash scripts/e2e-library.sh
#
# 纪律（docs/backend.md §9）：验收必须真跑——不是"编译过了""接口 200"。
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

echo "=== 0. 等 API ==="
curl -s --retry 30 --retry-connrefused --retry-delay 1 -o /dev/null "$API/api/healthz"

echo "=== 1. 入队一次扫描，等 worker 跑完 ==="
curl -s -X POST "$API/api/library/scan" | jqget '["jobId"]' >/dev/null
done_scan=0
for _ in $(seq 1 90); do
  depth=$(curl -s "$API/api/library/status" | jqget '["queueDepth"]')
  if [ "$depth" = "0" ]; then done_scan=1; break; fi
  sleep 2
done
chk "扫描跑完（队列清空）" "$done_scan" "1"

echo "=== 2. 清单真的落了三源 ==="
for src in gameatlas emby komga; do
  chk "$src 清单非空" "$($PSQL -c "select count(*)>0 from library_manifest where source='$src'")" "t"
  chk "$src 最近一次扫描 ok" "$($PSQL -c "select ok from scan_runs where source='$src' order by id desc limit 1")" "t"
done
chk "清单条数 = 三源之和" \
  "$($PSQL -c 'select count(*) from library_manifest where missing_since is null')" \
  "$($PSQL -c "select count(*) from library_manifest where missing_since is null and source in ('gameatlas','emby','komga')")"

echo "=== 3. 池子 API：无文档 = 清单条数（M1 还没建档，所以全部无文档） ==="
pool_n=$(curl -s "$API/api/library/pool?has_doc=false" | python3 -c 'import sys,json;print(len(json.load(sys.stdin)))')
manifest_n=$($PSQL -c "select count(*) from library_manifest where missing_since is null")
chk "池子(无文档) 条数 = 清单条数" "$pool_n" "$manifest_n"
chk "池子(有文档) 现在为空" "$(curl -s "$API/api/library/pool?has_doc=true" | python3 -c 'import sys,json;print(len(json.load(sys.stdin)))')" "0"
chk "按源过滤（gameatlas）条数对" \
  "$(curl -s "$API/api/library/pool?source=gameatlas" | python3 -c 'import sys,json;print(len(json.load(sys.stdin)))')" \
  "$($PSQL -c "select count(*) from library_manifest where source='gameatlas' and missing_since is null")"

echo "=== 4. 真 UI：池子页渲染 + 过滤 ==="
$AB open "$WEB" >/dev/null
$AB wait 1200 >/dev/null
$AB click ".side-links .side-link:nth-child(2)" >/dev/null
$AB wait 1500 >/dev/null
chk "池子页出现" "$($AB eval "document.querySelector('.pool') ? 'yes':'no'" 2>&1 | tail -1)" '"yes"'
chk "渲染出行（>0）" "$($AB eval "document.querySelectorAll('.pool-row').length > 0 ? 'yes':'no'" 2>&1 | tail -1)" '"yes"'
chk "「无文档」标签出现" "$($AB eval "[...document.querySelectorAll('.pool-row .ant-tag')].some(t=>t.textContent.includes('无文档')) ? 'yes':'no'" 2>&1 | tail -1)" '"yes"'
rows_before=$($AB eval "document.querySelectorAll('.pool-row').length" 2>&1 | tail -1)
$AB fill "input[placeholder='过滤标题']" "使命召唤" >/dev/null
$AB wait 800 >/dev/null
rows_after=$($AB eval "document.querySelectorAll('.pool-row').length" 2>&1 | tail -1)
chk "过滤后行数变少且 >0" "$(python3 -c "print('yes' if 0 < $rows_after < $rows_before else 'no')")" "yes"

echo
echo "=== 结果：通过 $ok 项，失败 $bad 项 ==="
[ "$bad" = "0" ]

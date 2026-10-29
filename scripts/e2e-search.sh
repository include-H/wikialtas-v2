#!/bin/bash
# M8 验收：正文检索命中 + 可点结果 + 好回答存为资料；不调用模型。
set -euo pipefail
cd "$(dirname "$0")/.."

API_PORT=18088
WEB_PORT=15173
API="http://127.0.0.1:$API_PORT"
WEB="http://127.0.0.1:$WEB_PORT"
DB_NAME="wikiatlas_e2e_search_${RANDOM}_$$"
TMP=$(mktemp -d)
PIDS=()

cleanup() {
  for pid in "${PIDS[@]:-}"; do kill "$pid" 2>/dev/null || true; done
  sleep 0.3
  psql "$ADMIN_DB_URL" -q -c "DROP DATABASE IF EXISTS $DB_NAME WITH (FORCE)" >/dev/null 2>&1 || true
  rm -rf "$TMP"
}
trap cleanup EXIT

ok=0; bad=0
chk() {
  if [ "$2" = "$3" ]; then ok=$((ok+1)); echo "  ✓ $1";
  else bad=$((bad+1)); echo "  ✗ $1  得到[$2] 期望[$3]"; fi
}
json_get() { python3 -c "import sys,json; d=json.load(sys.stdin); print($1)"; }

echo "=== 0. 隔离环境 ==="
set -a; . .secrets/wikiatlas.pg.env; set +a
ADMIN_DB_URL="$WIKIATLAS_DB_URL"; export ADMIN_DB_URL
psql "$ADMIN_DB_URL" -q -c "CREATE DATABASE $DB_NAME" >/dev/null
TEST_DB_URL=$(python3 - "$ADMIN_DB_URL" "$DB_NAME" <<'PY'
import sys, urllib.parse
u = urllib.parse.urlsplit(sys.argv[1])
print(urllib.parse.urlunsplit((u.scheme, u.netloc, "/" + sys.argv[2], u.query, u.fragment)))
PY
)
export DATABASE_URL="$TEST_DB_URL" WIKIATLAS_DB_URL="$TEST_DB_URL"
export WIKIATLAS_BIND="127.0.0.1:$API_PORT"
cargo build -q -p wikiatlas-api
./target/debug/wikiatlas-api >"$TMP/api.log" 2>&1 & PIDS+=($!)
curl -s --retry 60 --retry-connrefused --retry-delay 1 -o /dev/null "$API/api/healthz"
cd frontend
WIKIATLAS_WEB_PORT=$WEB_PORT WIKIATLAS_API_TARGET="$API" ./node_modules/.bin/vite --host 127.0.0.1 >"$TMP/web.log" 2>&1 &
PIDS+=($!)
cd ..
curl -s --retry 60 --retry-connrefused --retry-delay 1 -o /dev/null "$WEB"

echo "=== 1. 正文独有事实可检索 ==="
NODE=$(curl -s -X POST "$API/api/nodes" -H 'Content-Type: application/json' \
  -d '{"parent_id":null,"kind":"work","medium":"game","title":"空之轨迹 SC"}')
NID=$(printf '%s' "$NODE" | json_get 'd["id"]')
BODY='{"body_md":"# 空之轨迹 SC（游戏）Wiki\n\n青羽测试是只在正文里出现的事实。\n\n## 1. 作品概览\n\n正文。","base_ver":0}'
curl -s -X PUT "$API/api/nodes/$NID/draft" -H 'Content-Type: application/json' -d "$BODY" >/dev/null
curl -s -X POST "$API/api/nodes/$NID/commit" -H 'Content-Type: application/json' -d "$BODY" >/dev/null
HITS=$(curl -s "$API/api/search?q=%E9%9D%92%E7%BE%BD%E6%B5%8B%E8%AF%95")
chk "正文事实命中一篇" "$(printf '%s' "$HITS" | json_get 'len(d)')" "1"
chk "命中正确节点" "$(printf '%s' "$HITS" | json_get 'd[0]["node_id"]')" "$NID"
chk "返回正文片段" "$(printf '%s' "$HITS" | json_get '"青羽测试" in d[0]["snippet"]')" "True"

echo "=== 2. 好回答存成资料（不改作品正文）==="
SAVED=$(curl -s -X POST "$API/api/answers" -H 'Content-Type: application/json' \
  -d "{\"parent_id\":\"$NID\",\"title\":\"问答：SC 与前作关系\",\"body_md\":\"SC 是 FC 的直接续作。\",\"ref_ids\":[\"$NID\"]}")
SID=$(printf '%s' "$SAVED" | json_get 'd["id"]')
chk "存成 doc" "$(printf '%s' "$SAVED" | json_get 'd["kind"]')" "doc"
chk "资料带引用" \
  "$(psql "$TEST_DB_URL" -qtA -c "select count(*) from doc_refs where doc_id='$SID'")" "1"
chk "作品正文仍是 v1" \
  "$(curl -s "$API/api/nodes/$NID" | json_get 'd["content"]["ver"]')" "1"
chk "资料正文可再次检索" \
  "$(curl -s "$API/api/search?q=%E7%9B%B4%E6%8E%A5%E7%BB%AD%E4%BD%9C" | json_get 'any(x["node_id"]=="'"$SID"'" for x in d)')" "True"

echo "=== 3. UI 检索页与可点结果 ==="
AB=agent-browser
$AB set viewport 1440 1000 >/dev/null
$AB open "$WEB" >/dev/null
$AB wait 900 >/dev/null
$AB click ".side-links .side-link:nth-child(3)" >/dev/null
$AB wait 500 >/dev/null
$AB fill ".search-page input" "青羽测试" >/dev/null
$AB wait 900 >/dev/null
chk "搜索页打开" \
  "$($AB eval "document.querySelector('.search-head h1')?.innerText" 2>&1 | tail -1)" '"全文检索"'
chk "结果行出现" \
  "$($AB eval "document.querySelector('.search-hit')?.innerText.includes('空之轨迹 SC')" 2>&1 | tail -1)" "true"
chk "片段里能看见独有事实" \
  "$($AB eval "document.querySelector('.search-hit-snippet')?.innerText.includes('青羽测试')" 2>&1 | tail -1)" "true"
$AB click ".search-hit" >/dev/null
$AB wait 900 >/dev/null
chk "点击结果进入文档" \
  "$($AB eval "document.querySelector('.doc-title')?.innerText" 2>&1 | tail -1)" '"空之轨迹 SC"'

echo
echo "=== 结果：通过 $ok 项，失败 $bad 项 ==="
[ "$bad" = "0" ]

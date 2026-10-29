#!/bin/bash
# M6 验收：真实 worker 跑体检 + UI 裁决建议；临时库，不调用模型。
set -euo pipefail
cd "$(dirname "$0")/.."

API_PORT=18088
WEB_PORT=15173
API="http://127.0.0.1:$API_PORT"
WEB="http://127.0.0.1:$WEB_PORT"
DB_NAME="wikiatlas_e2e_audit_${RANDOM}_$$"
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
wait_jobs() {
  for _ in $(seq 1 90); do
    [ "$(curl -s "$API/api/library/status" | json_get 'd["queueDepth"]')" = "0" ] && return 0
    sleep 0.4
  done
  return 1
}

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
cargo build -q -p wikiatlas-api -p wikiatlas-worker
./target/debug/wikiatlas-api >"$TMP/api.log" 2>&1 & PIDS+=($!)
curl -s --retry 60 --retry-connrefused --retry-delay 1 -o /dev/null "$API/api/healthz"
./target/debug/wikiatlas-worker >"$TMP/worker.log" 2>&1 & PIDS+=($!)
cd frontend
WIKIATLAS_WEB_PORT=$WEB_PORT WIKIATLAS_API_TARGET="$API" ./node_modules/.bin/vite --host 127.0.0.1 >"$TMP/web.log" 2>&1 &
PIDS+=($!)
cd ..
curl -s --retry 60 --retry-connrefused --retry-delay 1 -o /dev/null "$WEB"

echo "=== 1. 制造体检项 + 待裁决建议 ==="
BAD=$(curl -s -X POST "$API/api/nodes" -H 'Content-Type: application/json' \
  -d '{"parent_id":null,"kind":"work","medium":"game","title":"定稿却没有正文"}')
BADID=$(printf '%s' "$BAD" | json_get 'd["id"]')
psql "$TEST_DB_URL" -q -c "UPDATE nodes SET status='ready' WHERE id='$BADID'" >/dev/null
SUG=$(curl -s -X POST "$API/api/suggestions" -H 'Content-Type: application/json' \
  -d '{"kind":"create","payload":{"title":"Review 接受的游戏","kind":"work","medium":"game","reason":"清单里有一条高相关候选"}}')
SUGID=$(printf '%s' "$SUG" | json_get 'd["id"]')
chk "建议已入队" "$(curl -s "$API/api/suggestions?state=pending" | json_get 'len(d)')" "1"

echo "=== 2. UI 运行体检并查看发现 ==="
AB=agent-browser
$AB set viewport 1440 1000 >/dev/null
$AB open "$WEB" >/dev/null
$AB wait 1000 >/dev/null
$AB eval "(()=>{const n=[...document.querySelectorAll('.side-links .side-link')].find(x=>x.innerText.includes('体检与 Review'));n?.click();return 'ok'})()" >/dev/null
$AB wait 600 >/dev/null
chk "体检页打开" \
  "$($AB eval "document.querySelector('.audit-head h1')?.innerText" 2>&1 | tail -1)" '"体检与 Review"'
chk "待裁决建议出现" \
  "$($AB eval "document.querySelector('.review-list')?.innerText.includes('Review 接受的游戏')" 2>&1 | tail -1)" "true"
$AB eval "(()=>{const b=[...document.querySelectorAll('button')].find(x=>x.innerText.includes('运行体检'));b?.click();return 'ok'})()" >/dev/null
wait_jobs
for _ in $(seq 1 40); do
  n=$(curl -s "$API/api/health-findings?state=open" | json_get 'len(d)')
  [ "$n" -ge 2 ] && break
  sleep 0.4
done
HEALTH=$(curl -s "$API/api/health-findings?state=open")
chk "发现缺正文" \
  "$(printf '%s' "$HEALTH" | json_get 'any(x["kind"]=="ready_without_content" for x in d)')" "True"
chk "发现孤立节点" \
  "$(printf '%s' "$HEALTH" | json_get 'any(x["kind"]=="isolated_node" for x in d)')" "True"
$AB wait 1200 >/dev/null
chk "UI 列出缺正文发现" \
  "$($AB eval "document.querySelector('.finding-list')?.innerText.includes('标记定稿但没有正文')" 2>&1 | tail -1)" "true"

echo "=== 3. UI 接受建议 → 真建节点 ==="
$AB eval "(()=>{const r=[...document.querySelectorAll('.review-row')].find(x=>x.innerText.includes('Review 接受的游戏'));const b=[...r.querySelectorAll('button')].find(x=>x.innerText.includes('接受'));b?.click();return 'ok'})()" >/dev/null
$AB wait 1200 >/dev/null
chk "建议变成 accepted" \
  "$(curl -s "$API/api/suggestions?state=accepted" | json_get 'd[0]["state"]')" "accepted"
chk "接受后节点已建" \
  "$(curl -s "$API/api/tree" | json_get 'any(x["title"]=="Review 接受的游戏" for x in d)')" "True"
chk "pending 已清空" \
  "$(curl -s "$API/api/suggestions?state=pending" | json_get 'len(d)')" "0"

echo
echo "=== 结果：通过 $ok 项，失败 $bad 项 ==="
[ "$bad" = "0" ]

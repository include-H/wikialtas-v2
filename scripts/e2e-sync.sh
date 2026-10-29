#!/bin/bash
# M7 验收：隔离临时库 + 假 GameAtlas/Komga + 真 api/worker/UI。
#
# 覆盖：定稿事务入队 → GA 全文+摘要 / Komga 摘要 → 状态 ok；
#       条目消失 → stale → 候选 → 重挂 → 旧链 replaced / 文档不变。
#
# 不碰现有 wikiatlas 库、不调用真媒体库、不调用模型。
set -euo pipefail
cd "$(dirname "$0")/.."

API_PORT=18088
WEB_PORT=15173
MOCK_PORT=18090
API="http://127.0.0.1:$API_PORT"
WEB="http://127.0.0.1:$WEB_PORT"
MOCK="http://127.0.0.1:$MOCK_PORT"
DB_NAME="wikiatlas_e2e_sync_${RANDOM}_$$"
TMP=$(mktemp -d)
PIDS=()

cleanup() {
  for pid in "${PIDS[@]:-}"; do kill "$pid" 2>/dev/null || true; done
  sleep 0.3
  psql "$ADMIN_DB_URL" -q -c "DROP DATABASE IF EXISTS $DB_NAME WITH (FORCE)" >/dev/null 2>&1 || true
  rm -rf "$TMP"
  rm -rf frontend/node_modules/.vite-e2e-sync 2>/dev/null || true
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
ADMIN_DB_URL="$WIKIATLAS_DB_URL"
export ADMIN_DB_URL
psql "$ADMIN_DB_URL" -q -c "CREATE DATABASE $DB_NAME" >/dev/null
TEST_DB_URL=$(python3 - "$ADMIN_DB_URL" "$DB_NAME" <<'PY'
import sys, urllib.parse
u = urllib.parse.urlsplit(sys.argv[1])
print(urllib.parse.urlunsplit((u.scheme, u.netloc, "/" + sys.argv[2], u.query, u.fragment)))
PY
)

MOCK_PORT=$MOCK_PORT python3 scripts/mock-library.py >"$TMP/mock.log" 2>&1 &
PIDS+=($!)
curl -s --retry 30 --retry-connrefused --retry-delay 1 "$MOCK/__state" >/dev/null

export DATABASE_URL="$TEST_DB_URL"
export WIKIATLAS_DB_URL="$TEST_DB_URL"
export WIKIATLAS_GA_URL="$MOCK"
export WIKIATLAS_GA_PASSWORD="e2e-pass"
export WIKIATLAS_KOMGA_URL="$MOCK"
export WIKIATLAS_KOMGA_API_KEY="e2e-key"
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

wait_jobs() {
  for _ in $(seq 1 90); do
    [ "$(curl -s "$API/api/library/status" | json_get 'd["queueDepth"]')" = "0" ] && return 0
    sleep 0.5
  done
  return 1
}

echo "=== 1. 扫两源并建档 ==="
curl -s -X POST "$API/api/library/scan" -H 'Content-Type: application/json' -d '{}' >/dev/null
wait_jobs
chk "GA 清单落库" \
  "$(curl -s "$API/api/library/manifest?source=gameatlas" | json_get 'len(d)')" "1"
chk "Komga 清单落库" \
  "$(curl -s "$API/api/library/manifest?source=komga" | json_get 'len(d)')" "1"

GA_BUILD=$(curl -s -X POST "$API/api/library/links" -H 'Content-Type: application/json' \
  -d '{"source":"gameatlas","external_id":"sync-e2e-old"}')
KG_BUILD=$(curl -s -X POST "$API/api/library/links" -H 'Content-Type: application/json' \
  -d '{"source":"komga","external_id":"sync-e2e-komga"}')
GA_NODE=$(printf '%s' "$GA_BUILD" | json_get 'd["node"]["id"]')
KG_NODE=$(printf '%s' "$KG_BUILD" | json_get 'd["node"]["id"]')
GA_LINK=$(printf '%s' "$GA_BUILD" | json_get 'd["link_id"]')
chk "两个节点都建了" "$([ -n "$GA_NODE" ] && [ -n "$KG_NODE" ] && echo yes)" "yes"

BODY_GA='{"body_md":"# 同步测试游戏（游戏）Wiki\n\n> 说明：E2E 测试。\n\n:::epigraph\n测试题记\n——测试\n:::\n\n这是 GameAtlas 自动摘要。\n\n## 1. 作品概览\n\n完整正文。","base_ver":0}'
BODY_KG='{"body_md":"# 同步测试漫画（漫画）Wiki\n\n> 说明：E2E 测试。\n\n:::epigraph\n测试题记\n——测试\n:::\n\n这是 Komga 摘要。\n\n## 1. 作品概览\n\n漫画正文。","base_ver":0}'
curl -s -X PUT "$API/api/nodes/$GA_NODE/draft" -H 'Content-Type: application/json' -d "$BODY_GA" >/dev/null
curl -s -X POST "$API/api/nodes/$GA_NODE/commit" -H 'Content-Type: application/json' -d "$BODY_GA" >/dev/null
curl -s -X PUT "$API/api/nodes/$KG_NODE/draft" -H 'Content-Type: application/json' -d "$BODY_KG" >/dev/null
curl -s -X POST "$API/api/nodes/$KG_NODE/commit" -H 'Content-Type: application/json' -d "$BODY_KG" >/dev/null

echo "=== 2. worker 真写回两源 ==="
for _ in $(seq 1 90); do
  state=$(curl -s "$MOCK/__state")
  ga_ok=$(printf '%s' "$state" | json_get 'len(d["ga_pushes"])')
  kg_ok=$(printf '%s' "$state" | json_get 'len(d["komga_patches"])')
  [ "$ga_ok" -ge 1 ] && [ "$kg_ok" -ge 1 ] && break
  sleep 0.5
done
chk "GameAtlas 收到写回" "$ga_ok" "1"
chk "GameAtlas 全文" \
  "$(printf '%s' "$state" | json_get '"完整正文" in d["ga_pushes"][0]["body"]["content"]')" "True"
chk "GameAtlas 摘要跳过文档头" \
  "$(printf '%s' "$state" | json_get 'd["ga_pushes"][0]["body"]["summary"]')" "这是 GameAtlas 自动摘要。"
chk "Komga 收到摘要" \
  "$(printf '%s' "$state" | json_get 'd["komga_patches"][0]["summary"]')" "这是 Komga 摘要。"
chk "GA 同步状态 ok v1" \
  "$(curl -s "$API/api/nodes/$GA_NODE/links" | json_get 'd[0]["sync_targets"][0]["status"]+"/"+str(d[0]["sync_targets"][0]["synced_ver"])')" "ok/1"

echo "=== 3. UI 显示来源与反哺状态 ==="
AB=agent-browser
$AB set viewport 1440 1000 >/dev/null
$AB open "$WEB" >/dev/null
$AB wait 1200 >/dev/null
$AB eval "(()=>{const n=[...document.querySelectorAll('.side-tree-scroll .ant-tree-treenode')].find(x=>x.innerText.includes('同步测试游戏'));n?.querySelector('.ant-tree-node-content-wrapper')?.click();return 'ok'})()" >/dev/null
$AB wait 1000 >/dev/null
chk "文档页来源条出现" \
  "$($AB eval "document.querySelector('.doc-source')?.innerText.includes('已反哺 v1')" 2>&1 | tail -1)" "true"

echo "=== 4. 条目消失 → stale → 候选 ==="
curl -s -X POST "$MOCK/__control" -H 'Content-Type: application/json' -d '{"mode":"relink"}' >/dev/null
curl -s -X POST "$API/api/library/scan" -H 'Content-Type: application/json' -d '{}' >/dev/null
wait_jobs
chk "旧链 stale" \
  "$(curl -s "$API/api/nodes/$GA_NODE/links" | json_get 'next(x["state"] for x in d if x["id"]=="'"$GA_LINK"'")')" "stale"
chk "候选命中新条目" \
  "$(curl -s "$API/api/library/links/$GA_LINK/relink-candidates" | json_get 'd[0]["external_id"]')" "sync-e2e-new"

$AB open "$WEB" >/dev/null
$AB wait 900 >/dev/null
$AB eval "(()=>{const n=[...document.querySelectorAll('.side-tree-scroll .ant-tree-treenode')].find(x=>x.innerText.includes('同步测试游戏'));n?.querySelector('.ant-tree-node-content-wrapper')?.click();return 'ok'})()" >/dev/null
$AB wait 900 >/dev/null
chk "UI 显示失效提示" \
  "$($AB eval "document.querySelector('.ant-alert')?.innerText.includes('来源已失效')" 2>&1 | tail -1)" "true"
$AB eval "(()=>{const b=[...document.querySelectorAll('button')].find(x=>x.innerText.includes('重新挂载'));b?.click();return 'ok'})()" >/dev/null
$AB wait 700 >/dev/null
chk "候选弹窗列出重制版" \
  "$($AB eval "document.querySelector('.ant-modal')?.innerText.includes('同步测试游戏 重制版')" 2>&1 | tail -1)" "true"
$AB eval "(()=>{const b=[...document.querySelectorAll('.ant-modal button')].find(x=>x.innerText.includes('挂到这里'));b?.click();return 'ok'})()" >/dev/null
$AB wait 1400 >/dev/null
chk "UI 来源换成新条目" \
  "$($AB eval "document.querySelector('.doc-source')?.innerText.includes('同步测试游戏 重制版')" 2>&1 | tail -1)" "true"

echo "=== 5. 服务端核对：旧链留档，新链 active，文档不变 ==="
chk "旧链 replaced 且指向新链" \
  "$(curl -s "$API/api/nodes/$GA_NODE/links" | json_get 'next(x["state"]+"/"+str(x["replaced_by"] is not None) for x in d if x["id"]=="'"$GA_LINK"'")')" "replaced/True"
chk "新链 active" \
  "$(curl -s "$API/api/nodes/$GA_NODE/links" | json_get 'next(x["state"] for x in d if x["state"]=="active" and x["source"]=="gameatlas")')" "active"
chk "正文版本仍是 v1" \
  "$(curl -s "$API/api/nodes/$GA_NODE" | json_get 'd["content"]["ver"]')" "1"

echo
echo "=== 结果：通过 $ok 项，失败 $bad 项 ==="
[ "$bad" = "0" ]

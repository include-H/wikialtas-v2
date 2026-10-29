#!/bin/bash
# M5 验收：关系边 + 综合页 + 宿主索引，隔离临时库，不碰现有数据。
set -euo pipefail
cd "$(dirname "$0")/.."

API_PORT=18088
WEB_PORT=15173
API="http://127.0.0.1:$API_PORT"
WEB="http://127.0.0.1:$WEB_PORT"
DB_NAME="wikiatlas_e2e_rel_${RANDOM}_$$"
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
post_node() {
  curl -s -X POST "$API/api/nodes" -H 'Content-Type: application/json' \
    -d "{\"parent_id\":$1,\"kind\":\"$2\",\"medium\":$3,\"title\":\"$4\"}"
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
cargo build -q -p wikiatlas-api
./target/debug/wikiatlas-api >"$TMP/api.log" 2>&1 & PIDS+=($!)
curl -s --retry 60 --retry-connrefused --retry-delay 1 -o /dev/null "$API/api/healthz"
cd frontend
WIKIATLAS_WEB_PORT=$WEB_PORT WIKIATLAS_API_TARGET="$API" ./node_modules/.bin/vite --host 127.0.0.1 >"$TMP/web.log" 2>&1 &
PIDS+=($!)
cd ..
curl -s --retry 60 --retry-connrefused --retry-delay 1 -o /dev/null "$WEB"

echo "=== 1. 建树 + 类型化关系 ==="
SERIES=$(post_node null series null '轨迹系列')
SID=$(printf '%s' "$SERIES" | json_get 'd["id"]')
FC=$(post_node "\"$SID\"" work '"game"' '空之轨迹 FC')
SC=$(post_node "\"$SID\"" work '"game"' '空之轨迹 SC')
FCID=$(printf '%s' "$FC" | json_get 'd["id"]')
SCID=$(printf '%s' "$SC" | json_get 'd["id"]')
REL=$(curl -s -X POST "$API/api/relations" -H 'Content-Type: application/json' \
  -d "{\"from_id\":\"$SCID\",\"to_id\":\"$FCID\",\"type\":\"sequel_of\"}")
chk "关系建成" "$(printf '%s' "$REL" | json_get 'bool(d["id"])')" "True"
chk "SC 出去是「续作」" \
  "$(curl -s "$API/api/relations?node_id=$SCID" | json_get 'd[0]["label"]')" "续作"
chk "FC 收到是「前作」" \
  "$(curl -s "$API/api/relations?node_id=$FCID" | json_get 'd[0]["label"]')" "前作"

echo "=== 2. 综合页 + 宿主索引 ==="
SYN=$(curl -s -X POST "$API/api/synthesis" -H 'Content-Type: application/json' \
  -d "{\"parent_id\":\"$SID\",\"title\":\"利贝尔人物志\",\"doc_type\":\"role\",\"work_ids\":[\"$FCID\",\"$SCID\"]}")
SYNID=$(printf '%s' "$SYN" | json_get 'd["id"]')
chk "综合页是 doc" "$(printf '%s' "$SYN" | json_get 'd["kind"]')" "doc"
chk "综合页引用两部作品" "$(printf '%s' "$SYN" | json_get 'len(d["tags"]) > 1')" "True"

INDEX=$(curl -s -X POST "$API/api/nodes/$SID/index" -H 'Content-Type: application/json' -d '{}')
INDEXID=$(printf '%s' "$INDEX" | json_get 'd["id"]')
BODY=$(curl -s "$API/api/nodes/$INDEXID" | json_get 'd["content"]["body_md"]')
chk "索引列出 FC" "$(python3 -c "print('空之轨迹 FC' in '''$BODY''')")" "True"
chk "索引列出 SC" "$(python3 -c "print('空之轨迹 SC' in '''$BODY''')")" "True"
chk "索引列出综合页" "$(python3 -c "print('利贝尔人物志' in '''$BODY''')")" "True"
chk "索引由 host 维护" \
  "$(curl -s "$API/api/nodes/$INDEXID" | json_get 'd["content"]["updated_by"]')" "host"

echo "=== 3. UI 关系图 ==="
AB=agent-browser
$AB set viewport 1440 1000 >/dev/null
$AB open "$WEB" >/dev/null
$AB wait 1000 >/dev/null
$AB eval "(()=>{const n=[...document.querySelectorAll('.side-tree-scroll .ant-tree-treenode')].find(x=>x.innerText.includes('空之轨迹 SC'));n?.querySelector('.ant-tree-node-content-wrapper')?.click();return 'ok'})()" >/dev/null
$AB wait 900 >/dev/null
$AB click "button[aria-label='更多']" >/dev/null
$AB wait 300 >/dev/null
$AB eval "(()=>{const x=[...document.querySelectorAll('.ant-dropdown-menu-title-content')].find(x=>x.innerText.includes('关系与综合页'));x?.click();return 'ok'})()" >/dev/null
$AB wait 900 >/dev/null
chk "关系抽屉打开" \
  "$($AB eval "document.querySelector('.ant-drawer-title')?.innerText.includes('关系与综合')" 2>&1 | tail -1)" "true"
chk "关系图显示前作" \
  "$($AB eval "document.querySelector('.relation-graph')?.innerText.includes('空之轨迹 FC')" 2>&1 | tail -1)" "true"
chk "综合页在抽屉出现" \
  "$($AB eval "document.querySelector('.ant-drawer-body')?.innerText.includes('利贝尔人物志')" 2>&1 | tail -1)" "true"

echo
echo "=== 结果：通过 $ok 项，失败 $bad 项 ==="
[ "$bad" = "0" ]

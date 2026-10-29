#!/bin/bash
# 节点管理验收：真 PG + 真 api/worker + 真界面。
# 覆盖：编辑标题/别名/标签 + 移动到新父节点 → 删除（归档）→ 取消归档（整棵子树恢复）。
# 隔离临时库，不碰现有数据。
set -euo pipefail
cd "$(dirname "$0")/.."

API_PORT=18090
WEB_PORT=15174
API="http://127.0.0.1:$API_PORT"
WEB="http://127.0.0.1:$WEB_PORT"
DB_NAME="wikiatlas_e2e_node_${RANDOM}_$$"
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
ev() { agent-browser eval "$1" 2>&1 | tail -1; }
pick() {
  agent-browser eval "[...document.querySelectorAll('.side-tree-scroll .ant-tree-treenode')].find(n=>n.innerText.includes('待改名作品'))?.querySelector('.ant-tree-node-content-wrapper')?.click()" >/dev/null
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
cd frontend
WIKIATLAS_WEB_PORT=$WEB_PORT WIKIATLAS_API_TARGET="$API" ./node_modules/.bin/vite --host 127.0.0.1 >"$TMP/web.log" 2>&1 & PIDS+=($!)
cd ..
curl -s --retry 60 --retry-connrefused --retry-delay 1 -o /dev/null "$WEB"

echo "=== 1. 两个根 + 一个子节点 ==="
A=$(curl -s -X POST "$API/api/nodes" -H 'Content-Type: application/json' \
  -d '{"kind":"series","title":"原父系列"}' | json_get 'd["id"]')
B=$(curl -s -X POST "$API/api/nodes" -H 'Content-Type: application/json' \
  -d '{"kind":"series","title":"新父系列"}' | json_get 'd["id"]')
NID=$(curl -s -X POST "$API/api/nodes" -H 'Content-Type: application/json' \
  -d "{\"parent_id\":\"$A\",\"kind\":\"work\",\"medium\":\"game\",\"title\":\"待改名作品\"}" | json_get 'd["id"]')
chk "子节点挂在原父下" "$(curl -s "$API/api/nodes/$NID" | json_get 'd["node"]["parent_id"]')" "$A"

echo "=== 2. 界面：编辑标题/别名/标签 + 移动到新父 ==="
AB=agent-browser
$AB set viewport 1440 1000 >/dev/null
$AB open "$WEB" >/dev/null
$AB wait 1500 >/dev/null
pick
$AB wait 1400 >/dev/null
$AB eval "(()=>{document.querySelector('.doc-actions button[aria-label=更多]')?.click();return 'ok'})()" >/dev/null
$AB wait 400 >/dev/null
$AB eval "(()=>{const m=[...document.querySelectorAll('.ant-dropdown-menu-item')].find(x=>x.innerText.includes('编辑节点'));m?.click();return 'ok'})()" >/dev/null
$AB wait 500 >/dev/null
chk "编辑框打开" "$(ev "[...document.querySelectorAll('.ant-modal')].some(m=>m.innerText.includes('编辑节点'))")" "true"
$AB fill ".node-edit input" "改名后的作品" >/dev/null
$AB fill ".node-edit textarea" "别名甲
别名乙" >/dev/null
$AB eval "(()=>{const m=[...document.querySelectorAll('.ant-modal')].find(x=>x.innerText.includes('编辑节点'));const t=m.querySelectorAll('textarea')[1];const set=Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set;set.call(t,'标签A\\n标签B');t.dispatchEvent(new Event('input',{bubbles:true}));return 'ok'})()" >/dev/null
$AB click ".ant-modal .ant-select" >/dev/null
$AB wait 500 >/dev/null
chk "父节点下拉已打开" "$(ev "[...document.querySelectorAll('.ant-select-dropdown')].some(d=>getComputedStyle(d).display!=='none'&&d.innerText.includes('新父系列'))")" "true"
$AB type ".ant-modal .ant-select input" "新父系列" >/dev/null
$AB wait 400 >/dev/null
$AB press Enter >/dev/null
$AB wait 300 >/dev/null
$AB eval "(()=>{const m=[...document.querySelectorAll('.ant-modal')].find(x=>x.innerText.includes('编辑节点'));m.querySelector('.ant-btn-primary')?.click();return 'ok'})()" >/dev/null
$AB wait 1500 >/dev/null
NODE=$(curl -s "$API/api/nodes/$NID")
chk "标题已改" "$(printf '%s' "$NODE" | json_get 'd["node"]["title"]')" "改名后的作品"
chk "别名已存" "$(printf '%s' "$NODE" | json_get '",".join(d["node"]["aliases"])')" "别名甲,别名乙"
chk "标签已存" "$(printf '%s' "$NODE" | json_get '",".join(d["node"]["tags"])')" "标签A,标签B"
chk "已移动到新父" "$(printf '%s' "$NODE" | json_get 'd["node"]["parent_id"]')" "$B"
chk "页面标题跟着变" "$(ev "document.querySelector('.doc-title')?.innerText")" '"改名后的作品"'

echo "=== 3. 删除 = 归档（整棵子树） ==="
$AB eval "(()=>{document.querySelector('.doc-actions button[aria-label=更多]')?.click();return 'ok'})()" >/dev/null
$AB wait 400 >/dev/null
$AB eval "(()=>{const m=[...document.querySelectorAll('.ant-dropdown-menu-item')].find(x=>x.innerText.includes('删除文章'));m?.click();return 'ok'})()" >/dev/null
$AB wait 500 >/dev/null
chk "确认框出现" "$(ev "[...document.querySelectorAll('.ant-modal-confirm')].some(x=>x.innerText.includes('删除'))")" "true"
$AB eval "(()=>{const b=[...document.querySelectorAll('.ant-modal-confirm button')].find(x=>x.innerText.replace(/\\s/g,'')==='删除');b?.click();return 'ok'})()" >/dev/null
$AB wait 1500 >/dev/null
chk "数据库状态 archived" "$(psql "$TEST_DB_URL" -qtA -c "select status from nodes where id='$NID'")" "archived"
chk "树里不再出现" "$(curl -s "$API/api/tree" | json_get "any(x['id']=='$NID' for x in d)")" "False"

echo "=== 4. 从左栏「已归档」找回来 → 取消归档 ==="
chk "左栏出现「已归档」分组" "$(ev "document.querySelector('.side-archived summary')?.innerText.includes('已归档')")" "true"
$AB eval "(()=>{document.querySelector('.side-archived summary')?.click();return 'ok'})()" >/dev/null
$AB wait 300 >/dev/null
$AB eval "(()=>{const r=[...document.querySelectorAll('.side-archived-row')].find(x=>x.innerText.includes('改名后的作品'));r?.click();return 'ok'})()" >/dev/null
$AB wait 1400 >/dev/null
$AB eval "(()=>{document.querySelector('.doc-actions button[aria-label=更多]')?.click();return 'ok'})()" >/dev/null
$AB wait 400 >/dev/null
chk "菜单出现取消归档" "$(ev "[...document.querySelectorAll('.ant-dropdown-menu-item')].some(x=>x.innerText.includes('取消归档'))")" "true"
$AB eval "(()=>{const m=[...document.querySelectorAll('.ant-dropdown-menu-item')].find(x=>x.innerText.includes('取消归档'));m?.click();return 'ok'})()" >/dev/null
$AB wait 500 >/dev/null
$AB eval "(()=>{const b=[...document.querySelectorAll('.ant-modal-confirm button')].find(x=>x.innerText.replace(/\\s/g,'')==='恢复');b?.click();return 'ok'})()" >/dev/null
$AB wait 1500 >/dev/null
chk "状态回到 draft" "$(psql "$TEST_DB_URL" -qtA -c "select status from nodes where id='$NID'")" "draft"
chk "树里重新出现" "$(curl -s "$API/api/tree" | json_get "any(x['id']=='$NID' for x in d)")" "True"
chk "父节点仍是新父" "$(psql "$TEST_DB_URL" -qtA -c "select parent_id from nodes where id='$NID'")" "$B"

echo
echo "=== 结果：通过 $ok 项，失败 $bad 项 ==="
[ "$bad" = "0" ]

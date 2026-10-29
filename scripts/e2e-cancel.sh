#!/bin/bash
# 取消运行验收：真 PG + 真 api/worker + 慢速 mock 模型。
#
# 覆盖：POST /api/runs 起工单 → GET 状态 running → POST /cancel 置标志 →
#       worker 在工具边界收尾成 cancelled → run.end(status=cancelled) → job done。
# 不碰现有库、不调真模型。
set -euo pipefail
cd "$(dirname "$0")/.."

API_PORT=18089
WEB_PORT=15175
MOCK_PORT=18099
API="http://127.0.0.1:$API_PORT"
WEB="http://127.0.0.1:$WEB_PORT"
DB_NAME="wikiatlas_e2e_cancel_${RANDOM}_$$"
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

MOCK_SLOW=1 python3 scripts/mock-llm.py "$MOCK_PORT" >"$TMP/mock.log" 2>&1 & PIDS+=($!)
sleep 0.5

cargo build -q -p wikiatlas-api -p wikiatlas-worker
./target/debug/wikiatlas-api >"$TMP/api.log" 2>&1 & PIDS+=($!)
curl -s --retry 60 --retry-connrefused --retry-delay 1 -o /dev/null "$API/api/healthz"
cd frontend
WIKIATLAS_WEB_PORT=$WEB_PORT WIKIATLAS_API_TARGET="$API" ./node_modules/.bin/vite --host 127.0.0.1 >"$TMP/web.log" 2>&1 & PIDS+=($!)
cd ..
curl -s --retry 60 --retry-connrefused --retry-delay 1 -o /dev/null "$WEB"
psql "$TEST_DB_URL" -q <<SQL
INSERT INTO secrets (key, value) VALUES
  ('llm.base_url', 'http://127.0.0.1:$MOCK_PORT'),
  ('llm.api_key', 'mock-not-real'),
  ('llm.model', 'mock-model')
ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value;
SQL
./target/debug/wikiatlas-worker >"$TMP/worker.log" 2>&1 & PIDS+=($!)

echo "=== 1. 起一条会持续几轮的工单 ==="
NID=$(curl -s -X POST "$API/api/nodes" -H 'Content-Type: application/json' \
  -d '{"parent_id":null,"kind":"work","medium":"game","title":"cancel-check·测试"}' \
  | json_get 'd["id"]')
RUN=$(curl -s -X POST "$API/api/runs" -H 'Content-Type: application/json' \
  -d "{\"node_id\":\"$NID\",\"prompt\":\"HARNESS_TEST_EDIT\",\"context\":{\"doc_mode\":\"edit\"}}")
RID=$(printf '%s' "$RUN" | json_get 'd["runId"]')
JID=$(printf '%s' "$RUN" | json_get 'd["jobId"]')
chk "工单已起" "$(psql "$TEST_DB_URL" -qtA -c "select count(*) from runs where id='$RID'")" "1"

echo "=== 2. 等它真的在跑（GET /api/runs/{id}）==="
running=0
for _ in $(seq 1 80); do
  st=$(curl -s "$API/api/runs/$RID" | json_get 'd["status"]')
  [ "$st" = "running" ] && { running=1; break; }
  sleep 0.2
done
chk "状态是 running" "$running" "1"
for _ in $(seq 1 50); do
  n=$(psql "$TEST_DB_URL" -qtA -c "select count(*) from run_events where run_id='$RID' and type='wikiatlas.tool.result'")
  [ "$n" -ge 1 ] && break
  sleep 0.2
done
chk "已经产生了工具结果（不是空跑）" "$([ "${n:-0}" -ge 1 ] && echo yes || echo no)" "yes"

echo "=== 3. 请求取消 ==="
CANCEL=$(curl -s -X POST "$API/api/runs/$RID/cancel")
chk "cancel 接口返回 cancel_requested" "$(printf '%s' "$CANCEL" | json_get 'd["cancel_requested"]')" "True"
chk "数据库里标志已置" "$(psql "$TEST_DB_URL" -qtA -c "select cancel_requested from runs where id='$RID'")" "t"
chk "有 cancel.requested 事件" "$(psql "$TEST_DB_URL" -qtA -c "select count(*) from run_events where run_id='$RID' and type='wikiatlas.cancel.requested'")" "1"

echo "=== 4. worker 在工具边界收尾 ==="
cancelled=0
for _ in $(seq 1 100); do
  st=$(curl -s "$API/api/runs/$RID" | json_get 'd["status"]')
  [ "$st" = "cancelled" ] && { cancelled=1; break; }
  sleep 0.2
done
chk "run 收尾为 cancelled" "$cancelled" "1"
chk "run.end 事件状态 cancelled" \
  "$(psql "$TEST_DB_URL" -qtA -c "select payload->>'status' from run_events where run_id='$RID' and type='wikiatlas.run.end' limit 1")" \
  "cancelled"
chk "job 已 done" "$(psql "$TEST_DB_URL" -qtA -c "select state from jobs where id=$JID")" "done"

echo "=== 5. 幂等：再取消一次不报错 ==="
AGAIN=$(curl -s -X POST "$API/api/runs/$RID/cancel")
chk "重复取消仍返回 cancelled" "$(printf '%s' "$AGAIN" | json_get 'd["status"]')" "cancelled"

echo "=== 6. 真界面：右栏「停止」 ==="
NID2=$(curl -s -X POST "$API/api/nodes" -H 'Content-Type: application/json' \
  -d '{"parent_id":null,"kind":"work","medium":"game","title":"cancel-ui·测试"}' \
  | json_get 'd["id"]')
AB=agent-browser
$AB set viewport 1440 1000 >/dev/null
$AB open "$WEB" >/dev/null
$AB wait 1500 >/dev/null
$AB eval "[...document.querySelectorAll('.side-tree-scroll .ant-tree-treenode')].find(n=>n.innerText.includes('cancel-ui·测试'))?.querySelector('.ant-tree-node-content-wrapper')?.click()" >/dev/null
$AB wait 1200 >/dev/null
$AB eval "[...document.querySelectorAll('button')].find(b=>b.innerText.includes('问 Atlas'))?.click()" >/dev/null
$AB wait 600 >/dev/null
$AB fill ".ai-foot textarea" "HARNESS_TEST_EDIT" >/dev/null
$AB eval "document.querySelector('.ai-composer-bar button.ai-send')?.click()" >/dev/null
stop=0
for _ in $(seq 1 60); do
  [ "$($AB eval "[...document.querySelectorAll('.ai-composer-bar button')].some(b=>b.innerText.includes('停止'))" 2>&1 | tail -1)" = "true" ] && { stop=1; break; }
  sleep 0.3
done
chk "右栏出现停止按钮" "$stop" "1"
$AB eval "(()=>{const b=[...document.querySelectorAll('.ai-composer-bar button')].find(x=>x.innerText.includes('停止'));b?.click();return 'ok'})()" >/dev/null
ui_cancelled=0
for _ in $(seq 1 100); do
  st=$(psql "$TEST_DB_URL" -qtA -c "select status from runs where checkpoint->>'node_id'='$NID2' order by started_at desc limit 1")
  [ "$st" = "cancelled" ] && { ui_cancelled=1; break; }
  sleep 0.2
done
chk "UI 发起的 run 也取消成功" "$ui_cancelled" "1"
chk "UI 发起的 run 有 run.end cancelled" \
  "$(psql "$TEST_DB_URL" -qtA -c "select payload->>'status' from run_events where run_id=(select id from runs where checkpoint->>'node_id'='$NID2' order by started_at desc limit 1) and type='wikiatlas.run.end' limit 1")" \
  "cancelled"

echo
echo "=== 结果：通过 $ok 项，失败 $bad 项 ==="
[ "$bad" = "0" ]

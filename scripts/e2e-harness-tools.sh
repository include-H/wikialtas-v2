#!/bin/bash
# Harness 工具面验收：免费 mock 模型 + 真 PG/API/worker。
# 覆盖：编辑模式真执行 todo/upsert_work/edit/answer；只读模式即使误调 edit 也被动作层拒绝。
set -euo pipefail
cd "$(dirname "$0")/.."

API_PORT=18188
MOCK_PORT=18199
API="http://127.0.0.1:$API_PORT"
DB_NAME="wikiatlas_e2e_tools_${RANDOM}_$$"
TMP=$(mktemp -d)
PIDS=()

cleanup() {
  local code=$?
  if [ "$code" != "0" ]; then
    echo "--- api.log ---"; tail -n 80 "$TMP/api.log" 2>/dev/null || true
    echo "--- worker.log ---"; tail -n 80 "$TMP/worker.log" 2>/dev/null || true
    echo "--- mock.log ---"; tail -n 40 "$TMP/mock.log" 2>/dev/null || true
  fi
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
TEST_URL=$(python3 - "$ADMIN_DB_URL" "$DB_NAME" <<'PY'
import sys, urllib.parse
u = urllib.parse.urlsplit(sys.argv[1])
print(urllib.parse.urlunsplit((u.scheme, u.netloc, "/" + sys.argv[2], u.query, u.fragment)))
PY
)
export WIKIATLAS_DB_URL="$TEST_URL" DATABASE_URL="$TEST_URL"
export WIKIATLAS_BIND="127.0.0.1:$API_PORT"

python3 scripts/mock-llm.py "$MOCK_PORT" >"$TMP/mock.log" 2>&1 & PIDS+=($!)
sleep 0.5

cargo build -q -p wikiatlas-api -p wikiatlas-worker
./target/debug/wikiatlas-api >"$TMP/api.log" 2>&1 & PIDS+=($!)
curl -s --retry 60 --retry-connrefused --retry-delay 1 -o /dev/null "$API/api/healthz"

psql "$TEST_URL" -q <<SQL
INSERT INTO secrets (key,value) VALUES
  ('llm.base_url','http://127.0.0.1:$MOCK_PORT'),
  ('llm.api_key','mock'),
  ('llm.model','mock')
ON CONFLICT (key) DO UPDATE SET value=EXCLUDED.value;
SQL
./target/debug/wikiatlas-worker >"$TMP/worker.log" 2>&1 & PIDS+=($!)

echo "=== 1. 编辑模式：todo → 建节点 → edit → answer ==="
NODE=$(curl -s -X POST "$API/api/nodes" -H 'Content-Type: application/json' \
  -d '{"parent_id":null,"kind":"work","medium":"game","title":"Harness 工具测试"}')
NID=$(printf '%s' "$NODE" | json_get 'd["id"]')
BODY='{"body_md":"# Harness 工具测试\n\n青羽测试。","base_ver":0}'
curl -s -X PUT "$API/api/nodes/$NID/draft" -H 'Content-Type: application/json' -d "$BODY" >/dev/null
curl -s -X POST "$API/api/nodes/$NID/commit" -H 'Content-Type: application/json' -d "$BODY" >/dev/null
RUN1=$(curl -s -X POST "$API/api/runs" -H 'Content-Type: application/json' \
  -d "{\"node_id\":\"$NID\",\"prompt\":\"HARNESS_TEST_EDIT\",\"context\":{\"doc_mode\":\"edit\"}}")
RUN1_ID=$(printf '%s' "$RUN1" | json_get 'd["runId"]')
curl -s -N --max-time 30 "$API/api/runs/$RUN1_ID/events" >"$TMP/run1.sse" || true
chk "run1 完成" "$(psql "$TEST_URL" -qtA -c "select status from runs where id='$RUN1_ID'")" "completed"
chk "edit 真落草稿" \
  "$(psql "$TEST_URL" -qtA -c "select case when position('青羽工具验收' in body_md)>0 then 'yes' else 'no' end from content_drafts where node_id='$NID'")" "yes"
chk "upsert_work 真建节点" \
  "$(psql "$TEST_URL" -qtA -c "select count(*) from nodes where title='工具创建的节点'")" "1"
chk "todo/upsert/edit/answer 工具都执行成功" \
  "$(psql "$TEST_URL" -qtA -c "select count(*) from run_events where run_id='$RUN1_ID' and type='wikiatlas.tool.result' and payload->>'ok'='true'")" "4"

echo "=== 2. 只读模式：模型误调 edit 也必须被拒 ==="
BEFORE=$(psql "$TEST_URL" -qtA -c "select body_md from content_drafts where node_id='$NID'")
RUN2=$(curl -s -X POST "$API/api/runs" -H 'Content-Type: application/json' \
  -d "{\"node_id\":\"$NID\",\"prompt\":\"HARNESS_TEST_READ\",\"context\":{\"doc_mode\":\"read\"}}")
RUN2_ID=$(printf '%s' "$RUN2" | json_get 'd["runId"]')
curl -s -N --max-time 30 "$API/api/runs/$RUN2_ID/events" >"$TMP/run2.sse" || true
chk "read run 完成" "$(psql "$TEST_URL" -qtA -c "select status from runs where id='$RUN2_ID'")" "completed"
chk "动作层拒绝 edit" \
  "$(psql "$TEST_URL" -qtA -c "select count(*) from run_events where run_id='$RUN2_ID' and type='wikiatlas.tool.result' and payload->>'name'='edit' and payload->>'ok'='false'")" "1"
AFTER=$(psql "$TEST_URL" -qtA -c "select body_md from content_drafts where node_id='$NID'")
chk "只读模式正文未变" "$( [ "$BEFORE" = "$AFTER" ] && echo yes || echo no )" "yes"

echo "=== 3. 第三轮：从持久会话恢复前两轮上下文 ==="
RUN3=$(curl -s -X POST "$API/api/runs" -H 'Content-Type: application/json' \
  -d "{\"node_id\":\"$NID\",\"prompt\":\"HARNESS_TEST_MEMORY\",\"context\":{\"doc_mode\":\"read\"}}")
RUN3_ID=$(printf '%s' "$RUN3" | json_get 'd["runId"]')
SESSION3=$(printf '%s' "$RUN3" | json_get 'd["sessionId"]')
curl -s -N --max-time 30 "$API/api/runs/$RUN3_ID/events" >"$TMP/run3.sse" || true
chk "第三轮完成" "$(psql "$TEST_URL" -qtA -c "select status from runs where id='$RUN3_ID'")" "completed"
chk "运行绑定同一个默认会话" "$SESSION3" "work:$NID"
chk "模型确实收到上一轮上下文" \
  "$(psql "$TEST_URL" -qtA -c "select count(*) from run_events where run_id='$RUN3_ID' and type='wikiatlas.answer' and payload->>'text' like '%记得上一轮%'")" "1"
chk "会话列表有消息" \
  "$(curl -s "$API/api/sessions?node_id=$NID" | json_get 'd[0]["message_count"] >= 5')" "True"

echo "=== 4. 显式加载 Skill：正文注入 + 联网工具常开 ==="
chk "初始发现只暴露一个总入口" \
  "$(curl -s "$API/api/skills" | json_get 'len(d["skills"]) == 1 and d["skills"][0]["name"] == "atlas"')" "True"
RUN4=$(curl -s -X POST "$API/api/runs" -H 'Content-Type: application/json' \
  -d "{\"node_id\":\"$NID\",\"prompt\":\"HARNESS_TEST_SKILL\",\"context\":{\"doc_mode\":\"edit\",\"skills\":[\"atlas\"]}}")
RUN4_ID=$(printf '%s' "$RUN4" | json_get 'd["runId"]')
SESSION4=$(printf '%s' "$RUN4" | json_get 'd["sessionId"]')
chk "显式 Skill 请求被接受" "$(printf '%s' "$SESSION4" | cut -d: -f1)" "work"
curl -s -N --max-time 30 "$API/api/runs/$RUN4_ID/events" >"$TMP/run4.sse" || true
chk "Skill 加载事件已广播" \
  "$(psql "$TEST_URL" -qtA -c "select count(*) from run_events where run_id='$RUN4_ID' and type='wikiatlas.skill.loaded' and payload->'skills' ? 'atlas'")" "1"
chk "总入口注入、子模块按需读到、联网工具常开" \
  "$(psql "$TEST_URL" -qtA -c "select count(*) from run_events where run_id='$RUN4_ID' and type='wikiatlas.answer' and payload->>'text' = '总入口已加载，子模块可按需读取，联网工具始终可用。'")" "1"

echo
echo "=== 结果：通过 $ok 项，失败 $bad 项 ==="
[ "$bad" = "0" ]

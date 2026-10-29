#!/bin/bash
# M9 验收：MCP 只读检索 + 设置/用量 + 项目导出导入 + pg_dump 恢复。
set -euo pipefail
cd "$(dirname "$0")/.."

API_PORT=18088
API2_PORT=18089
WEB_PORT=15173
API="http://127.0.0.1:$API_PORT"
API2="http://127.0.0.1:$API2_PORT"
WEB="http://127.0.0.1:$WEB_PORT"
DB1="wikiatlas_e2e_ops_${RANDOM}_$$"
DB2="wikiatlas_e2e_ops_import_${RANDOM}_$$"
DB3="wikiatlas_e2e_ops_restore_${RANDOM}_$$"
TMP=$(mktemp -d)
PIDS=()

cleanup() {
  for pid in "${PIDS[@]:-}"; do kill "$pid" 2>/dev/null || true; done
  sleep 0.3
  for db in "$DB1" "$DB2" "$DB3"; do
    psql "$ADMIN_DB_URL" -q -c "DROP DATABASE IF EXISTS $db WITH (FORCE)" >/dev/null 2>&1 || true
  done
  rm -rf "$TMP"
}
trap cleanup EXIT

ok=0; bad=0
chk() {
  if [ "$2" = "$3" ]; then ok=$((ok+1)); echo "  ✓ $1";
  else bad=$((bad+1)); echo "  ✗ $1  得到[$2] 期望[$3]"; fi
}
json_get() { python3 -c "import sys,json; d=json.load(sys.stdin); print($1)"; }
db_url() {
  python3 - "$ADMIN_DB_URL" "$1" <<'PY'
import sys, urllib.parse
u = urllib.parse.urlsplit(sys.argv[1])
print(urllib.parse.urlunsplit((u.scheme, u.netloc, "/" + sys.argv[2], u.query, u.fragment)))
PY
}

echo "=== 0. 隔离环境 ==="
set -a; . .secrets/wikiatlas.pg.env; set +a
ADMIN_DB_URL="$WIKIATLAS_DB_URL"; export ADMIN_DB_URL
for db in "$DB1" "$DB2" "$DB3"; do
  psql "$ADMIN_DB_URL" -q -c "CREATE DATABASE $db" >/dev/null
done
DB1_URL=$(db_url "$DB1")
DB2_URL=$(db_url "$DB2")
DB3_URL=$(db_url "$DB3")

export DATABASE_URL="$DB1_URL" WIKIATLAS_DB_URL="$DB1_URL"
export WIKIATLAS_BIND="127.0.0.1:$API_PORT"
cargo build -q -p wikiatlas-api -p wikiatlas-mcp
./target/debug/wikiatlas-api >"$TMP/api.log" 2>&1 & PIDS+=($!)
curl -s --retry 60 --retry-connrefused --retry-delay 1 -o /dev/null "$API/api/healthz"

echo "=== 1. 造一篇可检索内容 + 用量/密钥 ==="
NODE=$(curl -s -X POST "$API/api/nodes" -H 'Content-Type: application/json' \
  -d '{"parent_id":null,"kind":"work","medium":"game","title":"MCP 测试作品"}')
NID=$(printf '%s' "$NODE" | json_get 'd["id"]')
BODY='{"body_md":"# MCP 测试作品\n\n青羽MCP 是给外部 agent 检索引用的独有事实。","base_ver":0}'
curl -s -X PUT "$API/api/nodes/$NID/draft" -H 'Content-Type: application/json' -d "$BODY" >/dev/null
curl -s -X POST "$API/api/nodes/$NID/commit" -H 'Content-Type: application/json' -d "$BODY" >/dev/null
curl -s -X PUT "$API/api/settings" -H 'Content-Type: application/json' \
  -d '{"llm.price_input_per_million":2,"llm.price_output_per_million":4,"llm.model":"ops-model"}' >/dev/null
curl -s -X PUT "$API/api/secrets" -H 'Content-Type: application/json' \
  -d '{"llm.api_key":"super-secret-value"}' >/dev/null
psql "$DB1_URL" -q <<'SQL'
INSERT INTO runs (id, intent, goal, status, model, started_at, last_active)
VALUES ('run:ops', '问答', 'test', 'completed', 'ops-model', now(), now());
INSERT INTO run_events (id, run_id, seq, type, payload, created_at)
VALUES ('evt:ops', 'run:ops', 0, 'response.completed',
        '{"usage":{"input_tokens":1000000,"output_tokens":500000}}', now());
SQL
SETTINGS=$(curl -s "$API/api/settings")
chk "设置页不回显密钥" \
  "$(python3 -c 'import json,sys; print("super-secret-value" in json.dumps(json.load(sys.stdin)))' <<<"$SETTINGS")" "False"
chk "密钥只报已配置" "$(printf '%s' "$SETTINGS" | json_get 'd["secrets_configured"]["llm.api_key"]')" "True"
chk "用量 token 正确" "$(curl -s "$API/api/usage" | json_get 'd["total_tokens"]')" "1500000"
chk "成本可见" "$(curl -s "$API/api/usage" | json_get 'd["estimated_cost"]')" "4.0"

echo "=== 2. MCP stdio 只读检索 ==="
MCP_OUT=$(WIKIATLAS_DB_URL="$DB1_URL" ./target/debug/wikiatlas-mcp <<'EOF'
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search_works","arguments":{"query":"青羽MCP"}}}
EOF
)
MCP_LINES=$(printf '%s\n' "$MCP_OUT" | wc -l)
chk "MCP 只回三个请求（通知不占行）" "$MCP_LINES" "3"
chk "MCP 工具清单没有写工具" \
  "$(printf '%s' "$MCP_OUT" | sed -n '2p' | python3 -c 'import sys,json; d=json.load(sys.stdin); print(any(x["name"] in ("write_wiki","write_content") for x in d["result"]["tools"]))')" "False"
chk "MCP 检索命中正文事实" \
  "$(printf '%s' "$MCP_OUT" | sed -n '3p' | python3 -c 'import sys,json,re; d=json.load(sys.stdin); text=d["result"]["content"][0]["text"]; print("青羽MCP" in text)')" "True"

echo "=== 3. 导出 → 空库导入 ==="
curl -s "$API/api/export" >"$TMP/project.json"
WIKIATLAS_DB_URL="$DB2_URL" DATABASE_URL="$DB2_URL" WIKIATLAS_BIND="127.0.0.1:$API2_PORT" \
  ./target/debug/wikiatlas-api >"$TMP/api2.log" 2>&1 & PIDS+=($!)
curl -s --retry 60 --retry-connrefused --retry-delay 1 -o /dev/null "$API2/api/healthz"
IMP=$(curl -s -X POST "$API2/api/import" -H 'Content-Type: application/json' --data-binary @"$TMP/project.json")
chk "导入一个节点" "$(printf '%s' "$IMP" | json_get 'd["nodes"]')" "1"
chk "导入后正文还在" "$(curl -s "$API2/api/nodes/$NID" | json_get '"青羽MCP" in d["content"]["body_md"]')" "True"
chk "导入不包含密钥" \
  "$(python3 -c 'import json; print("super-secret-value" in open("'"$TMP"'/project.json").read())')" "False"

echo "=== 4. pg_dump → pg_restore ==="
pg_dump "$DB1_URL" -Fc -f "$TMP/db.dump"
pg_restore -d "$DB3_URL" --no-owner "$TMP/db.dump"
chk "恢复库节点在" "$(psql "$DB3_URL" -qtA -c "select count(*) from nodes where id='$NID'")" "1"
chk "恢复库正文在" \
  "$(psql "$DB3_URL" -qtA -c "select case when position('青羽MCP' in body_md)>0 then 'yes' else 'no' end from contents where node_id='$NID'")" "yes"

echo "=== 5. 设置页 UI ==="
cd frontend
WIKIATLAS_WEB_PORT=$WEB_PORT WIKIATLAS_API_TARGET="$API" ./node_modules/.bin/vite --host 127.0.0.1 >"$TMP/web.log" 2>&1 &
PIDS+=($!)
cd ..
curl -s --retry 60 --retry-connrefused --retry-delay 1 -o /dev/null "$WEB"
AB=agent-browser
$AB set viewport 1440 1000 >/dev/null
$AB open "$WEB" >/dev/null
$AB wait 900 >/dev/null
$AB click ".side-links .side-link:nth-child(5)" >/dev/null
$AB wait 700 >/dev/null
chk "设置页打开" \
  "$($AB eval "document.querySelector('.settings-head h1')?.innerText" 2>&1 | tail -1)" '"设置与用量"'
chk "模型用量可见" \
  "$($AB eval "document.querySelector('.usage-cards')?.innerText.includes('1,500,000')" 2>&1 | tail -1)" "true"
chk "密钥显示已配置不回显" \
  "$($AB eval "(()=>{const t=document.querySelector('.settings-section:nth-of-type(2)')?.innerText||'';return t.includes('已配置') && !t.includes('super-secret-value')})()" 2>&1 | tail -1)" "true"

echo "=== 6. 可选访问密码 ==="
curl -s -X PUT "$API/api/secrets" -H 'Content-Type: application/json' \
  -d '{"access.password":"e2e-access"}' >/dev/null
chk "未登录 API 被挡" \
  "$(curl -s -o /dev/null -w '%{http_code}' "$API/api/tree")" "401"
curl -s -c "$TMP/cookie" -X POST "$API/api/login" -H 'Content-Type: application/json' \
  -d '{"password":"e2e-access"}' >/dev/null
chk "登录后可读 API" \
  "$(curl -s -b "$TMP/cookie" -o /dev/null -w '%{http_code}' "$API/api/tree")" "200"

echo
echo "=== 结果：通过 $ok 项，失败 $bad 项 ==="
[ "$bad" = "0" ]

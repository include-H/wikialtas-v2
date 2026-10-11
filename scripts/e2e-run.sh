#!/bin/bash
# M3 验收：**真跑一次 run** —— 流式正文投影 + 一个写工具 → SSE 到浏览器 → 草稿落库。
#
# ⚠️ 这个脚本会**调真模型**（花钱）。只在已配 `secrets: llm.base_url / llm.api_key` 时跑；
#    没配就直接跳过（不是失败）。
#
# 前置：api 8088、worker、前端 5173、agent-browser。
# 用法：bash scripts/e2e-run.sh
set -e
cd "$(dirname "$0")/.."

AB=agent-browser
API=http://127.0.0.1:8088
WEB=http://127.0.0.1:5173
PW=$(grep -oP '(?<=wikiatlas:)[^@]+' .secrets/wikiatlas.pg.env)
export PGPASSWORD="$PW"
PSQL="psql -h 127.0.0.1 -U wikiatlas -d wikiatlas -qtA"

ok=0; bad=0
gt0() { if [ "$1" -gt 0 ] 2>/dev/null; then echo yes; else echo no; fi; }
chk() { if [ "$2" = "$3" ]; then ok=$((ok+1)); echo "  ✓ $1"; else bad=$((bad+1)); echo "  ✗ $1  得到[$2] 期望[$3]"; fi; }

curl -s --retry 30 --retry-connrefused --retry-delay 1 -o /dev/null "$API/api/healthz"

echo "=== 0. 有没有配模型 ==="
if [ "$($PSQL -c "select count(*) from secrets where key='llm.api_key' and value <> ''")" != "1" ]; then
  echo "  没配 llm.api_key —— 跳过（真模型调用必须先配）"
  exit 0
fi
chk "模型已配" "yes" "yes"

echo "=== 1. 备一个干净节点 ==="
$PSQL -q -c "delete from nodes where title = 'e2e-run·测试节点'" >/dev/null 2>&1 || true
NID=$(curl -s -X POST "$API/api/nodes" -H 'Content-Type: application/json' \
  -d '{"kind":"work","medium":"game","title":"e2e-run·测试节点"}' | python3 -c "import sys,json;print(json.load(sys.stdin)['id'])")
chk "节点建好" "$($PSQL -c "select count(*) from nodes where id='$NID'")" "1"

echo "=== 2. 真界面：在 Altas 里发一句话 ==="
$AB set viewport 1440 1000 >/dev/null
$AB open "$WEB" >/dev/null
$AB wait 1500 >/dev/null
$AB eval "[...document.querySelectorAll('.side-tree-scroll .ant-tree-treenode')].find(n=>n.innerText.includes('e2e-run·测试节点'))?.querySelector('.ant-tree-node-content-wrapper')?.click()" >/dev/null
$AB wait 1200 >/dev/null
# 右栏默认收起：点「问 Altas」展开
$AB eval "[...document.querySelectorAll('.top-bar-actions button')].find(b=>b.innerText.includes('问 Altas'))?.click()" >/dev/null
$AB wait 800 >/dev/null
chk "Altas 面板开了" "$($AB eval "document.querySelectorAll('.ai-panel').length" 2>&1 | tail -1)" "1"
$AB fill ".ai-foot textarea" "用 ## 1. 作品概览 起头，写 80 字介绍《空之轨迹 FC》。" >/dev/null
$AB eval "document.querySelector('.ai-foot-actions button').click()" >/dev/null
$AB wait 3000 >/dev/null

echo "=== 3. 等 run 收工（最多 90s） ==="
done_run=0
for _ in $(seq 1 45); do
  st=$($PSQL -c "select status from runs order by started_at desc limit 1")
  case "$st" in completed) done_run=1; break;; failed) break;; esac
  sleep 2
done
chk "run 跑完（completed）" "$done_run" "1"

echo "=== 4. 回服务端核对：事件 + 草稿 ==="
RID=$($PSQL -c "select id from runs order by started_at desc limit 1")
chk "有 content.staging 事件（开写就报）" "$($PSQL -c "select count(*) > 0 from run_events where run_id='$RID' and type='wikiatlas.content.staging'")" "t"
chk "有 content.delta（流式正文投影）" "$(gt0 "$($PSQL -c "select count(*) from run_events where run_id='$RID' and type='wikiatlas.content.delta'")")" "yes"
chk "写工具执行成功" "$(gt0 "$($PSQL -c "select count(*) from run_events where run_id='$RID' and type='wikiatlas.tool.result' and payload->>'ok'='true'")")" "yes"
chk "草稿真的落了库" "$($PSQL -c "select count(*) from content_drafts where node_id='$NID' and length(body_md) > 20")" "1"
chk "草稿记为 llm 写的" "$($PSQL -c "select \"by\" like 'llm:%' from content_drafts where node_id='$NID'")" "t"
chk "会话日志 append-only（user + assistant）" "$($PSQL -c "select count(*) >= 2 from session_messages where session_id='work:$NID'")" "t"
chk "浏览器里看到了流式正文" "$($AB eval "(document.querySelector('.ai-live-body')?.textContent||'').length > 20 ? 'yes':'no'" 2>&1 | tail -1)" '"yes"'

echo
echo "=== 结果：通过 $ok 项，失败 $bad 项 ==="
[ "$bad" = "0" ]

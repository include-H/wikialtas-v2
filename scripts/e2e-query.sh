#!/bin/bash
# M4.5 验收：**真跑一次「问答」工单**（D69）——只读通用 harness：查库 → 读内容 → answer（带引用）。
#
# ⚠️ 这个脚本会**调真模型**（花钱）。没配 `secrets: llm.base_url / llm.api_key` 就跳过（不是失败）。
# ⚠️ **绝不 TRUNCATE**：只建/删自己命名的测试节点（`e2e-query·测试作品`）。
#
# 前置：api 8088、**worker**、前端 5173、agent-browser。
# 用法：bash scripts/e2e-query.sh
set -e
cd "$(dirname "$0")/.."

AB=agent-browser
API=http://127.0.0.1:8088
WEB=http://127.0.0.1:5173
TITLE='e2e-query·测试作品'
PW=$(grep -oP '(?<=wikiatlas:)[^@]+' .secrets/wikiatlas.pg.env)
export PGPASSWORD="$PW"
PSQL="psql -h 127.0.0.1 -U wikiatlas -d wikiatlas -qtA"

ok=0; bad=0
chk() { if [ "$2" = "$3" ]; then ok=$((ok+1)); echo "  ✓ $1"; else bad=$((bad+1)); echo "  ✗ $1  得到[$2] 期望[$3]"; fi; }
gt0() { if [ "$1" -gt 0 ] 2>/dev/null; then echo yes; else echo no; fi; }
ev() { $AB eval "$1" 2>&1 | tail -1; }

curl -s --retry 30 --retry-connrefused --retry-delay 1 -o /dev/null "$API/api/healthz"

echo "=== 0. 有没有配模型 ==="
if [ "$($PSQL -c "select count(*) from secrets where key='llm.api_key' and value <> ''")" != "1" ]; then
  echo "  没配 llm.api_key —— 跳过（真模型调用必须先配）"
  exit 0
fi

echo "=== 1. 备一个自己的测试节点（含一段只有库里有的事实）==="
$PSQL -q -c "delete from nodes where parent_id in (select id from nodes where title='$TITLE')" >/dev/null 2>&1 || true
$PSQL -q -c "delete from nodes where title='$TITLE'" >/dev/null 2>&1 || true
NID=$(curl -s -X POST "$API/api/nodes" -H 'Content-Type: application/json' \
  -d "{\"kind\":\"work\",\"medium\":\"game\",\"title\":\"$TITLE\"}" \
  | python3 -c "import sys,json;print(json.load(sys.stdin)['id'])")
chk "节点建好" "$($PSQL -c "select count(*) from nodes where id='$NID'")" "1"

BODY=$(python3 - <<'PY'
import json
body = """# e2e-query·测试作品（游戏）Wiki

> 说明：验收用测试条目。

## 1. 作品概览

本作的主角叫「青羽测试」，搭档叫「灰羽测试」。

## 2. 基础信息速览

- 首发日期：2026-10-13
- 开发：验收工作室
"""
print(json.dumps({"body_md": body, "base_ver": 0, "summary": "e2e-query 铺垫"}))
PY
)
curl -s -o /dev/null -X POST "$API/api/nodes/$NID/commit" -H 'Content-Type: application/json' -d "$BODY"
chk "定稿落库" "$($PSQL -c "select count(*) from contents where node_id='$NID'")" "1"

echo "=== 2. 真界面：选节点 → Atlas 里直接提问（没有写/问开关）==="
$AB set viewport 1440 1000 >/dev/null
$AB open "$WEB" >/dev/null
$AB wait 1500 >/dev/null
$AB eval "[...document.querySelectorAll('.side-tree-scroll .ant-tree-treenode')].find(n=>n.innerText.includes('$TITLE'))?.querySelector('.ant-tree-node-content-wrapper')?.click()" >/dev/null
$AB wait 1200 >/dev/null
$AB eval "[...document.querySelectorAll('.top-bar-actions button')].find(b=>b.innerText.includes('问 Atlas'))?.click()" >/dev/null
$AB wait 800 >/dev/null
chk "Atlas 面板开了" "$(ev "document.querySelectorAll('.ai-panel').length")" "1"
chk "没有「写/问」模式开关（通用 harness）" "$(ev "document.querySelectorAll('.ai-head .ant-segmented-item').length")" "0"
$AB fill ".ai-foot textarea" "这部作品的主角叫什么？只依据库里内容回答。" >/dev/null
$AB eval "document.querySelector('.ai-foot-actions button').click()" >/dev/null

echo "=== 3. 等 run 收工（最多 150s） ==="
done_run=0
for _ in $(seq 1 75); do
  st=$($PSQL -c "select status from runs order by started_at desc limit 1")
  case "$st" in completed) done_run=1; break;; failed|interrupted) break;; esac
  sleep 2
done
chk "run 跑完（completed）" "$done_run" "1"

echo "=== 4. 回服务端核对 ==="
RID=$($PSQL -c "select id from runs order by started_at desc limit 1")
chk "新开了一条工单" "$(gt0 "$($PSQL -c "select count(*) from runs")")" "yes"
chk "有 answer 事件" "$(gt0 "$($PSQL -c "select count(*) from run_events where run_id='$RID' and type='wikiatlas.answer'")")" "yes"
chk "回答提到了库里那句事实（青羽测试）" "$($PSQL -c "select case when payload->>'text' like '%青羽测试%' then 'yes' else 'no' end from run_events where run_id='$RID' and type='wikiatlas.answer' order by seq desc limit 1")" "yes"
chk "读了节点内容（read_node / read_doc 被调过）" "$(gt0 "$($PSQL -c "select count(*) from run_events where run_id='$RID' and type='wikiatlas.tool.result' and payload->>'name' in ('read_node','read_doc')")")" "yes"
chk "回答带可点引用（refs 非空）" "$(gt0 "$($PSQL -c "select jsonb_array_length(payload->'refs') from run_events where run_id='$RID' and type='wikiatlas.answer' order by seq desc limit 1")")" "yes"
chk "没有写工具被执行（只读）" "$($PSQL -c "select count(*) from run_events where run_id='$RID' and type='wikiatlas.tool.result' and payload->>'name' in ('write_content','patch_section','ledger_put')")" "0"
chk "没落草稿（问答不改正文）" "$($PSQL -c "select count(*) from content_drafts where node_id='$NID'")" "0"
chk "会话日志记了这轮问答" "$($PSQL -c "select count(*) >= 2 from session_messages where session_id='work:$NID'")" "t"

echo "=== 5. 浏览器里回答渲染出来了 ==="
$AB wait 1000 >/dev/null
chk "页面上有回答卡片" "$(ev "document.querySelectorAll('.ai-answer').length")" "1"
chk "回答里是库里那句事实" "$(ev "(document.querySelector('.ai-answer')?.innerText||'').includes('青羽测试')")" "true"
chk "有可点引用按钮" "$(gt0 "$(ev "document.querySelectorAll('.ai-refs button').length")")" "yes"

echo
echo "=== 结果：通过 $ok 项，失败 $bad 项 ==="
[ "$bad" = "0" ]

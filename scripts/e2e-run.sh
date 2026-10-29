#!/bin/bash
# M4.5 通用 harness 冒烟（**读路径，无浏览器**）：一个循环 + 一整套工具面。
# 用户问一句话 → Atlas 用读工具查库 → `answer`（带 refs）；**不动正文**。
#
# ⚠️ 会调**真模型**（花钱）。没配 `secrets: llm.api_key` 就跳过（不算失败）。
# 前置：api 8088、worker。
# 用法：bash scripts/e2e-run.sh
set -e
cd "$(dirname "$0")/.."

API=http://127.0.0.1:8088
TITLE='e2e-run·测试作品'
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

echo "=== 1. 备一个自己的测试节点（含一段只有库里有的事实）==="
$PSQL -q -c "delete from nodes where parent_id in (select id from nodes where title='$TITLE')" >/dev/null 2>&1 || true
$PSQL -q -c "delete from nodes where title='$TITLE'" >/dev/null 2>&1 || true
NID=$(curl -s -X POST "$API/api/nodes" -H 'Content-Type: application/json' \
  -d "{\"kind\":\"work\",\"medium\":\"game\",\"title\":\"$TITLE\"}" \
  | python3 -c "import sys,json;print(json.load(sys.stdin)['id'])")
BODY=$(python3 - <<'PY'
import json
print(json.dumps({"body_md": "# e2e-run·测试作品（游戏）Wiki\n\n> 说明：冒烟用。\n\n## 1. 作品概览\n\n本作的主舞台是「雾港测试镇」。\n", "base_ver": 0, "summary": "e2e-run 铺垫"}))
PY
)
curl -s -o /dev/null -X POST "$API/api/nodes/$NID/commit" -H 'Content-Type: application/json' -d "$BODY"

echo "=== 2. API 起工单（问一句，只该走读工具）==="
RID=$(curl -s -X POST "$API/api/runs" -H 'Content-Type: application/json' \
  -d "{\"node_id\":\"$NID\",\"prompt\":\"这部作品的主舞台叫什么？只依据库里内容回答。\"}" \
  | python3 -c "import sys,json;print(json.load(sys.stdin)['runId'])")
chk "工单已起" "$($PSQL -c "select count(*) from runs where id='$RID'")" "1"

echo "=== 3. 等收工（最多 120s）==="
done_run=0
for _ in $(seq 1 60); do
  st=$($PSQL -c "select status from runs where id='$RID'")
  case "$st" in completed) done_run=1; break;; failed|interrupted) break;; esac
  sleep 2
done
chk "run 跑完（completed）" "$done_run" "1"

echo "=== 4. 回服务端核对 ==="
chk "有 answer 事件" "$(gt0 "$($PSQL -c "select count(*) from run_events where run_id='$RID' and type='wikiatlas.answer'")")" "yes"
chk "回答引用了库里那句事实" "$($PSQL -c "select case when payload->>'text' like '%雾港测试镇%' then 'yes' else 'no' end from run_events where run_id='$RID' and type='wikiatlas.answer' order by seq desc limit 1")" "yes"
chk "读了节点内容（read_node/read_doc）" "$(gt0 "$($PSQL -c "select count(*) from run_events where run_id='$RID' and type='wikiatlas.tool.result' and payload->>'name' in ('read_node','read_doc')")")" "yes"
chk "没执行写工具" "$($PSQL -c "select count(*) from run_events where run_id='$RID' and type='wikiatlas.tool.result' and payload->>'name' in ('write_wiki','write_content','patch_section','ledger_put')")" "0"
chk "没落草稿" "$($PSQL -c "select count(*) from content_drafts where node_id='$NID'")" "0"
chk "run.end 事件在" "$(gt0 "$($PSQL -c "select count(*) from run_events where run_id='$RID' and type='wikiatlas.run.end'")")" "yes"

echo
echo "=== 结果：通过 $ok 项，失败 $bad 项 ==="
[ "$bad" = "0" ]

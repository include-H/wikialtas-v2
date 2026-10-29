#!/bin/bash
# M4 验收（**规则档**，`docs/design.md §4`）：空库跑一篇完整正文 → 过质量门 → 定稿 → 快照。
#
# 这是三段式管线（研究 → 骨架 → 生成）的端到端真跑。
#
# ⚠️ 这个脚本会**调真模型且会联网检索**（花钱）。只在已配 `secrets: llm.api_key / exa.api_key`
#    时跑；没配就直接跳过（不算失败）。**跑之前请先确认。**
#
# 前置：api 8088、worker、前端 5173（浏览器检查需要）、agent-browser。
# 用法：bash scripts/e2e-write.sh
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
# 断言含某段文本（grep -q）
has() { if echo "$3" | grep -q "$2"; then ok=$((ok+1)); echo "  ✓ $1"; else bad=$((bad+1)); echo "  ✗ $1（没找到 [$2]）"; fi; }

curl -s --retry 30 --retry-connrefused --retry-delay 1 -o /dev/null "$API/api/healthz"

echo "=== 0. 有没有配模型 ==="
if [ "$($PSQL -c "select count(*) from secrets where key='llm.api_key' and value <> ''")" != "1" ]; then
  echo "  没配 llm.api_key —— 跳过（真模型调用必须显式确认并先配）"
  exit 0
fi
chk "模型已配" "yes" "yes"

echo "=== 1. 备一个干净作品节点（空库起点） ==="
# 先删子节点（账本等）再删作品节点——nodes.parent_id 是 ON DELETE RESTRICT
$PSQL -q -c "delete from nodes where parent_id in (select id from nodes where parent_id is null and title = 'e2e-write·空之轨迹 FC')" >/dev/null 2>&1 || true
$PSQL -q -c "delete from nodes where parent_id is null and title = 'e2e-write·空之轨迹 FC'" >/dev/null 2>&1 || true
NID=$(curl -s -X POST "$API/api/nodes" -H 'Content-Type: application/json' \
  -d '{"kind":"work","medium":"game","title":"e2e-write·空之轨迹 FC"}' | python3 -c "import sys,json;print(json.load(sys.stdin)['id'])")
chk "节点建好" "$($PSQL -c "select count(*) from nodes where id='$NID'")" "1"

echo "=== 2. 真界面：让 Atlas 写一篇（三段式） ==="
if [ "${NO_BROWSER:-0}" != "1" ] && command -v "$AB" >/dev/null 2>&1; then
  $AB set viewport 1440 1000 >/dev/null 2>&1 || true
  $AB open "$WEB" >/dev/null 2>&1 || true
  $AB wait 1500 >/dev/null 2>&1 || true
  $AB eval "[...document.querySelectorAll('.side-tree-scroll .ant-tree-treenode')].find(n=>n.innerText.includes('e2e-write·空之轨迹 FC'))?.querySelector('.ant-tree-node-content-wrapper')?.click()" >/dev/null 2>&1 || true
  $AB wait 1000 >/dev/null 2>&1 || true
  $AB eval "[...document.querySelectorAll('.top-bar-actions button')].find(b=>b.innerText.includes('问 Atlas'))?.click()" >/dev/null 2>&1 || true
  $AB wait 600 >/dev/null 2>&1 || true
  $AB fill ".ai-foot textarea" "为《空之轨迹 FC》写一篇完整 wiki 正文。" >/dev/null 2>&1 || true
  $AB eval "document.querySelector('.ai-foot-actions button')?.click()" >/dev/null 2>&1 || true
  RID=$($PSQL -c "select id from runs order by started_at desc limit 1")
else
  RID=$(curl -s -X POST "$API/api/runs" -H 'Content-Type: application/json' \
    -d "{\"node_id\":\"$NID\",\"prompt\":\"为《空之轨迹 FC》写一篇完整 wiki 正文。\"}" \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['runId'])")
fi
chk "工单已起" "$($PSQL -c "select count(*) from runs where id='$RID'")" "1"

echo "=== 3. 等三段跑完（研究+骨架+生成，最多 30 分钟） ==="
done_run=0
for _ in $(seq 1 900); do
  st=$($PSQL -c "select status from runs where id='$RID'")
  case "$st" in completed) done_run=1; break;; failed) break;; esac
  sleep 2
done
chk "run 跑完（completed）" "$done_run" "1"
[ "$done_run" = "1" ] || { echo "  失败原因：$($PSQL -c "select error_json from runs where id='$RID'")"; }

echo "=== 4. 三段都发生过 ==="
for ph in research outline generation; do
  chk "阶段 $ph 有事件" "$($PSQL -c "select count(*) > 0 from run_events where run_id='$RID' and type='wikiatlas.phase' and payload->>'phase'='$ph'")" "t"
done

echo "=== 5. 事实账本落了库（kind=doc + tag 账本） ==="
LID=$($PSQL -c "select id from nodes where parent_id='$NID' and tags @> ARRAY['账本']::text[] limit 1")
chk "账本资料节点存在" "$([ -n "$LID" ] && echo yes || echo no)" "yes"
if [ -n "$LID" ]; then
  chk "账本正文非空" "$($PSQL -c "select length(body_md) > 50 from contents where node_id='$LID'")" "t"
  LROWS=$($PSQL -c "select count(*) from contents where node_id='$LID' and body_md like '%## 断言%'")
  chk "账本有「## 断言」表" "$LROWS" "1"
fi
chk "账本事件带覆盖率" "$($PSQL -c "select count(*) > 0 from run_events where run_id='$RID' and type='wikiatlas.ledger' and (payload->>'coverage') is not null")" "t"

echo "=== 6. 草稿落库 + 正文门/引用门 ==="
BODY=$($PSQL -c "select body_md from content_drafts where node_id='$NID'")
CHARS=$(python3 -c "import sys; print(len(sys.stdin.read()))" <<<"$BODY")
chk "草稿字数 ≥ 6000" "$([ "$CHARS" -ge 6000 ] && echo yes || echo no)" "yes"
for c in "## 1." "## 2." "## 3." "## 4." "## 5." "## 6." "## 7." "参考资料"; do
  has "正文有 $c" "$c" "$BODY"
done
chk "有题记块" "$(echo "$BODY" | grep -c '^:::epigraph')" "1"
chk "质量门通过（write.end）" "$($PSQL -c "select payload->>'quality' from run_events where run_id='$RID' and type='wikiatlas.write.end' limit 1")" "passed"
chk "引用门通过（write.end）" "$($PSQL -c "select payload->>'citations' from run_events where run_id='$RID' and type='wikiatlas.write.end' limit 1")" "passed"
chk "账本覆盖率 ≥ 80" "$($PSQL -c "select ((payload->>'ledger_coverage')::int >= 80) from run_events where run_id='$RID' and type='wikiatlas.write.end' limit 1")" "t"

echo "=== 7. 定稿 → 快照 ==="
VER=$(curl -s -X POST "$API/api/nodes/$NID/commit" -H 'Content-Type: application/json' \
  -d "$(python3 -c "import json,sys; print(json.dumps({'body_md': sys.stdin.read(), 'base_ver': 0, 'summary': 'M4 验收定稿'}))" <<<"$BODY")" \
  | python3 -c "import sys,json;print(json.load(sys.stdin).get('ver'))")
chk "定稿成功（ver=1）" "$VER" "1"
chk "快照生成" "$($PSQL -c "select count(*) from content_versions where node_id='$NID' and ver=1")" "1"
chk "写入事实记录" "$($PSQL -c "select count(*) from write_events where node_id='$NID' and ver=1")" "1"

if [ "${NO_BROWSER:-0}" != "1" ] && command -v "$AB" >/dev/null 2>&1; then
  echo "=== 8. 浏览器里看到了（阶段/实时正文） ==="
  LIVE=$($AB eval "(document.querySelector('.ai-live-body')?.textContent||'').length" 2>&1 | tail -1 | tr -dc '0-9')
  chk "实时正文区有内容或已完成" "$([ "${LIVE:-0}" -gt 20 ] 2>/dev/null && echo yes || echo no)" "yes"
fi

echo
echo "=== 结果：通过 $ok 项，失败 $bad 项 ==="
[ "$bad" = "0" ]

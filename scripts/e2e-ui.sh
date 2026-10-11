#!/bin/bash
# 尖刺 A 的验收脚本：从空库走一遍**真实 UI** 流程，每一步都回服务端核对。
#
# 前置：后端在 127.0.0.1:8088（`cargo run -p wikiatlas-api`）、前端在 5173（`npm run dev`）、
#       agent-browser 可用（`npm i -g agent-browser`，本机能自动找到系统 Chrome）。
#
# 用法：bash scripts/e2e-ui.sh
#
# 为什么要有它（`docs/backend.md` §9 的纪律）：验收必须**真跑**，
# 不许只凭「编译过了、接口 200」就算完——这条脚本第一次跑就抓出了
# 编辑器把空段落序列化成 `<br />` 污染正文的真 bug。
set -e
cd "$(dirname "$0")/.."

AB=agent-browser
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# 在应用脚本之前挂钩子，抓 window 上的未捕获错误（否则页面白屏时无从下手）
cat > "$TMP/hook.js" <<'EOF'
window.__err = 'none';
window.addEventListener('error', (e) => {
  window.__err = String((e.error && e.error.stack) || e.message || 'error-event');
}, true);
window.addEventListener('unhandledrejection', (e) => {
  window.__err = 'unhandledrejection: ' + String((e.reason && e.reason.stack) || e.reason);
});
EOF

PW=$(grep -oP '(?<=wikiatlas:)[^@]+' .secrets/wikiatlas.pg.env)
export PGPASSWORD="$PW"
PSQL="psql -h 127.0.0.1 -U wikiatlas -d wikiatlas -qtA"
API=http://127.0.0.1:8088
WEB=http://127.0.0.1:5173

ok=0; bad=0
chk() { if [ "$2" = "$3" ]; then ok=$((ok+1)); echo "  ✓ $1"; else bad=$((bad+1)); echo "  ✗ $1  得到[$2] 期望[$3]"; fi; }
ev() { $AB eval "$1" 2>&1 | tail -1; }

echo "=== 0. 等 API，清库 ==="
curl -s --retry 30 --retry-connrefused --retry-delay 1 -o /dev/null "$API/api/healthz"
$PSQL -c "TRUNCATE nodes CASCADE" >/dev/null
chk "库已清空（节点数）" "$($PSQL -c 'select count(*) from nodes')" "0"

echo "=== 1. 打开页面 ==="
$AB open --init-script "$TMP/hook.js" "$WEB" >/dev/null
$AB wait 900 >/dev/null
chk "页面无 JS 错误" "$($AB eval "window.__err || 'none'" 2>&1 | tail -1)" '"none"'

echo "=== 2. 用「新建」弹窗建一个作品 ==="
$AB click "button[aria-label='新建节点']" >/dev/null
$AB wait 500 >/dev/null
$AB fill "#title" "空之轨迹 SC" >/dev/null
$AB click ".ant-modal-footer .ant-btn-primary" >/dev/null
$AB wait 900 >/dev/null
chk "库里有一个节点" "$($PSQL -c 'select count(*) from nodes')" "1"
chk "kind/medium 对" "$($PSQL -c "select kind||'/'||medium from nodes")" "work/game"
chk "标题归一化进了 title_norm" "$($PSQL -c 'select title_norm from nodes')" "空之轨迹 sc"

echo "=== 3. 在编辑器里打字（真实键盘事件）→ 防抖存草稿 ==="
$AB click ".doc-body .ProseMirror" >/dev/null
$AB keyboard type "# 空之轨迹 SC

第一章 导力革命之后，利贝尔王国迎来短暂的和平。

约修亚与艾丝蒂尔从游击士协会出发。" >/dev/null
$AB wait 2800 >/dev/null
chk "草稿已落库" "$($PSQL -c 'select count(*) from content_drafts')" "1"
chk "草稿作者" "$($PSQL -c 'select by from content_drafts')" "human:local"
echo "  草稿正文："; $PSQL -c "select body_md from content_drafts" | sed 's/^/    | /'
chk "草稿里没有 <br /> 垃圾" "$($PSQL -c "select position('<br' in body_md) from content_drafts")" "0"
chk "还没定稿" "$($PSQL -c 'select count(*) from contents')" "0"
chk "界面显示草稿已存" "$($AB eval "document.querySelector('.doc-bar').innerText.includes('草稿已存')" 2>&1 | tail -1)" "true"

echo "=== 4. 点「定稿」==="
$AB click ".doc-bar .ant-btn-primary" >/dev/null
$AB wait 1200 >/dev/null
chk "定稿 v1 落库" "$($PSQL -c 'select ver from contents')" "1"
chk "快照 1 条" "$($PSQL -c 'select count(*) from content_versions')" "1"
chk "写入事实 1 条（source=human）" "$($PSQL -c 'select source from write_events')" "human"
chk "草稿被清掉" "$($PSQL -c 'select count(*) from content_drafts')" "0"
chk "快照正文与定稿一致" "$($PSQL -c 'select count(*) from content_versions v join contents c on c.node_id=v.node_id and c.ver=v.ver and c.body_md=v.body_md')" "1"
chk "界面显示定稿 v1" "$($AB eval "document.querySelector('.doc-bar').innerText.includes('定稿 v1')" 2>&1 | tail -1)" "true"

echo "=== 5. 刷新页面 → 内容与版本都还在 ==="
$AB open --init-script "$TMP/hook.js" "$WEB" >/dev/null
$AB wait 900 >/dev/null
chk "刷新后无错误" "$($AB eval "window.__err || 'none'" 2>&1 | tail -1)" '"none"'
$AB click ".ant-tree-node-content-wrapper" >/dev/null
$AB wait 1200 >/dev/null
chk "编辑器里是定稿正文" "$($AB eval "document.querySelector('.ProseMirror').innerText.includes('导力革命')" 2>&1 | tail -1)" "true"
chk "状态条显示定稿 v1" "$($AB eval "document.querySelector('.doc-bar').innerText.includes('定稿 v1')" 2>&1 | tail -1)" "true"

mkdir -p _baseline
$AB screenshot "$(pwd)/_baseline/ui-e2e.png" >/dev/null

echo "=== 6. 题记：书页留白渲染 + 源码不能丢 ==="
NID=$(psql -h 127.0.0.1 -U wikiatlas -d wikiatlas -qtA -c "select id from nodes limit 1")
BODY=$(python3 - <<'PY2'
import json
body = """:::epigraph
絶対だいじょうぶだよ。
一定没问题的。
——木之本樱
:::

## 1. 作品概览

正文。

## 2. 基础信息

正文。
"""
print(json.dumps({"body_md": body, "base_ver": 1}))
PY2
)
curl -s -o /dev/null -X PUT "$API/api/nodes/$NID/draft" -H 'Content-Type: application/json' -d "$BODY"
$AB open --init-script "$TMP/hook.js" "$WEB" >/dev/null
$AB wait 1000 >/dev/null
$AB click ".side-tree-scroll .ant-tree-node-content-wrapper" >/dev/null
$AB wait 1600 >/dev/null
chk "题记渲染出来了（.doc-epigraph）" "$(ev "!!document.querySelector('.doc-epigraph')")" "true"
chk "装饰引号来自 CSS 伪元素" "$(ev "getComputedStyle(document.querySelector('.doc-epigraph-content'),'::before').content.includes('“') ? 'yes' : 'no'")" '"yes"'
chk "署名在右下（右对齐 + 有名字）" "$(ev "(()=>{const a=document.querySelector('.doc-epigraph-author');return a && a.innerText.includes('木之本樱') && getComputedStyle(a).textAlign==='right' ? 'yes':'no'})()")" '"yes"'
chk "中文译文大字 > 原文小字（逐行交替的观感）" "$(ev "parseFloat(getComputedStyle(document.querySelector('.doc-epigraph-line--display')).fontSize) > parseFloat(getComputedStyle(document.querySelector('.doc-epigraph-line--latin')).fontSize) ? 'yes':'no'")" '"yes"'
chk "不是卡片：无底色、无左右边框、有上下细线" "$(ev "(()=>{const s=getComputedStyle(document.querySelector('.doc-epigraph'));return s.backgroundColor==='rgba(0, 0, 0, 0)' && s.borderLeftWidth==='0px' && parseFloat(s.borderTopWidth)>0 ? 'yes':'no'})()")" '"yes"'
chk "编辑器里没有裸露的 :::epigraph" "$(ev "document.querySelector('.doc-body .ProseMirror').innerText.includes(':::epigraph')")" "false"
$AB screenshot "$(pwd)/_baseline/doc-epigraph.png" >/dev/null

echo "  — 改题记 → 自动存草稿 → 定稿，围栏都要在 —"
$AB click "button[aria-label='编辑题记']" >/dev/null
$AB wait 700 >/dev/null
$AB fill ".ant-modal textarea" ":::epigraph
絶対だいじょうぶだよ。
一定没问题的。
——木之本樱（改过）
:::" >/dev/null
$AB click ".ant-modal-footer .ant-btn-primary" >/dev/null
$AB wait 2600 >/dev/null
chk "草稿里围栏还在" "$($PSQL -c "select case when position(':::epigraph' in body_md) > 0 then 'yes' else 'no' end from content_drafts")" "yes"
chk "草稿里改动的署名在" "$($PSQL -c "select case when position('改过' in body_md) > 0 then 'yes' else 'no' end from content_drafts")" "yes"
$AB click ".doc-bar .ant-btn-primary" >/dev/null
$AB wait 1600 >/dev/null
chk "定稿 v2" "$($PSQL -c 'select ver from contents')" "2"
chk "定稿正文里围栏还在" "$($PSQL -c "select case when position(':::epigraph' in body_md) > 0 then 'yes' else 'no' end from contents")" "yes"
chk "定稿正文里署名也改了" "$($PSQL -c "select case when position('改过' in body_md) > 0 then 'yes' else 'no' end from contents")" "yes"
chk "定稿快照与定稿一致（含围栏）" "$($PSQL -c "select case when position(':::epigraph' in body_md) > 0 then 'yes' else 'no' end from content_versions order by ver desc limit 1")" "yes"

echo
echo "—— $ok 通过 / $bad 失败 ——"
[ "$bad" = "0" ]

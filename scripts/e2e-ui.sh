#!/bin/bash
# 尖刺 A 的验收脚本：从空库走一遍**真实 UI** 流程，每一步都回服务端核对。
#
# 前置：后端在 127.0.0.1:8088（`cargo run -p wikiatlas-api`）、前端在 5173（`npm run dev`）、
#       agent-browser 可用（`npm i -g agent-browser`，本机能自动找到系统 Chrome）。
#
# ⚠️ **绝不 TRUNCATE**：这个库是人正在用的真库。脚本只碰自己那个测试节点
#    （标题 `e2e-ui·测试作品`），所有断言都按 node_id 圈定——别人的数据一行不动。
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
NODE_TITLE='e2e-ui·测试作品'

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
pick() { $AB eval "[...document.querySelectorAll('.side-tree-scroll .ant-tree-treenode')].find(n=>n.innerText.includes('$NODE_TITLE'))?.querySelector('.ant-tree-node-content-wrapper')?.click()" >/dev/null; }

echo "=== 0. 等 API，清掉**自己**的测试节点（不动别人的数据）==="
curl -s --retry 30 --retry-connrefused --retry-delay 1 -o /dev/null "$API/api/healthz"
$PSQL -q -c "delete from nodes where parent_id in (select id from nodes where title = '$NODE_TITLE')" >/dev/null 2>&1 || true
$PSQL -q -c "delete from nodes where title = '$NODE_TITLE'" >/dev/null 2>&1 || true
chk "自己的测试节点已清" "$($PSQL -c "select count(*) from nodes where title = '$NODE_TITLE'")" "0"

echo "=== 1. 打开页面 ==="
$AB open --init-script "$TMP/hook.js" "$WEB" >/dev/null
$AB wait 900 >/dev/null
chk "页面无 JS 错误" "$($AB eval "window.__err || 'none'" 2>&1 | tail -1)" '"none"'

echo "=== 2. 用「新建」弹窗建一个作品 ==="
$AB click "button[aria-label='新建节点']" >/dev/null
$AB wait 500 >/dev/null
$AB fill "#title" "$NODE_TITLE" >/dev/null
$AB click ".ant-modal-footer .ant-btn-primary" >/dev/null
$AB wait 900 >/dev/null
NID=$($PSQL -c "select id from nodes where title = '$NODE_TITLE'")
chk "节点建好了" "$($PSQL -c "select count(*) from nodes where id = '$NID'")" "1"
chk "kind/medium 对" "$($PSQL -c "select kind||'/'||medium from nodes where id = '$NID'")" "work/game"
chk "标题归一化进了 title_norm" "$($PSQL -c "select title_norm from nodes where id = '$NID'")" "e2e-ui·测试作品"

echo "=== 3. 在编辑器里打字（真实键盘事件）→ 防抖存草稿 ==="
$AB click ".doc-body .ProseMirror" >/dev/null
$AB keyboard type "# 空之轨迹 SC

第一章 导力革命之后，利贝尔王国迎来短暂的和平。

约修亚与艾丝蒂尔从游击士协会出发。" >/dev/null
$AB wait 2800 >/dev/null
chk "草稿已落库" "$($PSQL -c "select count(*) from content_drafts where node_id = '$NID'")" "1"
chk "草稿作者" "$($PSQL -c "select by from content_drafts where node_id = '$NID'")" "human:local"
echo "  草稿正文："; $PSQL -c "select body_md from content_drafts where node_id = '$NID'" | sed 's/^/    | /'
chk "草稿里没有 <br /> 垃圾" "$($PSQL -c "select position('<br' in body_md) from content_drafts where node_id = '$NID'")" "0"
chk "还没定稿" "$($PSQL -c "select count(*) from contents where node_id = '$NID'")" "0"
chk "界面显示草稿已存" "$($AB eval "document.querySelector('.doc-bar').innerText.includes('已保存')" 2>&1 | tail -1)" "true"

echo "=== 4. 点「定稿」==="
$AB click ".doc-bar .ant-btn-primary" >/dev/null
$AB wait 1200 >/dev/null
chk "定稿 v1 落库" "$($PSQL -c "select ver from contents where node_id = '$NID'")" "1"
chk "快照 1 条" "$($PSQL -c "select count(*) from content_versions where node_id = '$NID'")" "1"
chk "写入事实 1 条（source=human）" "$($PSQL -c "select source from write_events where node_id = '$NID'")" "human"
chk "草稿被清掉" "$($PSQL -c "select count(*) from content_drafts where node_id = '$NID'")" "0"
chk "快照正文与定稿一致" "$($PSQL -c "select count(*) from content_versions v join contents c on c.node_id=v.node_id and c.ver=v.ver and c.body_md=v.body_md where v.node_id = '$NID'")" "1"
chk "界面显示定稿 v1" "$($AB eval "document.querySelector('.doc-bar').innerText.includes('定稿 v1')" 2>&1 | tail -1)" "true"

echo "=== 5. 刷新页面 → 内容与版本都还在 ==="
$AB open --init-script "$TMP/hook.js" "$WEB" >/dev/null
$AB wait 900 >/dev/null
chk "刷新后无错误" "$($AB eval "window.__err || 'none'" 2>&1 | tail -1)" '"none"'
pick
$AB wait 1200 >/dev/null
chk "编辑器里是定稿正文" "$($AB eval "document.querySelector('.ProseMirror').innerText.includes('导力革命')" 2>&1 | tail -1)" "true"
chk "状态条显示定稿 v1" "$($AB eval "document.querySelector('.doc-bar').innerText.includes('定稿 v1')" 2>&1 | tail -1)" "true"

mkdir -p _baseline
$AB screenshot "$(pwd)/_baseline/ui-e2e.png" >/dev/null

echo "=== 6. 题记：书页留白渲染 + 源码不能丢 ==="
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
pick
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
chk "草稿里围栏还在" "$($PSQL -c "select case when position(':::epigraph' in body_md) > 0 then 'yes' else 'no' end from content_drafts where node_id = '$NID'")" "yes"
chk "草稿里改动的署名在" "$($PSQL -c "select case when position('改过' in body_md) > 0 then 'yes' else 'no' end from content_drafts where node_id = '$NID'")" "yes"
$AB click ".doc-bar .ant-btn-primary" >/dev/null
$AB wait 1600 >/dev/null
chk "定稿 v2" "$($PSQL -c "select ver from contents where node_id = '$NID'")" "2"
chk "定稿正文里围栏还在" "$($PSQL -c "select case when position(':::epigraph' in body_md) > 0 then 'yes' else 'no' end from contents where node_id = '$NID'")" "yes"
chk "定稿正文里署名也改了" "$($PSQL -c "select case when position('改过' in body_md) > 0 then 'yes' else 'no' end from contents where node_id = '$NID'")" "yes"
chk "定稿快照与定稿一致（含围栏）" "$($PSQL -c "select case when position(':::epigraph' in body_md) > 0 then 'yes' else 'no' end from content_versions where node_id = '$NID' order by ver desc limit 1")" "yes"

echo "  — 丢弃草稿（D74：拒绝 AI 修订 = 回到最近定稿）—"
BODY=$(python3 - <<'PY2'
import json
body = """# 《魔卡少女樱》（漫画）Wiki

> 说明：这是一段待丢弃的修订。

:::epigraph
絶対だいじょうぶだよ。
一定没问题的。
——木之本樱
:::

## 1. 作品概览

待丢弃的修订内容。
"""
print(json.dumps({"body_md": body, "base_ver": 2}))
PY2
)
curl -s -o /dev/null -X PUT "$API/api/nodes/$NID/draft" -H 'Content-Type: application/json' -d "$BODY"
$AB open --init-script "$TMP/hook.js" "$WEB" >/dev/null
$AB wait 1000 >/dev/null
pick
$AB wait 1600 >/dev/null
chk "草稿已装载" "$(ev "document.querySelector('.ProseMirror').innerText.includes('待丢弃的修订内容')")" "true"
$AB eval "(()=>{document.querySelector('.doc-actions button[aria-label=更多]')?.click();return 'ok'})()" >/dev/null
$AB wait 500 >/dev/null
$AB eval "(()=>{const m=[...document.querySelectorAll('.ant-dropdown-menu-item')].find(x=>x.innerText.includes('丢弃草稿'));m?.click();return 'ok'})()" >/dev/null
$AB wait 600 >/dev/null
chk "确认框说明会回到定稿 v2" "$(ev "[...document.querySelectorAll('.ant-modal-confirm')].some(x=>x.innerText.includes('定稿 v2'))")" "true"
$AB eval "(()=>{const b=[...document.querySelectorAll('.ant-modal-confirm button')].find(x=>x.innerText.replace(/\\s/g,'').includes('丢弃'));b?.click();return 'ok'})()" >/dev/null
$AB wait 1800 >/dev/null
chk "草稿行已删除" "$($PSQL -c "select count(*) from content_drafts where node_id = '$NID'")" "0"
chk "编辑器回到定稿正文（不再显示待丢弃内容）" "$(ev "document.querySelector('.ProseMirror').innerText.includes('待丢弃的修订内容')")" "false"

echo
echo "—— $ok 通过 / $bad 失败 ——"
[ "$bad" = "0" ]

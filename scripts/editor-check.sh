#!/bin/bash
# 编辑器浮层的视觉/行为验收（`docs/TRACKING.md §3.1` 的编辑器待迁项）。
#
# 盯三件事（都是「能用但没打扮」踩过的坑）：
#   1. 选区工具栏是**浮层卡片**（不是浏览器默认按钮）、AI 动作是紫色；
#   2. `/` 菜单**挂到光标处**（不是停在视口左上角 0,0）——它曾被一个每帧重渲染的
#      忙循环卡住 200ms 防抖，永远不出来；
#   3. 块手柄是**悬停出现的图标按钮**，不是灰方块，且落在正文左侧空白里。
#
# 前置：api 8088、前端 5173、agent-browser 可用。用法：bash scripts/editor-check.sh
set -e
cd "$(dirname "$0")/.."

AB=agent-browser
API=http://127.0.0.1:8088
WEB=http://127.0.0.1:5173
export PGPASSWORD=$(grep -oP '(?<=wikiatlas:)[^@]+' .secrets/wikiatlas.pg.env)
PSQL="psql -h 127.0.0.1 -U wikiatlas -d wikiatlas -qtA"

ok=0; bad=0
chk() { if [ "$2" = "$3" ]; then ok=$((ok+1)); echo "  ✓ $1"; else bad=$((bad+1)); echo "  ✗ $1  得到[$2] 期望[$3]"; fi; }
ev() { $AB eval "$1" 2>&1 | tail -1; }

echo "=== 备一个带正文的节点 ==="
curl -s --retry 30 --retry-connrefused --retry-delay 1 -o /dev/null "$API/api/healthz"
$PSQL -q -c "delete from nodes where parent_id is null and title = 'editor-check·测试'" >/dev/null 2>&1 || true
NID=$(curl -s -X POST "$API/api/nodes" -H 'Content-Type: application/json' \
  -d '{"kind":"work","medium":"game","title":"editor-check·测试"}' | python3 -c "import sys,json;print(json.load(sys.stdin)['id'])")
BODY=$(python3 -c "import json;print(json.dumps({'body_md':'# editor-check·测试\n\n> 说明：含剧透。\n\n:::epigraph\n「台词。」\n——某人\n:::\n\n## 1. 作品概览\n\n这一句用来选中看工具栏。\n\n## 2. 基础信息\n\n正文。','base_ver':0}))")
curl -s -o /dev/null -X PUT "$API/api/nodes/$NID/draft" -H 'Content-Type: application/json' -d "$BODY"
$PSQL -q -c "delete from nodes where parent_id in (select id from nodes where parent_id is null and title='editor-check·测试')" >/dev/null 2>&1 || true
chk "节点备好" "$($PSQL -c "select count(*) from content_drafts where node_id='$NID'")" "1"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
cat > "$TMP/hook.js" <<'EOF'
window.__err = 'none';
window.addEventListener('error', (e) => {
  window.__err = String((e.error && e.error.stack) || e.message || 'error-event');
}, true);
window.addEventListener('unhandledrejection', (e) => {
  window.__err = 'unhandledrejection: ' + String((e.reason && e.reason.stack) || e.reason);
});
EOF

$AB set viewport 1440 900 >/dev/null
$AB open --init-script "$TMP/hook.js" "$WEB" >/dev/null
$AB wait 1400 >/dev/null
$AB eval "[...document.querySelectorAll('.side-tree-scroll .ant-tree-treenode')].find(n=>n.innerText.includes('editor-check·测试'))?.querySelector('.ant-tree-node-content-wrapper')?.click()" >/dev/null
$AB wait 1200 >/dev/null

echo "=== 0. 题记：围栏不外漏 ==="
chk "题记渲染出来了" "$(ev "!!document.querySelector('.doc-epigraph')")" "true"
chk "题记里没有裸露的 :::epigraph" "$(ev "document.querySelector('.doc-epigraph-body')?.innerText.includes(':::') ? 'leaked' : 'clean'")" '"clean"'
chk "题记内容在（台词）" "$(ev "document.querySelector('.doc-epigraph-body')?.innerText.includes('台词') ? 'yes':'no'")" '"yes"'

echo "=== 1. 选区工具栏：浮层卡片 + 紫色 AI 区 ==="
$AB click ".doc-body .ProseMirror p" >/dev/null
$AB press Home >/dev/null
for _ in 1 2 3 4; do $AB press "Shift+ArrowRight" >/dev/null; done
$AB wait 600 >/dev/null
chk "工具栏出现在选区上" "$(ev "(()=>{const p=document.querySelector('.feishu-toolbar')?.closest('.editor-portal');return p?.getAttribute('data-show')})()")" '"true"'
chk "工具栏是卡片（有底色 + 圆角）" "$(ev "(()=>{const t=document.querySelector('.feishu-toolbar');if(!t)return 'no';const s=getComputedStyle(t);return (s.backgroundColor!=='rgba(0, 0, 0, 0)' && parseFloat(s.borderRadius)>0)?'yes':'no'})()")" '"yes"'
chk "按钮不止 5 个（H1/H2/H3/B/I/链接…）" "$(ev "document.querySelectorAll('.feishu-toolbar .ant-btn').length > 5 ? 'yes':'no'")" '"yes"'
# AI 区与普通按钮**颜色不同**（不硬编 RGB；只验"AI 是独立强调色"）
chk "AI 区是独立强调色（与普通按钮不同）" "$(ev "(()=>{const a=document.querySelector('.tb-ai .ant-btn');const n=document.querySelector('.feishu-toolbar .ant-btn:not(.tb-ai .ant-btn)');if(!a||!n)return 'no';return getComputedStyle(a).color!==getComputedStyle(n).color?'yes':'no'})()")" '"yes"'

echo "=== 2. / 菜单：挂到光标处（不是 0,0）==="
# 重新开一页再测，避免上一段的选区/工具栏状态干扰
$AB open --init-script "$TMP/hook.js" "$WEB" >/dev/null
$AB wait 1300 >/dev/null
$AB eval "[...document.querySelectorAll('.side-tree-scroll .ant-tree-treenode')].find(n=>n.innerText.includes('editor-check·测试'))?.querySelector('.ant-tree-node-content-wrapper')?.click()" >/dev/null
$AB wait 1000 >/dev/null
$AB click ".doc-body .ProseMirror p" >/dev/null
$AB press End >/dev/null
$AB press Enter >/dev/null
$AB keyboard type "/" >/dev/null
# 定位是防抖 200ms 之后的事，给足时间并轮询（不然会拍到还没定位的 0,0）
SLASH_OK=no
for _ in $(seq 1 15); do
  sleep 0.2
  if [ "$(ev "(()=>{const r=document.querySelector('.slash-menu')?.getBoundingClientRect();return r && r.x>280 ? 'yes':'no'})()")" = '"yes"' ]; then SLASH_OK=yes; break; fi
done
chk "/ 菜单出现了" "$(ev "document.querySelectorAll('.slash-menu').length")" "1"
chk "菜单挂到光标处（x > 280，不是 0,0）" "$SLASH_OK" "yes"
# 清掉这一行，免得多一个空段
$AB press Escape >/dev/null 2>&1 || true
$AB press Backspace >/dev/null 2>&1 || true

echo "=== 3. 块手柄：悬停图标按钮（不是灰方块）==="
$AB hover ".doc-body .ProseMirror p" >/dev/null 2>&1 || true
$AB wait 500 >/dev/null
chk "手柄出现" "$(ev "(()=>{const p=document.querySelector('.block-handle')?.closest('.editor-portal');return p?.getAttribute('data-show')})()")" '"true"'
chk "手柄按钮是透明图标（不是灰底方块）" "$(ev "(()=>{const b=document.querySelector('.block-handle-btn');if(!b)return 'no';const s=getComputedStyle(b);return (s.backgroundColor==='rgba(0, 0, 0, 0)' && s.borderStyle==='none')?'yes':'no'})()")" '"yes"'
chk "手柄落在正文左侧（x 小于正文左缘）" "$(ev "(()=>{const h=document.querySelector('.block-handle')?.getBoundingClientRect();const p=document.querySelector('.doc-body .ProseMirror p')?.getBoundingClientRect();return h&&p&&h.right<=p.left+2?'yes':'no'})()")" '"yes"'

echo "=== 4. 正文内 Ctrl+A 后粘贴整篇：标题、说明、题记一起替换 ==="
NEW_DOC=$'# 替换后的标题\n\n> 说明：替换后的说明文。\n\n:::epigraph\n「替换后的题记。」\n——新作者\n:::\n\n## 1. 新概览\n\n新正文一。\n\n## 2. 新资料\n\n新正文二。'
$AB focus ".doc-body .ProseMirror" >/dev/null
$AB press Control+a >/dev/null
chk "Ctrl+A 的选择被限定在编辑器内" "$(ev "(()=>{const pm=document.querySelector('.ProseMirror');const s=window.getSelection();return s?.anchorNode && pm?.contains(s.anchorNode) && s.toString().length>0 ? 'yes':'no'})()")" '"yes"'
$AB clipboard write "$NEW_DOC" >/dev/null
$AB press Control+v >/dev/null
$AB wait 700 >/dev/null
chk "说明文换成新样式内容" "$(ev "document.querySelector('.doc-note')?.innerText.includes('替换后的说明文') ? 'yes':'no'")" '"yes"'
chk "题记换成新样式内容" "$(ev "document.querySelector('.doc-epigraph-body')?.innerText.includes('替换后的题记') ? 'yes':'no'")" '"yes"'
chk "正文只剩新文章两章" "$(ev "document.querySelectorAll('.outline-item').length")" "2"
chk "编辑器里没有漏出文档头围栏或 H1" "$(ev "(()=>{const p=document.querySelector('.ProseMirror');return p?.innerText.includes(':::epigraph')||p?.querySelector('h1')?'bad':'clean'})()")" '"clean"'

echo "=== 5. 删除文章：可从 ⋯ 菜单归档并移出知识库 ==="
$AB click "button[aria-label='更多']" >/dev/null
$AB wait 250 >/dev/null
$AB find text "删除文章" click >/dev/null
$AB wait 250 >/dev/null
chk "删除前有确认对话框" "$(ev "document.querySelector('.ant-modal-confirm-title')?.innerText.includes('删除') ? 'yes':'no'")" '"yes"'
$AB click ".ant-modal-confirm-btns .ant-btn-dangerous" >/dev/null
$AB wait 700 >/dev/null
chk "后端状态已归档" "$($PSQL -c "select status from nodes where id='$NID'")" "archived"
chk "已从知识库树中消失" "$(ev "document.querySelector('.side-tree-scroll')?.innerText.includes('editor-check·测试') ? 'no':'yes'")" '"yes"'

chk "全程无 JS 错误" "$(ev "window.__err || 'none'")" '"none"'

echo
echo "—— 编辑器：$ok 通过 / $bad 失败 ——"
[ "$bad" = "0" ]

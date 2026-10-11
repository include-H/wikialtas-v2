#!/bin/bash
# 布局验收：三档视口 + 收起/展开 + 触底 + 窄屏抽屉。全部在真浏览器里**量像素**。
#
# 骨架照 v1（`archive/v1/VISUAL_SPEC.md` §1）：左栏 280（可收起）、右侧 Altas 480（可收起）、
# 顶部 52px 顶栏只在内容区上方。断点：≥992 三栏 · 768–992 右栏默认收起 · <768 单栏 + 抽屉。
#
# 前置：后端 8088、前端 5173、agent-browser 可用。用法：bash scripts/layout-check.sh
set -e
cd "$(dirname "$0")/.."

AB=agent-browser
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

WEB=http://127.0.0.1:5173
API=http://127.0.0.1:8088
export PGPASSWORD=$(grep -oP '(?<=wikiatlas:)[^@]+' .secrets/wikiatlas.pg.env)

ok=0; bad=0
chk() { if [ "$2" = "$3" ]; then ok=$((ok+1)); echo "  ✓ $1"; else bad=$((bad+1)); echo "  ✗ $1  得到[$2] 期望[$3]"; fi; }
ev() { $AB eval "$1" 2>&1 | tail -1; }
side_w() { ev "document.querySelector('.pane-side')?.offsetWidth ?? -1"; }
ai_w() { ev "document.querySelector('.pane-ai')?.offsetWidth ?? -1"; }
pane_count() { ev "document.querySelectorAll('.pane').length"; }
bottom_ok() { ev "(()=>{const vh=window.innerHeight;return [...document.querySelectorAll('.pane')].every(p=>Math.abs(p.getBoundingClientRect().bottom-vh)<=1)?'yes':'no'})()"; }

echo "=== 准备：清库 + 建一个节点（用接口，不依赖界面）==="
curl -s --retry 30 --retry-connrefused --retry-delay 1 -o /dev/null "$API/api/healthz"
psql -h 127.0.0.1 -U wikiatlas -d wikiatlas -qtA -c "TRUNCATE nodes CASCADE" >/dev/null
curl -s -o /dev/null -X POST "$API/api/nodes" -H 'Content-Type: application/json' \
  -d '{"kind":"work","medium":"game","title":"空之轨迹 SC"}'
NID=$(psql -h 127.0.0.1 -U wikiatlas -d wikiatlas -qtA -c "select id from nodes limit 1")
echo "  已建节点：$NID"

echo "=== 宽屏 1440×900：左 280 + 正文 + Altas 480 ==="
$AB set viewport 1440 900 >/dev/null
$AB open --init-script "$TMP/hook.js" "$WEB" >/dev/null
$AB wait 1300 >/dev/null
chk "layout=wide" "$(ev "document.querySelector('.shell').dataset.layout")" '"wide"'
chk "默认两栏：左栏 + 正文（Altas 默认收起，v1 规矩）" "$(pane_count)" "2"
chk "左栏 280（v1 的 .shell-side）" "$(side_w)" "280"
$AB find text "问 Altas" click >/dev/null
$AB wait 700 >/dev/null
chk "点「问 Altas」后三栏" "$(pane_count)" "3"
chk "Altas 480（v1 的右栏）" "$(ai_w)" "480"
chk "三栏宽度之和 = 视口宽（没有悬空/留白）" "$(ev "(()=>{const w=[...document.querySelectorAll('.pane')].reduce((a,p)=>a+p.offsetWidth,0);return Math.abs(w-window.innerWidth)<=4?'yes':'no'})()")" '"yes"'
chk "顶栏 52px 高" "$(ev "Math.round(document.querySelector('.top-bar').getBoundingClientRect().height)")" "52"
chk "页面无 JS 错误" "$(ev "window.__err")" '"none"'
$AB screenshot "$(pwd)/_baseline/layout-wide.png" >/dev/null

echo "  — 高度：必须触底；页面本身不许出现纵向滚动条 —"
chk "各栏都触底（底部=视口底部）" "$(bottom_ok)" '"yes"'
chk "shell 撑满视口" "$(ev "Math.abs(document.querySelector('.shell').getBoundingClientRect().height - window.innerHeight) <= 1 ? 'yes' : 'no'")" '"yes"'
chk "页面无纵向滚动条" "$(ev "document.documentElement.scrollHeight <= window.innerHeight + 1 ? 'yes' : 'no'")" '"yes"'

echo "  — 正文很长时：滚的是正文区，不是整个页面 —"
LONG=$(python3 -c "import json;print(json.dumps({'body_md':'# 空之轨迹 SC\n\n## 1. 作品概览\n\n'+'\n\n'.join('第 %d 段。'%i for i in range(240))+'\n\n## 2. 基础信息\n\n正文\n\n## 3. 故事背景','base_ver':0}))")
curl -s -o /dev/null -X PUT "$API/api/nodes/$NID/draft" -H 'Content-Type: application/json' -d "$LONG"
$AB click ".side-tree-scroll .ant-tree-node-content-wrapper" >/dev/null
$AB wait 1600 >/dev/null
chk "正文区自己可滚（scrollHeight > clientHeight）" "$(ev "document.querySelector('.doc-body').scrollHeight > document.querySelector('.doc-body').clientHeight + 10 ? 'yes' : 'no'")" '"yes"'
chk "长正文下页面仍不滚" "$(ev "document.documentElement.scrollHeight <= window.innerHeight + 1 ? 'yes' : 'no'")" '"yes"'
chk "长正文下各栏仍触底" "$(bottom_ok)" '"yes"'
chk "大纲从 ## 解析出来" "$(ev "[...document.querySelectorAll('.outline-item')].map(b=>b.innerText).join('|')")" '"1. 作品概览|2. 基础信息|3. 故事背景"'
$AB eval "[...document.querySelectorAll('.outline-item')].find(b=>b.innerText.startsWith('3.'))?.click()" >/dev/null
$AB wait 900 >/dev/null
chk "点大纲能跳到那一章" "$(ev "document.querySelector('.doc-body').scrollTop > 0 ? 'yes' : 'no'")" '"yes"'
$AB screenshot "$(pwd)/_baseline/layout-long-body.png" >/dev/null

echo "  — 顶栏两个开关 —"
$AB click "button[aria-label='收起知识库']" >/dev/null
$AB wait 600 >/dev/null
chk "收起知识库后左栏消失（三栏→两栏）" "$(pane_count)" "2"
$AB click "button[aria-label='展开知识库']" >/dev/null
$AB wait 600 >/dev/null
chk "展开后回到 280" "$(side_w)" "280"
$AB click "button[aria-label='收起 Altas']" >/dev/null
$AB wait 600 >/dev/null
chk "收起 Altas 后右栏消失" "$(pane_count)" "2"
$AB click "button[aria-label='展开 Altas']" >/dev/null
$AB wait 600 >/dev/null
chk "展开后回到 480" "$(ai_w)" "480"

echo "=== 中屏 900×800：Altas 默认收起（不然正文没地方）==="
$AB set viewport 900 800 >/dev/null
$AB wait 900 >/dev/null
chk "layout=medium" "$(ev "document.querySelector('.shell').dataset.layout")" '"medium"'
chk "中屏也不自动开 Altas（两栏）" "$(pane_count)" "2"
chk "左栏仍在" "$(side_w)" "280"
chk "正文还有地方（>400）" "$(ev "document.querySelector('.doc-body').clientWidth > 400 ? 'yes' : 'no'")" '"yes"'
chk "中屏不显示大纲（先舍它）" "$(ev "getComputedStyle(document.querySelector('.outline')).display")" '"none"'
$AB screenshot "$(pwd)/_baseline/layout-medium.png" >/dev/null

echo "=== 窄屏 420×800：单栏 + 抽屉 ==="
$AB set viewport 420 800 >/dev/null
$AB wait 900 >/dev/null
chk "layout=narrow" "$(ev "document.querySelector('.shell').dataset.layout")" '"narrow"'
chk "只剩一栏" "$(pane_count)" "1"
$AB screenshot "$(pwd)/_baseline/layout-narrow.png" >/dev/null

echo "  — 打开「知识库」抽屉，在里面选节点 —"
$AB find text "知识库" click >/dev/null
$AB wait 800 >/dev/null
chk "抽屉开了" "$(ev "document.querySelectorAll('.ant-drawer-open').length")" "1"
chk "抽屉里有树" "$(ev "document.querySelectorAll('.ant-drawer .ant-tree').length")" "1"
$AB click ".ant-drawer .ant-tree-node-content-wrapper" >/dev/null
$AB wait 900 >/dev/null
chk "选完自动收起抽屉" "$(ev "document.querySelectorAll('.ant-drawer-open').length")" "0"
chk "正文里是那个节点" "$(ev "document.querySelector('.doc-head').innerText.includes('空之轨迹 SC')")" "true"
chk "窄屏也无 JS 错误" "$(ev "window.__err")" '"none"'
$AB screenshot "$(pwd)/_baseline/layout-narrow-drawer.png" >/dev/null

echo
echo "—— $ok 通过 / $bad 失败 ——"
[ "$bad" = "0" ]

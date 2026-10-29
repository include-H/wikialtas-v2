#!/bin/bash
# 视觉验收截图（`docs/design.md §6.6`）：首页 / 文档 / ⋯ 菜单 / 版本抽屉 / Atlas 时间线。
#
# 前置：api 8088、前端 5173、agent-browser 可用；库里至少有一个作品节点（跑过 e2e-ui.sh 就有）。
# 用法：bash scripts/visual-shots.sh
set -e
cd "$(dirname "$0")/.."

AB=agent-browser
WEB=http://127.0.0.1:5173
OUT="$(pwd)/_baseline"

$AB set viewport 1440 900 >/dev/null
$AB open "$WEB" >/dev/null
$AB wait 1500 >/dev/null

# 1) 首页（没选节点时）
$AB eval "document.querySelector('.side-link')?.click()" >/dev/null
$AB wait 1000 >/dev/null
$AB screenshot "$OUT/visual-home.png" >/dev/null
echo "  ✓ visual-home.png"

# 2) 文档页（选树里第一个节点）
$AB click ".side-tree-scroll .ant-tree-node-content-wrapper" >/dev/null
$AB wait 1200 >/dev/null
$AB screenshot "$OUT/visual-doc.png" >/dev/null
echo "  ✓ visual-doc.png"

# 3) ⋯ 菜单
if $AB click "button[aria-label='更多']" >/dev/null 2>&1; then
  $AB wait 500 >/dev/null
  $AB screenshot "$OUT/visual-doc-menu.png" >/dev/null
  echo "  ✓ visual-doc-menu.png"
  $AB eval "document.body.click()" >/dev/null || true
fi

# 4) 版本抽屉
if $AB click "button[aria-label='更多']" >/dev/null 2>&1; then
  $AB wait 400 >/dev/null
  $AB find text "版本历史" click >/dev/null 2>&1 || true
  $AB wait 700 >/dev/null
  $AB screenshot "$OUT/visual-versions.png" >/dev/null
  echo "  ✓ visual-versions.png"
  $AB eval "document.querySelector('.ant-drawer-close')?.click()" >/dev/null || true
fi

# 5) Atlas 右栏时间线（空态起手式）
$AB click "button[aria-label='展开 Atlas']" >/dev/null 2>&1 || true
$AB wait 700 >/dev/null
$AB screenshot "$OUT/visual-atlas.png" >/dev/null
echo "  ✓ visual-atlas.png"

# 6) Skill 分层选择器
if $AB click ".ai-skill-select" >/dev/null 2>&1; then
  $AB wait 350 >/dev/null
  $AB screenshot "$OUT/visual-atlas-skills.png" >/dev/null
  echo "  ✓ visual-atlas-skills.png"
fi

echo "—— 截图已存到 _baseline/visual-*.png ——"

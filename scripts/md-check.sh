#!/bin/bash
# markdown 验收：用**真实基准样本**（19 篇里挑一篇有表格/说明块/题记的）跑两件事：
#   ① 每个 markdown 构造都渲染成了对的元素（表格不能退化成带竖线的纯文本）
#   ② **无损往返**：装载 → 不改任何字 → 定稿，存回去的必须与规范化后的原文逐字节相同
#
# 前置：后端 8088、前端 5173、agent-browser 可用。用法：bash scripts/md-check.sh
set -e
cd "$(dirname "$0")/.."

AB=agent-browser
SAMPLE=skills/atlas/references/samples/书籍/百变小樱魔术卡_WIKI.md
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
PSQL="psql -h 127.0.0.1 -U wikiatlas -d wikiatlas -qtA"

ok=0; bad=0
chk() { if [ "$2" = "$3" ]; then ok=$((ok+1)); echo "  ✓ $1"; else bad=$((bad+1)); echo "  ✗ $1  得到[$2] 期望[$3]"; fi; }
ev() { $AB eval "$1" 2>&1 | tail -1; }

echo "=== 准备：建一个**自己的**节点 + 灌入真实样本 ==="
echo "    （不 TRUNCATE——库里可能有别人正在写的东西；用完删自己这个）"
curl -s --retry 30 --retry-connrefused --retry-delay 1 -o /dev/null "$API/api/healthz"
NODE_TITLE="md-check·魔卡少女樱"
NID=$(curl -s -X POST "$API/api/nodes" -H 'Content-Type: application/json' \
  -d "{\"kind\":\"work\",\"medium\":\"manga\",\"title\":\"$NODE_TITLE\"}" \
  | python3 -c "import json,sys;print(json.load(sys.stdin)['id'])")
cleanup() { psql -h 127.0.0.1 -U wikiatlas -d wikiatlas -qtA -c "delete from nodes where id = '$NID'" >/dev/null 2>&1 || true; }
trap 'rm -rf "$TMP"; cleanup' EXIT
python3 - "$NID" <<'PY'
import json, sys, urllib.request
body = open('skills/atlas/references/samples/书籍/百变小樱魔术卡_WIKI.md', encoding='utf-8').read()
req = urllib.request.Request(
    f'http://127.0.0.1:8088/api/nodes/{sys.argv[1]}/draft',
    data=json.dumps({'body_md': body, 'base_ver': 0}).encode(),
    headers={'Content-Type': 'application/json'}, method='PUT')
urllib.request.urlopen(req).read()
print(f"  样本已灌入：{len(body)} 字")
PY

echo "=== 打开 + 选中 ==="
$AB set viewport 1440 1000 >/dev/null
$AB open --init-script "$TMP/hook.js" "$WEB" >/dev/null
$AB wait 1100 >/dev/null
$AB eval "[...document.querySelectorAll('.side-tree-scroll .ant-tree-treenode')].find(n=>n.innerText.includes('md-check·魔卡少女樱'))?.querySelector('.ant-tree-node-content-wrapper')?.click()" >/dev/null
$AB wait 2600 >/dev/null
chk "无 JS 错误" "$(ev "window.__err")" '"none"'

echo "=== ① 构造审计：markdown 该变成什么元素 ==="
AUDIT=$(ev "(()=>{const r=document.querySelector('.ProseMirror');const c=(s)=>r.querySelectorAll(s).length;return JSON.stringify({h1:c('h1'),h2:c('h2'),h3:c('h3'),table:c('table'),th:c('th'),hr:c('hr'),blockquote:c('blockquote'),ol:c('ol'),ul:c('ul'),li:c('li'),strong:c('strong'),a:c('a'),pipesInText:(r.innerText.match(/\\|/g)||[]).length,colon3:(r.innerText.match(/:::/g)||[]).length})})()")
echo "  $AUDIT"
num() { echo "$AUDIT" | python3 -c "import json,sys;print(json.loads(json.loads(sys.stdin.read()))['$1'])"; }
ge() { if [ "$1" -ge "$2" ]; then echo yes; else echo no; fi; }
chk "标题不在编辑器里（被摘成文档头）+ h2/h3 正常" "$(num h1) $(ge "$(num h2)" 8) $(ge "$(num h3)" 10)" "0 yes yes"
chk "**表格渲染出来了**（table + th，而不是纯文本带竖线）" "$(ge "$(num table)" 1) $(ge "$(num th)" 4)" "yes yes"
chk "编辑器里没有裸竖线残留" "$(num pipesInText)" "0"
chk "编辑器里没有裸 ::: 残留（题记被摘走）" "$(num colon3)" "0"
chk "分隔线 hr" "$(ge "$(num hr)" 5)" "yes"
chk "列表项 >= 50" "$(ge "$(num li)" 50)" "yes"
chk "粗体渲染" "$(ge "$(num strong)" 20)" "yes"
chk "链接渲染" "$(ge "$(num a)" 5)" "yes"

echo "=== ② 文档头：说明块 + 题记各自成组件，标题用节点的 ==="
chk "说明块渲染成 .doc-note（不是普通引用块）" "$(ev "document.querySelectorAll('.doc-note').length")" "1"
chk "题记渲染成 .doc-epigraph" "$(ev "document.querySelectorAll('.doc-epigraph').length")" "1"
chk "页面标题用节点标题" "$(ev "document.querySelector('.doc-title').innerText")" "\"$NODE_TITLE\""
# 说明块当前设计：左边线 + 次要色文字（不再用浅灰底 / 斜体；见 index.css）
chk "说明块有左边线与次要色" "$(ev "(()=>{const s=getComputedStyle(document.querySelector('.doc-note'));return s.borderLeftStyle!=='none' && s.borderLeftWidth!=='0px' && s.color!=='rgba(0, 0, 0, 0)' ? 'yes':'no'})()")" '"yes"'
$AB screenshot "$(pwd)/_baseline/md-sample.png" >/dev/null

echo "=== ③ 无损往返：一个字不改，定稿后必须与规范化原文等价 ==="
# Milkdown 的 GFM 序列化只保证语义：`-` → `*`、`---` → `***`、表格补对齐空格。
# 所以比较「规范形」而不是原始字节（R7）；内容少一个字都必须暴露。
canon() {
python3 - "$1" <<'PY'
import hashlib, re, sys
s = open(sys.argv[1], encoding='utf-8').read()
lines = s.replace('\r\n', '\n').split('\n')
if lines and lines[-1] == '':
    lines.pop()
out, blank = [], 0
for l in lines:
    t = l.strip()
    if t in ('<br />', '<br/>', '<br>'):
        continue
    if t == '':
        blank += 1
        if blank > 1:
            continue
    else:
        blank = 0
    l = l.rstrip()
    # Milkdown 会给链接 URL 里的 () / & 加反斜杠转义；反斜杠转义与字面量等价。
    l = re.sub(r'\\([\\`*{}\[\]()#+\-.!_>~|&])', r'\1', l)
    if re.fullmatch(r'\s*(?:-\s*){3,}', l) or re.fullmatch(r'\s*(?:\*\s*){3,}', l) or re.fullmatch(r'\s*(?:_\s*){3,}', l):
        l = '***'
    m = re.match(r'^(\s*)[-*+]\s+(.*)$', l)
    if m:
        l = f'{m.group(1)}* {m.group(2)}'
    if l.lstrip().startswith('|') and l.rstrip().endswith('|'):
        cells = [c.strip() for c in l.strip().strip('|').split('|')]
        if all(re.fullmatch(r':?-{3,}:?', c) for c in cells if c):
            cells = ['---' for _ in cells]
        l = '| ' + ' | '.join(cells) + ' |'
    out.append(l)
while out and out[-1] == '':
    out.pop()
print(hashlib.md5(('\n'.join(out) + '\n').encode()).hexdigest())
PY
}
WANT_FILE="$TMP/want.md"
cp "skills/atlas/references/samples/书籍/百变小樱魔术卡_WIKI.md" "$WANT_FILE"
WANT=$(canon "$WANT_FILE")
$AB click ".doc-bar .ant-btn-primary" >/dev/null
$AB wait 1800 >/dev/null
GOT_FILE="$TMP/got.md"
$PSQL -c "select body_md from contents where node_id = '$NID'" > "$GOT_FILE"
GOT=$(canon "$GOT_FILE")
chk "定稿正文与原文规范形一致（md5）" "$GOT" "$WANT"
chk "定稿里 H1 还在" "$($PSQL -c "select case when position('# 《魔卡少女樱》' in body_md) > 0 then 'yes' else 'no' end from contents where node_id = '$NID'")" "yes"
chk "定稿里说明块还在" "$($PSQL -c "select case when position('> 说明：' in body_md) > 0 then 'yes' else 'no' end from contents where node_id = '$NID'")" "yes"
chk "定稿里题记围栏还在" "$($PSQL -c "select case when position(':::epigraph' in body_md) > 0 then 'yes' else 'no' end from contents where node_id = '$NID'")" "yes"
chk "定稿里表格还在（忽略对齐空格）" "$($PSQL -c "select case when position('|卷|卷名|' in replace(body_md, ' ', '')) > 0 then 'yes' else 'no' end from contents where node_id = '$NID'")" "yes"

echo
echo "—— $ok 通过 / $bad 失败 ——"
[ "$bad" = "0" ]

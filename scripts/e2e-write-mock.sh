#!/bin/bash
# 三段式管线的**免费自测**：用 `scripts/mock-llm.py` 顶替真模型，把
# 「研究 → 骨架 → 生成 → 过规则档 → 定稿 → 快照」整条链在真 PG / 真 API / 真 worker 上跑一遍。
#
# 它不动真 llm 配置：跑前把 secrets 的 llm.base_url/llm.api_key 换成 mock，**跑完必还原**（trap）。
# 不花钱、不联网。真跑（真模型）是 `scripts/e2e-write.sh`，那个要显式确认。
#
# 前置：api 8088 在跑（worker 由本脚本临时起）。
# 用法：bash scripts/e2e-write-mock.sh
set -euo pipefail
cd "$(dirname "$0")/.."

API=http://127.0.0.1:8088
PORT=8099
PW=$(grep -oP '(?<=wikiatlas:)[^@]+' .secrets/wikiatlas.pg.env)
export PGPASSWORD="$PW"
PSQL="psql -h 127.0.0.1 -U wikiatlas -d wikiatlas -qtA"

curl -s --retry 30 --retry-connrefused --retry-delay 1 -o /dev/null "$API/api/healthz"

echo "=== 起 mock 模型（:$PORT）==="
python3 scripts/mock-llm.py "$PORT" &
MOCK=$!
WORKER=""

restore() {
  local code=$?
  [ -n "${WORKER:-}" ] && kill "$WORKER" 2>/dev/null || true
  kill "$MOCK" 2>/dev/null || true
  $PSQL -c "update secrets set value='$OLD_BASE' where key='llm.base_url'" >/dev/null 2>&1 || true
  $PSQL -c "update secrets set value='$OLD_KEY' where key='llm.api_key'" >/dev/null 2>&1 || true
  exit $code
}
trap restore EXIT

OLD_BASE=$($PSQL -c "select value from secrets where key='llm.base_url'")
OLD_KEY=$($PSQL -c "select value from secrets where key='llm.api_key'")
$PSQL -c "update secrets set value='http://127.0.0.1:$PORT' where key='llm.base_url'" >/dev/null
$PSQL -c "update secrets set value='mock-not-real' where key='llm.api_key'" >/dev/null
echo "  llm.base_url → mock（跑完还原）"

echo "=== 起 worker ==="
set -a; . .secrets/wikiatlas.pg.env; set +a
cargo build -q -p wikiatlas-worker
./target/debug/wikiatlas-worker >/tmp/mock-worker.log 2>&1 &
WORKER=$!
sleep 2

echo "=== 跑规则档验收（NO_BROWSER=${NO_BROWSER:-1}）==="
NO_BROWSER="${NO_BROWSER:-1}" bash scripts/e2e-write.sh
echo "=== mock 自测通过；已还原真 llm 配置 ==="

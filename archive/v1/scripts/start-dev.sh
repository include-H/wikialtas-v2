#!/usr/bin/env bash
# start-dev.sh — WikiAltas dev 栈生命周期管理（唯一入口；旧 start-backend/sh、start-frontend/sh 是其包装）
#
# backend   : go build → setsid /tmp/wikiatlas-v2      log /tmp/wikiatlas-be-<后端端口>.log
# frontend  : vite dev (npm run dev)                   log /tmp/wikiatlas-fe-<前端端口>.log
# 状态      : pidfile 在 /tmp/wikiatlas-dev/backend-<后端端口>.pid（前端同理）；
#             按实例分开 + 按进程组精确 kill，两套栈（5173 主 / 5200 mock 联调）互不误伤
#
# 用法:
#   start-dev.sh [start|stop|restart|status|logs] [be|fe|both] [--force] [行数]
#     start   缺省。已运行的跳过（幂等）
#     stop    停 be/fe。停 be 前查 runs 表：有 running 工单就拒绝（--force 强制，工单会被中断）
#     status  进程/端口/health/running 工单
#     logs    tail -f；logs be|fe 单份，缺省两份；末位数字 = 回显行数（缺省 40）
#
# 环境变量（不设置 = 主实例 5173 → 8080，与 vite 默认一致）:
#   WIKIATLAS_ADDR                 后端监听      默认 127.0.0.1:8080
#   WIKIATLAS_DEV_PORT             前端端口      默认 5173
#   WIKIATLAS_API_TARGET           代理目标      默认 http://localhost:<ADDR端口>
#   WIKIATLAS_DB                   SQLite 路径   默认 backend/data/wikiatlas.db
#   WIKIATLAS_MOCK_WRITE_DELAY_MS  透传后端（mock 写入延迟）
#
# 例: start-dev.sh               # 起全部
#     start-dev.sh restart be    # 只重启后端（有 running 工单会拦下）
#     start-dev.sh stop --force  # 强停全部
#     start-dev.sh logs be 100   # 看后端日志
set -euo pipefail

ROOT="/root/WikiAltas"
export PATH="/usr/local/go/bin:/root/.nvm/versions/node/v24.20.0/bin:/usr/local/bin:/usr/bin:/bin"
export GOCACHE=/root/.cache/wikiatlas-go-build
# /tmp 是 tmpfs，这台机器上被别的活（固件镜像之类）占满过——满了 Go 的链接步骤会
# "no space left on device" 直接构建失败。把临时目录挪到 / 上，别让别人的临时文件
# 拖死我们的 dev 栈。可用 WIKIATLAS_TMPDIR 覆盖。
export TMPDIR="${WIKIATLAS_TMPDIR:-/root/.cache/wikiatlas-tmp}"
mkdir -p "$TMPDIR"

ADDR="${WIKIATLAS_ADDR:-127.0.0.1:8080}"
ADDR_PORT="${ADDR##*:}"
DEV_PORT="${WIKIATLAS_DEV_PORT:-5173}"
# 用 127.0.0.1 而不是 localhost：后端只绑 IPv4 回环，localhost 在有些环境先解到 ::1，
# 代理会先撞一次不可达再回退（Node 的 happy-eyeballs 能救，但没必要让它撞）。
API_TARGET="${WIKIATLAS_API_TARGET:-http://127.0.0.1:${ADDR_PORT}}"
DB="${WIKIATLAS_DB:-$ROOT/backend/data/wikiatlas.db}"
BINARY="/tmp/wikiatlas-v2"
STATE_DIR="/tmp/wikiatlas-dev"
# 一套实例 = 一个 (后端端口, 前端端口) 组合，pidfile 与日志都**按实例分开**。
# 共用一份 pidfile 的后果是实打实的：第二套起来时看到第一套的 pidfile，会以为
# "已在运行"而静默跳过；stop 更糟——它会按那个 pid 去杀**另一套**的进程。
# （踩过：主栈 5173 启动时读到联调栈 5200 的 pidfile，直接跳过；两边日志也串一起。）
BE_PIDFILE="$STATE_DIR/backend-$ADDR_PORT.pid"
FE_PIDFILE="$STATE_DIR/frontend-$DEV_PORT.pid"
BE_LOG="/tmp/wikiatlas-be-$ADDR_PORT.log"
FE_LOG="/tmp/wikiatlas-fe-$DEV_PORT.log"
SKILL_DIR="$ROOT/skills/wiki-writing"

say()  { printf '● %s\n' "$*"; }
warn() { printf '✗ %s\n' "$*" >&2; }
die()  { warn "$*"; exit 1; }
usage() { sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

# ---------- 探测 ----------
# live_pid <pidfile> <cmdline 特征>：pidfile 存在、进程存活、cmdline 对得上才输出 pid
live_pid() {
  local f=$1 expect=$2 pid
  [[ -f $f ]] || return 0
  pid=$(cat "$f" 2>/dev/null || true)
  [[ -n $pid && -d /proc/$pid ]] || return 0
  if tr '\0' ' ' < "/proc/$pid/cmdline" 2>/dev/null | grep -qF -- "$expect"; then
    echo "$pid"
  fi
}
be_pid() { live_pid "$BE_PIDFILE" "/tmp/wikiatl"; }
fe_pid() { live_pid "$FE_PIDFILE" "npm"; }

port_listening() { ss -ltn "sport = :$1" 2>/dev/null | grep -q LISTEN; }
port_pids() { ss -ltnp "sport = :$1" 2>/dev/null | grep -oP 'pid=\K[0-9]+' | sort -u; }
health_code() { curl -s -o /dev/null -w '%{http_code}' -m 2 "$1" 2>/dev/null || echo 000; }
wait_http() { # <url> <tries>（每 0.5s）
  local i code
  for ((i = 0; i < $2; i++)); do
    code=$(health_code "$1")
    [[ $code == 200 ]] && return 0
    sleep 0.5
  done
  return 1
}

# ---------- runs 护栏（交接纪律：重启后端前必须先查 running 工单） ----------
running_runs() { # 第 1 行 count，之后每行 id\tintent\tstarted_at
  python3 - "$DB" <<'EOF' 2>/dev/null || true
import sqlite3, sys
try:
    con = sqlite3.connect(sys.argv[1])
    rows = con.execute(
        "SELECT id, intent, started_at FROM runs WHERE status='running' ORDER BY started_at LIMIT 20"
    ).fetchall()
    print(len(rows))
    for r in rows:
        print(f"{r[0]}\t{r[1]}\t{r[2]}")
except Exception:
    pass
EOF
}
guard_runs() {
  [[ $FORCE -eq 1 ]] && return 0
  local out n
  out=$(running_runs)
  n=$(printf '%s\n' "$out" | head -n1)
  n=${n:-0}
  if [[ $n -gt 0 ]]; then
    warn "有 $n 个 running 工单，现在停 backend 会杀掉在跑的 run：$(printf '%s\n' "$out" | tail -n +2 | cut -f1-2 | paste -sd' ' -)"
    die "等工单跑完再停（scripts/watch-run.py 看进度），确认没影响再用 --force"
  fi
}

# ---------- 起 ----------
start_backend() {
  local pid
  pid=$(be_pid)
  [[ -n $pid ]] && { say "backend 已在运行 (pid $pid)，跳过"; return 0; }
  if port_listening "$ADDR_PORT"; then
    die "端口 $ADDR_PORT 被占用 (pids: $(port_pids "$ADDR_PORT" | paste -sd' ' -))；先 stop 或换 WIKIATLAS_ADDR"
  fi
  say "build backend…"
  ( cd "$ROOT/backend" && go build -o "$BINARY" ./cmd/wikiatlas ) || die "go build 失败"
  mkdir -p "$(dirname "$DB")" "$STATE_DIR"
  say "start backend @ $ADDR (db: $DB)"
  # 会话内进程自己写 pidfile：$! 记录不到真正的会话主进程（& 优先级 / setsid fork 两坑都免疫）
  setsid bash -c 'echo $$ > "$1" && exec "$2" -db "$3" -addr "$4" -seed -skill "$5"' _ \
    "$BE_PIDFILE" "$BINARY" "$DB" "$ADDR" "$SKILL_DIR" >"$BE_LOG" 2>&1 < /dev/null &
  if wait_http "http://$ADDR/api/health" 40; then
    say "backend ready (pid $(cat "$BE_PIDFILE"))  log: $BE_LOG"
  else
    warn "backend 未就绪："; tail -n 15 "$BE_LOG" 2>/dev/null || true
    return 1
  fi
}

start_frontend() {
  local pid
  pid=$(fe_pid)
  [[ -n $pid ]] && { say "frontend 已在运行 (pid $pid)，跳过"; return 0; }
  if port_listening "$DEV_PORT"; then
    die "端口 $DEV_PORT 被占用 (pids: $(port_pids "$DEV_PORT" | paste -sd' ' -))；先 stop 或换 WIKIATLAS_DEV_PORT"
  fi
  ( cd "$ROOT/frontend" && { [[ -d node_modules ]] || npm install; } ) || die "npm install 失败"
  mkdir -p "$STATE_DIR"
  # 这两个变量必须**进到 vite 进程里**：vite.config.ts 读它们决定端口与代理目标。
  # 只算出来打印、不 export 的话，第二套实例会把 /api 代理到默认的 8080 并一路挂住
  # （踩过：5200 → 8090 的联调栈，所有 /api 都是 connect ETIMEDOUT 127.0.0.1:8080）。
  export WIKIATLAS_DEV_PORT="$DEV_PORT"
  export WIKIATLAS_API_TARGET="$API_TARGET"
  say "start frontend @ :$DEV_PORT (proxy /api → $API_TARGET)"
  setsid bash -c 'cd "$1" && echo $$ > "$2" && exec npm run dev' _ \
    "$ROOT/frontend" "$FE_PIDFILE" >"$FE_LOG" 2>&1 < /dev/null &
  if wait_http "http://127.0.0.1:$DEV_PORT/" 60; then
    if [[ -n $(be_pid) ]]; then
      if wait_http "http://127.0.0.1:$DEV_PORT/api/health" 10; then
        say "frontend ready  ui: http://localhost:$DEV_PORT  log: $FE_LOG"
      else
        say "frontend ready，但 /api 尚未 200（backend 可能刚起）  log: $FE_LOG"
      fi
    else
      say "frontend ready  注意：backend 没起，/api 会 502  log: $FE_LOG"
    fi
  else
    warn "frontend 未就绪："; tail -n 15 "$FE_LOG" 2>/dev/null || true
    return 1
  fi
}

# ---------- 停 ----------
stop_one() { # <be|fe>
  local kind=$1 pidfile expect pid killed=0
  case $kind in
    be) pidfile=$BE_PIDFILE; expect="/tmp/wikiatl" ;;
    fe) pidfile=$FE_PIDFILE; expect="npm" ;;
  esac
  pid=$(live_pid "$pidfile" "$expect")
  if [[ -z $pid ]]; then
    # pidfile 失联兜底：**按自己的端口**找孤儿。
    # 不能按二进制名 pgrep -f（两套栈跑的是同一个 /tmp/wikiatlas-v2）：那会把另一套的
    # 后端一起杀掉（踩过：重启 8090 联调栈，把 8080 主栈的后端也带走了）。
    local orphans=""
    case $kind in
      be) orphans=$(port_pids "$ADDR_PORT" | paste -sd' ' -) ;;
      fe) orphans=$(port_pids "$DEV_PORT" | paste -sd' ' -) ;;
    esac
    [[ -n $orphans ]] && kill $orphans 2>/dev/null || true
    rm -f "$pidfile"
    if [[ -n ${orphans:-} ]]; then
      say "$kind 无 pidfile 但找到孤儿 (pids: $orphans) 已清理"
    else
      say "$kind 未在运行"
    fi
    return 0
  fi
  # setsid 启动 → pid 就是进程组号，整组 kill（fe 的 vite 在组内）
  kill -- "-$pid" 2>/dev/null || kill "$pid" 2>/dev/null || true
  for ((i = 0; i < 40; i++)); do
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.25
  done
  if kill -0 "$pid" 2>/dev/null; then
    warn "$kind pid $pid 未及时退出，kill -9"
    kill -9 -- "-$pid" 2>/dev/null || kill -9 "$pid" 2>/dev/null || true
  fi
  rm -f "$pidfile"
  say "$kind stopped (pid $pid)"
}

# ---------- status / logs ----------
show_status() {
  local pid
  pid=$(be_pid)
  if [[ -n $pid ]]; then
    printf 'backend   : RUNNING  pid=%s  up=%s  @ %s\n' "$pid" "$(ps -o etime= -p "$pid" | tr -d ' ')" "$ADDR"
    printf '            health=%s  log=%s\n' "$(health_code "http://$ADDR/api/health")" "$BE_LOG"
  else
    printf 'backend   : STOPPED%s\n' "$(port_listening "$ADDR_PORT" && echo '  ⚠ 端口被未知进程占用')"
  fi
  pid=$(fe_pid)
  if [[ -n $pid ]]; then
    printf 'frontend  : RUNNING  pid=%s  up=%s  @ :%s → %s\n' "$pid" "$(ps -o etime= -p "$pid" | tr -d ' ')" "$DEV_PORT" "$API_TARGET"
    printf '            ui=http://localhost:%s  log=%s\n' "$DEV_PORT" "$FE_LOG"
  else
    printf 'frontend  : STOPPED%s\n' "$(port_listening "$DEV_PORT" && echo '  ⚠ 端口被未知进程占用')"
  fi
  local out n
  out=$(running_runs); n=$(printf '%s\n' "$out" | head -n1)
  printf 'running runs: %s\n' "${n:-0}"
}

# ---------- 参数 ----------
CMD="${1:-start}"; shift 2>/dev/null || true
FORCE=0; TARGETS="be fe"; LINES=40
for a in "$@"; do
  case $a in
    --force) FORCE=1 ;;
    # `both` 要展开成 "be fe"：run_over 只认 be|fe 两个值，把字面量 both 传下去
    # 会让它一个分支都不匹配 —— 静默什么都不做（踩过：start both 假装成功）。
    be|fe) TARGETS=$a ;;
    both) TARGETS="be fe" ;;
    ''|logs) : ;;
    [0-9]*) LINES=$a ;;
    -h|--help) usage 0 ;;
    *) die "未知参数: $a（子命令 start|stop|restart|status|logs，target be|fe|both，--force，行数）" ;;
  esac
done

# 按 be→fe 顺序跑 $1
run_over() {
  local t rc=0
  for t in $TARGETS; do
    case $t in be) "do_$1_be" ;; fe) "do_$1_fe" ;; esac || rc=1
  done
  return $rc
}
do_start_be() { start_backend; }
do_stop_be() {
  # 护栏只在 be 真活着（pidfile 或孤儿）时才有意义；be 死了，running 行就是残留，不该卡 stop
  local live
  live=$(be_pid)
  # 同上：按端口判存活，别按二进制名（否则另一套栈在跑就会误判"我这套还活着"）
  [[ -n $live ]] || live=$(port_pids "$ADDR_PORT" | head -n1 || true)
  [[ -n $live ]] && guard_runs
  stop_one be
}
do_start_fe() { start_frontend; }
do_stop_fe() { stop_one fe; }

case $CMD in
  start)   run_over start ;;
  stop)    run_over stop; say "dev 栈已停止" ;;
  restart) run_over stop; run_over start ;;
  status)  show_status ;;
  logs)
    FILES=()
    for t in $TARGETS; do case $t in be) FILES+=("$BE_LOG") ;; fe) FILES+=("$FE_LOG") ;; esac; done
    exec tail -n "$LINES" -f "${FILES[@]}" ;;
  -h|--help|help) usage 0 ;;
  *) usage 1 ;;
esac

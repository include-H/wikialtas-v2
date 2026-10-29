#!/bin/bash
# 数据库备份（`docs/database.md §6`：pg_dump 定时 + 迁移前后各一次）。
#
# 用法：bash scripts/backup.sh [输出目录]
# 产物：<输出目录>/wikiatlas-YYYYmmdd-HHMMSS.dump（pg_dump 自定义格式，可 pg_restore）
#
# 恢复：
#   createdb -h 127.0.0.1 -U wikiatlas wikiatlas_restore
#   pg_restore -h 127.0.0.1 -U wikiatlas -d wikiatlas_restore --no-owner <文件>.dump
set -euo pipefail
cd "$(dirname "$0")/.."

OUT="${1:-.backups}"
mkdir -p "$OUT"
PW=$(grep -oP '(?<=wikiatlas:)[^@]+' .secrets/wikiatlas.pg.env)
export PGPASSWORD="$PW"

STAMP=$(date +%Y%m%d-%H%M%S)
FILE="$OUT/wikiatlas-$STAMP.dump"
pg_dump -h 127.0.0.1 -U wikiatlas -d wikiatlas -Fc -f "$FILE"
echo "已备份：$FILE（$(du -h "$FILE" | cut -f1)）"

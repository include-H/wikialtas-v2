#!/bin/bash
# 兼容包装：生命周期统一由 start-dev.sh 管理（stop/restart/status/logs 见 start-dev.sh）
# 旧用法 WIKIATLAS_ADDR 仍支持
exec "$(dirname "$0")/start-dev.sh" start be

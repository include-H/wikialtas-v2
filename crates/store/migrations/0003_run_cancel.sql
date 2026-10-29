-- 取消运行（`docs/backend.md §1`）：只置标志，worker 在工具边界检查。
-- 不依赖进程内 CancellationToken，worker 崩溃/重启后也能看到。
ALTER TABLE runs ADD COLUMN cancel_requested boolean NOT NULL DEFAULT false;

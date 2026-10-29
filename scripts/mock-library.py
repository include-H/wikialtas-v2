#!/usr/bin/env python3
"""M7 验收用的本地 GameAtlas + Komga 双源仿真。

只服务 Atlas 实际调用的端点，并把写回载荷留在内存里供断言：
  GET  /__state
  POST /__control  {"mode":"relink"}
"""

import json
import os
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse

LOCK = threading.Lock()

GA_OLD = {
    "public_id": "sync-e2e-old",
    "title": "同步测试游戏",
    "release_date": "2020-01-02",
    "series": {"id": 77, "name": "同步测试系列"},
}
GA_NEW = {
    "public_id": "sync-e2e-new",
    "title": "同步测试游戏 重制版",
    "release_date": "2024-03-04",
    "series": {"id": 77, "name": "同步测试系列"},
}

STATE = {
    "mode": "normal",
    "ga": [GA_OLD],
    "ga_pushes": [],
    "komga_patches": [],
}


class Handler(BaseHTTPRequestHandler):
    server_version = "AtlasMock/1.0"

    def log_message(self, fmt, *args):
        return

    def send_json(self, status, payload, headers=None):
        raw = json.dumps(payload, ensure_ascii=False).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(raw)))
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(raw)

    def body(self):
        n = int(self.headers.get("Content-Length", "0"))
        if n == 0:
            return {}
        return json.loads(self.rfile.read(n) or b"{}")

    def do_GET(self):
        path = urlparse(self.path).path
        if path == "/__state":
            with LOCK:
                return self.send_json(200, STATE)

        if path == "/api/games/all":
            if "gameatlas_admin=sess-e2e" not in self.headers.get("Cookie", ""):
                return self.send_json(401, {"success": False, "error": "需要管理员登录", "data": None})
            with LOCK:
                games = list(STATE["ga"])
            return self.send_json(200, {"success": True, "data": games})

        if path == "/api/v1/series":
            series = [{
                "id": "sync-e2e-komga",
                "library_id": "lib",
                "name": "同步测试漫画",
                "metadata": {
                    "title": "同步测试漫画",
                    "title_sort": "sync",
                    "status": "ONGOING",
                    "summary": "",
                    "publisher": "E2E",
                    "language": "zh",
                },
            }]
            return self.send_json(200, {"content": series, "totalPages": 1})

        if path == "/api/v1/series/sync-e2e-komga/books":
            books = [{
                "id": "sync-e2e-volume-1",
                "name": "第1卷",
                "media": {"mediaType": "application/zip"},
                "metadata": {"title": "第1卷"},
            }]
            return self.send_json(200, {"content": books, "totalPages": 1})

        return self.send_json(404, {"error": "not found"})

    def do_POST(self):
        path = urlparse(self.path).path
        if path == "/__control":
            body = self.body()
            with LOCK:
                STATE["mode"] = body.get("mode", "normal")
                if STATE["mode"] == "relink":
                    STATE["ga"] = [GA_NEW]
                else:
                    STATE["ga"] = [GA_OLD]
            return self.send_json(200, {"ok": True, "mode": STATE["mode"]})

        if path == "/api/auth/login":
            body = self.body()
            if body.get("password") != "e2e-pass":
                return self.send_json(401, {"success": False, "error": "密码错误", "data": None})
            return self.send_json(
                200,
                {"success": True, "data": {"is_admin": True}},
                {"Set-Cookie": "gameatlas_admin=sess-e2e; Path=/; HttpOnly"},
            )

        return self.send_json(404, {"error": "not found"})

    def do_PUT(self):
        path = urlparse(self.path).path
        if path.startswith("/api/games/") and path.endswith("/wiki"):
            if "gameatlas_admin=sess-e2e" not in self.headers.get("Cookie", ""):
                return self.send_json(401, {"success": False, "error": "需要管理员登录", "data": None})
            public_id = path.removeprefix("/api/games/").removesuffix("/wiki")
            body = self.body()
            with LOCK:
                STATE["ga_pushes"].append({"id": public_id, "body": body})
            return self.send_json(200, {"success": True, "data": {"game_id": 1}})
        return self.send_json(404, {"error": "not found"})

    def do_PATCH(self):
        path = urlparse(self.path).path
        if path == "/api/v1/series/sync-e2e-komga/metadata":
            if self.headers.get("X-API-Key") != "e2e-key":
                return self.send_json(401, {"error": "bad key"})
            with LOCK:
                STATE["komga_patches"].append(self.body())
            self.send_response(204)
            self.end_headers()
            return None
        return self.send_json(404, {"error": "not found"})


if __name__ == "__main__":
    port = int(os.environ.get("MOCK_PORT", "18090"))
    print(json.dumps({"port": port}), flush=True)
    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()

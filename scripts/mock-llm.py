#!/usr/bin/env python3
"""极简 Responses API mock（只为端到端自测三段式管线，**不调真模型**）。

它按请求里的系统提示判断当前是哪一段（研究 / 骨架 / 生成），回放脚本化的输出：
- 研究段：逐轮 `ledger_put`，把游戏介质 12 个必填字段与 ≥5 个来源补齐；
- 骨架段：回一份 9 章 JSON 骨架；
- 生成段：从 user 消息里抠出「锚点」，回一次 `patch_section`（每章一次）。

用法：python3 scripts/mock-llm.py [port]，默认 8099。
"""
import json
import os
import re
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

# ---- 研究段脚本：字段 / 断言 / 来源 URL / 标题 ----
SRC = {
    "S1": ("Falcom 官方商品页", "https://www.falcom.co.jp/sora_fc/"),
    "S2": ("综合百科条目", "https://zh.wikipedia.org/wiki/英雄传说_空之轨迹"),
    "S3": ("专业评测站", "https://www.famitsu.example/sora"),
    "S4": ("游戏数据库", "https://www.mobygames.example/sora-fc"),
    "S5": ("官方新闻稿", "https://www.falcom.co.jp/news/sora",
           ),
    "S6": ("媒体访谈", "https://dengeki.example/sora-fc-interview"),
}
# (字段, 断言, 来源, 状态)
RESEARCH = [
    ("原名", "英雄传说 空之轨迹 FC", "S1", "已核实"),
    ("开发商", "日本 Falcom", "S1", "已核实"),
    ("发行商", "日本 Falcom", "S2", "已核实"),
    ("首发日期", "2004-06-24（Windows）", "S1", "已核实"),
    ("首发平台", "Windows", "S1", "已核实"),
    ("版本关系", "FC / SC / the 3rd 三部曲第一部", "S2", "已核实"),
    ("世界观", "利贝尔王国与导力器文明", "S2", "已核实"),
    ("剧情", "游击士艾丝蒂尔与约修亚寻找父亲的旅程", "S2", "已核实"),
    ("玩法机制·导力器", "导力器回路决定魔法与属性", "S3", "已核实"),
    ("玩法机制·游击士任务", "游击士协会接受委托推进支线", "S3", "已核实"),
    ("玩法机制·战斗系统", "回合制战棋与 AT 条决定行动顺序", "S3", "已核实"),
    ("主要角色·艾丝蒂尔", "主角，游击士见习生", "S4", "已核实"),
    ("主要角色·约修亚", "艾丝蒂尔的义弟与搭档", "S4", "已核实"),
    ("主要角色·雪拉", "游击士前辈", "S4", "已核实"),
    ("主要角色·奥利维尔", "来自帝国的旅行者", "S4", "已核实"),
    ("评价", "Fami通 32 分，进入金殿堂", "S3", "已核实"),
    ("题记素材", "「我是游击士。」——艾丝蒂尔", "S6", "已核实"),
]


def frame(ty: str, obj: dict) -> bytes:
    return f"event: {ty}\ndata: {json.dumps(obj, ensure_ascii=False)}\n\n".encode()


def text_stream(text: str) -> bytes:
    out = b""
    for ch in text:
        out += frame("response.output_text.delta", {"type": "response.output_text.delta", "delta": ch})
    out += frame("response.completed", {"type": "response.completed", "response": {"output": [], "usage": {"total_tokens": 1}}})
    return out


def call_stream(name: str, args: dict) -> bytes:
    item_id, call_id = "i1", "c1"
    arg_json = json.dumps(args, ensure_ascii=False)
    out = frame("response.output_item.added", {
        "type": "response.output_item.added",
        "item": {"id": item_id, "type": "function_call", "name": name, "call_id": call_id},
    })
    out += frame("response.function_call_arguments.delta", {
        "type": "response.function_call_arguments.delta", "item_id": item_id, "delta": arg_json,
    })
    out += frame("response.function_call_arguments.done", {
        "type": "response.function_call_arguments.done", "item_id": item_id, "name": name, "arguments": arg_json,
    })
    out += frame("response.completed", {
        "type": "response.completed",
        "response": {"output": [{"type": "function_call", "name": name}], "usage": {"total_tokens": 1}},
    })
    return out


OUTLINE = {
    "title": "# 空之轨迹 FC（游戏）Wiki",
    "note": "本条目依据官方资料整理，含完整剧透，检索时间 2026-10。",
    "epigraph": {"kind": "quote", "source_id": "S1"},
    "chapters": [
        {"no": 1, "name": "作品概览", "sections": [{"name": "是什么", "ledger_rows": [1]}]},
        {"no": 2, "name": "基础信息速览", "sections": [{"name": "基本资料", "ledger_rows": [1, 2, 4]}]},
        {"no": 3, "name": "世界观与背景设定", "sections": [{"name": "世界规则", "ledger_rows": [7]}]},
        {"no": 4, "name": "剧情概要", "sections": [{"name": "主线", "ledger_rows": [8]}]},
        {"no": 5, "name": "玩法与结构特色", "sections": [{"name": "战斗", "ledger_rows": [9, 10, 11]}]},
        {"no": 6, "name": "主要角色与势力", "sections": [{"name": "角色", "ledger_rows": [12, 13, 14, 15]}]},
        {"no": 7, "name": "评价与历史定位", "sections": [{"name": "评价", "ledger_rows": [16]}]},
        {"no": 8, "name": "一句话总结", "sections": [{"name": "总结", "ledger_rows": [1]}]},
        {"no": 9, "name": "参考资料", "sections": [{"name": "来源", "ledger_rows": [1, 2, 3, 4, 5]}]},
    ],
}
OUTLINE_JSON = json.dumps(OUTLINE, ensure_ascii=False)

FILLER = (
    "这一段落到具体对象上：艾丝蒂尔在洛连特的街道上接受委托，按导力器回路调整战术。"
    "游击士协会的委托分主线与支线，主线推进到利贝尔各都市，支线补充人物关系。"
    "战斗按 AT 条决定行动顺序，玩家据此安排魔法与战技的释放时机。"
) * 12

# 账本里真正会出现的来源（决定参考资料能引用哪几个；宿主按去重 URL 顺序编号）
USED = ["S1", "S2", "S3", "S4", "S5"]


def gen_body(anchor: str) -> str:
    if anchor.startswith("# "):
        return (
            anchor + "\n\n"
            "> 说明：本条目依据 Falcom 官方资料与专业评测整理，含完整剧透；主要来源见参考资料，检索时间 2026-10。\n\n"
            ":::epigraph\n「我是游击士。」\n——艾丝蒂尔《英雄传说 空之轨迹 FC》\n:::\n"
        )
    if "参考资料" in anchor:
        lines = [anchor, ""]
        for sid in USED:
            title, url = SRC[sid]
            lines.append(f"- [{sid}] {title} — {url} — 抓取 2026-10-12")
        return "\n".join(lines) + "\n"
    if "一句话总结" in anchor:
        return anchor + "\n\n《空之轨迹 FC》以导力器文明与游击士制度搭起舞台，用一部公路叙事把系列打开。\n"
    return anchor + "\n\n" + FILLER + "\n"


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):  # 保持安静
        pass

    def do_POST(self):
        # 取消验收用：每个模型往返慢一点，给 /cancel 留出窗口。
        if os.environ.get("MOCK_SLOW") == "1":
            time.sleep(1.5)
        n = int(self.headers.get("Content-Length", 0))
        body = json.loads(self.rfile.read(n) or b"{}")
        blob = json.dumps(body, ensure_ascii=False)
        system = ""
        user = ""
        for item in body.get("input", []):
            role = item.get("role")
            for part in item.get("content", []) if isinstance(item.get("content"), list) else []:
                if part.get("type") == "input_text":
                    if role == "system":
                        system = part.get("text", "")
                    elif role == "user":
                        user = part.get("text", "")
        tools = [t.get("name") for t in body.get("tools", [])]
        calls = [
            it.get("name")
            for it in body.get("input", [])
            if it.get("type") == "function_call"
        ]

        # 通用 Harness 工具面验收：只走确定性的工具序列，不模拟正文质量。
        # 用测试口令判断，不能用 system 里的「当前运行约束」——根 Atlas 提示也有这一段。
        if "HARNESS_TEST_" in user:
            phase = "harness"
            history_text = "\n".join(
                part.get("text", "")
                for item in body.get("input", [])
                if item.get("role") in ("user", "assistant")
                for part in (item.get("content", []) if isinstance(item.get("content"), list) else [])
                if part.get("type") == "input_text"
            )
            if "HARNESS_TEST_SKILL" in user:
                loaded = "用户显式加载的技能" in system and "# Atlas Skill 路由" in system
                web = "search_web" in tools and "fetch_url" in tools
                if "read_skill" not in calls:
                    data = call_stream("read_skill", {"name": "wiki-write"})
                elif loaded and web and "# 写正文之前" in blob:
                    data = call_stream("answer", {
                        "text": "总入口已加载，子模块可按需读取，联网工具始终可用。",
                        "refs": [],
                    })
                else:
                    data = text_stream("总入口、子模块或联网工具缺失。")
            elif "HARNESS_TEST_MEMORY" in user:
                remembered = "HARNESS_TEST_EDIT" in history_text
                data = call_stream("answer", {
                    "text": "我记得上一轮是编辑工具验收。" if remembered else "我没有拿到上一轮上下文。",
                    "refs": [],
                })
            elif "HARNESS_TEST_READ" in user:
                if "edit" not in calls:
                    data = call_stream("edit", {
                        "old_string": "青羽测试",
                        "new_string": "不该写入",
                    })
                else:
                    data = text_stream("只读工具面验收完成。")
            else:
                if "todo_write" not in calls:
                    data = call_stream("todo_write", {
                        "todos": [{"text": "验收编辑工具", "status": "in_progress"}]
                    })
                elif "upsert_work" not in calls:
                    data = call_stream("upsert_work", {
                        "title": "工具创建的节点",
                        "kind": "work",
                        "medium": "game",
                    })
                elif "edit" not in calls:
                    data = call_stream("edit", {
                        "old_string": "青羽测试",
                        "new_string": "青羽工具验收",
                    })
                elif "answer" not in calls:
                    data = call_stream("answer", {
                        "text": "工具验收完成。",
                        "refs": [],
                    })
                else:
                    data = text_stream("工具验收完成。")
        # 根 Atlas 循环（D71）：用户要整篇 → 调一次 write_wiki，拿到结果后 answer。
        elif "你是 Atlas，Atlas 的馆员" in system:
            phase = "atlas"
        # 用各段独有的前缀判断（技能目录里也会出现「骨架」等词，不能用宽词）
        elif "只查资料、记事实账本" in system:
            phase = "research"
        elif "patch_section" in system and "写**正文**" in system:
            phase = "generation"
        elif "只输出一个 JSON 对象" in system:
            phase = "outline"
        else:
            phase = "research"

        if phase == "harness":
            pass
        elif phase == "atlas":
            if "write_wiki" not in calls:
                m = re.search(r"当前作品节点 id：([0-9a-fA-F-]+)", user)
                data = call_stream("write_wiki", {"node_id": m.group(1) if m else ""})
            else:
                data = call_stream("answer", {
                    "text": "整篇草稿已写好，请过目。",
                    "refs": [],
                })
        elif phase == "outline":
            data = text_stream(OUTLINE_JSON)
        elif phase == "generation":
            m = re.search(r"锚点必须是「(.+?)」", user)
            anchor = m.group(1) if m else "# 空之轨迹 FC（游戏）Wiki"
            data = call_stream("patch_section", {
                "node_id": "", "anchor": anchor, "new_markdown": gen_body(anchor),
            })
        else:
            # 研究段：数一数已经发生了几次 ledger_put（只数 input 里的函数调用项，
            # 别把 tools 定义里的 `"name": "ledger_put"` 也算进去）
            done = sum(
                1
                for it in body.get("input", [])
                if it.get("type") == "function_call" and it.get("name") == "ledger_put"
            )
            if done < len(RESEARCH):
                field, claim, sid, status = RESEARCH[done]
                title, url = SRC[sid]
                data = call_stream("ledger_put", {
                    "field": field, "claim": claim, "status": status,
                    "source_title": title, "source_url": url, "quote": claim[:50],
                })
            else:
                data = text_stream("研究完成。")

        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream; charset=utf-8")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "close")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)
        self.wfile.flush()


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8099
    print(f"mock-llm listening on 127.0.0.1:{port}", flush=True)
    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()

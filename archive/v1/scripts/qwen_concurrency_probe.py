#!/usr/bin/env python3
"""
qwen_concurrency_probe.py -- probe how much concurrency a Qwen "Responses" API
endpoint can hold, using the OpenAI **Responses** protocol:

    POST <base-url>/responses

Target defaults to the internal gateway on this net (plain HTTP -- that host's
HTTPS/TLS is broken, so do NOT pass an https:// base URL unless you know why):

    http://10.78.1.254:8081/v1

Stdlib only (no pip). The API key comes from the env (QWEN_KEY / WIKIATLAS_KEY /
OPENAI_API_KEY) or --key and is never printed. Do not commit the key.

Modes
  sweep   (default)  fire batches at doubling concurrency levels; report where
                     it degrades (hard-fail rate > --fail-limit or p95 latency
                     > --latency-factor x baseline) -- "the ceiling".
  soak             hold a fixed concurrency in-flight for --duration seconds;
                     steady-state success rate + latency (finds slow errors that
                     only show over time).
  once             fire a single batch at --level concurrency.
  models           list GET <base-url>/models and exit.

Examples
  QWEN_KEY=sk-... python3 qwen_concurrency_probe.py --models
  QWEN_KEY=sk-... python3 qwen_concurrency_probe.py --max-level 64
  QWEN_KEY=sk-... python3 qwen_concurrency_probe.py --mode soak --level 24 --duration 60
  QWEN_KEY=sk-... python3 qwen_concurrency_probe.py --mode once --level 16 --long
  QWEN_KEY=sk-... python3 qwen_concurrency_probe.py --levels 1,2,4,8,16 --rounds 5
"""
from __future__ import annotations

import argparse
import json
import math
import os
import socket
import ssl
import statistics
import sys
import threading
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor

DEFAULT_BASE = "http://10.78.1.254:8081/v1"
DEFAULT_MODEL = "Qwen"
ENV_NAMES = ("QWEN_KEY", "WIKIATLAS_KEY", "OPENAI_API_KEY")


def short(s, n=90):
    s = str(s).replace(chr(10), " ")
    return s if len(s) <= n else s[:n] + "\u2026(+%d)" % (len(s)-n)

def mask(key: str) -> str:
    if not key:
        return "(none)"
    return key[:3] + "..." + key[-4:] if len(key) > 8 else "****"


class Result:
    __slots__ = ("ok", "soft", "fail", "status", "lat", "err", "out_tokens")

    def __init__(self):
        self.ok = False     # HTTP 200 and response.status == "completed"
        self.soft = False   # HTTP 200 but status "incomplete" (content cut)
        self.fail = False   # hard failure: 4xx/5xx / timeout / conn / tls
        self.status = None  # int http status or None
        self.lat = 0.0      # seconds
        self.err = None     # short error tag, e.g. "http 429", "timeout"
        self.out_tokens = None


def build_payload(cfg, stream: bool) -> bytes:
    p = {"model": cfg.model, "input": cfg.prompt, "stream": stream}
    if cfg.max_tokens:
        p["max_output_tokens"] = cfg.max_tokens
    return json.dumps(p).encode("utf-8")


def err_code(err) -> str:
    """Stable short class used for grouping in the table."""
    if not err:
        return "ok"
    e = err.strip()
    if e.startswith("http"):
        parts = e.split()
        return (parts[0] + " " + parts[1]) if len(parts) > 1 else "http?"
    return e.split()[0]


def read_sse(resp) -> bool:
    """Consume an SSE stream to EOF; return True if a completion event was seen."""
    seen = False
    try:
        for raw in resp:
            line = raw.decode("utf-8", "replace")
            if "response.completed" in line:
                seen = True
    except Exception:
        pass
    return seen


def do_request(cfg, barrier) -> Result:
    r = Result()
    url = cfg.base_url.rstrip("/") + "/responses"
    ctx = None
    if url.startswith("https"):
        ctx = ssl.create_default_context()
        if cfg.insecure:
            ctx.check_hostname = False
            ctx.verify_mode = ssl.CERT_NONE
    headers = {
        "Authorization": "Bearer " + cfg.key,
        "Content-Type": "application/json",
        "Accept": "text/event-stream" if cfg.stream else "application/json",
    }
    req = urllib.request.Request(url, data=build_payload(cfg, cfg.stream),
                                 headers=headers, method="POST")
    if barrier is not None:
        barrier.wait()
    t0 = time.perf_counter()
    try:
        with urllib.request.urlopen(req, timeout=cfg.timeout, context=ctx) as resp:
            r.status = resp.status
            if resp.status >= 400:
                r.fail = True
                r.err = "http %d" % resp.status
            elif cfg.stream:
                r.ok = read_sse(resp)
                if not r.ok:
                    r.err = "stream-no-complete"
                r.out_tokens = None
            else:
                raw = resp.read()
                try:
                    data = json.loads(raw.decode("utf-8", "replace"))
                    st = data.get("status")
                    r.out_tokens = (data.get("usage") or {}).get("output_tokens")
                    if st == "completed":
                        r.ok = True
                    else:
                        r.soft = True
                        r.err = "incomplete"
                except Exception:
                    r.soft = True
                    r.err = "non-json-200"
    except urllib.error.HTTPError as e:
        r.status = e.code
        r.fail = True
        try:
            body = e.read().decode("utf-8", "replace")[:200]
        except Exception:
            body = ""
        r.err = "http %d" % e.code
        if body:
            r.err += " %s" % body
    except (socket.timeout, TimeoutError):
        r.fail = True
        r.err = "timeout"
    except urllib.error.URLError as e:
        r.fail = True
        r.err = "urlerr %s" % str(getattr(e, "reason", e))[:60]
    except ssl.SSLError as e:
        r.fail = True
        r.err = "tls %s" % str(e)[:60]
    except (ConnectionError, OSError) as e:
        r.fail = True
        r.err = "conn %s" % str(e)[:60]
    except Exception as e:  # noqa: BLE001 - never drop a request silently
        r.fail = True
        r.err = "exc %s:%s" % (type(e).__name__, str(e)[:60])
    r.lat = time.perf_counter() - t0
    return r


def pct(vals, p: float) -> float:
    if not vals:
        return 0.0
    vals = sorted(vals)
    if len(vals) == 1:
        return vals[0]
    k = (len(vals) - 1) * p / 100.0
    f, c = math.floor(k), math.ceil(k)
    if f == c:
        return vals[int(k)]
    return vals[f] * (c - k) + vals[c] * (k - f)


def summarize(results) -> dict:
    n = len(results)
    ok = sum(1 for x in results if x.ok)
    soft = sum(1 for x in results if x.soft)
    fail = sum(1 for x in results if x.fail)
    oklat = [x.lat for x in results if x.ok]
    alllat = [x.lat for x in results]
    errs: dict[str, int] = {}
    for x in results:
        if not x.ok:
            c = err_code(x.err)
            errs[c] = errs.get(c, 0) + 1
    return {
        "n": n, "ok": ok, "soft": soft, "fail": fail,
        "ok_pct": (100.0 * ok / n) if n else 0.0,
        "lat_min": min(alllat) if alllat else 0.0,
        "lat_mean": statistics.fmean(alllat) if alllat else 0.0,
        "lat_p50": pct(oklat, 50), "lat_p90": pct(oklat, 90),
        "lat_p95": pct(oklat, 95), "lat_p99": pct(oklat, 99),
        "lat_max": max(alllat) if alllat else 0.0,
        "errs": errs,
    }


def run_batch(cfg, C: int, simultaneous: bool = True) -> list[Result]:
    if C <= 0:
        return []
    bar = threading.Barrier(C) if (simultaneous and C > 1) else None
    with ThreadPoolExecutor(max_workers=C) as ex:
        futs = [ex.submit(do_request, cfg, bar) for _ in range(C)]
        return [f.result() for f in futs]


def parse_levels(cfg) -> list[int]:
    if cfg.levels:
        out = [int(t) for t in cfg.levels.replace(" ", "").split(",") if t]
        return out
    lv, last = [cfg.min_level or 1], (cfg.min_level or 1)
    while last * 2 <= cfg.max_level:
        last *= 2
        lv.append(last)
    if last < cfg.max_level:
        lv.append(cfg.max_level)
    return lv


HDR = "   C   reqs   ok  fail  soft  ok%    p50   p90   p95   p99    max  rps   verdict"


def print_row(L, s, wall, verdict):
    def f(x):
        return f"{x:6.2f}"
    print(f"{L:5d} {s['n']:6d} {s['ok']:5d} {s['fail']:6d} {s['soft']:6d} "
          f"{s['ok_pct']:6.1f} {f(s['lat_p50'])} {f(s['lat_p90'])} "
          f"{f(s['lat_p95'])} {f(s['lat_p99'])} {f(s['lat_max'])} "
          f"{(s['n']/wall if wall else 0):6.1f}   {verdict}")
    if s["errs"]:
        top = sorted(s["errs"].items(), key=lambda kv: -kv[1])
        print(f"           errs: " + ", ".join(f"{k}={v}" for k, v in top[:6]))


def sweep(cfg) -> int:
    levels = parse_levels(cfg)
    print(HDR)
    baseline = None
    ceiling_msg = None
    prev_stable = None
    degraded_at = None
    for i, L in enumerate(levels):
        allres, t0 = [], time.perf_counter()
        for _ in range(max(1, cfg.rounds)):
            allres.extend(run_batch(cfg, L))
        wall = time.perf_counter() - t0
        s = summarize(allres)

        degraded, why = False, ""
        hard_rate = (s["fail"] / s["n"]) if s["n"] else 0.0
        if hard_rate > cfg.fail_limit:
            degraded, why = True, f"fail {hard_rate*100:.0f}%>{cfg.fail_limit*100:.0f}%"
        if baseline is not None and s["n"] > 1 and s["lat_p95"] > cfg.latency_factor * baseline and baseline > 0:
            degraded = True
            why += ("; " if why else "") + f"p95 {s['lat_p95']:.2f}s>{cfg.latency_factor}x base {baseline:.2f}s"
        if not degraded and s["soft"] / max(1, s["n"]) > cfg.fail_limit:
            degraded = True
            why += ("; " if why else "") + f"incomplete {s['soft']/s['n']*100:.0f}%"

        if baseline is None:
            baseline = s["lat_p95"]
        verdict = "OK" if not degraded else f"DEGRADED ({why})"
        print_row(L, s, wall, verdict)

        if not degraded:
            prev_stable = L
        elif degraded_at is None:
            degraded_at = L

        if degraded and not cfg.no_stop:
            ceiling_msg = (f"Ceiling: first degradation at C={L}; "
                           f"stable up to C={prev_stable if prev_stable else 1}.")
            break
    print()
    if ceiling_msg:
        print(ceiling_msg)
    else:
        print(f"No degradation observed up to C={levels[-1]} (rounds={cfg.rounds}). "
              f"Try --max-level higher or --mode soak for sustained load.")
    return 0


def soak(cfg) -> int:
    level = cfg.level or 8
    dur = cfg.duration
    stop = threading.Event()
    res, lock = [], threading.Lock()

    def worker():
        while not stop.is_set():
            r = do_request(cfg, None)
            with lock:
                res.append(r)

    t0 = time.perf_counter()
    threads = [threading.Thread(target=worker, daemon=True) for _ in range(level)]
    for t in threads:
        t.start()
    print(f"Soaking C={level} for {dur}s... (Ctrl-C to stop early)")
    try:
        time.sleep(dur)
    except KeyboardInterrupt:
        print("\nInterrupted; finalizing...")
    stop.set()
    wall = time.perf_counter() - t0
    for t in threads:
        t.join(timeout=cfg.timeout + 5)

    s = summarize(res)
    print()
    print(f"soak: C={level} wall={wall:.1f}s reqs={s['n']} rps={s['n']/wall if wall else 0:.1f}")
    print(f"      ok={s['ok']} ({s['ok_pct']:.1f}%)  soft/incomplete={s['soft']}  fail={s['fail']}")
    print(f"      latency(s) min={s['lat_min']:.2f} mean={s['lat_mean']:.2f} "
          f"p50={s['lat_p50']:.2f} p90={s['lat_p90']:.2f} p95={s['lat_p95']:.2f} "
          f"p99={s['lat_p99']:.2f} max={s['lat_max']:.2f}")
    if s["errs"]:
        top = sorted(s["errs"].items(), key=lambda kv: -kv[1])
        print("      errs: " + ", ".join(f"{k}={v}" for k, v in top[:8]))
    return 0


def once(cfg) -> int:
    level = cfg.level or 8
    allres, t0 = [], time.perf_counter()
    for _ in range(max(1, cfg.rounds)):
        allres.extend(run_batch(cfg, level))
    wall = time.perf_counter() - t0
    s = summarize(allres)
    print(HDR)
    print_row(level, s, wall, "OK" if s["fail"] == 0 else "HAS-FAILS")
    return 0


def models(cfg) -> int:
    url = cfg.base_url.rstrip("/") + "/models"
    ctx = None
    if url.startswith("https"):
        ctx = ssl.create_default_context()
        if cfg.insecure:
            ctx.check_hostname = False
            ctx.verify_mode = ssl.CERT_NONE
    req = urllib.request.Request(url, headers={"Authorization": "Bearer " + cfg.key})
    with urllib.request.urlopen(req, timeout=cfg.timeout, context=ctx) as resp:
        data = json.loads(resp.read().decode("utf-8", "replace"))
    for m in data.get("data", []):
        eps = ",".join(m.get("supported_endpoint_types", []))
        print(f"  {m['id']:<24} endpoints: {eps}")
    return 0


def dump_json(cfg, cfg_dict, mode, result) -> None:
    if cfg.json:
        with open(cfg.json, "w") as f:
            json.dump({"config": cfg_dict, "mode": mode, "result": result}, f, indent=2)


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--base-url", default=DEFAULT_BASE, help="gateway base (default plain HTTP: %(default)s)")
    ap.add_argument("--model", default=DEFAULT_MODEL, help="model id (default %(default)s; see --mode models)")
    ap.add_argument("--key", default=None, help="API key (else env %s)" % " / ".join(ENV_NAMES))
    ap.add_argument("--insecure", action="store_true", help="skip TLS verify (https self-signed)")
    ap.add_argument("--timeout", type=float, default=60.0, help="per-request timeout seconds (default %(default)s)")
    ap.add_argument("--prompt", default=None, help="custom input (default a tiny 'ping' so ceiling is latency-free)")
    ap.add_argument("--prompt-file", default=None,
                      help="read input from a file (dash = stdin); overrides --prompt (for 128k-context tests)")
    ap.add_argument("--long", action="store_true", help="use a longer prompt + cap output (~256 tokens) for realistic in-flight load")
    ap.add_argument("--max-tokens", type=int, default=None, help="max_output_tokens to request")
    ap.add_argument("--stream", action="store_true", help="use SSE streaming (measures full-stream time, ok on response.completed)")

    ap.add_argument("--models", action="store_true", help="shorthand for --mode models")
    ap.add_argument("--mode", choices=["sweep", "soak", "once", "models"], default="sweep")
    ap.add_argument("--levels", default=None, help="explicit csv levels for sweep, e.g. 1,2,4,8,16,32")
    ap.add_argument("--min-level", type=int, default=1)
    ap.add_argument("--max-level", type=int, default=64, help="auto-sweep upper bound (default %(default)s)")
    ap.add_argument("--rounds", type=int, default=2, help="batches per level in sweep/once (default %(default)s)")
    ap.add_argument("--fail-limit", type=float, default=0.05, help="hard-fail fraction marking a level degraded (default %(default)s)")
    ap.add_argument("--latency-factor", type=float, default=4.0, help="p95 multiple of baseline marking degraded (default %(default)s)")
    ap.add_argument("--level", type=int, default=8, help="concurrency for soak/once (default %(default)s)")
    ap.add_argument("--duration", type=int, default=30, help="soak seconds (default %(default)s)")
    ap.add_argument("--no-stop", action="store_true", help="in sweep, do not stop at the first degraded level")
    ap.add_argument("--json", default=None, help="write a JSON summary to this path")
    args = ap.parse_args(argv)
    if args.models:
        args.mode = "models"

    # prompt + tokens
    if args.long:
        args.prompt = args.prompt or ("Write a brief 120-word summary of why load testing "
                                      "matters for LLM gateways. End with the word END.")
        args.max_tokens = args.max_tokens or 256
    if args.prompt is None:
        args.prompt = "Reply with exactly: pong"
    if args.prompt_file:
        pf = args.prompt_file
        args.prompt = sys.stdin.read() if pf == "-" else open(pf, encoding="utf-8", errors="replace").read()

    key = args.key or "".join(os.environ.get(n, "") and os.environ[n] for n in ENV_NAMES)
    if not key:
        key = next((os.environ[n] for n in ENV_NAMES if os.environ.get(n)), "")
    if not key and args.mode != "models":
        print("error: no API key. set QWEN_KEY or pass --key", file=sys.stderr)
        return 2

    cfg = argparse.Namespace(base_url=args.base_url, model=args.model, key=key,
                             insecure=args.insecure, timeout=args.timeout, prompt=args.prompt,
                             max_tokens=args.max_tokens, stream=args.stream, mode=args.mode,
                             levels=args.levels, min_level=args.min_level, max_level=args.max_level,
                             rounds=args.rounds, fail_limit=args.fail_limit,
                             latency_factor=args.latency_factor, level=args.level,
                             duration=args.duration, no_stop=args.no_stop, json=args.json)

    scheme = cfg.base_url.split("://", 1)[0]
    if scheme == "http":
        print(f"note: base-url is plain HTTP ({cfg.base_url}). "
              f"this host's HTTPS/TLS is broken; only use https if the server actually serves TLS.")

    if cfg.mode == "models":
        return models(cfg)

    print(f"probe: url={cfg.base_url} model={cfg.model} key={mask(key)} "
          f"timeout={cfg.timeout}s stream={cfg.stream}")
    print(f"      prompt={short(cfg.prompt)!r} len={len(cfg.prompt)} max_tokens={cfg.max_tokens}")
    print(f"      mode={cfg.mode}" + (f" level={cfg.level} duration={cfg.duration}s" if cfg.mode == "soak"
          else (f" level={cfg.level}" if cfg.mode == "once"
                else f" levels={parse_levels(cfg)} rounds={cfg.rounds}")))
    print()

    if cfg.mode == "sweep":
        rc = sweep(cfg)
    elif cfg.mode == "soak":
        rc = soak(cfg)
    else:
        rc = once(cfg)
    return rc


if __name__ == "__main__":
    sys.exit(main())

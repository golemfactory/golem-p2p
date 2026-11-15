#!/usr/bin/env python3
"""Summarise the night soaks: one connection kept for the whole run?"""
import json, sys, os
D = os.path.dirname(os.path.abspath(__file__))  # run from this folder: ./check.py
for name in ("direct", "relay"):
    ev = [json.loads(l) for l in open(f"{D}/{name}.jsonl") if l.strip()]
    kinds = {}
    for e in ev: kinds[e.get("event")] = kinds.get(e.get("event"), 0) + 1
    stats = [e for e in ev if e.get("event") == "soak_stats"]
    last = stats[-1] if stats else {}
    t = last.get("totals", {})
    up = last.get("session", {}).get("uptime_s")
    worst = max((s["rtt_ms"]["max"] for s in stats if s.get("rtt_ms")), default=None)
    paths = sorted({s.get("path") for s in stats})
    ok = (t.get("new_conns") == 1 and t.get("failed") == 0 and t.get("bulk_failed") == 0
          and kinds.get("disconnect", 0) == 0 and kinds.get("conn", 0) == 1)
    print(f"== {name}: {'PASS' if ok else 'FAIL'}  final={last.get('final')} uptime_s={up}")
    print(f"   requests sent={t.get('sent')} ok={t.get('ok')} failed={t.get('failed')}  new_conns={t.get('new_conns')}")
    print(f"   uploads ok={t.get('bulk_ok')} failed={t.get('bulk_failed')}  paths seen={paths}  worst rtt={worst} ms")
    print(f"   events: {kinds}")
    for e in ev:
        if e.get("event") in ("disconnect", "error", "path_change", "conn"):
            print("   ", e.get("ts", ""), e.get("event"), {k: e[k] for k in ("path", "reason", "kind", "from", "to", "remote_addr", "message") if k in e})

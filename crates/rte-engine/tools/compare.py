#!/usr/bin/env python3
"""Rust 엔진 덤프와 원본 JS 덤프를 비교한다. 사용: compare.py rust.json js.json [--max N]"""
import json, sys, math

OP_KEYS = {"gop", "top", "op"}  # style.opacity 는 브라우저가 소수 6자리로 직렬화 → 절대오차 허용


def close(a, b, key):
    if a is None or b is None or isinstance(a, bool) or isinstance(b, bool):
        return a == b
    if key in OP_KEYS:
        return abs(a - b) <= 2e-6
    if isinstance(a, (int, float)) and isinstance(b, (int, float)):
        return abs(a - b) <= 1e-9 * max(1.0, abs(a), abs(b))
    return a == b


def walk(a, b, path, out, key=""):
    if isinstance(a, dict) and isinstance(b, dict):
        for k in sorted(set(a) | set(b)):
            if k not in a: out.append((path + "." + k, "<missing in rust>", b[k])); continue
            if k not in b: out.append((path + "." + k, a[k], "<missing in js>")); continue
            walk(a[k], b[k], path + "." + k, out, k)
    elif isinstance(a, list) and isinstance(b, list):
        if len(a) != len(b): out.append((path + ".len", len(a), len(b))); n = min(len(a), len(b))
        else: n = len(a)
        for i in range(n): walk(a[i], b[i], f"{path}[{i}]", out, key)
    else:
        if not close(a, b, key): out.append((path, a, b))


def main():
    rust, js = json.load(open(sys.argv[1])), json.load(open(sys.argv[2]))
    mx = int(sys.argv[sys.argv.index("--max") + 1]) if "--max" in sys.argv else 12
    if len(rust) != len(js):
        print(f"FAIL dump count: rust={len(rust)} js={len(js)}"); sys.exit(1)
    bad, shown = 0, 0
    for i, (a, b) in enumerate(zip(rust, js)):
        d = []
        walk(a, b, "", d)
        if d:
            bad += 1
            if shown < mx:
                print(f"--- dump #{i} '{a.get('label')}' : {len(d)} diffs")
                for p, x, y in d[:6]: print(f"    {p}: rust={x!r} js={y!r}")
                shown += 1
    print(f"{'PASS' if bad == 0 else 'FAIL'} {len(rust)-bad}/{len(rust)} dumps identical")
    sys.exit(1 if bad else 0)


main()

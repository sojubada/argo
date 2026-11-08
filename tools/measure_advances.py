#!/usr/bin/env python3
"""글자 종류별 폭 측정 (지침 A). 컨테이너에서만 실행한다.

사용: python3 tools/measure_advances.py PC.pdf [Argo.pdf]
출력: 글자 종류(한글·영문·숫자·공백)별 (x1-x0)/글자크기 중앙값.
8pt 미만 글자는 뺀다. 여기서 나온 값만 fontprofile.rs 에 넣는다.
"""
import statistics
import sys

import pdfplumber


def klass(c):
    o = ord(c)
    if 0xAC00 <= o <= 0xD7A3:
        return "hangul"
    if c == " ":
        return "space"
    if c.isascii() and c.isdigit():
        return "digit"
    if c.isascii() and c.isalpha() and c.islower():
        return "latin"
    return None


def measure(path):
    out = {"hangul": [], "latin": [], "digit": [], "space": []}
    with pdfplumber.open(path) as pdf:
        for page in pdf.pages:
            for ch in page.chars:
                size = ch.get("size") or 0
                k = klass(ch.get("text", ""))
                if k and size >= 8:
                    out[k].append((ch["x1"] - ch["x0"]) / size)
    return {k: (round(statistics.median(v), 3) if v else None, len(v)) for k, v in out.items()}


if __name__ == "__main__":
    for p in sys.argv[1:]:
        print(p)
        for k, (m, n) in measure(p).items():
            print(f"  {k:7s} {m}  (n={n})")

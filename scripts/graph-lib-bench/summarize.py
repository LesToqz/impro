#!/usr/bin/env python3
"""Condense results/*.md rows: `summarize.py results/a-plain.md ...` prints
one line per resolution with pan and zoom-out as `p50/p95/max` in ms."""
import re
import sys


def rows(path):
    out = {}
    for line in open(path):
        if not re.match(r"\| \d+x\d+@", line):
            continue
        c = [x.strip() for x in line.strip().strip("|").split("|")]
        res, scen, setup, cold, p50, p95, p99, mx = c[:8]
        out.setdefault(res.split(" ")[0], {"cold": cold, "setup": setup, "rss": c[8] if len(c) > 8 else ""})[scen] = (p50, p95, p99, mx)
    return out


for path in sys.argv[1:]:
    print(f"== {path}")
    for res, d in rows(path).items():
        def cell(k):
            v = d.get(k)
            return f"{v[0]}/{v[1]}/{v[3]}" if v else "-"
        print(f"  {res:<14} cold {d['cold']:>7}  pan {cell('pan'):<20} zoom-out {cell('zoom-out'):<20} rss {d['rss']}")

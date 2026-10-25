#!/usr/bin/env python3
"""Dependency-closure resolver for an MSYS2 (pacman) repository database.

Usage: msys2_resolve.py <db-dir> <package> [<package> ...]

<db-dir> is the extracted `<repo>.db` tarball (one `<pkg>-<ver>/desc` file per
package). Package names may be given in full or short (`gtk3`); short names get
the `mingw-w64-ucrt-x86_64-` prefix. Prints one tab-separated line per package
of the closure: name, filename, sha256, compressed size, installed size.
Understands %DEPENDS% (with >=, <=, =, <, > constraints) and %PROVIDES%.
Diagnostics go to stderr. Standard library only.
"""
import os
import re
import sys

PREFIX = "mingw-w64-ucrt-x86_64-"
DEP_RE = re.compile(r"^([^<>=]+?)(?:(<=|>=|=|<|>)(.+))?$")


def parse_desc(path):
    fields = {}
    key = None
    with open(path, encoding="utf-8") as fh:
        for raw in fh:
            line = raw.rstrip("\n")
            if line.startswith("%") and line.endswith("%"):
                key = line.strip("%")
                fields[key] = []
            elif line and key:
                fields[key].append(line)
    return fields


def split_evr(ver):
    epoch = "0"
    if ":" in ver:
        epoch, ver = ver.split(":", 1)
    rel = ""
    if "-" in ver:
        ver, rel = ver.rsplit("-", 1)
    return epoch, ver, rel


def rpm_cmp(a, b):
    """Segment-wise version comparison in the style of libalpm's rpmvercmp."""
    if a == b:
        return 0
    sa = re.findall(r"[0-9]+|[A-Za-z]+", a)
    sb = re.findall(r"[0-9]+|[A-Za-z]+", b)
    for x, y in zip(sa, sb):
        xd, yd = x.isdigit(), y.isdigit()
        if xd and yd:
            xi, yi = int(x), int(y)
            if xi != yi:
                return 1 if xi > yi else -1
        elif xd != yd:
            return 1 if xd else -1  # numeric segments are newer than alpha
        elif x != y:
            return 1 if x > y else -1
    if len(sa) == len(sb):
        return 0
    longer = sa if len(sa) > len(sb) else sb
    tail = longer[min(len(sa), len(sb))]
    sign = 1 if len(sa) > len(sb) else -1
    return sign if tail.isdigit() else -sign


def vercmp(a, b):
    ea, va, ra = split_evr(a)
    eb, vb, rb = split_evr(b)
    for x, y in ((ea, eb), (va, vb)):
        c = rpm_cmp(x, y)
        if c:
            return c
    if ra and rb:
        return rpm_cmp(ra, rb)
    return 0


def satisfies(have, op, want):
    if op is None:
        return True
    c = vercmp(have, want)
    return {"=": c == 0, ">=": c >= 0, "<=": c <= 0, ">": c > 0, "<": c < 0}[op]


def load(db_dir):
    pkgs = {}
    providers = {}  # virtual name -> [(pkgname, provided-version-or-None)]
    for entry in sorted(os.listdir(db_dir)):
        desc = os.path.join(db_dir, entry, "desc")
        if not os.path.isfile(desc):
            continue
        f = parse_desc(desc)
        name = f["NAME"][0]
        pkgs[name] = {
            "name": name,
            "version": f["VERSION"][0],
            "filename": f["FILENAME"][0],
            "sha256": f["SHA256SUM"][0],
            "csize": int(f.get("CSIZE", ["0"])[0]),
            "isize": int(f.get("ISIZE", ["0"])[0]),
            "depends": f.get("DEPENDS", []),
        }
        for prov in f.get("PROVIDES", []):
            m = DEP_RE.match(prov)
            providers.setdefault(m.group(1), []).append((name, m.group(3)))
    return pkgs, providers


def pick(dep, pkgs, providers, chosen):
    m = DEP_RE.match(dep)
    name, op, want = m.group(1), m.group(2), m.group(3)
    if name in pkgs:
        if not satisfies(pkgs[name]["version"], op, want):
            print(f"warning: {name} {pkgs[name]['version']} does not satisfy {dep}", file=sys.stderr)
        return name
    cands = [p for p, v in providers.get(name, []) if v is None or satisfies(v, op, want)]
    if not cands:
        return None
    for c in cands:
        if c in chosen:
            return c
    return sorted(cands)[0]


def main(argv):
    if len(argv) < 3:
        print(__doc__, file=sys.stderr)
        return 2
    pkgs, providers = load(argv[1])
    queue = [a if a.startswith(PREFIX) else PREFIX + a for a in argv[2:]]
    chosen = {}
    missing = []
    while queue:
        name = queue.pop(0)
        if name in chosen:
            continue
        if name not in pkgs:
            missing.append(name)
            continue
        chosen[name] = pkgs[name]
        for dep in pkgs[name]["depends"]:
            target = pick(dep, pkgs, providers, chosen)
            if target is None:
                missing.append(dep)
            else:
                queue.append(target)
    for name in sorted(chosen):
        p = chosen[name]
        print("\t".join([name, p["filename"], p["sha256"], str(p["csize"]), str(p["isize"])]))
    if missing:
        print("error: unresolved: " + ", ".join(sorted(set(missing))), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

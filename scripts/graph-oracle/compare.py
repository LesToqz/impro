#!/usr/bin/env python3
"""Differential run: the reference graph logic against the Rust engine.

Generates the synthetic trees, runs ``oracle.py`` and ``graph_dump`` on the same
requests, canonicalises both answers and prints a readable diff.  Exit status is
non-zero when a difference is not listed in ``known.json``.
"""
import argparse
import collections
import fnmatch
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import cases  # noqa: E402
import fixtures  # noqa: E402

NODE_FIELDS = ('kind', 'label', 'path', 'folder', 'tags', 'in', 'out', 'size_hint', 'group')


def run_side(side, request, dump, workdir):
    req = workdir / 'request.json'
    req.write_text(json.dumps(request, ensure_ascii=False), encoding='utf-8')
    env = dict(os.environ, PYTHONDONTWRITEBYTECODE='1')
    if side == 'oracle':
        cmd = [sys.executable, '-I', str(HERE / 'oracle.py'), str(req)]
    else:
        cmd = [str(dump), str(req)]
    done = subprocess.run(cmd, capture_output=True, text=True, env=env, encoding='utf-8')
    if done.returncode != 0:
        raise RuntimeError(f'{side} failed on {request["mode"]}:\n{done.stderr[-3000:]}')
    return json.loads(done.stdout)


def diff_graph(a, b, times=True):
    """Differences between oracle graph ``a`` and engine graph ``b`` as (class, detail)."""
    out = []
    an = {n['id']: n for n in a['nodes']}
    bn = {n['id']: n for n in b['nodes']}
    for key in sorted(an.keys() - bn.keys()):
        out.append(('node:missing-in-impro', f'{key} ({an[key]["kind"]})'))
    for key in sorted(bn.keys() - an.keys()):
        out.append(('node:extra-in-impro', f'{key} ({bn[key]["kind"]})'))
    # The renderer's degree counts distinct directed endpoint pairs (a self pair twice).
    pairs = {(l['source'], l['target']) for l in a['links']}
    outs, ins = collections.Counter(p[0] for p in pairs), collections.Counter(p[1] for p in pairs)
    for key in sorted(an.keys() & bn.keys()):
        an[key]['out'], an[key]['in'] = outs[key], ins[key]
        an[key]['size_hint'] = 1 + an[key]['in'] + an[key]['out']
        x, y = an[key]['ctime'], bn[key]['ctime']
        if times and ((x is None) != (y is None) or (x is not None and abs(x - y) > 1e-3)):
            out.append(('node.ctime', f'{key}: oracle={x!r} impro={y!r}'))
        for f in NODE_FIELDS:
            if an[key][f] != bn[key][f]:
                out.append((f'node.{f}', f'{key}: oracle={an[key][f]!r} impro={bn[key][f]!r}'))
    al = {(l['source'], l['target'], l['kind']): l for l in a['links']}
    bl = {(l['source'], l['target'], l['kind']): l for l in b['links']}
    if len(al) != len(a['links']) or len(bl) != len(b['links']):
        out.append(('link:duplicate-key', ''))
    for key in sorted(al.keys() - bl.keys()):
        out.append(('link:missing-in-impro', ' -> '.join(key[:2]) + f' [{key[2]}]'))
    for key in sorted(bl.keys() - al.keys()):
        out.append(('link:extra-in-impro', ' -> '.join(key[:2]) + f' [{key[2]}]'))
    for key in sorted(al.keys() & bl.keys()):
        for f in ('resolved', 'count', 'self'):
            if al[key][f] != bl[key][f]:
                out.append((f'link.{f}', f'{key}: oracle={al[key][f]!r} impro={bl[key][f]!r}'))
    for f in ('truncated', 'focus'):
        if a[f] != b[f]:
            out.append((f, f'oracle={a[f]!r} impro={b[f]!r}'))
    sa, sb = a['stats'], b['stats']
    for f in ('counts', 'orphans', 'unresolved', 'components'):
        if sa[f] != sb[f]:
            out.append((f'stats.{f}', f'oracle={sa[f]!r} impro={sb[f]!r}'))
    if sa['ambiguous'] != sb['ambiguous']:
        out.append(('stats.ambiguous', f'oracle={sa["ambiguous"]!r} impro={sb["ambiguous"]!r}'))
    return out


def strict_equal(x, y):
    """JSON equality that tells booleans from numbers."""
    if isinstance(x, bool) or isinstance(y, bool):
        return type(x) is type(y) and x == y
    if isinstance(x, (int, float)) and isinstance(y, (int, float)):
        return x == y
    if isinstance(x, dict) and isinstance(y, dict):
        return x.keys() == y.keys() and all(strict_equal(x[k], y[k]) for k in x)
    if isinstance(x, list) and isinstance(y, list):
        return len(x) == len(y) and all(strict_equal(p, q) for p, q in zip(x, y))
    return type(x) is type(y) and x == y


def first_diff(x, y):
    """Shortest description of how two lists of records differ."""
    for i in range(max(len(x), len(y))):
        p, q = (x[i] if i < len(x) else None), (y[i] if i < len(y) else None)
        if p != q:
            return f'[{i}] oracle={p!r} impro={q!r} (lens {len(x)}/{len(y)})'
    return ''


def norm_link(link):
    """Link record without the oracle-only flag; the unused display alias compares up to blank runs."""
    link = {k: v for k, v in link.items() if k != 'markup'}
    if link['alias']:
        link['alias'] = ' '.join(link['alias'].split(' '))
        while '  ' in link['alias']:
            link['alias'] = link['alias'].replace('  ', ' ')
    return link


def diff_text(a, b):
    out = []
    for sa, sb in zip(a['snippets'], b['snippets']):
        name = sa['name']
        if not name.lower().endswith(('.md', '.markdown')) and not name.startswith('fuzzmd/'):
            # only notes carry links, tags and properties; other files only imports
            if sa['code'] != sb['code']:
                klass = 'text.code.python-syntax-error' if sa.get('py_syntax_error') else 'text.code'
                out.append((klass, f'{name}: {first_diff(sa["code"], sb["code"])}'))
            continue
        if not strict_equal(sa['props'], sb['props']):
            out.append(('text.props', f'{name}: oracle={sa["props"]!r} impro={sb["props"]!r}'))
        if sa['body_start'] != sb['body_start']:
            out.append(('text.body_start', f'{name}: oracle={sa["body_start"]} impro={sb["body_start"]}'))
        markup = [l for l in sa['links'] if l.get('markup')]
        if markup:
            out.append(('text.links.autolink-without-dot', f'{name}: {len(markup)} tag-like <…> links'))
        want = [norm_link(l) for l in sa['links'] if not l.get('markup')]
        got = [norm_link(l) for l in sb['links']]
        if want != got:
            out.append(('text.links', f'{name}: {first_diff(want, got)}'))
        if sa['tags'] != sb['tags']:
            out.append(('text.tags', f'{name}: oracle={sa["tags"]!r} impro={sb["tags"]!r}'))
    return out


def diff_prefs(values, a, b):
    out = []
    for v, x, y in zip(values, a['values'], b['values']):
        if not strict_equal(x, y):
            keys = sorted(k for k in set(x) | set(y) if not strict_equal(x.get(k), y.get(k)))
            out.append(('prefs', f'input={json.dumps(v)[:200]} keys={keys} oracle={[x.get(k) for k in keys]!r} impro={[y.get(k) for k in keys]!r}'))
    return out


class Report:
    def __init__(self, known):
        self.known = known
        self.groups = collections.OrderedDict()
        self.checked = 0

    def add(self, scope, case, klass, detail):
        entry = self.groups.setdefault((scope, klass), [])
        entry.append((case, detail))

    def explained(self, scope, klass):
        for item in self.known:
            if fnmatch.fnmatchcase(scope, item.get('scope', '*')) and fnmatch.fnmatchcase(klass, item['class']):
                return item['id']
        return None

    def print(self, limit):
        bad = 0
        for (scope, klass), items in self.groups.items():
            why = self.explained(scope, klass)
            tag = f'KNOWN {why}' if why else 'DIFFERENT'
            if not why:
                bad += len(items)
            print(f'[{tag}] {scope} :: {klass}  x{len(items)}')
            for case, detail in items[:limit]:
                print(f'    {case}: {detail[:600]}')
        print(f'\n{self.checked} comparisons, {sum(len(v) for v in self.groups.values())} differences, {bad} unexplained')
        return bad


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--dump', type=Path, required=True)
    ap.add_argument('--only', default='*', help='fixture name glob')
    ap.add_argument('--parts', default='graph,caps,live,random,text,prefs')
    ap.add_argument('--fuzz', type=int, default=4000)
    ap.add_argument('--limit', type=int, default=3)
    ap.add_argument('--real', action='append', default=[], help='real tree to compare (local only)')
    ap.add_argument('--corpus', action='append', default=[], help='extra directory of files for the text comparison (local only)')
    ap.add_argument('--corpus-limit', type=int, default=3000)
    ap.add_argument('--keep', action='store_true')
    ap.add_argument('--random', type=int, default=60, help='number of random projects (graph and live)')
    args = ap.parse_args()
    parts = set(args.parts.split(','))
    known = json.loads((HERE / 'known.json').read_text()) if (HERE / 'known.json').exists() else []
    report = Report(known)
    tmp = Path(tempfile.mkdtemp(prefix='graph-oracle-'))
    work = tmp / 'work'
    work.mkdir()
    roots, graph_roots = {}, {}
    try:
        for name, build in fixtures.FIXTURES.items():
            if not fnmatch.fnmatchcase(name, args.only):
                continue
            root = tmp / 'trees' / name
            root.mkdir(parents=True)
            build(root)
            roots[name] = root
            if name in fixtures.TEXT_ONLY:
                continue
            graph_roots[name] = root
        if 'graph' in parts:
            for name, root in graph_roots.items():
                big = name.startswith(('chain', 'mixed'))
                request = cases.graph_request(root, rich=not big, focus_count=3 if big else 5,
                                              depths=(1, 2, 6) if big else range(1, 7))
                compare_graph(report, name, request, args.dump, work)
        if 'caps' in parts:
            for name, root in graph_roots.items():
                if name.startswith(('chain', 'mixed', 'live')):
                    continue
                for request, label in cases.cap_requests(root):
                    compare_graph(report, name, request, args.dump, work, label)
        if 'live' in parts and 'live_base' in roots:
            for request, label in cases.live_sequences(roots['live_base']):
                copies = []
                for side in ('oracle', 'impro'):
                    copy = tmp / 'live' / side / label
                    shutil.copytree(roots['live_base'], copy, symlinks=True)
                    copies.append(copy)
                answers = []
                for side, copy in zip(('oracle', 'impro'), copies):
                    answers.append(run_side(side, dict(request, root=str(copy)), args.dump, work))
                for step, (x, y) in enumerate(zip(answers[0]['steps'], answers[1]['steps'])):
                    report.checked += 1
                    for klass, detail in diff_graph(x['result'], y['result'], times=False):
                        report.add(f'live:{label}', f'step {step} {json.dumps(x["op"])}', klass, detail)
        if 'random' in parts:
            compare_random(report, tmp, args.dump, work, args.random)
        if 'text' in parts:
            request = cases.text_request(list(roots.items()) + [('corpus' + str(i), d) for i, d in enumerate(args.corpus)], fuzz=args.fuzz, limit=args.corpus_limit)
            a, b = (run_side(s, request, args.dump, work) for s in ('oracle', 'impro'))
            report.checked += len(request['snippets'])
            for klass, detail in diff_text(a, b):
                report.add('text', '', klass, detail)
        if 'prefs' in parts:
            values = cases.prefs_values()
            request = {'mode': 'prefs', 'values': values}
            a, b = (run_side(s, request, args.dump, work) for s in ('oracle', 'impro'))
            report.checked += len(values)
            for klass, detail in diff_prefs(values, a, b):
                report.add('prefs', '', klass, detail)
        for real in args.real:
            request = cases.graph_request(Path(real), rich=False, focus_count=3, depths=(1, 2))
            request['runs'] = [r for r in request['runs'] if r['mode'] != 'folder'][:80]
            compare_graph(report, 'real:' + Path(real).name, request, args.dump, work)
    finally:
        if args.keep:
            print('kept', tmp)
        else:
            shutil.rmtree(tmp, ignore_errors=True)
    return 1 if report.print(args.limit) else 0


def compare_random(report, tmp, dump, work, count):
    for seed in range(count):
        root = tmp / 'random' / str(seed)
        root.mkdir(parents=True)
        fixtures.random_project(seed)(root)
        import random
        queries = ['alpha', 'tag:t1', '-beta OR gamma', 'path:sub', 'content:alpha', '/a.p/'] + cases.random_queries(random.Random(seed), 25)
        request = cases.graph_request(root, rich=False, focus_count=3, depths=(1, 2), query_set=queries)
        request['runs'] += [{'name': f'q:{q}', 'mode': 'build', 'options': dict(o, query=q, groups=[q] if i == 0 else [])}
                            for q in queries for i, o in enumerate((cases.base_sets()['all'], cases.base_sets()['strict']))]
        compare_graph(report, 'random', request, dump, work, f'seed{seed}')
        ops = fixtures.random_ops(seed, root)
        answers = []
        for side in ('oracle', 'impro'):
            copy = tmp / 'randlive' / side / str(seed)
            shutil.copytree(root, copy, symlinks=True)
            req = {'mode': 'live', 'root': str(copy), 'options': cases.LIVE_OPTS, 'ops': ops}
            answers.append(run_side(side, req, dump, work))
        for step, (x, y) in enumerate(zip(answers[0]['steps'], answers[1]['steps'])):
            report.checked += 1
            for klass, detail in diff_graph(x['result'], y['result'], times=False):
                report.add('random-live', f'seed {seed} step {step} {json.dumps(x["op"])[:200]}', klass, detail)


def compare_graph(report, scope, request, dump, work, label=''):
    a, b = run_side('oracle', request, dump, work), run_side('impro', request, dump, work)
    if scope.startswith('real:'):
        full = next(x for x in a['runs'] if x['name'] == 'tg:111111')['result']
        full_b = next(x for x in b['runs'] if x['name'] == 'tg:111111')['result']
        print(f'{scope}: all-on build: oracle {len(full["nodes"])} nodes / {len(full["links"])} links, '
              f'impro {len(full_b["nodes"])} nodes / {len(full_b["links"])} links')
    for x, y in zip(a['runs'], b['runs']):
        report.checked += 1
        for klass, detail in diff_graph(x['result'], y['result']):
            report.add(scope + (':' + label if label else ''), x['name'], klass, detail)


if __name__ == '__main__':
    sys.exit(main())

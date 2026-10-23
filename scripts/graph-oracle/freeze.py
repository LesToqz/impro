#!/usr/bin/env python3
"""Freeze the reference's answers on the synthetic fixtures as offline goldens.

Writes ``crates/impro-graph/tests/golden/*.json``: each file holds a project
tree (or the name of a generator for the big ones), the requests, and the
reference's canonical answers (digests; a few readable ones in full).  The
Rust tests rebuild the tree, ask the engine the same questions and compare, so
``cargo test`` needs neither Python nor the reference.

    IMPRO_REFERENCE_DIR=<checkout> python3 -I scripts/graph-oracle/freeze.py
"""
import base64
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import cases  # noqa: E402
import compare  # noqa: E402
import fixtures  # noqa: E402

OUT = HERE.parents[1] / 'crates' / 'impro-graph' / 'tests' / 'golden'
GENERATED = {'chain_3000': ('chain', 3000), 'chain_3001': ('chain', 3001), 'chain_3500': ('chain', 3500),
             'mixed_3300': ('mixed', 3300)}
READABLE = ('tg:111111', 'tg:000000', 'loc:dflt', 'fold:dflt:sub')


def renderer_degrees(graph):
    """The renderer counts distinct directed endpoint pairs (see compare.diff_graph)."""
    pairs = {(l['source'], l['target']) for l in graph['links']}
    for node in graph['nodes']:
        node['out'] = sum(1 for p in pairs if p[0] == node['id'])
        node['in'] = sum(1 for p in pairs if p[1] == node['id'])
        node['size_hint'] = 1 + node['in'] + node['out']
    return graph


def canonical(value):
    def strip(v):
        if isinstance(v, dict):
            if 'nodes' in v and 'links' in v:
                v = renderer_degrees(json.loads(json.dumps(v)))
            return {k: strip(x) for k, x in v.items() if k != 'ctime'}
        if isinstance(v, list):
            return [strip(x) for x in v]
        return v
    return json.dumps(strip(value), sort_keys=True, separators=(',', ':'), ensure_ascii=False)


def digest(value):
    h = 0xcbf29ce484222325
    for b in canonical(value).encode('utf-8'):
        h = ((h ^ b) * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return f'{h:016x}'


def manifest(root):
    """The tree as data; big uniform files are described, not stored."""
    entries = []

    def visit(directory):
        for entry in sorted(os.scandir(directory), key=lambda e: e.name):
            rel = os.path.relpath(entry.path, root).replace(os.sep, '/')
            if entry.is_symlink():
                entries.append({'path': rel, 'link': os.readlink(entry.path)})
            elif entry.is_dir(follow_symlinks=False):
                visit(entry.path)
            else:
                data = Path(entry.path).read_bytes()
                entries.append(encode(rel, data))
    visit(root)
    return entries


def encode(rel, data):
    if len(data) > 4096:
        for ch in 'xyz':
            line = (ch * 79 + '\n').encode()
            if data.startswith((line * 20)):
                tail_at = len(data)
                head = b''
                # filler optionally followed by a short tail
                body = (line * (len(data) // 80 + 1))[:len(data)]
                if data == body:
                    return {'path': rel, 'fill': len(data), 'ch': ch}
                n = data.find(b' [[')
                if n > 0 and data[:n] == (line * (n // 80 + 1))[:n]:
                    return {'path': rel, 'fill': n, 'ch': ch, 'tail': data[n:].decode()}
        stripped = data.rstrip(b'\0')
        if len(data) - len(stripped) > 4096:
            return {'path': rel, 'pad_b64': base64.b64encode(stripped).decode(), 'zeros': len(data) - len(stripped)}
        if data.startswith(b'# ') and b'\nimport ok' in data:
            body = data[2:data.index(b'\nimport ok')].decode()
            if body == (('x' * 79 + '\n') * (len(body) // 80 + 1))[:len(body)]:
                return {'path': rel, 'fill': len(body), 'ch': 'x', 'head': '# ', 'tail': '\nimport ok\n'}
        raise ValueError(f'cannot describe {rel}')
    try:
        text = data.decode('utf-8')
        if '\0' not in text:
            return {'path': rel, 'text': text}
    except UnicodeDecodeError:
        pass
    return {'path': rel, 'b64': base64.b64encode(data).decode()}


def pick_graph_runs(request, big):
    keep = []
    for run in request['runs']:
        name = run['name']
        if name.startswith('tg:'):
            keep.append(run)
        elif name.startswith(('q:all:', 'q:strict:')) and not big:
            index = int(name.split(':')[2])
            if index % 3 == 0 or index < 14:
                keep.append(run)
        elif name.startswith('grp:all:') and not big:
            keep.append(run)
        elif name.startswith('loc:'):
            parts = name.split(':')
            if parts[1] in ('dflt', 'strict') and (big or parts[-2] in ('1', '2', '6')):
                keep.append(run)
        elif name.startswith('fold:dflt'):
            keep.append(run)
    return keep


def freeze_graph(name, root, work, dump):
    big = name in GENERATED
    request = cases.graph_request(root, rich=not big, focus_count=3 if big else 4,
                                  depths=(1, 2, 6) if big else range(1, 7))
    scenarios = [(3000, pick_graph_runs(request, big))]
    if not big:
        small = [r for r in request['runs'] if r['name'] in ('tg:111111', 'tg:011101') or r['name'].startswith('q:all:0:')]
        scenarios += [(cap, small + [dict(r, name=f'{r["name"]}') for r in request['runs']
                                     if r['name'].startswith('loc:all:') and r['name'].endswith(':3:11')][:2])
                      for cap in (0, 1, 3, 8)]
    out = {'tree': GENERATED[name] and {'generate': GENERATED[name][0], 'count': GENERATED[name][1]} if big else
           {'files': manifest(root)}, 'scenarios': []}
    for cap, runs in scenarios:
        req = dict(request, max_nodes=cap, runs=runs)
        answer = compare.run_side('oracle', req, dump, work)
        entry = {'max_nodes': cap, 'runs': [], 'full': {}}
        for run, got in zip(runs, answer['runs']):
            spec = {k: v for k, v in run.items()}
            spec['digest'] = digest(got['result'])
            entry['runs'].append(spec)
            if cap == 3000 and not big and any(run['name'].startswith(p) for p in READABLE):
                entry['full'][run['name']] = json.loads(canonical(got['result']))
        out['scenarios'].append(entry)
    return out


def freeze_live(label, root, ops, work, dump, options=None):
    copy = Path(tempfile.mkdtemp(prefix='frozen-live-')) / 'tree'
    shutil.copytree(root, copy, symlinks=True)
    request = {'mode': 'live', 'root': str(copy), 'options': options or cases.LIVE_OPTS, 'ops': ops}
    answer = compare.run_side('oracle', request, dump, work)
    shutil.rmtree(copy.parent, ignore_errors=True)
    return {'tree': {'files': manifest(root)}, 'options': request['options'], 'ops': ops,
            'digests': [digest(step['result']) for step in answer['steps']]}


def write(name, value):
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / name).write_text(json.dumps(value, ensure_ascii=False, indent=1, sort_keys=True) + '\n', encoding='utf-8')
    print('wrote', name, (OUT / name).stat().st_size)


def main():
    dump = Path(sys.argv[1]) if len(sys.argv) > 1 else Path('target/debug/examples/graph_dump')
    tmp = Path(tempfile.mkdtemp(prefix='graph-freeze-'))
    work = tmp / 'work'
    work.mkdir()
    try:
        roots = {}
        for name, build in fixtures.FIXTURES.items():
            if name in fixtures.TEXT_ONLY or name == 'empty':
                continue
            root = tmp / 'trees' / name
            root.mkdir(parents=True)
            build(root)
            roots[name] = root
        for name, root in roots.items():
            write(f'graph-{name}.json', freeze_graph(name, root, work, dump))
        for request, label in cases.live_sequences(roots['live_base']):
            write(f'live-{label}.json', freeze_live(label, roots['live_base'], request['ops'], work, dump))
        for seed in range(6):
            root = tmp / 'random' / str(seed)
            root.mkdir(parents=True)
            fixtures.random_project(seed)(root)
            name = f'random{seed}'
            write(f'graph-{name}.json', freeze_graph(name, root, work, dump))
            write(f'live-{name}.json', freeze_live(name, root, fixtures.random_ops(seed, root, 30), work, dump))
        write('text.json', freeze_text(roots, work, dump))
        write('prefs.json', freeze_prefs(work, dump))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def freeze_text(roots, work, dump):
    request = cases.text_request([(n, r) for n, r in roots.items() if n not in GENERATED], fuzz=300)
    answer = compare.run_side('oracle', request, dump, work)
    kept, digests = [], []
    for snippet, result in zip(request['snippets'], answer['snippets']):
        if result['py_syntax_error']:
            continue  # the engine reads imports from broken files too (see KNOWN_DIFFERENCES.md)
        result['links'] = [{k: v for k, v in l.items() if k != 'markup'} for l in result['links'] if not l['markup']]
        for l in result['links']:
            l['alias'] = ' '.join(l['alias'].split()) if l['alias'] else l['alias']
        result.pop('py_syntax_error')
        kept.append(snippet)
        digests.append(digest(result))
    return {'snippets': kept, 'digests': digests}


def freeze_prefs(work, dump):
    values = cases.prefs_values()
    answer = compare.run_side('oracle', {'mode': 'prefs', 'values': values}, dump, work)
    return {'values': values, 'expected': answer['values']}


if __name__ == '__main__':
    main()

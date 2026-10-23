"""Request builders: the option matrix, text corpus, live sequences and prefs corpus."""
import itertools
import os
import random
from pathlib import Path

TOGGLES = ('show_tags', 'show_attachments', 'existing_only', 'show_orphans', 'show_code', 'strict_filters')

QUERIES = [
    'alpha', 'ALPHA', 'alpha beta', 'alpha OR beta', 'alpha OR', 'OR alpha', 'OR', 'alpha  OR  beta gamma OR delta',
    '-alpha', '- alpha', '-', '--alpha', 'alpha -beta', '-alpha -beta', '"hello world"', '"hello"', '"', '""', '"unclosed',
    '"two words" alpha', '-"hello world"', '/al.ha/', '/^# alpha/', '/[/', '/(/', '//', '/a\\/b/', '/a/b/', '/ALPHA/',
    'path:sub', 'PATH:Sub', 'path:"sub/with"', 'path:/sub.*md/', 'file:note', 'file:note.md', 'file:"with space"',
    'file:/^n/', 'tag:x', 'tag:#x', 'tag:X', 'tag:y', 'tag:y/z', 'tag:a/b', 'tag:a', 'tag:', 'tag:#', '-tag:x',
    'tag:x tag:y', 'tag:x OR tag:y', 'tag:/x/', 'tag:"x"', 'content:hello', 'content:Hello world',
    'content:"hello world"', 'content:/h.llo/', '-content:hello', 'content:', 'unknown:foo', 'path:', 'a:b:c',
    'path:sub OR file:alpha', '#x', '#', 'sub/', 'png', '.md', 'é', 'Ünï', '日本', 'x y z', 'fence', 'the first',
    '[[alpha]]', '"a b" OR "c d"', 'path:a path:b', '-path:sub -file:alpha', 'TAG:x', 'Content:hello', 'File:note',
    '/hello/ /world/', '-/hello/', 'a\\b', 'a(b', '/\\//', '"a\\"b"', "'quoted'", '?', '*', '.', '\t', ' ',
]

GROUP_SETS = [
    ['alpha', 'beta'],
    ['tag:x', 'path:sub', '-alpha'],
    ['', '  ', 'alpha', 'content:hello'],
    ['file:png', 'tag:y/z', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j'],
    ['/[/', 'tag:#x'],
    ['x', 'x', 'x'],
]


def opts(**kw):
    return dict(kw)


def base_sets():
    return {
        'dflt': opts(),
        'all': opts(show_tags=True, show_attachments=True, existing_only=False),
        'strict': opts(show_tags=True, show_attachments=True, existing_only=False, show_orphans=False, strict_filters=True),
        'min': opts(show_tags=False, show_attachments=False, existing_only=True, show_code=False),
    }


def matrix_runs(rich):
    runs = []
    for bits in itertools.product((True, False), repeat=len(TOGGLES)):
        if not rich and sum(bits) not in (0, 1, len(TOGGLES)):
            continue
        o = dict(zip(TOGGLES, bits))
        runs.append({'name': 'tg:' + ''.join('1' if b else '0' for b in bits), 'mode': 'build', 'options': o})
    return runs


def query_runs(queries=QUERIES):
    runs = []
    for qname, base in base_sets().items():
        for i, q in enumerate(queries):
            runs.append({'name': f'q:{qname}:{i}:{q}', 'mode': 'build', 'options': dict(base, query=q)})
        for strict in (True, False):
            for j, g in enumerate(GROUP_SETS):
                runs.append({'name': f'grp:{qname}:{strict}:{j}', 'mode': 'build',
                             'options': dict(base, groups=g, strict_filters=strict)})
    return runs


def tree_files(root):
    out = []
    for d, dirs, files in os.walk(root):
        dirs.sort()
        for f in sorted(files):
            out.append(os.path.relpath(os.path.join(d, f), root).replace(os.sep, '/'))
    return out


def graph_request(root, rich=True, max_nodes=3000, focus_count=5, depths=range(1, 7), query_set=None):
    files = [f for f in tree_files(root) if not f.startswith(('.git/', '.ceo/'))]
    folders = sorted({f.rsplit('/', 1)[0] for f in files if '/' in f})
    runs = matrix_runs(rich) + (query_runs(query_set or QUERIES) if rich else [])
    cands = [f for f in files if f.lower().endswith(('.md', '.markdown', '.py', '.js', '.ts', '.css', '.html', '.rs'))]
    focus = cands[:focus_count]
    if cands:
        focus.append(cands[0].upper())
        focus.append(cands[len(cands) // 2])
    focus.append('does/not/exist.md')
    focus.append('../outside.md')
    seen_focus = list(dict.fromkeys(focus))
    for sets in ('dflt', 'all', 'strict'):
        for path in seen_focus:
            for depth in depths:
                for inc, out in ((True, True), (True, False), (False, True), (False, False)):
                    if not rich and (inc, out) != (True, True) and depth not in (1, 3):
                        continue
                    o = dict(base_sets()[sets])
                    if sets == 'strict':
                        o['query'] = 'alpha OR a'
                    runs.append({'name': f'loc:{sets}:{path}:{depth}:{int(inc)}{int(out)}', 'mode': 'local', 'path': path,
                                 'depth': depth, 'incoming': inc, 'outgoing': out, 'options': o})
    for prefix in ['', '/', 'sub', 'sub/', '/sub/', 'x', 'y/z', 'nonexistent'] + folders[:8]:
        for sets in ('dflt', 'all'):
            runs.append({'name': f'fold:{sets}:{prefix}', 'mode': 'folder', 'prefix': prefix, 'options': base_sets()[sets]})
    return {'mode': 'graph', 'root': str(root), 'max_nodes': max_nodes, 'runs': runs}


def cap_requests(root, caps=(0, 1, 2, 3, 5, 8)):
    out = []
    for cap in caps:
        runs = []
        for sets in base_sets():
            runs.append({'name': f'cap{cap}:{sets}', 'mode': 'build', 'options': base_sets()[sets]})
            runs.append({'name': f'cap{cap}:{sets}:q', 'mode': 'build', 'options': dict(base_sets()[sets], query='a')})
        files = [f for f in tree_files(root) if f.endswith('.md')][:3]
        for path in files:
            runs.append({'name': f'cap{cap}:loc:{path}', 'mode': 'local', 'path': path, 'depth': 3,
                         'options': dict(base_sets()['all'])})
        out.append(({'mode': 'graph', 'root': str(root), 'max_nodes': cap, 'runs': runs}, f'cap{cap}'))
    return out


# --- live sequences ---------------------------------------------------------

LIVE_OPTS = {'show_tags': True, 'show_attachments': True, 'existing_only': False}


def live_sequences(root):
    def seq(name, ops):
        return {'mode': 'live', 'root': str(root), 'options': LIVE_OPTS, 'ops': ops}, name
    return [
        seq('edit', [
            {'op': 'write', 'path': 'a.md', 'text': '[[c]] #t2 #t1\n'},
            {'op': 'write', 'path': 'a.md', 'text': '[[c]] #t2 #t1\n'},
            {'op': 'write', 'path': 'c.md', 'text': '[[a]] [[zzz]]\n'},
            {'op': 'write', 'path': 'new.md', 'text': '[[Eee]] [[img.png]]\n'},
            {'op': 'write', 'path': 'zzz.md', 'text': 'now exists\n'},
            {'op': 'write', 'path': 'dir/sub/e.md', 'text': '---\naliases: [Other]\n---\n'},
            {'op': 'write', 'path': 'img.png', 'text': 'not binary now'},
            {'op': 'write', 'path': 'n.py', 'text': 'import m\n'},
            {'op': 'write', 'path': 'k.js', 'text': "import './a'\n"},
        ]),
        seq('buffers', [
            {'op': 'set_text', 'path': 'a.md', 'text': '[[g]] #buf'},
            {'op': 'set_text', 'path': 'unsaved.md', 'text': '[[a]] #fresh'},
            {'op': 'set_text', 'path': 'a.md', 'text': None},
            {'op': 'set_text', 'path': 'unsaved.md', 'text': None},
            {'op': 'set_text', 'path': 'c.md', 'text': '[[b]]'},
            {'op': 'write', 'path': 'c.md', 'text': 'disk wins?\n'},
            {'op': 'set_text', 'path': 'c.md', 'text': None},
            {'op': 'set_text', 'path': 'img.png', 'text': '[[a]]'},
            {'op': 'set_text', 'path': 'img.png', 'text': None},
            {'op': 'set_text', 'path': '.git/x.md', 'text': '[[a]]'},
            {'op': 'set_text', 'path': '../out.md', 'text': '[[a]]'},
        ]),
        seq('delete', [
            {'op': 'delete', 'path': 'b.md'},
            {'op': 'write', 'path': 'b.md', 'text': '[[a]]\n'},
            {'op': 'delete', 'path': 'dir'},
            {'op': 'delete', 'path': 'img.png'},
            {'op': 'delete', 'path': 'm.py'},
        ]),
        seq('rename', [
            {'op': 'rename', 'from': 'b.md', 'to': 'bb.md'},
            {'op': 'rename', 'from': 'dir', 'to': 'moved/dir'},
            {'op': 'rename', 'from': 'moved/dir/sub/e.md', 'to': 'e2.md'},
            {'op': 'rename', 'from': 'f.md', 'to': 'F.md'},
            {'op': 'rename', 'from': 'img.png', 'to': 'sub/img.png'},
            {'op': 'rename', 'from': 'n.py', 'to': 'pkg/n.py'},
            {'op': 'rename', 'from': 'a.md', 'to': '.git/a.md'},
            {'op': 'rename', 'from': 'g.md', 'to': 'g.txt'},
        ]),
    ]


# --- text corpus ------------------------------------------------------------

FRAGMENTS = ['[[', ']]', '[', ']', '(', ')', '|', '#', '^', '!', '<', '>', '`', '```', '~~~', '---', '...', '\n', '\n',
             ' ', ' ', 'a', 'b', 'x/y', 'tag', '.md', ':', '"', "'", '\\', '%20', '%', '?', 'http://', 'mailto:', '/', './', '../',
             'é', '日', '😀', '\r\n', '<!--', '-->', 'aliases:', 'tags:', '- ', '[a]', '[[a]]', '#t', '[x](y.md)', '![[i.png]]',
             'import ', 'from ', 'require(', "'./a'", '"./b"', '@import ', 'url(', 'src=', 'href=', '\t', '&nbsp;', '﻿']


def fuzz_snippets(count, seed=1):
    rng = random.Random(seed)
    out = []
    for i in range(count):
        n = rng.randint(1, 40)
        text = ''.join(rng.choice(FRAGMENTS) for _ in range(n))
        if rng.random() < 0.25:
            text = '---\n' + text + '\n---\n' + ''.join(rng.choice(FRAGMENTS) for _ in range(rng.randint(0, 15)))
        out.append(text)
    return out


def text_request(roots, fuzz=4000):
    snippets = []
    for label, root in roots:
        for rel in tree_files(root):
            path = Path(root) / rel
            try:
                if path.stat().st_size > 20000 or path.is_symlink():
                    continue
                text = path.read_bytes().decode('utf-8')
            except (UnicodeError, OSError):
                continue
            if '\0' in text:
                continue
            snippets.append({'name': f'{label}/{rel}', 'path': rel, 'text': text})
    exts = ['x.md', 'x.py', 'x.js', 'x.css', 'x.html', 'x.ts', 'x.vue', 'x.scss', 'x.sass', 'x.htm', 'x.mjs', 'x.rs']
    for i, text in enumerate(fuzz_snippets(fuzz)):
        snippets.append({'name': f'fuzz/{i}', 'path': exts[i % len(exts)], 'text': text})
    for i, text in enumerate(fuzz_snippets(fuzz // 4, seed=2)):
        snippets.append({'name': f'fuzzmd/{i}', 'path': 'x.md', 'text': text})
    return {'mode': 'text', 'snippets': snippets}


# --- preferences corpus -----------------------------------------------------

def prefs_values():
    num_keys = ['textFadeMultiplier', 'nodeSizeMultiplier', 'lineSizeMultiplier', 'centerStrength', 'repelStrength',
                'linkStrength', 'linkDistance', 'scale', 'localDepth']
    bool_keys = ['showTags', 'showAttachments', 'hideUnresolved', 'showOrphans', 'showCode', 'showArrow', 'collapse-filter',
                 'collapse-display', 'collapse-forces', 'collapse-color-groups', 'localIncoming', 'localOutgoing', 'close',
                 'reduceMotion']
    hostile = [0, 1, -1, 2, 0.5, -0.0, 1e308, -1e308, 2.5, 3.5, 1.5, 0.4999, 5.5, 6.49, 6.5, 7, 1000, True, False, None, '3',
               '', 'x', [], [1], {}, {'a': 1}, 2 ** 53, 2 ** 70, -2 ** 70, 1.0, 0.3, 4.5]
    values = [None, 1, 'x', [], [1], True, {}, {'graph': {}}, {'unknown': 1, 'search': 'q'}]
    for key in num_keys:
        for v in hostile:
            values.append({key: v})
    for key in bool_keys:
        for v in hostile:
            values.append({key: v})
    for v in hostile + ['query', ' padded ', 'tag:#x', '\u0000']:
        values.append({'search': v})
    colors = ['#AbCdEf', '#abcdef', '#abcde', '#abcdeff', 'abcdef', '#ghijkl', '#12345g', ' #aabbcc', '#aabbcc ', '', None, 7,
              {'rgb': 0, 'a': 0}, {'rgb': 0xffffff, 'a': 1}, {'rgb': 0x1000000, 'a': 1}, {'rgb': -1, 'a': 1}, {'rgb': 1.0, 'a': 1},
              {'rgb': True, 'a': 1}, {'rgb': 5, 'a': 1.5}, {'rgb': 5, 'a': -0.1}, {'rgb': 5, 'a': 0.5}, {'rgb': 5, 'a': True},
              {'rgb': 5}, {'a': 1}, {'rgb': 5, 'a': 1, 'extra': 1}, {'rgb': '5', 'a': 1},
              {'rgb': 5, 'a': '1'}, [], '#ABCDEF']
    for c in colors:
        values.append({'colorGroups': [{'query': 'q', 'color': c}]})
    queries = ['q', '', None, 5, ['a'], {'a': 1}]
    for q in queries:
        values.append({'colorGroups': [{'query': q, 'color': '#aabbcc'}]})
    values.append({'colorGroups': [1, None, 'x', [], {}, {'query': 'ok', 'color': '#010203'}, {'color': '#010203'},
                                   {'query': 'ok'}, {'query': 'ok', 'color': '#010203', 'extra': 5}]})
    for n in (11, 12, 13, 20):
        values.append({'colorGroups': [{'query': f'g{i}', 'color': '#00000' + str(i % 10)} for i in range(n)]})
    values.append({'colorGroups': [{'query': 'bad', 'color': 3}] * 14 + [{'query': 'good', 'color': '#ffffff'}] * 14})
    values.append({'colorGroups': 'x'})
    values.append({'colorGroups': {'query': 'q', 'color': '#aabbcc'}})
    values.append({k: v for k, v in zip(num_keys, [9, 9, 9, 9, 99, 9, 999, 99, 99])})
    values.append({k: v for k, v in zip(num_keys, [-9, -9, -9, -9, -99, -9, -999, -99, -99])})
    return values

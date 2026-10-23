#!/usr/bin/env python3
"""Run the reference graph logic on a JSON request and print canonical JSON.

The reference package is discovered at run time under $IMPRO_REFERENCE_DIR
(the directory holding ``graphdata.py`` and ``linkparse.py``); it is imported
read-only and never written to.

Request modes (see ``cases.py`` for builders):

* ``graph``  - ``{"mode": "graph", "root", "max_nodes", "runs": [...]}``, one
  scan, then every run (``build`` / ``local`` / ``folder``) is dumped.
* ``text``   - ``{"mode": "text", "snippets": [{"path", "text"}]}``
* ``live``   - ``{"mode": "live", "root", "options", "ops": [...]}``; each op
  edits the tree, tells the index and the graph is dumped after it.
* ``prefs``  - ``{"mode": "prefs", "values": [...]}``
"""
import ast
import importlib
import json
import os
from pathlib import Path
import shutil
import sys

os.environ.setdefault('PYTHONDONTWRITEBYTECODE', '1')
sys.dont_write_bytecode = True


def load_modules():
    base = Path(os.environ['IMPRO_REFERENCE_DIR'])
    found = [p for p in base.iterdir() if p.is_dir()
             and (p / 'graphdata.py').is_file() and (p / 'linkparse.py').is_file()]
    if len(found) != 1:
        raise RuntimeError('reference must contain exactly one graph package')
    sys.path.insert(0, str(base))
    name = found[0].name

    def module(part):
        return importlib.import_module(name + '.' + part)
    return module('graphdata'), module('linkparse'), module('settings')


GD, LP, ST = load_modules()


def make_options(spec):
    spec = spec or {}
    keys = ('query', 'show_tags', 'show_attachments', 'existing_only',
            'show_orphans', 'show_code', 'strict_filters')
    values = {k: spec[k] for k in keys if k in spec}
    values['groups'] = [{'query': q} for q in spec.get('groups', [])]
    return GD.GraphOptions(**values)


def dump_graph(graph, focus=None):
    nodes = [{'id': n.id, 'label': n.label, 'kind': n.kind, 'path': n.path,
              'folder': n.folder, 'tags': list(n.tags), 'in': n.in_degree,
              'out': n.out_degree, 'size_hint': n.size_hint, 'group': n.group,
              'ctime': n.ctime}
             for n in graph.nodes]
    links = [{'source': e.source, 'target': e.target, 'kind': e.kind,
              'resolved': e.resolved, 'count': e.count, 'self': e.self}
             for e in graph.links]
    stats = graph.stats
    return {'nodes': nodes, 'links': links, 'truncated': graph.truncated,
            'focus': focus,
            'stats': {'counts': stats['counts'], 'orphans': stats['orphans'],
                      'unresolved': stats['unresolved'],
                      'ambiguous': [{'source': a['source'], 'target': a['target'],
                                     'candidates': a['candidates'],
                                     'resolved': a['resolved']}
                                    for a in stats['ambiguous']],
                      'components': stats['components']}}


def run_graph(request):
    index = GD.GraphIndex(request['root'], max_nodes=request.get('max_nodes', 3000))
    index.scan()
    for path, text in request.get('overrides', {}).items():
        index.set_text(path, text)
    out = []
    for run in request['runs']:
        options = make_options(run.get('options'))
        mode = run['mode']
        if mode == 'build':
            result = dump_graph(index.build(options))
        elif mode == 'local':
            graph = index.local(run['path'], run.get('depth', 1), run.get('incoming', True),
                                run.get('outgoing', True), options)
            focus = graph.stats.get('focus')
            result = dump_graph(graph, focus if any(n.id == focus for n in graph.nodes) else None)
        elif mode == 'folder':
            result = dump_graph(index.folder(run['prefix'], options))
        else:
            raise ValueError(mode)
        out.append({'name': run['name'], 'result': result})
    return {'runs': out}


def is_markup(text, link):
    """`<br>`-style autolinks: the engine does not take markup tags for references."""
    if link.syntax != 'md' or text[link.start:link.start + 1] != '<':
        return False
    inner = text[link.start + 1:text.find('>', link.start)]
    return '.' not in inner


def code_class(kind):
    if kind == 'python':
        return kind
    if kind in ('css', 'scss', 'sass'):
        return 'style'
    if kind in ('html', 'htm'):
        return 'html'
    return 'script'


def run_text(request):
    out = []
    for item in request['snippets']:
        path, text = item['path'], item['text']
        props, end = LP.parse_frontmatter(text)
        links = [{'target': l.target, 'kind': l.kind, 'alias': l.alias,
                  'heading': l.heading, 'block': l.block, 'line': l.line,
                  'start': l.start, 'syntax': l.syntax, 'markup': is_markup(text, l)}
                 for l in LP.extract_links(text)]
        code = [{'spec': c.spec, 'kind': code_class(c.kind), 'line': c.line}
                for c in LP.extract_code_links(path, text)]
        broken = False
        if path.lower().endswith('.py'):
            try:
                ast.parse(text)
            except (SyntaxError, ValueError):
                broken = True
        out.append({'name': item['name'], 'props': props, 'body_start': end,
                    'links': links, 'tags': LP.extract_tags(text), 'code': code,
                    'py_syntax_error': broken})
    return {'snippets': out}


def apply_op(index, root, op):
    def absolute(rel):
        return os.path.join(root, rel)
    kind = op['op']
    if kind == 'write':
        os.makedirs(os.path.dirname(absolute(op['path'])), exist_ok=True)
        with open(absolute(op['path']), 'w', encoding='utf-8') as stream:
            stream.write(op['text'])
        index.set_text(op['path'], None)  # saved: the buffer gives way to the file
    elif kind == 'set_text':
        index.set_text(op['path'], op['text'])
    elif kind == 'delete':
        target = absolute(op['path'])
        paths = [op['path']]
        if os.path.isdir(target):
            paths = [os.path.relpath(os.path.join(d, f), root).replace(os.sep, '/')
                     for d, _, files in os.walk(target) for f in files]
            shutil.rmtree(target)
        else:
            os.remove(target)
        for rel in paths:
            index.update_path(rel)
    elif kind == 'rename':
        source, dest = absolute(op['from']), absolute(op['to'])
        old = [op['from']]
        if os.path.isdir(source):
            old = [os.path.relpath(os.path.join(d, f), root).replace(os.sep, '/')
                   for d, _, files in os.walk(source) for f in files]
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        os.rename(source, dest)
        for rel in old:
            index.update_path(rel)
        if os.path.isdir(dest):
            for d, _, files in os.walk(dest):
                for f in files:
                    index.update_path(os.path.relpath(os.path.join(d, f), root).replace(os.sep, '/'))
        else:
            index.update_path(op['to'])
    else:
        raise ValueError(kind)


def run_live(request):
    root = os.path.abspath(request['root'])
    index = GD.GraphIndex(root, max_nodes=request.get('max_nodes', 3000))
    index.scan()
    options = make_options(request.get('options'))
    steps = [{'op': 'scan', 'result': dump_graph(index.build(options))}]
    for op in request['ops']:
        apply_op(index, root, op)
        steps.append({'op': op, 'result': dump_graph(index.build(options))})
    return {'steps': steps}


def run_prefs(request):
    return {'values': [ST.normalize_graph(v) for v in request['values']]}


MODES = {'graph': run_graph, 'text': run_text, 'live': run_live, 'prefs': run_prefs}


def main():
    request = json.loads(Path(sys.argv[1]).read_text(encoding='utf-8'))
    json.dump(MODES[request['mode']](request), sys.stdout, ensure_ascii=False, sort_keys=True)


if __name__ == '__main__':
    main()

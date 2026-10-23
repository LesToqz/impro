#!/usr/bin/env python3
"""Measure the reference graph's control strip and panel geometry (read-only run).

Prints one JSON object per window size: the rectangles of the pane, the closed
strip, its buttons and the open panel with its header buttons, in CSS pixels
relative to the pane's top-left corner.
"""
import argparse
import importlib
import json
import os
from pathlib import Path
import sys
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--size', action='append', default=[], help='WIDTHxHEIGHT, repeatable')
    args = parser.parse_args()
    packages = [p for p in args.reference.iterdir() if (p/'graphview.py').is_file()]
    if len(packages) != 1:
        raise RuntimeError('expected exactly one graph widget package')
    sys.path.insert(0, str(args.reference))
    def module(name):
        return importlib.import_module(packages[0].name+'.'+name)
    import gi
    if os.getenv('IMPRO_PARITY_GI_PATH'):
        gi.__path__.append(os.environ['IMPRO_PARITY_GI_PATH'])
    gi.require_foreign('cairo')
    gi.require_version('Gtk', '3.0')
    from gi.repository import Gtk, GLib
    config = tempfile.mkdtemp(prefix='impro-chip-')
    os.environ['XDG_CONFIG_HOME'] = config
    preferences = module('settings')
    preferences._path = lambda: Path(config)/'impro/settings.json'
    preferences.get_value = lambda key: {'graph_default_type': 'global', 'reduce_motion': False}.get(key)
    preferences.get_graph = lambda: {}
    preferences.set_graph = lambda value: None
    graphmod = module('graphview')
    root = Path(tempfile.mkdtemp(prefix='impro-chip-root-'))
    for name in ('a', 'b', 'c'):
        (root/f'{name}.md').write_text('[[a]] [[b]] [[c]]\n')
    context = GLib.MainContext.default()

    def pump(ms):
        end = time.monotonic()+ms/1000
        while True:
            while context.pending():
                context.iteration(False)
            if time.monotonic() >= end:
                return
            time.sleep(.002)

    graph = graphmod.GraphView(str(root), settings={})
    window = Gtk.Window(title='chip geometry')
    window.add(graph)
    window.show_all()
    window.move(0, 0)

    def js(script):
        done = []
        def finished(web, result, _):
            try:
                done.append((True, json.loads(web.evaluate_javascript_finish(result).to_json(0))))
            except Exception as error:
                done.append((False, str(error)))
        graph.webview.evaluate_javascript(script, -1, None, None, None, finished, None)
        end = time.monotonic()+15
        while not done:
            if time.monotonic() > end:
                raise RuntimeError('script deadline exceeded')
            pump(5)
        if not done[0][0]:
            raise RuntimeError(done[0][1])
        return done[0][1]

    end = time.monotonic()+20
    while not (graph._ready and graph.last_data is not None):
        if time.monotonic() > end:
            raise RuntimeError('readiness deadline exceeded')
        pump(5)
    pump(500)
    measure = """(()=>{
      const pane=document.querySelector('#graph').getBoundingClientRect();
      const r=e=>{if(!e)return null;const b=e.getBoundingClientRect();
        return b.width? {x:b.left-pane.left,y:b.top-pane.top,w:b.width,h:b.height}:null};
      const q=s=>document.querySelector(s), all=s=>Array.from(document.querySelectorAll(s));
      return {pane:r(q('#graph')), strip:r(q('.gc-chip')), strip_buttons:all('.gc-chip .icon-btn').map(r),
        panel:r(q('.gc-panel')), head_buttons:all('.gc-head .icon-btn').map(r)}})()"""
    sizes = args.size or ['904x1240']
    for size in sizes:
        w, h = (int(v) for v in size.split('x'))
        window.resize(w, h)
        pump(400)
        out = {'size': size}
        out['open'] = js(measure)
        js("document.querySelector('[data-tip=\"Hide graph controls\"]').click();true")
        pump(600)
        out['closed'] = js(measure)
        js("document.querySelector('.gc-chip .icon-btn').click();true")
        pump(600)
        print(json.dumps(out))
    window.destroy()


if __name__ == '__main__':
    main()

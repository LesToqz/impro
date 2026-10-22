#!/usr/bin/env python3
"""Read-only reference adapter. Discover its package; drive the real GTK window."""
import argparse
import dataclasses
import importlib
import json
import os
from pathlib import Path
import re
import sys
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--theme', required=True)
    parser.add_argument('--scene', required=True)
    args = parser.parse_args()
    packages = [p for p in args.reference.iterdir() if p.is_dir() and
                (p/'window.py').is_file() and (p/'terminal.py').is_file()]
    if len(packages) != 1:
        raise RuntimeError('reference must contain exactly one window/terminal package')
    package = packages[0].name
    sys.path.insert(0, str(args.reference))
    def module(name):
        return importlib.import_module(package+'.'+name)
    import gi
    if os.getenv('IMPRO_PARITY_GI_PATH'):
        gi.__path__.append(os.environ['IMPRO_PARITY_GI_PATH'])
    gi.require_foreign('cairo')
    gi.require_version('Gtk','3.0')
    gi.require_version('Gdk','3.0')
    gi.require_version('Vte','2.91')
    from gi.repository import Gdk, Gio, GLib, Gtk, Vte
    settings = module('settings')
    notes = module('notes')
    notes.NOTES_DIR = os.path.join('.impro','Notes')
    settings._path = lambda: Path(os.environ['XDG_CONFIG_HOME'])/'impro/settings.json'
    values = json.loads(os.environ['IMPRO_PARITY_SETTINGS'])
    # Resolve reference theme ids from its own registry, never by a literal prefix.
    themes = module('themes')
    themes.THEMES = tuple(dataclasses.replace(t, name=re.sub(re.escape(package), 'impro', t.name, flags=re.I)) for t in themes.THEMES)
    theme = themes.get_theme(themes.DEFAULT_THEME_ID.rsplit('-',1)[0]+'-'+args.theme)
    settings.load = lambda: dict(values, theme=theme.id)
    native = Gtk.Settings.get_default()
    native.set_property('gtk-enable-animations', False)
    native.set_property('gtk-cursor-blink', False)
    native.set_property('gtk-font-name', 'DejaVu Sans 10')
    app = Gtk.Application(application_id='dev.impro.ReferenceParity', flags=Gio.ApplicationFlags.NON_UNIQUE)
    app.register(None)
    w = module('window').MainWindow(app, str(args.root), theme=theme)
    w.set_title('impro')
    w.set_default_size(1190, 750)
    w.show_all()
    context = GLib.MainContext.default()
    def poll(predicate, timeout=20):
        deadline = time.monotonic()+timeout
        while not predicate():
            if time.monotonic() > deadline:
                raise RuntimeError('readiness deadline exceeded')
            context.iteration(False)
    def frame(widget=w):
        done = []
        signal = widget.connect_after('draw', lambda *_: done.append(True) and False)
        widget.queue_draw()
        poll(lambda: done)
        widget.disconnect(signal)
        while context.pending():
            context.iteration(False)
    def javascript(web, script):
        done = []
        def evaluated(view, result, _):
            done.append(json.loads(view.evaluate_javascript_finish(result).to_json(0)))
        web.evaluate_javascript(script, -1, None, None, None, evaluated, None)
        poll(lambda: done)
        return done[0]
    def open_file(name):
        w.editor.open_file(str(args.root/name))
        poll(lambda: any(t.path == str(args.root/name) for t in w.editor.tabs))
    for name in ['main.py','README.md','data.json']:
        open_file(name)
    frame()
    scene = args.scene
    target = w
    if scene in ('default','terminal-bottom'):
        pass
    elif scene == 'explorer-expanded':
        w.explorer.tree.expand_all()
    elif scene in ('explorer-selected','explorer-hover','explorer-menu','explorer-rename'):
        w.explorer.tree.get_selection().select_iter(w.explorer._find(str(args.root/'main.py')))
        w.explorer.focus_tree()
        if scene == 'explorer-hover':
            tree = w.explorer.tree
            path = tree.get_model().get_path(w.explorer._find(str(args.root/'main.py')))
            area = tree.get_background_area(path, None)
            _, x, y = tree.get_bin_window().get_origin()
            tree.get_display().get_default_seat().get_pointer().warp(Gdk.Screen.get_default(), x+100, y+area.y+area.height//2)
            frame()
        if scene == 'explorer-menu':
            w.explorer.tree.emit('popup-menu')
    elif scene in ('search-results','search-empty'):
        query = 'Example' if scene == 'search-results' else 'missing-value-xyz'
        w.explorer.focus_search(query)
        w.explorer.search.search_now()
    elif scene == 'explorer-notes':
        w.explorer.show_notes_section(True)
    elif scene.startswith('editor-') and scene.split('-',1)[1] in ('python','json','rust','toml','markdown'):
        open_file(dict(python='main.py',json='data.json',rust='src/main.rs',toml='settings.toml',markdown='README.md')[scene[7:]])
    elif scene == 'editor-selection-find':
        open_file('main.py')
        tab = next(t for t in w.editor.tabs if t.path == str(args.root/'main.py'))
        tab.buffer.select_range(tab.buffer.get_iter_at_offset(10),tab.buffer.get_iter_at_offset(21))
        w.editor.search_bar.set_search_mode(True)
        w.editor.search_entry.grab_focus()
        w.editor.search_entry.set_text('greet')
    elif scene == 'editor-unsaved':
        tab = next(t for t in w.editor.tabs if t.path == str(args.root/'data.json'))
        tab.buffer.insert(tab.buffer.get_end_iter(),'\n')
    elif scene in ('tab-hover','tab-close'):
        tab = next(t for t in w.editor.tabs if t.path == str(args.root/'main.py'))
        tab.header_inner.get_style_context().add_class('hover')
        if scene == 'tab-close':
            tab.close.set_state_flags(Gtk.StateFlags.PRELIGHT,False)
    elif scene == 'terminal-right':
        w.toggle_terminal_bottom()
        w.toggle_terminal_right()
    elif scene == 'terminal-split':
        w.terminal_bottom.new_terminal()
    elif scene == 'terminal-fullscreen':
        w.toggle_terminal_fullscreen()
    elif scene == 'terminal-maximized':
        w.terminal_bottom.new_terminal()
        w.terminal_bottom.toggle_maximize()
    elif scene == 'terminal-hover':
        w.terminal_bottom.add_button.set_state_flags(Gtk.StateFlags.PRELIGHT,False)
    elif scene in ('palette','palette-query'):
        w.open_command_palette()
        if scene == 'palette-query':
            w._palette.entry.set_text('terminal')
            w._palette.entry.set_position(-1)
    elif scene == 'switcher':
        w.open_quick_switcher()
    elif scene == 'notice':
        w.notify('Parity notice','info')
    elif scene.startswith('settings-'):
        w.open_settings(scene[9:])
        target = w._settings_dialog
    elif scene.startswith(('git-','commit-')):
        w._values['gitgraph_orientation'] = scene.split('-')[1]
        w.open_gitgraph()
        poll(lambda: all(v._ready and v.last_data is not None for v in w._gitgraph_views()))
        api = "Object.keys(window).filter(k=>k.toLowerCase().endsWith('git')).map(k=>window[k]).find(v=>v && typeof v.stats==='function')"
        for view in w._gitgraph_views():
            poll(lambda: javascript(view.webview, "(()=>{const a="+api+";return a ? a.stats().commits : 0})()") == 2)
        if scene.startswith('commit-'):

            w.open_commit(os.environ['IMPRO_PARITY_HEAD'])
            poll(lambda: not w._commit_pending and w._commit_views)
            poll(lambda: all(v._ready and v.last_data is not None for v in w._commit_views.values()))
    elif scene == 'graph':
        graph = w.open_graph()
        poll(lambda: graph._ready and graph.last_data is not None)
        graph.post('pause')
        data = graph.last_data
        colors = module('graphview').graph_colors(w.theme.ui, w.theme.ui['accent'])
        structure = dict(nodes=len(data['nodes']),edges=len({tuple(sorted((e['s'], e['t']))) for e in data['links']}),
                         defaults=dict(graph._settings, reduceMotion=w._values['reduce_motion']),panel_open=not graph._settings['close'],
                         panel_size=[graph.get_allocated_width(),graph.get_allocated_height()],
                         legend={key: colors['node' if key == 'fill' else 'node_'+key]
                                 for key in ['fill','tag','attachment','unresolved','code']})
        # Read labels and geometry from the real WebView controls, without
        # relying on the reference's JavaScript global name.
        done = []
        def evaluated(web, result, _):
            value = web.evaluate_javascript_finish(result)
            structure.update(json.loads(value.to_json(0)))
            done.append(True)
        graph.webview.evaluate_javascript("JSON.parse(JSON.stringify({control_labels:Array.from(document.querySelectorAll('#controls .sec-head>span:not(.chev), #controls .name, #controls .groups-new, #controls .row>.btn')).flatMap(e=>e.childElementCount ? Array.from(e.children).map(s=>s.textContent.trim()) : [e.textContent.trim()]),control_panel_width:document.querySelector('#controls').getBoundingClientRect().width}))", -1, None, None, None, evaluated, None)
        poll(lambda: done)
        args.output.with_suffix('.json').write_text(json.dumps(structure,indent=2))
    else:
        raise ValueError('unknown scene '+scene)

    def normalize(widget):
        # Normalize visible product labels while preserving the reference CSS.
        if isinstance(widget, Gtk.Label):
            old = widget.get_text()
            new = re.sub(re.escape(package), 'impro', old, flags=re.I)
            if new != old:
                widget.set_text(new)
        if isinstance(widget, Gtk.Container):
            widget.forall(normalize)
    def finish():
        if scene.startswith('commit-'):
            for view in w._commit_views.values():
                poll(lambda: javascript(view.webview, "!!document.querySelector('.file-name')"))
                javascript(view.webview, "window.__parityPaint=0;requestAnimationFrame(()=>requestAnimationFrame(()=>{window.__parityPaint=1}));true")
                poll(lambda: javascript(view.webview, 'window.__parityPaint') == 1)
        if scene == 'editor-selection-find':
            poll(lambda: w.editor.selected_text() == 'greet')
        poll(lambda: all(not t.highlighter.pending for t in w.editor.tabs))
        for panel in (w.terminal_bottom,w.terminal_right):
            panel.set_cursor_blink(False)
            poll(lambda: all(p.terminal is None or p.pid for p in panel._panes))
            poll(lambda: all(p.terminal is None or '$ ' in (p.terminal.get_text_format(Vte.Format.TEXT) or '') for p in panel._panes))
        if scene not in ('palette','palette-query','switcher') and not scene.startswith('settings-'):
            w.set_focus(None)
        normalize(w)
        if target is not w:
            normalize(target)
        w.set_title('impro')
        frame(target)
        deadline, previous, stable = time.monotonic()+8, None, 0
        while stable < 4:
            if time.monotonic()>deadline:
                raise RuntimeError('scene did not settle')
            frame(target)
            Gdk.Display.get_default().sync()
            surface, x, y = target.get_window(), 0, 0
            if scene == 'explorer-menu':
                _, x, y = surface.get_origin()
                surface = Gdk.Screen.get_default().get_root_window()
            pixbuf = Gdk.pixbuf_get_from_window(surface,x,y,target.get_allocated_width(),target.get_allocated_height())
            pixels = bytes(pixbuf.get_pixels())
            stable = stable+1 if pixels == previous else 0
            previous = pixels
        pixbuf.savev(str(args.output),'png',[],[])
        print('settled '+scene)
    if scene == 'explorer-rename':
        # The source dialog runs a nested GLib loop; capture from that loop.
        def capture_dialog():
            nonlocal target
            dialogs = [d for d in Gtk.Window.list_toplevels() if isinstance(d,Gtk.Dialog) and d.get_visible()]
            if not dialogs:
                return True
            target = dialogs[0]
            finish()
            target.response(Gtk.ResponseType.CANCEL)
            return False
        GLib.idle_add(capture_dialog)
        w.explorer._rename_dialog()
        w.destroy()
        return
    else:
        finish()
        w.destroy()


if __name__ == '__main__':
    main()

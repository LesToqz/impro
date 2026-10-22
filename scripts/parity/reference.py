#!/usr/bin/env python3
"""Read-only reference adapter. Discover its package; drive the real GTK window."""
import argparse
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
    gi.require_version('Gtk','3.0')
    gi.require_version('Gdk','3.0')
    gi.require_version('Vte','2.91')
    from gi.repository import Gdk, Gio, GLib, Gtk
    settings = module('settings')
    notes = module('notes')
    notes.NOTES_DIR = os.path.join('.impro','Notes')
    settings._path = lambda: Path(os.environ['XDG_CONFIG_HOME'])/'impro/settings.json'
    values = json.loads(os.environ['IMPRO_PARITY_SETTINGS'])
    # Resolve reference theme ids from its own registry, never by a literal prefix.
    themes = module('themes')
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
    w.set_default_size(1200, 800)
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
    def open_file(name):
        w.editor.open_file(str(args.root/name))
        poll(lambda: str(args.root/name) in w.editor.tabs)
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
            w.explorer.tree.set_state_flags(Gtk.StateFlags.PRELIGHT, False)
        if scene == 'explorer-menu':
            w.explorer.tree.emit('popup-menu')
    elif scene in ('search-results','search-empty'):
        query = 'Example' if scene == 'search-results' else 'missing-value-xyz'
        w.explorer.focus_search(query)
        w.explorer.search_panel.search_now()
    elif scene == 'explorer-notes':
        w.explorer.show_notes_section(True)
    elif scene.startswith('editor-') and scene.split('-',1)[1] in ('python','json','rust','toml','markdown'):
        open_file(dict(python='main.py',json='data.json',rust='src/main.rs',toml='settings.toml',markdown='README.md')[scene[7:]])
    elif scene == 'editor-selection-find':
        open_file('main.py')
        tab = w.editor.tabs[str(args.root/'main.py')]
        tab.buffer.select_range(tab.buffer.get_iter_at_offset(10),tab.buffer.get_iter_at_offset(21))
        w.editor.search_bar.set_search_mode(True)
        w.editor.search_entry.grab_focus()
        w.editor.search_entry.set_text('greet')
    elif scene == 'editor-unsaved':
        tab = w.editor.tabs[str(args.root/'data.json')]
        tab.buffer.insert(tab.buffer.get_end_iter(),'\n')
    elif scene in ('tab-hover','tab-close'):
        tab = w.editor.tabs[str(args.root/'main.py')]
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
        poll(lambda: all(v._ready for v in w._gitgraph_views()))
        if scene.startswith('commit-'):
            w.open_commit('HEAD')
            poll(lambda: not w._commit_pending and w._commit_views)
            poll(lambda: all(v._ready for v in w._commit_views.values()))
    elif scene == 'graph':
        w.open_graph()
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
        poll(lambda: all(not t.highlighter.pending for t in w.editor.tabs.values()))
        for panel in (w.terminal_bottom,w.terminal_right):
            panel.set_cursor_blink(False)
            poll(lambda: all(p.terminal is None or p.pid for p in panel._panes))
            poll(lambda: all(p.terminal is None or '$ ' in p.terminal.get_text()[0] for p in panel._panes))
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
            pixbuf = Gdk.pixbuf_get_from_window(target.get_window(),0,0,target.get_allocated_width(),target.get_allocated_height())
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

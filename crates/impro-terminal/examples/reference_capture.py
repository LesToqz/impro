"""Capture the read-only reference dock widgets in the dock demo composition.

Usage: reference_capture.py REFERENCE_ROOT OUTPUT_PNG
Run with PYTHONDONTWRITEBYTECODE=1 under Xvfb and a session bus.
"""
from pathlib import Path
import importlib
import sys

sys.dont_write_bytecode = True
root, output = Path(sys.argv[1]), Path(sys.argv[2])
sys.path.insert(0, str(root))
package = root.name.lower()
TerminalPanel = importlib.import_module(package + ".terminal").TerminalPanel
theme = importlib.import_module(package + ".themes").get_theme(None)
importlib.import_module(package + ".style").apply_style(theme)
from gi.repository import Gdk, GLib, Gtk

window = Gtk.Window()
window.set_default_size(1200, 800)
outer = Gtk.Paned(orientation=Gtk.Orientation.HORIZONTAL)
left = Gtk.Paned(orientation=Gtk.Orientation.VERTICAL)
editor = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
editor.get_style_context().add_class("background")
editor.pack_start(Gtk.Label(label="impro"), True, True, 0)
bottom = TerminalPanel("/tmp", autostart=False)
right = TerminalPanel("/tmp", autostart=False, stacked=True)
for panel in (bottom, right):
    panel.set_shell("/bin/bash")
    colors = theme.terminal
    panel.apply_palette(colors.foreground, colors.background, colors.ansi, colors.cursor)
    for _ in range(2):
        panel.new_terminal()
left.pack1(editor, True, False)
left.pack2(bottom, False, False)
left.set_position(470)
outer.pack1(left, True, False)
outer.pack2(right, False, False)
outer.set_position(800)
window.add(outer)
window.show_all()


def capture():
    pixbuf = Gdk.pixbuf_get_from_window(
        window.get_window(), 0, 0,
        window.get_allocated_width(), window.get_allocated_height(),
    )
    pixbuf.savev(str(output), "png", [], [])
    window.destroy()
    Gtk.main_quit()
    return False


GLib.timeout_add(800, capture)
Gtk.main()

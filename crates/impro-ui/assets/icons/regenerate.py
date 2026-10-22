#!/usr/bin/env python3
"""Rebuild checked-in raw rasters; requires GTK 3 tools, PyGObject and Pillow."""
from pathlib import Path
import json
import subprocess
import tempfile
import gi
from PIL import Image

gi.require_version('Gtk', '3.0')
gi.require_version('Gdk', '3.0')
from gi.repository import Gtk, Gdk  # noqa: E402

Gtk.init([])
root = Path(__file__).resolve().parent
colors = json.loads((root / 'colors.json').read_text())
theme = Gtk.IconTheme.get_default()
for source in sorted(root.glob('*.svg')):
    sizes = (16, 32, 64) if source.name.startswith('window-') else (16, 32)
    for size in sizes:
        output = root / str(size)
        output.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory() as temp:
            subprocess.run(['gtk-encode-symbolic-svg', str(source), f'{size}x{size}',
                            '-o', temp], check=True)
            png = Path(temp) / (source.stem + '.symbolic.png')
            (output / (source.stem + '.rgba')).write_bytes(
                Image.open(png).convert('RGBA').tobytes())
        info = theme.lookup_icon(source.stem, size, 0)
        assert Path(info.get_filename()).read_bytes() == source.read_bytes()
        atlas = bytearray()
        for color in colors:
            foreground = Gdk.RGBA()
            foreground.parse(color)
            image = info.load_symbolic(foreground, None, None, None)[0]
            assert (image.get_width(), image.get_height()) == (size, size)
            assert image.get_has_alpha()
            pixels = image.get_pixels()
            stride = image.get_rowstride()
            atlas.extend(b''.join(pixels[y*stride:y*stride+size*4]
                                  for y in range(size)))
        assert len(atlas) == len(colors) * size * size * 4
        (output / (source.stem + '-colors.rgba')).write_bytes(atlas)

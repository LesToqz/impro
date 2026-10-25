#!/usr/bin/env python3
"""Regenerate the raster icons and the Windows .ico from packaging/icons/impro.svg.

Run from anywhere:  python3 packaging/icons/build-icons.py

Needs python3-gi with GdkPixbuf and librsvg's pixbuf loader (both ship with GTK).
Outputs (committed):
  packaging/icons/hicolor/<N>x<N>/apps/impro.png   N in 16 24 32 48 64 128 256 512
  packaging/icons/hicolor/scalable/apps/impro.svg  (copy of impro.svg)
  packaging/icons/windows/impro.ico                PNG-compressed entries, every size
                                                   rendered from the vector
The script is deterministic: running it twice leaves the tree unchanged.
"""
import os
import shutil
import struct

import gi

gi.require_version("GdkPixbuf", "2.0")
from gi.repository import GdkPixbuf  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
SVG = os.path.join(HERE, "impro.svg")
HICOLOR_SIZES = (16, 24, 32, 48, 64, 128, 256, 512)
ICO_SIZES = (16, 20, 24, 32, 40, 48, 64, 96, 128, 256)


def render_png(size):
    pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_size(SVG, size, size)
    if pixbuf.get_width() != size or pixbuf.get_height() != size:
        raise SystemExit("unexpected render size for %d" % size)
    if not pixbuf.get_has_alpha():
        pixbuf = pixbuf.add_alpha(False, 0, 0, 0)
    ok, data = pixbuf.save_to_bufferv("png", [], [])
    if not ok:
        raise SystemExit("png encode failed for %d" % size)
    return bytes(data)


def write_ico(path, pngs):
    count = len(pngs)
    out = bytearray(struct.pack("<HHH", 0, 1, count))
    offset = 6 + 16 * count
    for size, data in pngs:
        dim = 0 if size >= 256 else size
        out += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    for _, data in pngs:
        out += data
    with open(path, "wb") as f:
        f.write(out)


def main():
    for size in HICOLOR_SIZES:
        directory = os.path.join(HERE, "hicolor", "%dx%d" % (size, size), "apps")
        os.makedirs(directory, exist_ok=True)
        with open(os.path.join(directory, "impro.png"), "wb") as f:
            f.write(render_png(size))
    scalable = os.path.join(HERE, "hicolor", "scalable", "apps")
    os.makedirs(scalable, exist_ok=True)
    shutil.copyfile(SVG, os.path.join(scalable, "impro.svg"))
    os.makedirs(os.path.join(HERE, "windows"), exist_ok=True)
    write_ico(
        os.path.join(HERE, "windows", "impro.ico"),
        [(size, render_png(size)) for size in ICO_SIZES],
    )


if __name__ == "__main__":
    main()

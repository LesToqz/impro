# Startup symbolic rasters

The raw RGBA files preserve GTK's symbolic channel encoding. A color atlas
also retains the SVG renderer's exact coverage rounding for the bundled theme
foregrounds. Other CSS colors and translucent state colors retain GTK's native
renderer so coverage rounding and transitions remain exact.
16 and 32 physical pixels cover ordinary menu icons at 1x/2x. Window controls
also include 64 physical pixels for their 32 logical pixel size at 2x.

Sources are the accompanying unmodified SVGs. The application verifies these
against the installed files on a worker and uses a raster only when the theme
lookup resolves to that exact source. Other desktop themes and revised SVGs
continue through the normal GTK lookup. No environment settings or accessibility
services are disabled. No installed theme or user cache is modified.

Yaru icons: copyright 2018 Sam Hewitt and the Ubuntu Yaru contributors,
https://github.com/ubuntu/yaru, licensed CC BY-SA 4.0:
https://creativecommons.org/licenses/by-sa/4.0/ . The raster derivatives retain
that license. The bottom-panel icon is from GNOME libpanel, copyright 2017–2022
Christian Hergert, LGPL version 3 or later:
https://gitlab.gnome.org/GNOME/libpanel and https://www.gnu.org/licenses/lgpl-3.0.html .

To regenerate the rasters (GTK development utilities required):

```sh
xvfb-run -a dbus-run-session -- python3 crates/impro-ui/assets/icons/regenerate.py
```

The raw rasters are checked in; normal Cargo builds need no extra tools.

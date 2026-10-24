# Third-party notices

The project's own code is licensed under the MIT licence (see `LICENSE`).
The bundled assets listed below are **not** covered by that licence; they keep
the licences of their sources. The generated `.rgba` rasters are adaptations of
their source SVGs and are licensed under the same terms as those SVGs.

| Files | Copyright holder | Licence | Source | Modifications |
| --- | --- | --- | --- | --- |
| `crates/impro-ui/assets/icons/*.svg` except `panel-bottom-symbolic.svg` (17 files) | 2018 Sam Hewitt (per the Debian `yaru-theme-icon` copyright file, `icons/*`) | CC-BY-SA-4.0, https://creativecommons.org/licenses/by-sa/4.0/ | Yaru icon theme, https://github.com/ubuntu/yaru | None to the SVG path data |
| `crates/impro-ui/assets/icons/panel-bottom-symbolic.svg` | 2017-2022 Christian Hergert (per the Debian `libpanel-common` copyright file, `Files: *`) | LGPL-3+ (version 3 or later), https://www.gnu.org/licenses/lgpl-3.0.html | GNOME libpanel, https://gitlab.gnome.org/GNOME/libpanel (path data identical to the `hicolor` theme file shipped by `libpanel-common`) | None to the SVG path data |
| `crates/impro-ui/assets/icons/{16,32,64}/*.rgba` | As the source SVG of each raster | As the source SVG of each raster | Generated from the SVGs above by `crates/impro-ui/assets/icons/regenerate.py` | Rasterised to RGBA for startup speed |

## Other bundled content

- `crates/impro-editor/src/docker.sublime-syntax` and
  `crates/impro-editor/src/toml.sublime-syntax`: short syntax definitions that
  were added to this repository by the project author. They carry no upstream
  copyright header and no upstream source was identified; they are treated as
  part of the project and covered by the MIT licence.
- `packaging/impro.svg`: the project's own logo (simple geometric shapes); MIT.
- No fonts or other images are bundled.

## Cargo dependencies

Rust dependencies keep their own licences. Their licence set is checked by
`cargo deny` (policy in `deny.toml`).

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

## Icon path data in code

`crates/impro-ui/src/glyphs.rs` draws the interface glyphs (panel toggles, the settings
cog, the Markdown preview/code buttons, the tab close button and the file and folder
glyphs of the explorer) with cairo from SVG path data. The glyphs marked "Lucide" in that
file follow the path data of the Lucide icon set:

> ISC License
>
> Copyright (c) for portions of Lucide are held by Cole Bemis 2013-2022 as part of Feather
> (MIT). All other copyright (c) for Lucide are held by Lucide Contributors 2022.
>
> Permission to use, copy, modify, and/or distribute this software for any purpose with or
> without fee is hereby granted, provided that the above copyright notice and this
> permission notice appear in all copies.
>
> THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO
> THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO
> EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL
> DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN
> AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
> CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

Source: https://lucide.dev (https://github.com/lucide-icons/lucide). The other glyphs in the
file (text lines, the bracketed T, the ringed R and the letter badges) were drawn for
impro and are covered by the MIT licence.

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

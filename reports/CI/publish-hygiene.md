# publish-hygiene report

STATUS: PARTIAL (all four items done; open questions below)

## Changes
1. **Paths** (25 files): `scripts/graph-compare/{README,DIVERGENCES}.md`, `scripts/graph-lib-bench/{README.md,bench.sh}`, 21 `results/*.md`.
   Rules: `<home>/projects/impro-wt/.heavy.lock` -> `<lock-file>` (described once per file as "any lock file shared by heavy jobs");
   `.../impro-wt/.qa-archive/` -> `<archive-dir>/`;
   `.../impro-wt/graph-lib-eval/.ceo/scratch/` -> `<scratch-dir>/`.
   Nothing else matched. Only path text changed: the diff of the results has no changed numeric line other than the `dbus-run-session` header line. `bash -n bench.sh` passes (shellcheck not installed).
2. **Fixture** (1 file): `gitcommit_reference.json`: author/committer name -> `Example Author`, email -> `author@example.com`; merge subject `Merge ceo/codex/pane-split: ...` -> `Merge feature/pane-split: ...`. No second copy of this data exists; no expectation depended on these values.
3. **Licence** : `LICENSE` (MIT, 2026, project author); `THIRD_PARTY_NOTICES.md`; "Licence and provenance" section appended to `crates/impro-ui/assets/icons/README.md`.

## Provenance findings
- Yaru icons: Debian `yaru-theme-icon` copyright file, `Files: icons/*`: Copyright 2018 Sam Hewitt, CC-BY-SA-4.0. I took the 17-of-18 match from the brief; I did not recompare them.
- `panel-bottom-symbolic.svg`: path data is identical (md5 of all `d=` attributes) to `/usr/share/icons/hicolor/scalable/actions/panel-bottom-symbolic.svg`, owned by package `libpanel-common`; its copyright file says `Files: *` Copyright 2017-2022 Christian Hergert, LGPL-3+.
- `docker.sublime-syntax`, `toml.sublime-syntax`: no upstream header; git history shows only the project author's commits. Origin otherwise unknown; treated as project code.
- `packaging/impro.svg`: simple project logo; no other fonts or images are tracked.

## Tests
- `cargo test -p impro-gitviews` (whole crate, locked and memory-capped): 18 + 18 (1 ignored) + 5 passed, 0 failed.
- `node --test crates/impro-gitviews/tests/js/*.test.mjs`: 16 pass, 0 fail.
- Acceptance greps 1 and 2 and the forbidden-word gates print nothing.

## Open questions for the maintainer
- Tracked files outside my ownership still mention `.ceo/` paths and `ceo/...` branch names: `scripts/**` (compare.py, inventory.py, bench-gui.sh, timing.sh, parity docs, startup-bench.py, ...), and the fixtures `gitgraph_deleted_branches.json` and `tests/js/gitgraph_core.test.mjs`, `tests/test_gitlog.rs`, `src/log.rs`. They are runnable defaults or test data, so I left them. Decide whether to rename before publishing.
- The sublime-syntax files have no verifiable origin; confirm the author wrote them.
- LGPL-3 icon next to MIT code: confirm you are comfortable with it, or replace the icon.

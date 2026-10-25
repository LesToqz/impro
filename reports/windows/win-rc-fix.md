# win-rc-fix

STATUS: DONE (Linux-verified only; the generated script is UNVERIFIED on a real GNU windres until the Windows runner runs)

## What changed
- `crates/impro/windows/impro.rc.in` (new template, replaces `impro.rc`): plain literals, numeric constants, no `#include`, no macros.
- `crates/impro/windows/rcgen.rs` (new): renders the template (version quad, absolute forward-slash paths); included by `build.rs` and the test with `#[path]`.
- `crates/impro/build.rs`: on Windows writes `OUT_DIR/impro-res.rc` and calls `embed_resource::compile_for` with no `-D` definitions and `manifest_required()`. Linux does no resource work, `IMPRO_SKIP_WIN_RESOURCES=1` still skips, rerun-if-changed covers template, generator, manifest and icon.
- `crates/impro/tests/test_rc_template.rs` (new, 4 tests).

## Verified (Linux host)
- `cargo test -p impro --test test_rc_template`: 4 passed.
- `cargo clippy -p impro --all-targets -- -D warnings`: clean. `cargo fmt --all --check`: clean.
- `scripts/windows/check.sh -p impro --lib --bins`: exit 0; `IMPRO_WIN_CARGO_CMD=clippy scripts/windows/check.sh -p impro --lib --bins -- -D warnings`: clean.
- `cargo build -p impro -vv`: build-script output only `cargo:rerun-if-env-changed=IMPRO_SKIP_WIN_RESOURCES`, no resource step.
- Not verifiable here: the `cfg(windows)` branch of `build.rs` is never compiled on Linux (build scripts run on the host), so it is untested, as is windres itself.

## Runbook
Generated script for version 0.1.0 (`<ABS>` = absolute checkout path with forward slashes, e.g. `D:/a/impro/impro`):

```rc
1 ICON "<ABS>/packaging/icons/windows/impro.ico"
1 24 "<ABS>/crates/impro/windows/impro.manifest"

1 VERSIONINFO
FILEVERSION 0,1,0,0
PRODUCTVERSION 0,1,0,0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0
FILEOS 0x00040004L
FILETYPE 1
FILESUBTYPE 0
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "FileDescription", "impro"
            VALUE "FileVersion", "0.1.0"
            VALUE "InternalName", "impro"
            VALUE "LegalCopyright", "MIT License"
            VALUE "OriginalFilename", "impro.exe"
            VALUE "ProductName", "impro"
            VALUE "ProductVersion", "0.1.0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
```

Constructs relied on: `N ICON "file"`; numeric resource type `1 24 "file"` (RT_MANIFEST); VERSIONINFO fixed header with numeric FILEVERSION/PRODUCTVERSION/FILEFLAGSMASK/FILEOS/FILETYPE; nested BLOCK/VALUE; `VALUE "Translation", 0x0409, 1200`. VALUE strings have no `\0`: both rc.exe and windres append the terminator.
embed-resource 3.0.12 runs `windres --input <rc> --output <OUT_DIR>/impro.lib?.a --include-dir <OUT_DIR> --output-format=coff --target <MINGW_CHOST or pe-x86-64> -c 65001` with the default gcc preprocessor, then links the archive via `rustc-link-arg-bins`.

If windres still complains, read the build-script stderr of `impro` in the runner log (`cargo build -vv`, section "impro@0.1.0 build.rs" / "windres: ..."): the message gives `<file>:<line>`; open `target/<profile>/build/impro-*/out/impro-res.rc` and compare with the text above. A "preprocessing failed" points at a path (spaces, quotes, backslashes) in the first two lines.

Fallbacks considered:
1. Drop the VERSIONINFO block (keep ICON and manifest) by deleting those lines from the template; isolates which construct fails.
2. Avoid the preprocessor/temp-file path: windres `--use-temp-file`, or `--preprocessor=cat`-style invocation, would need calling windres directly from build.rs (stop using `embed-resource`) and emitting `cargo:rustc-link-arg-bins` for the object ourselves. A simpler variant is copying icon and manifest next to the generated rc and using relative names.

## Open issues
None blocking. Assumes windres is on PATH in UCRT64 (as in the first run, which reached the syntax error).

#!/usr/bin/env bash
# Check the launcher and repeatable user installation without invoking Cargo.
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
work=$(mktemp -d -t impro-install-XXXXXXXX)
trap 'rm -rf -- "$work"' EXIT
export HOME="$work/home"
mkdir -p "$HOME"
# Use a real binary when available; otherwise test copying/argv with a fixture.
export IMPRO_INSTALL_BINARY=${IMPRO_INSTALL_BINARY:-"$repo/target/release/impro"}
if [[ ! -f "$IMPRO_INSTALL_BINARY" ]]; then
    export IMPRO_INSTALL_BINARY="$work/impro-fixture"
    cat > "$IMPRO_INSTALL_BINARY" <<'FIXTURE'
#!/usr/bin/env bash
if [[ "${1:-}" == --help ]]; then
    echo 'usage: impro [-h] [--version] [folder]'
else
    printf '%s\n' "$@"
fi
FIXTURE
    chmod 755 "$IMPRO_INSTALL_BINARY"
    echo 'Using packaging fixture (no release binary available).'
fi
python3 - "$repo/packaging/impro.desktop" <<'PY'
import configparser
import sys
entry = configparser.ConfigParser(interpolation=None)
entry.read(sys.argv[1])
entry = entry['Desktop Entry']
expected = dict(Name='impro', Exec='impro', Icon='impro', Terminal='false',
                Type='Application', Categories='Development;IDE;TextEditor;',
                StartupWMClass='dev.impro.impro')
for key, value in expected.items():
    assert entry[key] == value, (key, entry[key])
assert entry['Comment']
print('PASS desktop entry identity and categories')
PY
if command -v desktop-file-validate >/dev/null; then
    desktop-file-validate "$repo/packaging/impro.desktop"
    echo 'PASS desktop-file-validate'
else
    echo 'SKIP desktop-file-validate (not installed)'
fi
launcher="$HOME/.local/bin/impro"
desktop="$HOME/.local/share/applications/impro.desktop"
icon="$HOME/.local/share/icons/hicolor/scalable/apps/impro.svg"
for attempt in 1 2; do
    bash "$repo/packaging/install.sh"
    test -x "$launcher"
    cmp "$IMPRO_INSTALL_BINARY" "$launcher"
    cmp "$repo/packaging/impro.desktop" "$desktop"
    cmp "$repo/packaging/impro.svg" "$icon"
    test "$(stat -c %a "$launcher")" = 755
    test "$(stat -c %a "$desktop")" = 644
    test "$(stat -c %a "$icon")" = 644
    echo "PASS isolated install $attempt (binary, launcher, icon, modes)"
done
(cd -- "$HOME" && "$launcher" --help) > "$work/help"
rg -q '^usage: impro ' "$work/help"
echo 'PASS installed executable --help outside repository'
for attempt in 1 2; do
    bash "$repo/packaging/install.sh" --uninstall
    test ! -e "$launcher"
    test ! -e "$desktop"
    test ! -e "$icon"
    test -z "$(find "$HOME" -type f -print -quit)"
    echo "PASS isolated uninstall $attempt (no files remain)"
done
echo 'PASS packaging install/uninstall'

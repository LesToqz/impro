#!/usr/bin/env bash
# Check the launcher and repeatable user installation without invoking Cargo.
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
work=$(mktemp -d -t impro-install-XXXXXXXX)
trap 'rm -rf -- "$work"' EXIT
export HOME="$work/home"
mkdir -p "$HOME"
# Use a real binary when available; otherwise test copying/argv with a fixture.
if [[ -n "${IMPRO_INSTALL_BINARY:-}" && ! -x "$IMPRO_INSTALL_BINARY" ]]; then
    echo "impro: requested test binary is not executable: $IMPRO_INSTALL_BINARY" >&2
    exit 1
fi
fixture=0
export IMPRO_INSTALL_BINARY=${IMPRO_INSTALL_BINARY:-"$repo/target/release/impro"}
if [[ ! -f "$IMPRO_INSTALL_BINARY" ]]; then
    fixture=1
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
# StartupWMClass is the X11 WM_CLASS the window really sets (the program name),
# not the application id; the WM_CLASS probe further down checks it live.
expected = dict(Name='impro', Exec='impro %F', TryExec='impro', Icon='impro',
                Terminal='false', Type='Application',
                Categories='Development;IDE;TextEditor;',
                StartupWMClass='impro')
for key, value in expected.items():
    assert entry[key] == value, (key, entry[key])
assert entry['Comment'] and entry['GenericName'] and entry['Keywords']
# Offered in "Open With" for text and source files; never for directories and
# never a URL scheme handler.
assert entry['MimeType'].endswith(';')
mime = [m for m in entry['MimeType'].split(';') if m]
assert len(mime) == len(set(mime)), 'duplicate MimeType entries'
for required in ('text/plain', 'text/markdown', 'text/rust'):
    assert required in mime, required
for forbidden in ('inode/', 'x-scheme-handler/'):
    assert not any(m.startswith(forbidden) for m in mime), forbidden
print('PASS desktop entry identity, categories and MIME types')
PY
if command -v desktop-file-validate >/dev/null; then
    desktop-file-validate "$repo/packaging/impro.desktop"
    echo 'PASS desktop-file-validate'
else
    echo 'SKIP desktop-file-validate (not installed)'
fi
metainfo_src="$repo/packaging/dev.impro.impro.metainfo.xml"
python3 - "$metainfo_src" "$repo/Cargo.toml" <<'PY'
import re
import sys
import xml.etree.ElementTree as ET
root = ET.parse(sys.argv[1]).getroot()
assert root.tag == 'component' and root.get('type') == 'desktop-application'
assert root.findtext('id') == 'dev.impro.impro'
assert root.findtext('name') == 'impro'
assert root.findtext('summary') and root.find('description') is not None
assert root.findtext('metadata_license') == 'MIT'
# The code is MIT; bundled icon assets add their own licences (THIRD_PARTY_NOTICES.md).
assert root.findtext('project_license').split(' AND ')[0] == 'MIT'
notices = open(sys.argv[2].replace('Cargo.toml', 'THIRD_PARTY_NOTICES.md')).read()
for spdx, name in (('CC-BY-SA-4.0', 'Yaru'), ('LGPL-3.0-or-later', 'libpanel')):
    assert name in notices and spdx in root.findtext('project_license'), (spdx, name)
launchable = root.find("launchable[@type='desktop-id']")
assert launchable is not None and launchable.text == 'impro.desktop'
assert root.findtext('provides/binary') == 'impro'
assert root.find('content_rating').get('type') == 'oars-1.1'
assert {c.text for c in root.findall('categories/category')} >= {'Development', 'IDE'}
workspace = open(sys.argv[2]).read().split('[workspace.package]', 1)[1].split('\n[', 1)[0]
version = re.search(r'^version\s*=\s*"([^"]+)"', workspace, re.M).group(1)
release = root.find('releases/release')
assert release is not None and release.get('version') == version, (release.attrib, version)
print(f'PASS metainfo identity and release {version}')
PY
if command -v appstreamcli >/dev/null; then
    appstreamcli validate --no-net "$metainfo_src"
    echo 'PASS appstreamcli validate'
else
    echo 'SKIP appstreamcli validate (not installed)'
fi
launcher="$HOME/.local/bin/impro"
desktop="$HOME/.local/share/applications/impro.desktop"
mimecache="$HOME/.local/share/applications/mimeinfo.cache"
metainfo="$HOME/.local/share/metainfo/dev.impro.impro.metainfo.xml"
hicolor="$HOME/.local/share/icons/hicolor"
icon="$hicolor/scalable/apps/impro.svg"
sizes=(16 24 32 48 64 128 256 512)
for attempt in 1 2; do
    bash "$repo/packaging/install.sh"
    test -x "$launcher"
    cmp "$IMPRO_INSTALL_BINARY" "$launcher"
    cmp "$repo/packaging/impro.desktop" "$desktop"
    cmp "$metainfo_src" "$metainfo"
    cmp "$repo/packaging/impro.svg" "$icon"
    for size in "${sizes[@]}"; do
        cmp "$repo/packaging/icons/hicolor/${size}x${size}/apps/impro.png" "$hicolor/${size}x${size}/apps/impro.png"
        test "$(stat -c %a "$hicolor/${size}x${size}/apps/impro.png")" = 644
    done
    test "$(stat -c %a "$launcher")" = 755
    test "$(stat -c %a "$desktop")" = 644
    test "$(stat -c %a "$metainfo")" = 644
    test "$(stat -c %a "$icon")" = 644
    if command -v update-desktop-database >/dev/null; then
        # The desktop MIME cache feeds "Open With"; it must list markdown and
        # must not offer impro for directories.
        grep -qx 'text/markdown=impro.desktop;' "$mimecache"
        if grep -q '^inode/' "$mimecache"; then
            echo 'impro: the desktop entry must not claim directories' >&2
            exit 1
        fi
    fi
    echo "PASS isolated install $attempt (binary, launcher, desktop, metainfo, icon, modes)"
done
(cd -- "$HOME" && "$launcher" --help) > "$work/help"
grep -q '^usage: impro ' "$work/help"
echo 'PASS installed executable --help outside repository'
if [[ "$fixture" == 1 ]]; then
    test "$("$launcher" 'folder with spaces' '--literal')" = "$(printf '%s\n' 'folder with spaces' '--literal')"
    echo 'PASS installed fixture preserves argument boundaries'
elif command -v xvfb-run >/dev/null && command -v dbus-run-session >/dev/null \
    && command -v xwininfo >/dev/null && command -v xprop >/dev/null; then
    # StartupWMClass only matches the window if it equals the WM_CLASS the real
    # window sets, so start the installed program on a virtual display and ask.
    probe="$work/wmclass.sh"
    cat > "$probe" <<'PROBE'
#!/usr/bin/env bash
"$1" "$2" >/dev/null 2>&1 &
app=$!
id=
for _ in $(seq 1 80); do
    id=$(xwininfo -root -tree | sed -n 's/^ *\(0x[0-9a-f]*\) "impro":.*/\1/p' | head -n 1)
    [[ -n $id ]] && break
    sleep 0.25
done
[[ -n $id ]] && xprop -id "$id" WM_CLASS
kill "$app" 2>/dev/null
wait "$app" 2>/dev/null
true
PROBE
    mkdir -p "$work/probe-home" "$work/probe-project"
    wmclass=$(env HOME="$work/probe-home" XDG_CONFIG_HOME="$work/probe-home/.config" \
        GDK_BACKEND=x11 GIO_USE_VFS=local \
        xvfb-run -a dbus-run-session -- bash "$probe" "$launcher" "$work/probe-project" 2>/dev/null \
        | grep '^WM_CLASS' || true)
    if [[ -z "$wmclass" ]]; then
        echo 'SKIP WM_CLASS probe (no window appeared on the virtual display)'
    else
        want=$(sed -n 's/^StartupWMClass=//p' "$repo/packaging/impro.desktop")
        case "$wmclass" in
            *"\"$want\""*) echo "PASS StartupWMClass $want matches the window: $wmclass" ;;
            *) echo "FAIL StartupWMClass $want does not match the window: $wmclass" >&2; exit 1 ;;
        esac
    fi
else
    echo 'SKIP WM_CLASS probe (xvfb-run, dbus-run-session, xwininfo or xprop missing)'
fi
for attempt in 1 2; do
    bash "$repo/packaging/install.sh" --uninstall
    test ! -e "$launcher"
    test ! -e "$desktop"
    test ! -e "$metainfo"
    test ! -e "$icon"
    for size in "${sizes[@]}"; do
        test ! -e "$hicolor/${size}x${size}/apps/impro.png"
    done
    test -z "$(find "$HOME" -type f -print -quit)"
    echo "PASS isolated uninstall $attempt (no files remain)"
done
echo 'PASS packaging install/uninstall'

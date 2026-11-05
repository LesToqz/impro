"""Tests of the window probe and the screenshot cap (no Windows needed).

python3 -m unittest discover -s packaging/windows -p 'test_window_probe.py' -v

probe-vectors.json holds the expected results of the pure helpers of window-probe.ps1 and screenshot-log.ps1
(COLORREF to #RRGGBB, style bits, DPI scaling of offsets, caption height, probe points, line format, base64 size,
screenshot selection). test_window_probe.ps1 and test_screenshot_log.ps1 run the PowerShell functions against them in
the `check` job; this file checks the vectors themselves against an independent Python implementation, so a wrong
expectation cannot hide a wrong function. It also pins the Win32 constants of the probe to their documented values
and the wiring in smoke.ps1, window-capture.ps1, print-screenshots.ps1 and the README.
"""

import base64
import json
import math
import re
import unittest
from fractions import Fraction
from pathlib import Path

HERE = Path(__file__).resolve().parent


def read(name):
    return (HERE / name).read_text(encoding="utf-8").replace("\r\n", "\n")


VECTORS = json.loads(read("probe-vectors.json"))
PROBE = read("window-probe.ps1")
CAPTURE = read("window-capture.ps1")
SMOKE = read("smoke.ps1")
PRINT = read("print-screenshots.ps1")
SHOTLOG = read("screenshot-log.ps1")
README = read("README.md")

# winuser.h
STYLE_BITS = {
    "WS_CAPTION": 0x00C00000,  # WS_BORDER | WS_DLGFRAME
    "WS_THICKFRAME": 0x00040000,
    "WS_SYSMENU": 0x00080000,
    "WS_MINIMIZEBOX": 0x00020000,
    "WS_MAXIMIZEBOX": 0x00010000,
    "WS_VISIBLE": 0x10000000,
}


# --- independent implementations -----------------------------------------------------------------------------

def colorref(value):
    v = value & 0xFFFFFFFF
    if v > 0xFFFFFF:
        return "0x%08X" % v
    return "#%02X%02X%02X" % (v & 0xFF, (v >> 8) & 0xFF, (v >> 16) & 0xFF)


def hex32(value):
    return "0x%08X" % (value & 0xFFFFFFFF)


def style_flags(style):
    s = style & 0xFFFFFFFF
    return {name: (s & mask) == mask for name, mask in STYLE_BITS.items()}


def scaled_offset(offset, dpi):
    if dpi <= 0:
        dpi = 96
    exact = Fraction(offset * dpi, 96)
    return int(math.floor(exact + Fraction(1, 2)))  # half away from zero (offsets are positive)


def caption_height(has_caption, nonclient_top, window_height, client_height, dpi):
    if has_caption and nonclient_top >= 10:
        return nonclient_top, "client_origin"
    diff = window_height - client_height
    if has_caption and client_height > 0 and diff >= 10:
        return diff, "rect_difference"
    return scaled_offset(16, dpi), "default"


def probe_points(width, height, caption, dpi):
    half = width // 2
    below = caption + scaled_offset(6, dpi)
    inset = scaled_offset(12, dpi)
    raw = [
        ("caption_center", half, caption // 2),
        ("below_caption_x60", scaled_offset(60, dpi), below),
        ("below_caption_center", half, below),
        ("sidebar_body", scaled_offset(150, dpi), caption + scaled_offset(120, dpi)),
        ("editor_center", half, caption + (height - caption) // 2),
        ("bottom_right", width - inset, height - inset),
    ]
    return [(n, x, y, 0 <= x < width and 0 <= y < height) for n, x, y in raw]


def dpi_awareness_name(value):
    return {0: "unaware", 1: "system_aware", 2: "per_monitor_aware", -1: "invalid"}.get(value, "unknown")


def corner_name(value):
    return {0: "default", 1: "do_not_round", 2: "round", 3: "round_small"}.get(value, "unknown")


def dwm_value(kind, value):
    if kind == "color":
        return colorref(value)
    if kind == "corner":
        return "%d(%s)" % (value, corner_name(value))
    return str(value)


def one_line(text):
    return re.sub(r"\s*[\r\n]+\s*", " ", text).strip()


def format_line(key, value):
    return "[window-probe] %s=%s" % (key, one_line(value))


def base64_length(n):
    return (n + 2) // 3 * 4


def select_screenshots(names):
    ordered = sorted(names, key=str.upper)  # ordinal, case-insensitive
    chosen = [n for n in ordered if n.lower() == "desktop-window.png"][:1]
    chosen += [n for n in ordered if re.fullmatch(r"first-paint.*\.png", n, re.I)][:1]
    return chosen


class Vectors(unittest.TestCase):
    """The shared expectations agree with the independent implementation."""

    def test_colorref(self):
        for c in VECTORS["colorref"]:
            self.assertEqual(colorref(c["value"]), c["expect"], c)

    def test_colorref_is_bgr(self):
        self.assertEqual(colorref(0x0000FF), "#FF0000", "the low byte of a COLORREF is red")
        self.assertEqual(colorref(0xFF0000), "#0000FF", "the third byte of a COLORREF is blue")

    def test_hex32(self):
        for c in VECTORS["hex32"]:
            self.assertEqual(hex32(c["value"]), c["expect"], c)

    def test_pixel_color(self):
        for c in VECTORS["pixel_color"]:
            self.assertEqual("#%02X%02X%02X" % (c["r"], c["g"], c["b"]), c["expect"], c)

    def test_style_flags(self):
        for c in VECTORS["style_flags"]:
            self.assertEqual(style_flags(c["style"]), c["expect"], c["note"])

    def test_style_vectors_cover_both_outcomes_of_every_flag(self):
        for name in STYLE_BITS:
            seen = {c["expect"][name] for c in VECTORS["style_flags"]}
            self.assertEqual(seen, {True, False}, name)

    def test_scaled_offset(self):
        for c in VECTORS["scaled_offset"]:
            self.assertEqual(scaled_offset(c["offset"], c["dpi"]), c["expect"], c)

    def test_caption_height(self):
        for c in VECTORS["caption_height"]:
            got = caption_height(c["has_caption"], c["nonclient_top"], c["window_height"], c["client_height"], c["dpi"])
            self.assertEqual(got, (c["height"], c["source"]), c)

    def test_probe_points(self):
        for c in VECTORS["probe_points"]:
            got = probe_points(c["width"], c["height"], c["caption"], c["dpi"])
            want = [(p["name"], p["x"], p["y"], p["inrange"]) for p in c["points"]]
            self.assertEqual(got, want, c)

    def test_probe_points_scale_with_the_dpi_but_not_the_caption(self):
        # 150 % display: offsets grow by 1.5, the caption height (already device pixels) is used as given.
        by_name = {n: (x, y) for n, x, y, _ in probe_points(1500, 1000, 46, 144)}
        self.assertEqual(by_name["sidebar_body"], (225, 46 + 180))
        self.assertEqual(by_name["below_caption_x60"], (90, 46 + 9))

    def test_names(self):
        for c in VECTORS["dpi_awareness_name"]:
            self.assertEqual(dpi_awareness_name(c["value"]), c["expect"], c)
        for c in VECTORS["corner_preference_name"]:
            self.assertEqual(corner_name(c["value"]), c["expect"], c)

    def test_dwm_value(self):
        for c in VECTORS["dwm_value"]:
            self.assertEqual(dwm_value(c["kind"], c["value"]), c["expect"], c)

    def test_line_format(self):
        for c in VECTORS["one_line"]:
            self.assertEqual(one_line(c["text"]), c["expect"], c)
        for c in VECTORS["format_line"]:
            self.assertEqual(format_line(c["key"], c["value"]), c["expect"], c)
            self.assertRegex(c["expect"], r"^\[window-probe\] [^=]+=")

    def test_base64_length(self):
        for c in VECTORS["base64_length"]:
            self.assertEqual(base64_length(c["bytes"]), c["expect"], c)
            if c["bytes"] < 5000:
                self.assertEqual(len(base64.b64encode(bytes(c["bytes"]))), c["expect"], c)

    def test_select_screenshots(self):
        for c in VECTORS["select_screenshots"]:
            self.assertEqual(select_screenshots(c["names"]), c["expect"], c)


class ProbeSource(unittest.TestCase):
    """The constants and keys in window-probe.ps1."""

    def test_style_bits_match_winuser_h(self):
        body = re.search(r"function Get-WindowStyleFlags.*?\n}\n", PROBE, re.S).group(0)
        found = {m.group(1): int(m.group(2), 16) for m in re.finditer(r"^\s+(WS_\w+)\s*=\s*(0x[0-9A-Fa-f]+)\s*$", body, re.M)}
        self.assertEqual(found, STYLE_BITS)

    def test_dwm_attributes(self):
        specs = re.findall(r"@\('(dwm_\w+)',\s*(\d+),\s*'(\w+)'\)", PROBE)
        self.assertEqual(
            specs,
            [
                ("dwm_immersive_dark_mode", "20", "bool"),  # DWMWA_USE_IMMERSIVE_DARK_MODE
                ("dwm_window_corner_preference", "33", "corner"),  # DWMWA_WINDOW_CORNER_PREFERENCE
                ("dwm_border_color", "34", "color"),  # DWMWA_BORDER_COLOR
                ("dwm_caption_color", "35", "color"),  # DWMWA_CAPTION_COLOR
                ("dwm_text_color", "36", "color"),  # DWMWA_TEXT_COLOR
            ],
        )

    def test_win32_constants(self):
        for needle, why in {
            "DwmGetWindowAttribute(hWnd, 9, out r,": "DWMWA_EXTENDED_FRAME_BOUNDS is 9",
            "GetWindowLongW(hWnd, -16)": "GWL_STYLE is -16",
            "GetWindowLongW(hWnd, -20)": "GWL_EXSTYLE is -20",
            "SendMessageTimeout(hWnd, 0x7F,": "WM_GETICON is 0x7F",
            "GetDeviceCaps(dc, 88)": "LOGPIXELSX is 88",
            "new IntPtr(-4)": "DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 is -4",
            "OpenProcess(0x1000,": "PROCESS_QUERY_LIMITED_INFORMATION is 0x1000",
            "ClassIcon(IntPtr hWnd, int index)": "GetClassLongPtr wrapper",
            "ClassIcon($h, -14)": "GCLP_HICON is -14",
            "ClassIcon($h, -34)": "GCLP_HICONSM is -34",
            "9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3": "PKEY_AppUserModel_ID format id",
            "key.pid = 5;": "PKEY_AppUserModel_ID property id",
            "886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99": "IID_IPropertyStore",
            "vt == 31": "VT_LPWSTR is 31",
            "SHGetPropertyStoreForWindow": "window property store",
        }.items():
            self.assertIn(needle, PROBE, why)
        icons = re.findall(r"@\('(small|big|small2)',\s*(\d)\)", PROBE)
        self.assertEqual(icons, [("small", "0"), ("big", "1"), ("small2", "2")], "ICON_SMALL, ICON_BIG, ICON_SMALL2")

    def test_property_store_vtable_order(self):
        methods = re.findall(r"\[PreserveSig\] int (\w+)\(", PROBE)
        self.assertEqual(methods, ["GetCount", "GetAt", "GetValue", "SetValue", "Commit"], "IPropertyStore vtable order")

    def test_fact_keys_are_stable_and_documented(self):
        keys = set(re.findall(r"(?:Invoke-ProbeFact|Write-ProbeLine)\s+'([a-z0-9_]+)'", PROBE))
        required = {
            "os_version", "os_build", "pid", "title", "class", "style", "exstyle", "window_rect", "window_size",
            "client_size", "dwm_frame_bounds", "dpi_window", "dpi_source", "dpi_awareness_window",
            "dpi_awareness_process", "app_user_model_id", "app_user_model_id_matches", "class_icon_big",
            "class_icon_small", "caption_height_px", "caption_height_source", "bitmap_size", "done",
        }
        self.assertEqual(sorted(required - keys), [], "keys the probe must print")
        readme_keys = required | set(re.findall(r"@\('(dwm_\w+)',", PROBE)) | {n.lower() for n in STYLE_BITS}
        for key in sorted(readme_keys):
            self.assertTrue(key in README, f"README does not document the probe key {key}")
        for point in ("caption_center", "below_caption_x60", "below_caption_center", "sidebar_body", "editor_center", "bottom_right"):
            self.assertTrue(point in PROBE, point)
            self.assertTrue(point in README, f"README does not document the pixel probe {point}")
        self.assertIn("pixel(", PROBE)
        self.assertIn("'caption_height_px'", PROBE)

    def test_never_throws_and_nothing_runs_at_dot_source_time(self):
        code = re.sub(r"@'\n.*?\n'@", "", PROBE, flags=re.S)  # the C# of the here-strings starts at column 0
        top_level = [l for l in code.splitlines() if l and not l.startswith((" ", "\t", "#", "}", "function ", ")"))]
        self.assertEqual(top_level, [], "window-probe.ps1 may only define functions")
        body = re.search(r"function Write-WindowProbe.*", PROBE, re.S).group(0)
        self.assertTrue(body.rstrip().endswith("}"))
        self.assertRegex(body, r"(?s)\btry \{.*\} catch \{\s*Write-Host \('::warning::")

    def test_two_helpers_compile_independently(self):
        self.assertEqual(len(re.findall(r"Add-Type -TypeDefinition @'", PROBE)), 2)
        self.assertIn("public static class ImproProbe {", PROBE)
        self.assertIn("public static class ImproProbeShell {", PROBE)
        self.assertRegex(PROBE, r"(?s)\$win32 = \$true\n    \} catch \{.*?\$shell = \$true\n    \} catch")


class Wiring(unittest.TestCase):
    def test_smoke_runs_the_probe_after_the_capture_in_installer_mode_only(self):
        self.assertIn(". (Join-Path $PSScriptRoot 'window-probe.ps1')", SMOKE)
        self.assertLess(SMOKE.index("'window-capture.ps1'"), SMOKE.index("'window-probe.ps1'"))
        m = re.search(r"if \(\$Mode -eq 'installer'\) \{\n(.*?)\n    \}\n", SMOKE[SMOKE.index("function Test-App"):], re.S)
        self.assertIsNotNone(m)
        block = m.group(1)
        self.assertIn("Save-DesktopWindowScreenshot", block)
        self.assertRegex(block, r"-AfterCapture \{ param\(\$Window, \$Png\) Write-WindowProbe -Hwnd \$Window -ScreenshotPath \$Png")

    def test_capture_calls_the_probe_while_the_app_is_still_running(self):
        self.assertIn("[scriptblock]$AfterCapture", CAPTURE)
        call = CAPTURE.index("& $AfterCapture $hwnd $Path")
        self.assertLess(call, CAPTURE.index("} finally {", call), "the app is only stopped in the finally block")
        self.assertLess(CAPTURE.index("Save-ScreenRectangle -X"), call, "the probe reads the capture, so it runs after it")
        self.assertRegex(CAPTURE[call - 40 : call + 200], r"(?s)try \{.*catch \{ Write-Host \"::warning::")

    def test_print_screenshots_is_capped(self):
        self.assertRegex(PRINT, r"\[int\]\$MaxWidth = 700\b")
        self.assertRegex(PRINT, r"\[int\]\$MaxBase64KB = 60\b")
        self.assertIn("Select-ScreenshotNames", PRINT)
        self.assertIn("[screenshots] skipped {0} ({1} bytes", PRINT)
        self.assertIn("-ScaleAboveBytes 0", PRINT, "always downscale first")
        self.assertIn("function Select-ScreenshotNames", SHOTLOG)
        self.assertIn("function Get-Base64Length", SHOTLOG)

    def test_marker_lines_are_unchanged(self):
        self.assertIn('"-----BEGIN IMPRO-SCREENSHOT ${Name}-----"', SHOTLOG)
        self.assertIn('"-----END IMPRO-SCREENSHOT ${Name}-----"', SHOTLOG)

    def test_help_usage_line_follows_the_application(self):
        m = re.search(r"\[string\]\$ExpectedUsage = '([^']+)'", SMOKE)
        self.assertIsNotNone(m, "ExpectedUsage must be a smoke.ps1 parameter")
        self.assertEqual(m.group(1), "usage: impro [-h] [--version] [path ...]")
        self.assertNotIn("[folder]", SMOKE)
        workflow = (HERE.parents[1] / ".github" / "workflows" / "windows.yml").read_text(encoding="utf-8")
        real, standin = workflow.split("  smoke-standin:\n")
        self.assertNotIn("-ExpectedUsage", real, "the product smoke job uses the default (the real usage line)")
        # The temporary stand-in executable still prints the old line (crates/impro-ui/examples/packaging_standin.rs).
        self.assertEqual(standin.count("-ExpectedUsage 'usage: impro [-h] [--version] [folder]'"), 2)

    def test_new_tests_run_in_ci(self):
        workflow = (HERE.parents[1] / ".github" / "workflows" / "windows.yml").read_text(encoding="utf-8")
        self.assertIn("test_window_probe.ps1", workflow)
        self.assertIn("test_window_probe.py", workflow)


if __name__ == "__main__":
    unittest.main()

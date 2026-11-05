"""Tests of extract_log_screenshots.py on synthetic logs.

    python3 -I scripts/ci/test_extract_log_screenshots.py
    python3 -I -m unittest discover -s scripts/ci -p 'test_extract_log_screenshots.py' -v

The logs are built the way packaging/windows/print-screenshots.ps1 prints them: marker lines around base64 wrapped
at 100 columns, every line behind a GitHub Actions timestamp.
"""

import base64
import os
import struct
import subprocess
import sys
import tempfile
import unittest
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import extract_log_screenshots as ex  # noqa: E402


def chunk(kind, payload):
    body = kind + payload
    return struct.pack(">I", len(payload)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)


def make_png(width, height, seed):
    """A valid RGB PNG with a pattern that depends on the seed."""
    rows = b"".join(
        b"\x00" + bytes((x * 3 + y * 5 + seed * 17 + c * 40) % 256 for x in range(width) for c in range(3))
        for y in range(height)
    )
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return ex.PNG_MAGIC + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")


def block(name, data, width=100):
    text = base64.b64encode(data).decode("ascii")
    body = [text[i : i + width] for i in range(0, len(text), width)]
    return [f"-----BEGIN IMPRO-SCREENSHOT {name}-----", *body, f"-----END IMPRO-SCREENSHOT {name}-----"]


def stamp(lines, start=0):
    """Prefix every line the way GitHub Actions does."""
    return [f"2026-11-05T07:{24 + (start + i) // 60:02d}:{(start + i) % 60:02d}.{(start + i) * 1234567 % 10**7:07d}Z {line}"
            for i, line in enumerate(lines)]


def log_text(lines, eol="\r\n", bom=False):
    return ("\ufeff" if bom else "") + eol.join(lines) + eol


class Fixture(unittest.TestCase):
    def setUp(self):
        self.first = make_png(300, 200, 1)
        self.second = make_png(120, 90, 2)
        self.assertGreater(len(base64.b64encode(self.first)), 300, "the test image must span several 100-column lines")
        noise = ["##[group]Run ./packaging/windows/print-screenshots.ps1", "[screenshots] 2 PNG file(s) in D:\\a\\_temp\\smoke",
                 "[screenshots] first-paint-installer-cold.png: 51234 bytes on disk, 51234 bytes printed"]
        self.lines = (
            noise
            + block("first-paint-installer-cold.png", self.first)
            + ["[screenshots] desktop-window.png: 912345 bytes on disk, 301122 bytes printed"]
            + block("desktop-window.png", self.second)
            + ["[screenshots] printed 2 of 2 file(s)", "##[endgroup]"]
        )


class Extract(Fixture):
    def test_blocks_round_trip(self):
        shots, problems = ex.extract(log_text(stamp(self.lines)))
        self.assertEqual(problems, [])
        self.assertEqual([s.name for s in shots], ["first-paint-installer-cold.png", "desktop-window.png"])
        self.assertEqual(shots[0].data, self.first)
        self.assertEqual(shots[1].data, self.second)
        self.assertEqual([s.warning for s in shots], [None, None])

    def test_plain_log_without_timestamps_and_with_lf(self):
        shots, problems = ex.extract(log_text(self.lines, eol="\n"))
        self.assertEqual((len(shots), problems), (2, []))

    def test_byte_order_mark_and_carriage_returns(self):
        shots, problems = ex.extract(log_text(stamp(self.lines), eol="\r\n", bom=True))
        self.assertEqual((len(shots), problems), (2, []))
        shots, problems = ex.extract(log_text(stamp(self.lines), eol="\r\r\n"))
        self.assertEqual((len(shots), problems), (2, []))

    def test_gh_run_view_columns_and_ansi(self):
        lines = [f"smoke\tPrint screenshots to the log\t{line}" for line in stamp(self.lines)]
        lines = [lines[0], *[f"\x1b[0m{line}\x1b[0m" for line in lines[1:]]]
        shots, problems = ex.extract(log_text(lines))
        self.assertEqual((len(shots), problems), (2, []))
        self.assertEqual(shots[0].data, self.first)

    def test_blocks_are_found_between_other_output(self):
        other = ["2026-11-05T07:00:00.0000000Z start", "", "ok"]
        text = log_text(other + stamp(block("a.png", self.second)) + other + stamp(block("b.png", self.first)) + other)
        shots, problems = ex.extract(text)
        self.assertEqual((len(shots), problems), (2, []))

    def test_noise_inside_a_block_is_ignored(self):
        body = block("a.png", self.first)
        noisy = body[:2] + ["##[warning]something happened", "x y z"] + body[2:]
        shots, problems = ex.extract(log_text(stamp(noisy)))
        self.assertEqual(problems, [])
        self.assertEqual(shots[0].data, self.first)

    def test_a_short_word_between_full_lines_is_not_taken_for_base64(self):
        body = block("a.png", self.first)
        self.assertGreater(len(body), 4)
        noisy = body[:2] + ["Done"] + body[2:]
        shots, problems = ex.extract(log_text(stamp(noisy)))
        self.assertEqual(problems, [])
        self.assertEqual(shots[0].data, self.first)

    def test_other_wrap_widths_work(self):
        for width in (76, 64, 1000):
            shots, problems = ex.extract(log_text(stamp(block("w.png", self.first, width))))
            self.assertEqual((len(shots), problems), (1, []), width)
            self.assertEqual(shots[0].data, self.first)

    def test_no_blocks(self):
        self.assertEqual(ex.extract("nothing here\nat all\n"), ([], []))
        self.assertEqual(ex.extract(""), ([], []))


class Rejects(Fixture):
    def test_magic_bytes_are_verified(self):
        fake = b"GIF89a" + bytes(range(256)) * 3
        shots, problems = ex.extract(log_text(stamp(block("fake.png", fake) + block("real.png", self.second))))
        self.assertEqual([s.name for s in shots], ["real.png"])
        self.assertEqual(len(problems), 1)
        self.assertIn("PNG magic", problems[0])
        self.assertIn("fake.png", problems[0])

    def test_invalid_base64(self):
        lines = ["-----BEGIN IMPRO-SCREENSHOT bad.png-----", "AAAA" * 25, "AAA", "-----END IMPRO-SCREENSHOT bad.png-----"]
        shots, problems = ex.extract(log_text(stamp(lines)))
        self.assertEqual(shots, [])
        self.assertEqual(len(problems), 1)
        self.assertIn("bad.png", problems[0])

    def test_empty_block(self):
        lines = ["-----BEGIN IMPRO-SCREENSHOT e.png-----", "-----END IMPRO-SCREENSHOT e.png-----"]
        shots, problems = ex.extract(log_text(stamp(lines)))
        self.assertEqual((shots, len(problems)), ([], 1))
        self.assertIn("empty", problems[0])

    def test_unterminated_block_at_the_end(self):
        lines = block("cut.png", self.first)[:-1]
        shots, problems = ex.extract(log_text(stamp(lines)))
        self.assertEqual(shots, [])
        self.assertIn("not terminated", problems[0])

    def test_begin_inside_a_block_drops_the_first_one(self):
        first = block("one.png", self.first)
        shots, problems = ex.extract(log_text(stamp(first[:3] + block("two.png", self.second))))
        self.assertEqual([s.name for s in shots], ["two.png"])
        self.assertEqual(len(problems), 1)
        self.assertIn("one.png", problems[0])

    def test_end_without_begin_and_mismatched_end(self):
        lines = ["-----END IMPRO-SCREENSHOT lost.png-----"]
        self.assertEqual(len(ex.extract(log_text(lines))[1]), 1)
        body = block("a.png", self.second)
        body[-1] = "-----END IMPRO-SCREENSHOT b.png-----"
        shots, problems = ex.extract(log_text(body))
        self.assertEqual(shots, [])
        self.assertIn("closed by an END line", problems[0])

    def test_damaged_png_is_kept_with_a_warning(self):
        cut = self.first[:-20]
        shots, problems = ex.extract(log_text(stamp(block("cut.png", cut))))
        self.assertEqual(problems, [])
        self.assertEqual(shots[0].data, cut)
        self.assertIn("truncated", shots[0].warning)
        broken = bytearray(self.first)
        broken[40] ^= 0xFF  # inside the IDAT payload
        shots, problems = ex.extract(log_text(stamp(block("crc.png", bytes(broken)))))
        self.assertIn("bad CRC", shots[0].warning)

    def test_png_warning_of_a_good_file(self):
        self.assertIsNone(ex.png_warning(self.first))
        self.assertIsNone(ex.png_warning(self.second))


class Names(unittest.TestCase):
    def test_safe_name(self):
        for raw, want in {
            "desktop-window.png": "desktop-window.png",
            "..\\..\\evil.png": "evil.png",
            "../../etc/passwd": "passwd.png",
            "/abs/path/x.PNG": "x.PNG",
            "a b?.png": "a b_.png",
            "...": "screenshot.png",
            "": "screenshot.png",
            "dir/": "screenshot.png",
        }.items():
            self.assertEqual(ex.safe_name(raw), want, raw)


class Writing(Fixture):
    def test_unique_names_and_no_escape(self):
        shots = [ex.Shot("x.png", self.first), ex.Shot("X.png", self.second), ex.Shot("../../up.png", self.second),
                 ex.Shot("x.png", self.first)]
        with tempfile.TemporaryDirectory() as tmp:
            out = os.path.join(tmp, "nested", "out")
            written = ex.write_shots(shots, out)
            self.assertEqual(sorted(os.listdir(out)), ["X-2.png", "up.png", "x-3.png", "x.png"])
            self.assertEqual(os.listdir(tmp), ["nested"], "nothing was written outside the output directory")
            self.assertEqual([size for _, size in written], [len(self.first), len(self.second), len(self.second), len(self.first)])
            with open(os.path.join(out, "x.png"), "rb") as handle:
                self.assertEqual(handle.read(), self.first)


class CommandLine(Fixture):
    """Runs the script as CI would: python3 -I script, a log file or standard input."""

    def run_script(self, args, stdin=None):
        proc = subprocess.run([sys.executable, "-I", str(HERE / "extract_log_screenshots.py"), *args],
                              input=stdin, capture_output=True, timeout=60)
        return proc.returncode, proc.stdout.decode(), proc.stderr.decode()

    def test_log_file_and_stdin(self):
        data = log_text(stamp(self.lines)).encode("utf-8")
        with tempfile.TemporaryDirectory() as tmp:
            log = os.path.join(tmp, "job.log")
            with open(log, "wb") as handle:
                handle.write(data)
            code, out, err = self.run_script(["-o", os.path.join(tmp, "a"), log])
            self.assertEqual((code, err), (0, ""))
            self.assertIn("wrote", out)
            self.assertEqual(sorted(os.listdir(os.path.join(tmp, "a"))), ["desktop-window.png", "first-paint-installer-cold.png"])
            code, out, err = self.run_script(["-o", os.path.join(tmp, "b")], stdin=data)
            self.assertEqual(code, 0, err)
            code, out, err = self.run_script(["-o", os.path.join(tmp, "c"), "-"], stdin=data)
            self.assertEqual(code, 0, err)
            with open(os.path.join(tmp, "c", "desktop-window.png"), "rb") as handle:
                self.assertEqual(handle.read(), self.second)

    def test_exit_codes(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = os.path.join(tmp, "o")
            code, _, err = self.run_script(["-o", out], stdin=b"no screenshots in this log\n")
            self.assertEqual(code, 1)
            self.assertIn("no screenshot", err)
            mixed = log_text(block("good.png", self.second) + block("bad.png", b"not a png at all, just text" * 10)).encode()
            code, stdout, err = self.run_script(["-o", out], stdin=mixed)
            self.assertEqual(code, 3, err)
            self.assertEqual(os.listdir(out), ["good.png"])
            code, _, err = self.run_script([os.path.join(tmp, "missing.log")])
            self.assertEqual(code, 2)
            self.assertIn("cannot read", err)

    def test_isolated_mode_imports_only_the_standard_library(self):
        code, _, err = self.run_script(["--help"])
        self.assertEqual(code, 0, err)


if __name__ == "__main__":
    unittest.main()

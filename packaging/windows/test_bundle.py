"""Unit tests for bundle.py: python3 -m unittest packaging/windows/test_bundle.py -v"""

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import bundle  # noqa: E402

REPO = HERE.parents[1]


def find_sysroot():
    base = Path(os.environ.get("IMPRO_WIN_SYSROOT", Path.home() / ".cache" / "impro-win-sysroot"))
    for name in ("current-full", "current"):
        p = base / name / "ucrt64"
        if (p / "bin" / "libgtk-3-0.dll").is_file():
            return p
    return None


SYSROOT = find_sysroot()
HAVE_OBJDUMP = shutil.which("objdump") is not None


class ClosureTests(unittest.TestCase):
    GRAPH = {
        "app.exe": ["KERNEL32.dll", "libA.dll", "api-ms-win-crt-runtime-l1-1-0.dll", "MSVCRT.dll"],
        "/p/libA.dll": ["LIBB.DLL", "ucrtbase.dll"],
        "/p/libB.dll": ["liba.dll", "libMissing.dll"],  # cycle back to A, plus an unresolved one
    }
    PREFIX = {"liba.dll": "/p/libA.dll", "libb.dll": "/p/libB.dll"}

    def run_closure(self, graph=None):
        graph = graph or self.GRAPH
        return bundle.import_closure(
            ["app.exe"], lambda x: graph.get(x, []), lambda n: self.PREFIX.get(n.lower())
        )

    def test_transitive_cycles_and_case(self):
        self.assertEqual(self.run_closure(), {"liba.dll": "/p/libA.dll", "libb.dll": "/p/libB.dll"})

    def test_system_dlls_never_bundled_even_if_present(self):
        prefix = dict(self.PREFIX, **{"kernel32.dll": "/p/KERNEL32.dll", "api-ms-win-core-x.dll": "/p/x"})
        graph = {"app.exe": ["kernel32.dll", "API-MS-WIN-CORE-X.DLL", "libA.dll"]}
        got = bundle.import_closure(["app.exe"], lambda x: graph.get(x, []), lambda n: prefix.get(n.lower()))
        self.assertEqual(list(got), ["liba.dll"])

    def test_unresolved_is_skipped(self):
        self.assertNotIn("libmissing.dll", self.run_closure())

    def test_is_system_dll(self):
        for n in ("KERNEL32.dll", "api-ms-win-crt-heap-l1-1-0.dll", "Ext-MS-Win-x.dll", "USER32.DLL"):
            self.assertTrue(bundle.is_system_dll(n), n)
        self.assertFalse(bundle.is_system_dll("libglib-2.0-0.dll"))


class ManifestTests(unittest.TestCase):
    def test_manifest_format_and_size_guard(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "sub").mkdir()
            (root / "a.txt").write_bytes(b"abc")
            (root / "sub" / "b.bin").write_bytes(b"")
            (root / "bundle-manifest.txt").write_text("ignored")
            lines = bundle.manifest_lines(root)
            self.assertEqual(
                lines,
                [
                    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  3  a.txt",
                    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  0  sub/b.bin",
                ],
            )
            self.assertEqual(bundle.tree_size(root), 3 + 0 + len("ignored"))
        bundle.check_size(10, 10)
        with self.assertRaises(SystemExit):
            bundle.check_size(11, 10)
        self.assertEqual(bundle.MAX_BUNDLE_BYTES, 400 * 1024 * 1024)


class LoadersCacheTests(unittest.TestCase):
    def test_paths_become_relative(self):
        raw = (
            "# GdkPixbuf Image Loader Modules file\n"
            '"C:\\\\msys64\\\\ucrt64\\\\lib\\\\gdk-pixbuf-2.0\\\\2.10.0\\\\loaders\\\\libpixbufloader-png.dll"\n'
            '"png" 5 "gdk-pixbuf" "PNG" "LGPL"\n'
            '"/tmp/x/lib/gdk-pixbuf-2.0/2.10.0/loaders/pixbufloader_svg.dll"\n'
        )
        out = bundle.rewrite_loaders_cache(raw, Path("/ucrt64"))
        self.assertIn('"lib\\\\gdk-pixbuf-2.0\\\\2.10.0\\\\loaders\\\\libpixbufloader-png.dll"', out)
        self.assertIn('"lib\\\\gdk-pixbuf-2.0\\\\2.10.0\\\\loaders\\\\pixbufloader_svg.dll"', out)
        self.assertIn('"png" 5 "gdk-pixbuf" "PNG" "LGPL"', out)
        self.assertNotIn("msys64", out)
        self.assertNotIn("/tmp/x", out)


@unittest.skipUnless(SYSROOT and HAVE_OBJDUMP, "no full MSYS2 sysroot (scripts/windows/fetch-sysroot.sh --full) or objdump")
class RealSysrootTests(unittest.TestCase):
    def test_gtk_closure(self):
        gtk = SYSROOT / "bin" / "libgtk-3-0.dll"
        imports = bundle.objdump_imports("objdump")
        self.assertIn("libglib-2.0-0.dll", [i.lower() for i in imports(str(gtk))])
        got = bundle.import_closure([str(gtk)], imports, bundle.make_resolver([SYSROOT / "bin"]))
        for name in ("libglib-2.0-0.dll", "libgdk-3-0.dll", "libcairo-2.dll", "libpango-1.0-0.dll"):
            self.assertIn(name, got)
        self.assertFalse([n for n in got if bundle.is_system_dll(n)])

    def test_full_bundle_with_fake_exe(self):
        with tempfile.TemporaryDirectory() as d:
            fake = Path(d) / "impro.exe"
            shutil.copy2(SYSROOT / "bin" / "libgtk-3-0.dll", fake)  # a PE that imports GTK
            out = Path(d) / "out"
            ns = bundle.parse_args(
                ["--prefix", str(SYSROOT), "--exe", str(fake), "--out", str(out), "--no-tools",
                 "--repo-root", str(REPO)]
            )
            bundle.build_bundle(ns)
            for rel in ("impro.exe", "libgdk-3-0.dll", "libglib-2.0-0.dll", "LICENSE", "THIRD_PARTY_NOTICES.md",
                        "share/icons/Adwaita/index.theme", "share/icons/hicolor/index.theme",
                        "lib/gdk-pixbuf-2.0/2.10.0/loaders/pixbufloader_svg.dll",
                        "share/glib-2.0/schemas", "bundle-manifest.txt"):
                self.assertTrue((out / rel).exists(), rel)
            self.assertTrue(any((out / "licenses").iterdir()))
            self.assertIn("libgdk-3-0.dll", (out / "bundle-manifest.txt").read_text())

    def test_size_guard_trips(self):
        with tempfile.TemporaryDirectory() as d:
            fake = Path(d) / "impro.exe"
            shutil.copy2(SYSROOT / "bin" / "libgtk-3-0.dll", fake)
            ns = bundle.parse_args(
                ["--prefix", str(SYSROOT), "--exe", str(fake), "--out", str(Path(d) / "o"), "--no-tools",
                 "--repo-root", str(REPO), "--max-bytes", "1000"]
            )
            with self.assertRaises(SystemExit):
                bundle.build_bundle(ns)


if __name__ == "__main__":
    unittest.main()

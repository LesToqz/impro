"""Static checks of impro.iss against the bundle layout (no Inno Setup needed).

python3 -m unittest discover -s packaging/windows -p 'test_iss.py' -v
"""

import re
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import bundle  # noqa: E402
from test_bundle import REPO, SYSROOT, HAVE_OBJDUMP  # noqa: E402

ISS = (HERE / "impro.iss").read_text(encoding="utf-8")


def section(name):
    m = re.search(rf"^\[{name}\]\n(.*?)(?=^\[|\Z)", ISS, re.M | re.S)
    return m.group(1) if m else ""


def defines():
    return dict(re.findall(r'^\s*#define (\w+) "([^"]*)"', ISS, re.M))


def expand(s):
    d = defines()
    return re.sub(r"\{#(\w+)\}", lambda m: d.get(m.group(1), m.group(0)), s)


class SetupSection(unittest.TestCase):
    def setUp(self):
        self.keys = dict(
            line.split("=", 1) for line in section("Setup").splitlines() if "=" in line and not line.startswith(";")
        )

    def test_required_keys(self):
        for k, v in {
            "PrivilegesRequired": "lowest",
            "PrivilegesRequiredOverridesAllowed": "dialog",
            "ArchitecturesAllowed": "x64compatible",
            "DefaultDirName": r"{autopf}\{#MyAppName}",
            "OutputBaseFilename": "impro-{#MyAppVersion}-x64-setup",
        }.items():
            self.assertEqual(self.keys.get(k), v, k)
        self.assertTrue(self.keys["AppId"].startswith("{{") and self.keys["AppId"].endswith("}"))
        self.assertIn("lzma2", self.keys["Compression"])

    def test_referenced_files_exist(self):
        for k in ("LicenseFile", "SetupIconFile"):
            self.assertTrue((HERE / self.keys[k].replace("\\", "/")).is_file(), k)

    def test_tasks_default_off(self):
        tasks = {re.search(r'Name: "(\w+)"', l).group(1): l for l in section("Tasks").splitlines() if l.startswith("Name:")}
        for t in ("desktopicon", "contextmenu", "addtopath"):
            self.assertIn("unchecked", tasks[t], t)

    def test_context_menu_keys_removed_on_uninstall(self):
        reg = section("Registry")
        for key in (r"Directory\shell\impro", r"Directory\Background\shell\impro"):
            self.assertRegex(reg, re.escape(f'Subkey: "Software\\Classes\\{key}"; ValueType: string; ValueName: ""') + r".*uninsdeletekey")


@unittest.skipUnless(SYSROOT and HAVE_OBJDUMP, "no full sysroot")
class AgainstBundle(unittest.TestCase):
    def test_sources_match_bundle_layout(self):
        with tempfile.TemporaryDirectory() as d:
            fake = Path(d) / "impro.exe"
            shutil.copy2(SYSROOT / "bin" / "libgtk-3-0.dll", fake)
            out = Path(d) / "dist" / "impro"
            bundle.build_bundle(
                bundle.parse_args(["--prefix", str(SYSROOT), "--exe", str(fake), "--out", str(out), "--no-tools",
                                   "--repo-root", str(REPO)])
            )
            n = 0
            for line in section("Files").splitlines():
                m = re.match(r'Source: "([^"]+)"', line)
                if not m:
                    continue
                src = expand(m.group(1))
                prefix = defines()["MyBundleDir"]
                self.assertTrue(src.startswith(prefix + "\\"), src)
                rel = src[len(prefix) + 1 :].replace("\\", "/")
                if rel == "*":
                    self.assertTrue(any(out.iterdir()))
                else:
                    self.assertTrue((out / rel).is_file(), rel)
                n += 1
            self.assertGreaterEqual(n, 2)
            # Everything the [Icons]/[Run] sections launch lives in {app}.
            self.assertTrue((out / "impro.exe").is_file())


if __name__ == "__main__":
    unittest.main()

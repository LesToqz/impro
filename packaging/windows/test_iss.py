"""Static checks of impro.iss (no Inno Setup needed).

python3 -m unittest discover -s packaging/windows -p 'test_iss.py' -v

The compiler only runs in CI (scripts/package/windows/build-installer.ps1), so these tests guard what a typo
would break there: directive, parameter and flag names per section, the {constants} in use, balanced Pascal
blocks, the Check functions, and the contract between the script and the smoke test. They also pin the product
decisions: what each task registers, that nothing becomes a default handler, how PATH is added and removed.
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
SMOKE = (HERE / "smoke.ps1").read_text(encoding="utf-8")


def section(name):
    m = re.search(rf"^\[{name}\]\n(.*?)(?=^\[|\Z)", ISS, re.M | re.S)
    return m.group(1) if m else ""


def defines():
    return dict(re.findall(r'^\s*#define (\w+) "([^"]*)"', ISS, re.M))


def expand(s):
    d = defines()
    return re.sub(r"\{#(\w+)\}", lambda m: d.get(m.group(1), m.group(0)), s)


def split_params(line):
    """'Name: "a;b"; Flags: x y' -> [('Name', 'a;b'), ('Flags', 'x y')]; "" inside quotes is one quote."""
    out = []
    i, n = 0, len(line)
    while i < n:
        while i < n and line[i] in " \t;":
            i += 1
        if i >= n:
            break
        j = line.index(":", i)
        key = line[i:j].strip()
        i = j + 1
        while i < n and line[i] in " \t":
            i += 1
        if i < n and line[i] == '"':
            i += 1
            buf = []
            while True:
                if i >= n:
                    raise ValueError(f"unterminated quote: {line}")
                if line[i] == '"':
                    if i + 1 < n and line[i + 1] == '"':
                        buf.append('"')
                        i += 2
                        continue
                    i += 1
                    break
                buf.append(line[i])
                i += 1
            value = "".join(buf)
            while i < n and line[i] in " \t":
                i += 1
            if i < n and line[i] != ";":
                raise ValueError(f"text after a closing quote: {line}")
        else:
            j = line.find(";", i)
            if j < 0:
                j = n
            value = line[i:j].strip()
            i = j
        out.append((key, value))
    return out


def entries(name):
    """The parameter lines of a section as dicts (expanded preprocessor defines, flags as sets)."""
    result = []
    for line in section(name).splitlines():
        if not line.strip() or line.lstrip().startswith(";"):
            continue
        d = {}
        for k, v in split_params(line):
            assert k not in d, f"{k} twice in: {line}"
            d[k] = expand(v)
        d["_line"] = line
        result.append(d)
    return result


def flags(entry):
    return set(entry.get("Flags", "").split())


def tasks_by_name():
    return {e["Name"]: e for e in entries("Tasks")}


def strip_pascal(code):
    """The [Code] text without strings and comments (strings become '')."""
    out = []
    i, n = 0, len(code)
    while i < n:
        c = code[i]
        if c == "'":
            j = i + 1
            while True:
                if j >= n or code[j] == "\n":
                    raise ValueError("unterminated string literal in [Code]")
                if code[j] == "'":
                    if j + 1 < n and code[j + 1] == "'":
                        j += 2
                        continue
                    break
                j += 1
            out.append("''")
            i = j + 1
        elif c == "{":
            j = code.index("}", i)
            out.append(" ")
            i = j + 1
        elif code.startswith("//", i):
            j = code.find("\n", i)
            i = n if j < 0 else j
        elif code.startswith("(*", i):
            i = code.index("*)", i) + 2
        else:
            out.append(c)
            i += 1
    return "".join(out)


# Parameter and flag names of the Inno Setup 6 sections this script uses (a name outside these is a typo or
# needs a deliberate addition here after checking the Inno Setup documentation).
COMMON = {"Components", "Tasks", "Languages", "Check", "BeforeInstall", "AfterInstall", "MinVersion", "OnlyBelowVersion"}
KNOWN_PARAMS = {
    "Tasks": {"Name", "Description", "GroupDescription", "Flags", "Components", "Check", "MinVersion", "OnlyBelowVersion"},
    "Files": COMMON | {"Source", "DestDir", "DestName", "Excludes", "ExternalSize", "Attribs", "Permissions", "FontInstall",
                       "StrongAssemblyName", "Flags"},
    "Icons": COMMON | {"Name", "Filename", "Parameters", "WorkingDir", "HotKey", "Comment", "IconFilename", "IconIndex",
                       "AppUserModelID", "AppUserModelToastActivatorCLSID", "Flags"},
    "Registry": COMMON | {"Root", "Subkey", "ValueType", "ValueName", "ValueData", "Permissions", "Flags"},
    "Run": COMMON | {"Filename", "Description", "Parameters", "WorkingDir", "StatusMsg", "RunOnceId", "Verb", "Flags"},
    "Languages": {"Name", "MessagesFile", "LicenseFile", "InfoBeforeFile", "InfoAfterFile"},
}
KNOWN_FLAGS = {
    "Tasks": {"checkedonce", "checkablealone", "dontinheritcheck", "exclusive", "restart", "unchecked"},
    "Files": {"allowunsafefiles", "comparetimestamp", "confirmoverwrite", "createallsubdirs", "deleteafterinstall",
              "dontcopy", "dontverifychecksum", "external", "fontisnttruetype", "gacinstall", "ignoreversion",
              "isreadme", "noencryption", "nocompression", "noregerror", "onlyifdestfileexists", "onlyifdoesntexist",
              "overwritereadonly", "promptifolder", "recursesubdirs", "regserver", "regtypelib", "replacesameversion",
              "restartreplace", "setntfscompression", "sharedfile", "skipifsourcedoesntexist", "sortfilesbyextension",
              "sortfilesbyname", "touch", "uninsnosharedfileprompt", "uninsremovereadonly", "uninsrestartdelete",
              "uninsneveruninstall", "unsetntfscompression"},
    "Icons": {"closeonexit", "createonlyiffileexists", "dontcloseonexit", "excludefromshowinnewinstall",
              "foldershortcut", "runmaximized", "runminimized", "uninsneveruninstall", "useapppaths"},
    "Registry": {"createvalueifdoesntexist", "deletekey", "deletevalue", "dontcreatekey", "noerror",
                 "preservestringtype", "uninsclearvalue", "uninsdeletekey", "uninsdeletekeyifempty",
                 "uninsdeletevalue"},
    "Run": {"32bit", "64bit", "hidewizard", "nowait", "postinstall", "runascurrentuser", "runasoriginaluser",
            "runhidden", "runmaximized", "runminimized", "shellexec", "skipifdoesntexist", "skipifnotsilent",
            "skipifsilent", "unchecked", "waituntilidle", "waituntilterminated"},
}
KNOWN_ROOTS = {"HKCR", "HKCU", "HKLM", "HKU", "HKCC", "HKA"}
KNOWN_VALUE_TYPES = {"none", "string", "expandsz", "multisz", "dword", "qword", "binary"}
KNOWN_SETUP_DIRECTIVES = {
    "AllowNoIcons", "AllowUNCPath", "AppComments", "AppContact", "AppCopyright", "AppId", "AppModifyPath", "AppMutex",
    "AppName", "AppPublisher", "AppPublisherURL", "AppReadmeFile", "AppSupportPhone", "AppSupportURL", "AppUpdatesURL",
    "AppVerName", "AppVersion", "ArchitecturesAllowed", "ArchitecturesInstallIn64BitMode", "ChangesAssociations",
    "ChangesEnvironment", "CloseApplications", "CloseApplicationsFilter", "Compression", "CreateAppDir",
    "DefaultDirName", "DefaultGroupName", "DisableDirPage", "DisableFinishedPage", "DisableProgramGroupPage",
    "DisableReadyPage", "DisableWelcomePage", "LicenseFile", "MinVersion", "OutputBaseFilename", "OutputDir",
    "PrivilegesRequired", "PrivilegesRequiredOverridesAllowed", "RestartApplications", "SetupIconFile", "SetupLogging",
    "SetupMutex", "ShowLanguageDialog", "SolidCompression", "Uninstallable", "UninstallDisplayIcon",
    "UninstallDisplayName", "UsePreviousAppDir", "UsePreviousTasks", "VersionInfoCompany", "VersionInfoCopyright",
    "VersionInfoDescription", "VersionInfoProductName", "VersionInfoProductVersion", "VersionInfoVersion",
    "WizardImageFile", "WizardSmallImageFile", "WizardStyle",
}
# Constants available in the sections outside [Code].
KNOWN_CONSTANTS = {"app", "autopf", "autoprograms", "autodesktop", "olddata"}
# Functions of Inno Setup itself that a Check parameter may call.
BUILTIN_CHECKS = {"IsAdminInstallMode", "IsAdmin", "Is64BitInstallMode", "IsWin64"}

APP_USER_MODEL_ID = "dev.impro.impro"
WEBVIEW2_GUID = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"


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

    def test_directive_names_are_known(self):
        unknown = sorted(set(self.keys) - KNOWN_SETUP_DIRECTIVES)
        self.assertEqual(unknown, [], "unknown [Setup] directives (typo?)")

    def test_metadata_and_upgrade_behaviour(self):
        for k, v in {
            "SetupMutex": "impro-setup-mutex",
            "CloseApplications": "yes",
            "CloseApplicationsFilter": "impro.exe",
            "UsePreviousTasks": "yes",
            "ChangesEnvironment": "yes",
            "ChangesAssociations": "yes",
            "UninstallDisplayName": "{#MyAppName}",
            "VersionInfoProductName": "{#MyAppName}",
            "VersionInfoVersion": "{#MyAppVersion}",
            "MinVersion": "10.0.17763",
        }.items():
            self.assertEqual(self.keys.get(k), v, k)
        for k in ("AppPublisher", "AppPublisherURL", "AppSupportURL", "AppUpdatesURL", "VersionInfoCompany",
                  "VersionInfoDescription", "VersionInfoCopyright"):
            self.assertTrue(self.keys.get(k, "").strip(), f"{k} must be set")
        self.assertRegex(expand(self.keys["AppSupportURL"]), r"^https://")
        # Same AppId for every version, so a newer installer upgrades in place instead of installing twice.
        self.assertEqual(self.keys["AppId"], "{{084215CE-9F22-4B49-9F5F-65835332B838}")

    def test_referenced_files_exist(self):
        for k in ("LicenseFile", "SetupIconFile"):
            self.assertTrue((HERE / self.keys[k].replace("\\", "/")).is_file(), k)

    def test_defines_used_are_defined(self):
        used = set(re.findall(r"\{#(\w+)\}", ISS))
        self.assertEqual(sorted(used - set(defines())), [], "preprocessor symbols used but not #define'd")


class Grammar(unittest.TestCase):
    def test_parameter_names_and_flags(self):
        for name, known in KNOWN_PARAMS.items():
            for e in entries(name):
                bad = sorted(k for k in e if not k.startswith("_") and k not in known)
                self.assertEqual(bad, [], f"unknown parameter in [{name}]: {e['_line']}")
                if name in KNOWN_FLAGS:
                    bad = sorted(flags(e) - KNOWN_FLAGS[name])
                    self.assertEqual(bad, [], f"unknown flag in [{name}]: {e['_line']}")

    def test_registry_roots_and_types(self):
        for e in entries("Registry"):
            self.assertIn(e["Root"], KNOWN_ROOTS, e["_line"])
            self.assertIn(e["ValueType"], KNOWN_VALUE_TYPES, e["_line"])
            self.assertIn("ValueName", e, e["_line"])

    def test_constants_are_known(self):
        for name in ("Setup", "Tasks", "Files", "Icons", "Registry", "Run"):
            for line in section(name).splitlines():
                if line.lstrip().startswith(";"):
                    continue
                text = re.sub(r"\{#\w+\}", "", line.replace("{{", ""))
                for const in re.findall(r"\{([^{}]*)\}", text):
                    if const.startswith("%"):
                        self.assertRegex(const, r"^%\w+(\|.*)?$", f"{line}")
                        continue
                    self.assertIn(const.split(":")[0], KNOWN_CONSTANTS, f"unknown constant {{{const}}} in: {line}")

    def test_each_section_is_present_once(self):
        for name in ("Setup", "Languages", "Tasks", "Files", "Icons", "Registry", "Run", "Code"):
            self.assertEqual(len(re.findall(rf"^\[{name}\]$", ISS, re.M)), 1, name)

    def test_task_references_exist(self):
        names = set(tasks_by_name())
        for name in ("Files", "Icons", "Registry", "Run"):
            for e in entries(name):
                for t in e.get("Tasks", "").replace(",", " ").split():
                    self.assertIn(t, names, e["_line"])


class TasksSection(unittest.TestCase):
    def test_defaults(self):
        tasks = tasks_by_name()
        self.assertEqual(set(tasks), {"desktopicon", "contextmenu", "filecontextmenu", "addtopath"})
        self.assertIn("unchecked", flags(tasks["desktopicon"]))
        for t in ("contextmenu", "filecontextmenu", "addtopath"):
            self.assertNotIn("unchecked", flags(tasks[t]), f"{t} is on by default")

    def test_descriptions(self):
        tasks = tasks_by_name()
        self.assertIn("Open folder in impro", tasks["contextmenu"]["Description"])
        self.assertIn("Open with impro", tasks["filecontextmenu"]["Description"])
        for t in tasks.values():
            self.assertTrue(t["GroupDescription"].endswith(":"), t["_line"])


class IconsSection(unittest.TestCase):
    def test_shortcuts_start_in_the_profile_with_the_process_app_id(self):
        icons = entries("Icons")
        names = {e["Name"] for e in icons}
        self.assertEqual(names, {r"{autoprograms}\impro", r"{autodesktop}\impro"})
        for e in icons:
            self.assertEqual(e["Filename"], r"{app}\impro.exe", e["_line"])
            self.assertEqual(e["WorkingDir"], "{%USERPROFILE}", e["_line"])
            self.assertEqual(e["AppUserModelID"], APP_USER_MODEL_ID, e["_line"])
        desktop = next(e for e in icons if e["Name"].startswith("{autodesktop}"))
        self.assertEqual(desktop.get("Tasks"), "desktopicon")
        start = next(e for e in icons if e["Name"].startswith("{autoprograms}"))
        self.assertNotIn("Tasks", start, "the Start menu shortcut is not optional")

    def test_app_id_matches_the_application(self):
        identity = REPO / "crates" / "impro-core" / "src" / "identity.rs"
        if not identity.is_file():
            self.skipTest("identity.rs not present")
        m = re.search(r'APP_ID: &str = "([^"]+)"', identity.read_text(encoding="utf-8"))
        self.assertIsNotNone(m)
        self.assertEqual(m.group(1), APP_USER_MODEL_ID)

    def test_launch_after_install_starts_in_the_profile(self):
        run = entries("Run")
        self.assertEqual(len(run), 1)
        self.assertEqual(run[0]["WorkingDir"], "{%USERPROFILE}")
        self.assertTrue({"postinstall", "skipifsilent", "nowait", "runasoriginaluser"} <= flags(run[0]))


class RegistrySection(unittest.TestCase):
    def setUp(self):
        self.reg = entries("Registry")

    def values(self, subkey):
        """ValueName -> entry for one Subkey."""
        return {e["ValueName"]: e for e in self.reg if e["Subkey"] == subkey}

    def test_context_menu_keys_removed_on_uninstall(self):
        reg = section("Registry")
        for key in (r"Directory\shell\impro", r"Directory\Background\shell\impro", r"*\shell\impro"):
            self.assertRegex(reg, re.escape(f'Subkey: "Software\\Classes\\{key}"; ValueType: string; ValueName: ""') + r".*uninsdeletekey")

    def test_folder_menu(self):
        for key, arg in ((r"Directory\shell\impro", "%1"), (r"Directory\Background\shell\impro", "%V")):
            base = self.values("Software\\Classes\\" + key)
            self.assertEqual(base[""]["ValueData"], "Open folder in impro")
            self.assertEqual(base["Icon"]["ValueData"], '"{app}\\impro.exe"')
            cmd = self.values("Software\\Classes\\" + key + "\\command")[""]
            self.assertEqual(cmd["ValueData"], '"{app}\\impro.exe" "' + arg + '"')
            for e in (base[""], base["Icon"], cmd):
                self.assertEqual(e["Tasks"], "contextmenu")

    def test_file_menu(self):
        base = self.values(r"Software\Classes\*\shell\impro")
        self.assertEqual(base[""]["ValueData"], "Open with impro")
        self.assertEqual(base["Icon"]["ValueData"], '"{app}\\impro.exe"')
        cmd = self.values(r"Software\Classes\*\shell\impro\command")[""]
        self.assertEqual(cmd["ValueData"], '"{app}\\impro.exe" "%1"')
        for e in (base[""], base["Icon"], cmd):
            self.assertEqual(e["Root"], "HKA")
            self.assertEqual(e["Tasks"], "filecontextmenu")

    def test_app_paths(self):
        key = r"Software\Microsoft\Windows\CurrentVersion\App Paths\impro.exe"
        v = self.values(key)
        self.assertEqual(v[""]["ValueData"], r"{app}\impro.exe")
        self.assertEqual(v["Path"]["ValueData"], "{app}")
        for e in v.values():
            self.assertEqual(e["Root"], "HKA")
            self.assertEqual(e["ValueType"], "string")
            self.assertNotIn("Tasks", e, "App Paths is not optional")

    def test_open_with_registration(self):
        base = r"Software\Classes\Applications\impro.exe"
        self.assertEqual(self.values(base)["FriendlyAppName"]["ValueData"], "impro")
        cmd = self.values(base + r"\shell\open\command")[""]
        self.assertEqual(cmd["ValueData"], '"{app}\\impro.exe" "%1"')
        types = self.values(base + r"\SupportedTypes")
        required = (".txt .md .markdown .rs .toml .json .yaml .yml .py .js .ts .tsx .jsx .html .css .c .h .cpp .hpp "
                    ".go .java .sh .ps1 .sql .xml .ini .cfg .log .csv .gitignore").split()
        self.assertEqual(sorted(set(required) - set(types)), [])
        for name, e in types.items():
            self.assertRegex(name, r"^\.[A-Za-z0-9_-]+$")
            self.assertEqual(e["ValueData"], "")
            self.assertEqual(e["ValueType"], "string")
        self.assertEqual(len(types), len([e for e in self.reg if e["Subkey"] == base + r"\SupportedTypes"]), "duplicate extension")

    def test_never_a_default_handler(self):
        for e in self.reg:
            sub = e["Subkey"]
            self.assertNotRegex(sub, r"(?i)^Software\\Classes\\\.", f"extension key = default handler: {e['_line']}")
            self.assertNotRegex(sub, r"(?i)UserChoice|RegisteredApplications|Capabilities|FileExts|OpenWithProgids|OpenWithList|ProgID",
                                f"association key: {e['_line']}")
            self.assertNotEqual(e["Root"], "HKCR", "write through HKA\\Software\\Classes, not HKCR")

    def test_every_key_is_removed_on_uninstall(self):
        deleting = [(e["Root"], e["Subkey"]) for e in self.reg if "uninsdeletekey" in flags(e)]
        for e in self.reg:
            if e["Subkey"].endswith("Environment"):
                continue  # PATH is edited by RemoveFromPath, never deleted as a key
            covered = any(
                e["Root"] == root and (e["Subkey"] == sub or e["Subkey"].startswith(sub + "\\")) for root, sub in deleting
            )
            self.assertTrue(covered, f"not removed on uninstall: {e['_line']}")

    def test_path_entries(self):
        user = [e for e in self.reg if e["Subkey"] == "Environment"]
        system = [e for e in self.reg if e["Subkey"] == r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment"]
        self.assertEqual(len(user), 1)
        self.assertEqual(len(system), 1)
        self.assertEqual(user[0]["Root"], "HKCU")
        self.assertEqual(system[0]["Root"], "HKLM")
        for e, check in ((user[0], "NeedsAddUserPath"), (system[0], "NeedsAddSystemPath")):
            self.assertEqual(e["ValueType"], "expandsz")
            self.assertEqual(e["ValueName"], "Path")
            self.assertEqual(e["ValueData"], "{olddata};{app}")
            self.assertEqual(e["Check"], check)
            self.assertEqual(e["Tasks"], "addtopath")
            self.assertNotIn("uninsdeletekey", flags(e), "never delete the Environment key")
            self.assertNotIn("deletevalue", flags(e))


class Code(unittest.TestCase):
    def setUp(self):
        self.code = section("Code")
        self.bare = strip_pascal(self.code)

    def routines(self):
        return set(re.findall(r"(?im)^\s*(?:function|procedure)\s+(\w+)", self.bare))

    def test_blocks_are_balanced(self):
        depth = 0
        for tok in re.findall(r"(?i)\b(begin|case|try|record|end)\b", self.bare):
            depth += -1 if tok.lower() == "end" else 1
            self.assertGreaterEqual(depth, 0, "end without begin")
        self.assertEqual(depth, 0, "begin/case/try/record without end")

    def test_every_routine_has_a_body(self):
        n = len(re.findall(r"(?im)^\s*(?:function|procedure)\s+\w+", self.bare))
        bodies = len(re.findall(r"(?im)^begin\s*$", self.bare))
        self.assertEqual(n, bodies)

    def test_check_functions_exist(self):
        defined = self.routines()
        for name in ("Registry", "Files", "Icons", "Run", "Tasks"):
            for e in entries(name):
                if "Check" in e:
                    fn = re.match(r"\w+", e["Check"]).group(0)
                    self.assertTrue(fn in defined or fn in BUILTIN_CHECKS, f"Check function {fn} is not defined: {e['_line']}")

    def test_expected_routines(self):
        self.assertTrue({"NeedsAddPath", "NeedsAddUserPath", "NeedsAddSystemPath", "RemoveFromPath",
                         "CurUninstallStepChanged", "CurPageChanged", "GitOnPath", "WebView2RuntimePresent"} <= self.routines())

    def test_path_is_removed_after_uninstall(self):
        m = re.search(r"(?is)procedure CurUninstallStepChanged.*?\bend;", self.bare)
        self.assertIsNotNone(m)
        self.assertIn("usPostUninstall", m.group(0))
        self.assertIn("RemoveFromPath", m.group(0))

    def test_path_helpers_follow_the_install_scope(self):
        self.assertRegex(self.bare, r"(?s)function EnvKey.*?IsAdminInstallMode.*?end;")
        self.assertRegex(self.bare, r"(?s)function EnvRoot.*?IsAdminInstallMode.*?end;")
        self.assertIn("SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment", self.code)
        # Duplicate-free upgrades: the add is skipped when the directory is already an entry.
        self.assertRegex(self.bare, r"(?s)function NeedsAddPath.*?Pos\(.*?\)\s*=\s*0")

    def test_webview2_note(self):
        self.assertEqual(self.code.count(WEBVIEW2_GUID), 2, "machine (WOW6432Node) and per-user key")
        self.assertIn("'SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\" + WEBVIEW2_GUID + "'", self.code)
        self.assertIn("'Software\\Microsoft\\EdgeUpdate\\Clients\\" + WEBVIEW2_GUID + "'", self.code)
        self.assertIn("'pv'", self.code)
        self.assertIn("'0.0.0.0'", self.code)
        self.assertIn("https://developer.microsoft.com/microsoft-edge/webview2/", self.code)
        self.assertIn("https://git-scm.com/download/win", self.code)
        self.assertRegex(self.bare, r"(?s)HKEY_LOCAL_MACHINE.*?HKEY_CURRENT_USER")

    def test_notes_never_block_the_install(self):
        m = re.search(r"(?is)procedure CurPageChanged.*?\bend;\s*\Z", self.bare)
        self.assertIsNotNone(m, "CurPageChanged must be the last routine")
        body = m.group(0)
        self.assertIn("wpFinished", body)
        for forbidden in ("Abort", "MsgBox", "RaiseException", "SuppressibleMsgBox", "NextButton", "BackButton"):
            self.assertNotIn(forbidden, self.bare, f"{forbidden} could block or fail the installation")

    def test_no_stray_non_ascii(self):
        ISS.encode("ascii")  # the compiler reads the script in the ANSI code page of the runner


class SmokeContract(unittest.TestCase):
    """smoke.ps1 and startup-lab.ps1 must agree with the script they exercise."""

    def test_tasks_passed_to_the_installer_exist(self):
        names = set(tasks_by_name())
        found = 0
        for f in ("smoke.ps1", "startup-lab.ps1"):
            text = (HERE / f).read_text(encoding="utf-8")
            for value in re.findall(r"-Tasks '([^']*)'", text):
                found += 1
                for t in filter(None, value.split(",")):
                    self.assertIn(t.lstrip("!"), names, f"{f}: /TASKS names {t}")
        self.assertGreaterEqual(found, 1)

    def test_registry_keys_asserted_by_smoke_are_written_by_the_installer(self):
        subkeys = {e["Subkey"] for e in entries("Registry")}
        asserted = set(re.findall(r"'(?:HKCU:\\)?(Software\\[^']+)'", SMOKE))
        self.assertTrue(asserted, "smoke.ps1 asserts no registry keys")
        for key in asserted:
            if key.startswith("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall"):
                continue  # written by Inno Setup itself
            self.assertTrue(any(key == k or k.startswith(key + "\\") for k in subkeys), f"smoke asserts {key}, the script never writes it")

    def test_smoke_covers_the_integration(self):
        for needle in ("App Paths", "Applications", "WorkingDirectory", "AppUserModelID", "SupportedTypes",
                       "filecontextmenu", "desktop-window.png"):
            self.assertTrue(needle in SMOKE, f"smoke.ps1 does not mention {needle}")


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

; Inno Setup 6 script for impro.
;
;   ISCC.exe /DMyAppVersion=0.1.0 /DMyBundleDir=dist\impro /DMyOutputDir=dist packaging\windows\impro.iss
;
; MyBundleDir is the tree produced by packaging/windows/bundle.py. Relative
; paths are relative to this file.
;
; What the installer registers (see packaging/windows/README.md, "Windows integration"):
;   * Start Menu shortcut (always) and a desktop shortcut (task, off by default). Both start in the user's
;     profile folder: the app reopens the last folder when it is started from there ("ambient launch"),
;     and both carry the AppUserModelID the app sets on its process, so taskbar pinning and grouping match.
;   * Explorer: "Open folder in impro" on folders and folder backgrounds (task contextmenu) and
;     "Open with impro" on every file (task filecontextmenu); both on by default.
;   * "Add to PATH" of the install directory (task addtopath, on by default). The entry is added only when
;     it is missing (upgrades in place never duplicate it) and removed again by the uninstaller.
;   * App Paths, so Win+R "impro" and ShellExecute find impro.exe even without a PATH entry.
;   * Applications\impro.exe with SupportedTypes: impro is offered in the "Open with" dialog for common text
;     and source files. impro NEVER becomes a default handler: no extension key (Software\Classes\.ext),
;     UserChoice, RegisteredApplications or Capabilities entry is written.
;
; Everything under HKA goes to HKCU for the default per-user install and to HKLM for "all users".
;
; Uninstall: the uninstaller removes exactly what the installer wrote (files, shortcuts, the keys above,
; the PATH entry). It deliberately leaves the user's settings in %APPDATA%\impro untouched, so a later
; reinstall finds them again; delete that folder by hand to remove every trace.

#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif
#ifndef MyBundleDir
  #define MyBundleDir "..\..\dist\impro"
#endif
#ifndef MyOutputDir
  #define MyOutputDir "..\..\dist"
#endif

#define MyAppName "impro"
#define MyAppExe "impro.exe"
#define MyAppPublisher "LesToqz"
#define MyAppCopyright "Copyright (c) 2026 LesToqz"
#define MyAppUrl "https://github.com/LesToqz/impro"
; Must equal impro_core::identity::APP_ID, the process-level AppUserModelID the app sets.
#define MyAppUserModelId "dev.impro.impro"

[Setup]
; Same AppId for every version: a newer installer upgrades the existing install in place.
AppId={{084215CE-9F22-4B49-9F5F-65835332B838}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppUrl}
AppSupportURL={#MyAppUrl}/issues
AppUpdatesURL={#MyAppUrl}/releases
AppCopyright={#MyAppCopyright}
VersionInfoVersion={#MyAppVersion}
VersionInfoCompany={#MyAppPublisher}
VersionInfoDescription={#MyAppName} Setup
VersionInfoProductName={#MyAppName}
VersionInfoCopyright={#MyAppCopyright}
; Windows 10 or newer (the documented floor is 1809; only major.minor is enforced, the plain form of the directive).
MinVersion=10.0
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=..\..\LICENSE
SetupIconFile=..\icons\windows\impro.ico
UninstallDisplayIcon={app}\{#MyAppExe}
UninstallDisplayName={#MyAppName}
OutputDir={#MyOutputDir}
OutputBaseFilename=impro-{#MyAppVersion}-x64-setup
Compression=lzma2/ultra
SolidCompression=yes
WizardStyle=modern
; Only one installer or uninstaller at a time.
SetupMutex=impro-setup-mutex
; PATH and the Explorer/Open-with registrations change shell state: tell running programs.
ChangesEnvironment=yes
ChangesAssociations=yes
; A running impro.exe is closed (Restart Manager) before its files are replaced or removed.
CloseApplications=yes
CloseApplicationsFilter=impro.exe
RestartApplications=no
; A silent or interactive upgrade starts from the tasks chosen last time.
UsePreviousTasks=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop icon"; GroupDescription: "Additional icons:"; Flags: unchecked
Name: "contextmenu"; Description: "Add ""Open folder in impro"" to the Explorer context menu of folders"; GroupDescription: "Windows integration:"
Name: "filecontextmenu"; Description: "Add ""Open with impro"" to the Explorer context menu of files"; GroupDescription: "Windows integration:"
Name: "addtopath"; Description: "Add impro to the &PATH of the current user"; GroupDescription: "Windows integration:"

[Files]
Source: "{#MyBundleDir}\{#MyAppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MyBundleDir}\*"; DestDir: "{app}"; Excludes: "{#MyAppExe}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"; WorkingDir: "{%USERPROFILE}"; IconFilename: "{app}\{#MyAppExe}"; AppUserModelID: "{#MyAppUserModelId}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"; WorkingDir: "{%USERPROFILE}"; IconFilename: "{app}\{#MyAppExe}"; AppUserModelID: "{#MyAppUserModelId}"; Tasks: desktopicon

[Registry]
; --- Explorer: folders and folder backgrounds
Root: HKA; Subkey: "Software\Classes\Directory\shell\impro"; ValueType: string; ValueName: ""; ValueData: "Open folder in impro"; Flags: uninsdeletekey; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\Directory\shell\impro"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#MyAppExe}"""; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\Directory\shell\impro\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExe}"" ""%1"""; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\Directory\Background\shell\impro"; ValueType: string; ValueName: ""; ValueData: "Open folder in impro"; Flags: uninsdeletekey; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\Directory\Background\shell\impro"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#MyAppExe}"""; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\Directory\Background\shell\impro\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExe}"" ""%V"""; Tasks: contextmenu
; --- Explorer: all files
Root: HKA; Subkey: "Software\Classes\*\shell\impro"; ValueType: string; ValueName: ""; ValueData: "Open with impro"; Flags: uninsdeletekey; Tasks: filecontextmenu
Root: HKA; Subkey: "Software\Classes\*\shell\impro"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#MyAppExe}"""; Tasks: filecontextmenu
Root: HKA; Subkey: "Software\Classes\*\shell\impro\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExe}"" ""%1"""; Tasks: filecontextmenu
; --- App Paths: Win+R "impro" and ShellExecute work without a PATH entry
Root: HKA; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\{#MyAppExe}"; ValueType: string; ValueName: ""; ValueData: "{app}\{#MyAppExe}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\{#MyAppExe}"; ValueType: string; ValueName: "Path"; ValueData: "{app}"
; --- "Open with" list (registration only, never a default handler)
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#MyAppName}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".txt"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".log"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".md"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".markdown"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".rst"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".rs"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".toml"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".json"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".yaml"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".yml"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".py"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".js"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".mjs"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".cjs"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".ts"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".tsx"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".jsx"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".html"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".htm"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".css"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".scss"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".c"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".h"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".cpp"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".hpp"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".cc"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".go"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".java"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".kt"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".cs"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".rb"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".php"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".lua"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".sh"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".bat"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".cmd"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".ps1"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".sql"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".xml"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".ini"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".cfg"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".conf"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".csv"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".tsv"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".gitignore"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".gitattributes"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\SupportedTypes"; ValueType: string; ValueName: ".editorconfig"; ValueData: ""
; --- PATH (task addtopath). Per-user installs edit HKCU\Environment, all-users installs the system
; environment; NeedsAddPath keeps an upgrade in place from adding the directory a second time.
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; Check: NeedsAddUserPath; Tasks: addtopath
Root: HKLM; Subkey: "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; Check: NeedsAddSystemPath; Tasks: addtopath

[Run]
; Starts in the user's profile folder (ambient launch) and, in an all-users install, without elevation.
Filename: "{app}\{#MyAppExe}"; Description: "Launch impro"; WorkingDir: "{%USERPROFILE}"; Flags: nowait postinstall skipifsilent runasoriginaluser

[Code]
var
  FinishNotesAdded: Boolean;

function EnvKey(): String;
begin
  if IsAdminInstallMode then
    Result := 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment'
  else
    Result := 'Environment';
end;

function EnvRoot(): Integer;
begin
  if IsAdminInstallMode then
    Result := HKEY_LOCAL_MACHINE
  else
    Result := HKEY_CURRENT_USER;
end;

function NeedsAddPath(Dir: String): Boolean;
var
  Paths: String;
begin
  if not RegQueryStringValue(EnvRoot(), EnvKey(), 'Path', Paths) then
    Paths := '';
  Result := Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(Paths) + ';') = 0;
end;

function NeedsAddUserPath(): Boolean;
begin
  Result := (not IsAdminInstallMode) and NeedsAddPath(ExpandConstant('{app}'));
end;

function NeedsAddSystemPath(): Boolean;
begin
  Result := IsAdminInstallMode and NeedsAddPath(ExpandConstant('{app}'));
end;

procedure RemoveFromPath(Dir: String);
var
  Paths, Upper: String;
  P: Integer;
begin
  if not RegQueryStringValue(EnvRoot(), EnvKey(), 'Path', Paths) then
    Exit;
  Upper := Uppercase(Dir);
  P := Pos(';' + Upper + ';', ';' + Uppercase(Paths) + ';');
  if P = 0 then
    Exit;
  { P indexes the padded string, so it is the 1-based start of Dir in Paths. }
  Delete(Paths, P, Length(Dir));
  StringChangeEx(Paths, ';;', ';', True);
  if (Length(Paths) > 0) and (Paths[1] = ';') then
    Delete(Paths, 1, 1);
  if (Length(Paths) > 0) and (Paths[Length(Paths)] = ';') then
    Delete(Paths, Length(Paths), 1);
  RegWriteExpandStringValue(EnvRoot(), EnvKey(), 'Path', Paths);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemoveFromPath(ExpandConstant('{app}'));
end;

function GitOnPath(): Boolean;
begin
  Result := FileSearch('git.exe', GetEnv('PATH')) <> '';
end;

{ The Evergreen runtime registers its version ("pv") under its EdgeUpdate client GUID, machine-wide in the
  WOW6432Node view or per user. A missing value and "0.0.0.0" both mean "not installed". }
function WebView2KeyHasRuntime(RootKey: Integer; SubKey: String): Boolean;
var
  Version: String;
begin
  Result := False;
  if RegQueryStringValue(RootKey, SubKey, 'pv', Version) then
    Result := (Version <> '') and (Version <> '0.0.0.0');
end;

function WebView2RuntimePresent(): Boolean;
begin
  Result :=
    WebView2KeyHasRuntime(HKEY_LOCAL_MACHINE,
      'SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}') or
    WebView2KeyHasRuntime(HKEY_CURRENT_USER,
      'Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}');
end;

{ Notes on the finish page only; nothing here can stop or fail the installation. }
procedure CurPageChanged(CurPageID: Integer);
var
  Notes: String;
begin
  if (CurPageID <> wpFinished) or FinishNotesAdded then
    Exit;
  FinishNotesAdded := True;
  Notes := '';
  if not GitOnPath() then
    Notes := Notes + #13#10#13#10 +
      'Note: impro needs Git for Windows for its git features: https://git-scm.com/download/win';
  if not WebView2RuntimePresent() then
    Notes := Notes + #13#10#13#10 +
      'Note: the Git graph and commit pages need the Microsoft Edge WebView2 Runtime, which was not found: ' +
      'https://developer.microsoft.com/microsoft-edge/webview2/';
  if Notes <> '' then
    WizardForm.FinishedLabel.Caption := WizardForm.FinishedLabel.Caption + Notes;
end;

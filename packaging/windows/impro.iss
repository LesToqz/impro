; Inno Setup 6 script for impro.
;
;   ISCC.exe /DMyAppVersion=0.1.0 /DMyBundleDir=dist\impro /DMyOutputDir=dist packaging\windows\impro.iss
;
; MyBundleDir is the tree produced by packaging/windows/bundle.py. Relative
; paths are relative to this file.

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
#define MyAppUrl "https://github.com/LesToqz/impro"

[Setup]
AppId={{084215CE-9F22-4B49-9F5F-65835332B838}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppName}
AppPublisherURL={#MyAppUrl}
AppSupportURL={#MyAppUrl}
VersionInfoVersion={#MyAppVersion}
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
ChangesEnvironment=yes
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop icon"; GroupDescription: "Additional icons:"; Flags: unchecked
Name: "contextmenu"; Description: "Add ""Open folder in impro"" to the Explorer context menu"; GroupDescription: "Integration:"; Flags: unchecked
Name: "addtopath"; Description: "Add impro to the &PATH of the current user"; GroupDescription: "Integration:"; Flags: unchecked

[Files]
Source: "{#MyBundleDir}\{#MyAppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MyBundleDir}\*"; DestDir: "{app}"; Excludes: "{#MyAppExe}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"; IconFilename: "{app}\{#MyAppExe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"; IconFilename: "{app}\{#MyAppExe}"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\Classes\Directory\shell\impro"; ValueType: string; ValueName: ""; ValueData: "Open folder in impro"; Flags: uninsdeletekey; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\Directory\shell\impro"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#MyAppExe}"""; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\Directory\shell\impro\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExe}"" ""%1"""; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\Directory\Background\shell\impro"; ValueType: string; ValueName: ""; ValueData: "Open folder in impro"; Flags: uninsdeletekey; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\Directory\Background\shell\impro"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#MyAppExe}"""; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\Directory\Background\shell\impro\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExe}"" ""%V"""; Tasks: contextmenu
Root: HKA; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; Check: NeedsAddPath(ExpandConstant('{app}')); Tasks: addtopath

[Run]
Filename: "{app}\{#MyAppExe}"; Description: "Launch impro"; Flags: nowait postinstall skipifsilent

[Code]
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

procedure CurPageChanged(CurPageID: Integer);
begin
  if (CurPageID = wpFinished) and not GitOnPath() then
    WizardForm.FinishedLabel.Caption := WizardForm.FinishedLabel.Caption + #13#10#13#10 +
      'Note: impro needs Git for Windows for its git features: https://git-scm.com/download/win';
end;

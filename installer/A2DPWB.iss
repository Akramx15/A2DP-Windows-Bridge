; A2DPWB installer (Inno Setup 6)
; SPDX-License-Identifier: Apache-2.0 OR MIT
;
; Built by the A2DPWB_dist target (cmake/make_dist.cmake) from the unpacked
; portable package, so both contain the same files:
;   ISCC /DAppVersion=<ver> /DAppVersionNumeric=<x.y.z> /DPkgDir=<dir> /DOutputDir=<dir> A2DPWB.iss

#ifndef AppVersion
  #error AppVersion is not defined
#endif
#ifndef AppVersionNumeric
  #define AppVersionNumeric AppVersion
#endif
#ifndef PkgDir
  #error PkgDir is not defined
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif

#define AppName "A2DP Windows Bridge"
#define AppExe "A2DPWB.exe"
#define RepoUrl "https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge"

[Setup]
; Never change AppId: it ties upgrades and the uninstaller to earlier installs
AppId={{4D8EA889-0FE6-4753-A25F-2C1A3E4BF658}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Seiya Funaoka
AppPublisherURL={#RepoUrl}
AppSupportURL={#RepoUrl}/issues
AppUpdatesURL={#RepoUrl}/releases
VersionInfoVersion={#AppVersionNumeric}
; Per-user by default (no administrator rights); the dialog offers all users
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
DefaultDirName={autopf}\A2DPWB
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
; Same name as the app's single-instance mutex (main.cpp): setup and the
; uninstaller ask to close a running A2DPWB first
AppMutex=A2DPWB_SingleInstance
CloseApplications=yes
SetupIconFile=..\app\resources\app.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
WizardStyle=modern
ShowLanguageDialog=auto
Compression=lzma2/max
SolidCompression=yes
OutputDir={#OutputDir}
OutputBaseFilename=A2DPWB-{#AppVersion}-win64-setup

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "ja"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "de"; MessagesFile: "compiler:Languages\German.isl"
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "ko"; MessagesFile: "compiler:Languages\Korean.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#PkgDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[Code]
const
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';

{ On a first install, start A2DPWB in the language chosen for setup. The
  settings, link keys and firmware live in %APPDATA%\A2DPWB (shared with the
  portable version) and are left alone if they already exist. }
procedure CurStepChanged(CurStep: TSetupStep);
var
  Dir: String;
begin
  if CurStep <> ssPostInstall then Exit;
  Dir := ExpandConstant('{userappdata}\A2DPWB');
  if FileExists(Dir + '\settings.json') then Exit;
  ForceDirectories(Dir);
  SaveStringToFile(Dir + '\settings.json',
    '{"language": "' + ActiveLanguage + '"}' + #13#10, False);
end;

{ "Start with Windows" writes HKCU\...\Run\A2DPWB; remove it when it points
  at this installation. The config folder is kept. }
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Cmd: String;
begin
  if CurUninstallStep <> usPostUninstall then Exit;
  if RegQueryStringValue(HKCU, RunKey, 'A2DPWB', Cmd) and
     (Pos(Lowercase(ExpandConstant('{app}\{#AppExe}')), Lowercase(Cmd)) > 0) then
    RegDeleteValue(HKCU, RunKey, 'A2DPWB');
end;

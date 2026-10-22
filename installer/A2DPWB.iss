; A2DPWB installer (Inno Setup 6.5 or later)
; SPDX-License-Identifier: Apache-2.0 OR MIT
;
; Built by installer/build-release.ps1 from the unpacked portable package, so
; both contain the same files:
;   ISCC /DAppVersion=<ver> /DAppVersionNumeric=<x.y.z> /DArch=<x64|arm64>
;        /DPkgDir=<dir> /DWebView2Setup=<MicrosoftEdgeWebview2Setup.exe>
;        /DOutputDir=<dir> /DOutputBaseFilename=<name> A2DPWB.iss
;
; Install:
;   - a running v2 core is closed cleanly (`a2dpwb-core.exe --quit`: the
;     stream stops and the adapter is released) and started again after;
;   - a running v1 (its mutex A2DPWB_SingleInstance) is asked to exit, and v1
;     (Inno Setup, AppId below) is uninstalled. Its uninstaller removes the
;     program files only: the settings folder %APPDATA%\A2DPWB (profiles,
;     pairings, firmware) stays, and v2 reads it as it is;
;   - 2.0.0 to 2.0.2-beta.5 (Tauri's NSIS installer) is uninstalled in its
;     update mode, which keeps the adapters on WinUSB, the shortcuts and the
;     Run value; this installer takes its folder over;
;   - "start with Windows" (HKCU Run, value A2DPWB, shared with v1) is
;     pointed at `a2dpwb-core.exe --background` when it was on;
;   - the WebView2 Runtime is installed where it is missing.
; Uninstall: the core is closed; the user is asked whether the adapters
; A2DPWB put on WinUSB go back to Windows (one UAC prompt) and whether
; %APPDATA%\A2DPWB (settings, profiles, pairings, firmware) is deleted; the
; Run value is removed when it points at this installation.
;
; Command line, besides Inno Setup's own (/SILENT, /VERYSILENT, /DIR=, ...):
;   /UPDATE  the in-app update (app/src/ui/src-tauri/src/update.rs): starts
;            a2dpwb.exe again when done;
;   /P       NSIS's passive mode, which the updater of 2.0.2-beta.5 and
;            earlier passes: no pages, only the progress (like /SILENT);
;   /KEEPV1  leaves an installed v1, a running v1 and the Run value alone
;            (testing v2 beside an installed v1; both use the same settings
;            folder). The core still replaces v1 when it starts: it asks a
;            running v1 to exit and takes over v1's Run value (resident.rs,
;            refresh_autostart).
;
; Saved as UTF-8 with a BOM (the messages below).

#ifndef AppVersion
  #error AppVersion is not defined
#endif
#ifndef AppVersionNumeric
  #define AppVersionNumeric AppVersion
#endif
#ifndef Arch
  #define Arch "x64"
#endif
#ifndef PkgDir
  #error PkgDir is not defined
#endif
#ifndef WebView2Setup
  #error WebView2Setup is not defined
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif
#ifndef OutputBaseFilename
  #define OutputBaseFilename "A2DPWB-" + AppVersion + "-setup"
#endif

#define AppName "A2DP Windows Bridge"
#define AppExe "a2dpwb.exe"
#define CoreExe "a2dpwb-core.exe"
#define DriverExe "a2dpwb-driver.exe"
; tauri.conf.json's identifier: the windows' notifications and taskbar
; button belong to the shortcut with this AppUserModelID
#define AppUserModelId "io.github.seiyafunaokajp.a2dpwb"
#define RepoUrl "https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge"

[Setup]
; Never change AppId: it ties updates and the uninstaller to earlier installs.
; v1 has its own ({4D8EA889-0FE6-4753-A25F-2C1A3E4BF658}, see V1UninstKey).
AppId={{DE4C0AE2-6745-4C09-9478-7FA6C48527F4}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Seiya Funaoka
AppPublisherURL={#RepoUrl}
AppSupportURL={#RepoUrl}/issues
AppUpdatesURL={#RepoUrl}/releases
AppCopyright=Copyright (c) Seiya Funaoka. Apache-2.0 OR MIT.
VersionInfoVersion={#AppVersionNumeric}
VersionInfoProductTextVersion={#AppVersion}
; For the current user only (no administrator rights): an installation for
; all users would ask for them at every update
PrivilegesRequired=lowest
; Where the NSIS installer of 2.0.x put it, unless it was elsewhere
DefaultDirName={code:DefaultDir}
DisableProgramGroupPage=yes
#if Arch == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif
MinVersion=10.0
; Ends what still runs from the folder after the core's --quit (the
; window, which exits on the core's Bye); the core is started again in [Run]
CloseApplications=yes
RestartApplications=no
LicenseFile={#PkgDir}\LICENSE
SetupIconFile=..\app\src\ui\src-tauri\icons\icon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
WizardStyle=modern
ShowLanguageDialog=auto
Compression=lzma2/max
SolidCompression=yes
; %TEMP%\Setup Log <date> #<n>.txt, for reports of failed installs
SetupLogging=yes
OutputDir={#OutputDir}
OutputBaseFilename={#OutputBaseFilename}

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "ja"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "de"; MessagesFile: "compiler:Languages\German.isl"
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "ko"; MessagesFile: "compiler:Languages\Korean.isl"
; Not shipped with Inno Setup 6 (installer/README.md)
Name: "zhcn"; MessagesFile: "languages\ChineseSimplified.isl"
Name: "zhtw"; MessagesFile: "languages\ChineseTraditional.isl"

[CustomMessages]
en.V1Running=A2DP Windows Bridge v1 is running. Exit it (right-click its tray icon > Exit), then click Retry.%n%nCancel stops the installation.
ja.V1Running=A2DP Windows Bridge v1 が起動しています。タスクトレイのアイコンを右クリックして「終了」を選んでから、「再試行」を押してください。%n%n「キャンセル」でインストールを中止します。
de.V1Running=A2DP Windows Bridge v1 läuft noch. Beenden Sie es (Rechtsklick auf das Taskleistensymbol > Beenden) und klicken Sie dann auf Wiederholen.%n%nAbbrechen beendet die Installation.
fr.V1Running=A2DP Windows Bridge v1 est en cours d'exécution. Quittez-le (clic droit sur son icône dans la zone de notification > Quitter), puis cliquez sur Réessayer.%n%nAnnuler arrête l'installation.
es.V1Running=A2DP Windows Bridge v1 se está ejecutando. Ciérrelo (clic derecho en su icono de la bandeja > Salir) y pulse Reintentar.%n%nCancelar detiene la instalación.
ko.V1Running=A2DP Windows Bridge v1이 실행 중입니다. 트레이 아이콘을 마우스 오른쪽 버튼으로 클릭하고 [종료]를 선택한 다음 [다시 시도]를 누르세요.%n%n[취소]를 누르면 설치를 중단합니다.
zhcn.V1Running=A2DP Windows Bridge v1 正在运行。请右键单击其托盘图标并选择“退出”，然后单击“重试”。%n%n单击“取消”将停止安装。
zhtw.V1Running=A2DP Windows Bridge v1 正在執行。請以滑鼠右鍵按一下其系統匣圖示並選擇「結束」，然後按一下「重試」。%n%n按一下「取消」將停止安裝。

en.V1Left=A2DP Windows Bridge v1 could not be uninstalled. You can remove it later in Settings > Apps; your settings, pairings and firmware stay in either case.
ja.V1Left=A2DP Windows Bridge v1 をアンインストールできませんでした。あとで「設定」>「アプリ」から削除できます。設定、ペアリング、ファームウェアはどちらの場合も残ります。
de.V1Left=A2DP Windows Bridge v1 konnte nicht deinstalliert werden. Sie können es später unter Einstellungen > Apps entfernen; Ihre Einstellungen, Kopplungen und Firmware bleiben in jedem Fall erhalten.
fr.V1Left=A2DP Windows Bridge v1 n'a pas pu être désinstallé. Vous pourrez le supprimer plus tard dans Paramètres > Applications ; vos réglages, appairages et firmware sont conservés dans tous les cas.
es.V1Left=No se pudo desinstalar A2DP Windows Bridge v1. Puede quitarlo más tarde en Configuración > Aplicaciones; su configuración, emparejamientos y firmware se conservan en cualquier caso.
ko.V1Left=A2DP Windows Bridge v1을 제거하지 못했습니다. 나중에 [설정] > [앱]에서 제거할 수 있습니다. 설정, 페어링, 펌웨어는 어느 경우에도 유지됩니다.
zhcn.V1Left=无法卸载 A2DP Windows Bridge v1。您可以稍后在“设置”>“应用”中将其删除；无论如何，您的设置、配对和固件都会保留。
zhtw.V1Left=無法解除安裝 A2DP Windows Bridge v1。您可以稍後在「設定」>「應用程式」中將其移除；無論如何，您的設定、配對和韌體都會保留。

en.RestoreAsk=A2DP Windows Bridge assigned the WinUSB driver to your USB Bluetooth adapter, which hides it from Windows' Bluetooth. Give the adapter back to Windows now? Windows asks for administrator permission.%n%nNo leaves it on WinUSB; you can restore it later in Device Manager.
ja.RestoreAsk=A2DP Windows Bridge が USB Bluetooth アダプターに WinUSB ドライバーを割り当てたため、そのアダプターは Windows の Bluetooth から見えなくなっています。今アダプターを Windows に戻しますか？ Windows が管理者の許可を求めます。%n%n「いいえ」を選ぶと WinUSB のまま残ります。あとでデバイス マネージャーから戻せます。
de.RestoreAsk=A2DP Windows Bridge hat Ihrem USB-Bluetooth-Adapter den WinUSB-Treiber zugewiesen; dadurch sieht das Bluetooth von Windows ihn nicht. Den Adapter jetzt an Windows zurückgeben? Windows fragt nach Administratorrechten.%n%nNein lässt ihn auf WinUSB; Sie können ihn später im Geräte-Manager zurücksetzen.
fr.RestoreAsk=A2DP Windows Bridge a attribué le pilote WinUSB à votre adaptateur Bluetooth USB, ce qui le cache au Bluetooth de Windows. Rendre l'adaptateur à Windows maintenant ? Windows demande les droits d'administrateur.%n%nNon le laisse sur WinUSB ; vous pourrez le rétablir plus tard dans le Gestionnaire de périphériques.
es.RestoreAsk=A2DP Windows Bridge asignó el controlador WinUSB a su adaptador Bluetooth USB, por lo que el Bluetooth de Windows no lo ve. ¿Devolver ahora el adaptador a Windows? Windows pedirá permisos de administrador.%n%nNo lo deja en WinUSB; puede restaurarlo más tarde en el Administrador de dispositivos.
ko.RestoreAsk=A2DP Windows Bridge가 USB 블루투스 어댑터에 WinUSB 드라이버를 할당했기 때문에 Windows 블루투스에서 이 어댑터가 보이지 않습니다. 지금 어댑터를 Windows에 돌려주시겠습니까? Windows가 관리자 권한을 요청합니다.%n%n[아니요]를 누르면 WinUSB로 남습니다. 나중에 장치 관리자에서 되돌릴 수 있습니다.
zhcn.RestoreAsk=A2DP Windows Bridge 已为您的 USB 蓝牙适配器分配 WinUSB 驱动程序，因此 Windows 蓝牙看不到该适配器。现在将适配器还给 Windows 吗？Windows 会请求管理员权限。%n%n选择“否”将保留 WinUSB；之后可以在设备管理器中恢复。
zhtw.RestoreAsk=A2DP Windows Bridge 已為您的 USB 藍牙介面卡指派 WinUSB 驅動程式，因此 Windows 藍牙看不到該介面卡。現在要將介面卡交還給 Windows 嗎？Windows 會要求系統管理員權限。%n%n選擇「否」會保留 WinUSB；之後可以在裝置管理員中還原。

en.RestoreV1Note=%n%nA2DP Windows Bridge v1 is still on this PC and needs WinUSB too: choose No to keep using the adapter with v1.
ja.RestoreV1Note=%n%nこの PC には A2DP Windows Bridge v1 が残っていて、v1 も WinUSB を使います。v1 でアダプターを使い続けるなら「いいえ」を選んでください。
de.RestoreV1Note=%n%nA2DP Windows Bridge v1 ist noch auf diesem PC und braucht WinUSB ebenfalls: Wählen Sie Nein, um den Adapter weiter mit v1 zu verwenden.
fr.RestoreV1Note=%n%nA2DP Windows Bridge v1 est encore sur ce PC et a aussi besoin de WinUSB : choisissez Non pour continuer à utiliser l'adaptateur avec v1.
es.RestoreV1Note=%n%nA2DP Windows Bridge v1 sigue en este equipo y también necesita WinUSB: elija No para seguir usando el adaptador con v1.
ko.RestoreV1Note=%n%n이 PC에는 A2DP Windows Bridge v1이 남아 있으며 v1도 WinUSB를 사용합니다. v1에서 어댑터를 계속 사용하려면 [아니요]를 선택하세요.
zhcn.RestoreV1Note=%n%n此电脑上仍有 A2DP Windows Bridge v1，它也需要 WinUSB：如需继续在 v1 中使用该适配器，请选择“否”。
zhtw.RestoreV1Note=%n%n此電腦上仍有 A2DP Windows Bridge v1，它也需要 WinUSB：若要繼續在 v1 中使用該介面卡，請選擇「否」。

en.RestoreFailed=The adapter could not be given back to Windows completely: it may be unplugged, or Windows chose WinUSB again. Plug it in, right-click it in Device Manager, choose Uninstall device, then unplug and replug it (see "By hand in Device Manager" in the setup guide).
ja.RestoreFailed=アダプターを Windows に戻しきれませんでした。アダプターが抜かれているか、Windows がまた WinUSB を選んだ可能性があります。アダプターを挿した状態でデバイス マネージャーを開き、アダプターを右クリックして「デバイスのアンインストール」を選んでから、抜いて挿し直してください（セットアップガイドの「デバイス マネージャーで手動で行う」を参照）。
de.RestoreFailed=Der Adapter konnte nicht vollständig an Windows zurückgegeben werden: Er ist vielleicht nicht eingesteckt, oder Windows hat wieder WinUSB gewählt. Stecken Sie ihn ein, klicken Sie im Geräte-Manager mit der rechten Maustaste darauf, wählen Sie Gerät deinstallieren, und stecken Sie ihn dann aus und wieder ein (siehe „Von Hand im Geräte-Manager“ in der Einrichtungsanleitung).
fr.RestoreFailed=L'adaptateur n'a pas pu être entièrement rendu à Windows : il est peut-être débranché, ou Windows a de nouveau choisi WinUSB. Branchez-le, faites un clic droit dessus dans le Gestionnaire de périphériques, choisissez Désinstaller l'appareil, puis débranchez-le et rebranchez-le (voir « À la main dans le Gestionnaire de périphériques » dans le guide d'installation).
es.RestoreFailed=No se pudo devolver el adaptador a Windows por completo: puede que esté desconectado, o que Windows haya vuelto a elegir WinUSB. Conéctelo, haga clic derecho en él en el Administrador de dispositivos, elija Desinstalar el dispositivo y después desconéctelo y vuelva a conectarlo (consulte «A mano en el Administrador de dispositivos» en la guía de configuración).
ko.RestoreFailed=어댑터를 Windows에 완전히 돌려주지 못했습니다. 어댑터가 분리되었거나 Windows가 다시 WinUSB를 선택했을 수 있습니다. 어댑터를 꽂은 상태에서 장치 관리자를 열고 어댑터를 마우스 오른쪽 버튼으로 클릭한 다음 [디바이스 제거]를 선택하고, 어댑터를 뽑았다가 다시 꽂으세요(설정 가이드의 '장치 관리자에서 직접 하기' 참조).
zhcn.RestoreFailed=未能将适配器完全还给 Windows：适配器可能已拔出，或 Windows 再次选择了 WinUSB。请插入适配器，在设备管理器中右键单击它，选择“卸载设备”，然后拔出并重新插入（参见设置指南中的“在设备管理器中手动操作”）。
zhtw.RestoreFailed=無法將介面卡完全交還給 Windows：介面卡可能已拔除，或 Windows 再次選擇了 WinUSB。請插入介面卡，在裝置管理員中以滑鼠右鍵按一下它，選擇「解除安裝裝置」，然後拔除並重新插入（請參閱設定指南中的「在裝置管理員中手動操作」）。

en.DeleteDataAsk=Also delete the app data (settings, profiles, pairings and downloaded firmware in %1)?%n%nNo keeps it for a later installation.
ja.DeleteDataAsk=アプリのデータ（%1 にある設定、プロファイル、ペアリング、ダウンロードしたファームウェア）も削除しますか？%n%n「いいえ」を選ぶと残り、次にインストールしたときにそのまま使えます。
de.DeleteDataAsk=Auch die App-Daten löschen (Einstellungen, Profile, Kopplungen und heruntergeladene Firmware in %1)?%n%nNein behält sie für eine spätere Installation.
fr.DeleteDataAsk=Supprimer aussi les données de l'application (réglages, profils, appairages et firmware téléchargé dans %1) ?%n%nNon les conserve pour une installation ultérieure.
es.DeleteDataAsk=¿Eliminar también los datos de la aplicación (configuración, perfiles, emparejamientos y firmware descargado en %1)?%n%nNo los conserva para una instalación posterior.
ko.DeleteDataAsk=앱 데이터(%1에 있는 설정, 프로필, 페어링, 다운로드한 펌웨어)도 삭제하시겠습니까?%n%n[아니요]를 누르면 남겨 두며 나중에 다시 설치할 때 그대로 사용할 수 있습니다.
zhcn.DeleteDataAsk=是否同时删除应用数据（%1 中的设置、配置文件、配对和已下载的固件）？%n%n选择“否”将保留这些数据，供以后安装时使用。
zhtw.DeleteDataAsk=是否同時刪除應用程式資料（%1 中的設定、設定檔、配對和已下載的韌體）？%n%n選擇「否」會保留這些資料，供日後安裝時使用。

en.InstallingWebView2=Installing the Microsoft Edge WebView2 Runtime...
ja.InstallingWebView2=Microsoft Edge WebView2 ランタイムをインストールしています...
de.InstallingWebView2=Die Microsoft Edge WebView2-Runtime wird installiert...
fr.InstallingWebView2=Installation du runtime Microsoft Edge WebView2...
es.InstallingWebView2=Instalando el runtime de Microsoft Edge WebView2...
ko.InstallingWebView2=Microsoft Edge WebView2 런타임을 설치하는 중...
zhcn.InstallingWebView2=正在安装 Microsoft Edge WebView2 运行时...
zhtw.InstallingWebView2=正在安裝 Microsoft Edge WebView2 執行階段...

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#PkgDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
; Run only where WebView2 is missing (InstallWebView2)
Source: "{#WebView2Setup}"; Flags: dontcopy

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; AppUserModelID: "{#AppUserModelId}"
; The NSIS installer of 2.0.x always made one; it is kept when taken over
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; AppUserModelID: "{#AppUserModelId}"; Check: WantDesktopIcon

[Run]
Filename: "{app}\{#CoreExe}"; Parameters: "--background"; Flags: nowait; Check: RestartCore
Filename: "{app}\{#AppExe}"; Flags: nowait; Check: IsUpdate
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent; Check: not IsUpdate

[Code]
const
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';
  RunValue = 'A2DPWB';
  V1UninstKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{4D8EA889-0FE6-4753-A25F-2C1A3E4BF658}_is1';
  V1Mutex = 'A2DPWB_SingleInstance';
  { Tauri's NSIS installer of 2.0.0 to 2.0.2-beta.5 (installer.nsi:
    UNINSTKEY, MANUPRODUCTKEY, whose default value is the folder) }
  NsisUninstKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{#AppName}';
  NsisManuKey = 'Software\Seiya Funaoka';
  NsisProductKey = 'Software\Seiya Funaoka\{#AppName}';
  { The core's (ipc::core_mutex_name) and the window's
    (tauri-plugin-single-instance: <identifier>-sim) }
  AppMutexes = 'Local\A2DPWB_v2_Core,{#AppUserModelId}-sim';
  WebView2Key = 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';

var
  UpdateMode, PassiveMode, KeepV1, CoreWasRunning, RunWasSet, NsisHadDesktopIcon, DeleteData: Boolean;

function HasParam(const Name: String): Boolean;
var
  I: Integer;
begin
  Result := False;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), Name) = 0 then begin
      Result := True;
      Exit;
    end;
end;

function IsUpdate: Boolean;
begin
  Result := UpdateMode;
end;

function RestartCore: Boolean;
begin
  { An update starts the window, which starts the core }
  Result := CoreWasRunning and not UpdateMode;
end;

function WantDesktopIcon: Boolean;
begin
  Result := WizardIsTaskSelected('desktopicon') or NsisHadDesktopIcon;
end;

function DefaultDir(Param: String): String;
begin
  if not RegQueryStringValue(HKCU, NsisProductKey, '', Result) or (Result = '') then
    Result := ExpandConstant('{localappdata}\{#AppName}');
end;

{ The settings folder, as the core finds it }
function ConfigDir: String;
begin
  Result := GetEnv('A2DPWB_CONFIG_DIR');
  if Result = '' then
    Result := ExpandConstant('{userappdata}\A2DPWB');
end;

{ '"C:\...\unins000.exe" /x' -> 'C:\...\unins000.exe' }
function UnquotedPath(S: String): String;
var
  P: Integer;
begin
  S := Trim(S);
  if (Length(S) > 0) and (S[1] = '"') then begin
    Delete(S, 1, 1);
    P := Pos('"', S);
    if P > 0 then
      S := Copy(S, 1, P - 1);
  end;
  Result := S;
end;

{ Waits up to Seconds for the key to go (uninstallers that finish in a copy
  of themselves). True when it went. }
function WaitForKeyGone(const RootKey: Integer; const SubKey: String; const Seconds: Integer): Boolean;
var
  I: Integer;
begin
  for I := 1 to Seconds * 2 do begin
    if not RegKeyExists(RootKey, SubKey) then
      Break;
    Sleep(500);
  end;
  Result := not RegKeyExists(RootKey, SubKey);
end;

{ Asks the running v2 core of this user to exit (the copy in Dir speaks to
  whichever core holds the user's pipe): the stream stops and the adapter is
  released. True when one ran. }
function CloseCore(const Dir: String): Boolean;
var
  Code: Integer;
begin
  Result := False;
  if not FileExists(Dir + '\{#CoreExe}') then
    Exit;
  { Exit code 0: it ran and has exited; 2: none ran; 1: it still runs }
  if Exec(Dir + '\{#CoreExe}', '--quit', '', SW_HIDE, ewWaitUntilTerminated, Code) then begin
    Log(Format('%s --quit: exit code %d', [Dir + '\{#CoreExe}', Code]));
    Result := (Code = 0) or (Code = 1);
  end;
end;

{ The window ends on the core's Bye; 10 s at most }
procedure WaitForAppExit;
var
  I: Integer;
begin
  for I := 1 to 50 do begin
    if not CheckForMutexes(AppMutexes) then
      Exit;
    Sleep(200);
  end;
  Log('A2DPWB still runs');
end;

{ '' when v1 does not run (any more). Otherwise the message the installation
  stops with; in silent mode (nobody to ask) V1Running is set instead. }
function WaitForV1Exit(var V1Running: Boolean): String;
begin
  Result := '';
  V1Running := False;
  while CheckForMutexes(V1Mutex) do begin
    if WizardSilent then begin
      V1Running := True;
      Exit;
    end;
    if MsgBox(CustomMessage('V1Running'), mbError, MB_RETRYCANCEL) = IDCANCEL then begin
      Result := CustomMessage('V1Running');
      Exit;
    end;
  end;
end;

{ Uninstalls v1 registered under RootKey (HKCU: installed for the user, HKLM:
  for all users, whose uninstaller asks for administrator rights) }
procedure UninstallV1(const RootKey: Integer);
var
  Uninstaller: String;
  Code: Integer;
begin
  if not RegQueryStringValue(RootKey, V1UninstKey, 'UninstallString', Uninstaller) then
    Exit;
  Uninstaller := UnquotedPath(Uninstaller);
  Log('Uninstalling A2DP Windows Bridge v1: ' + Uninstaller);
  if FileExists(Uninstaller) then
    ShellExec('', Uninstaller, '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART', '', SW_SHOW, ewWaitUntilTerminated, Code);
  { Inno Setup's uninstaller finishes in a copy of itself }
  if not WaitForKeyGone(RootKey, V1UninstKey, 30) then begin
    Log(CustomMessage('V1Left'));
    SuppressibleMsgBox(CustomMessage('V1Left'), mbInformation, MB_OK, IDOK);
  end;
end;

{ Takes over an installation of Tauri's NSIS installer: its uninstaller in
  update mode (/UPDATE) removes the program files and its registration, but
  keeps the shortcuts, the Run value and the adapters' WinUSB driver
  (hooks.nsh of 2.0.x) }
procedure UninstallNsis;
var
  Uninstaller: String;
  Code: Integer;
begin
  if not RegQueryStringValue(HKCU, NsisUninstKey, 'UninstallString', Uninstaller) then
    Exit;
  Uninstaller := UnquotedPath(Uninstaller);
  Log('Uninstalling the NSIS installation: ' + Uninstaller);
  if FileExists(Uninstaller) then begin
    Exec(Uninstaller, '/S /UPDATE', '', SW_HIDE, ewWaitUntilTerminated, Code);
    { NSIS's uninstaller also finishes in a copy of itself }
    if not WaitForKeyGone(HKCU, NsisUninstKey, 30) then
      Log('The NSIS uninstaller did not finish; its files are overwritten');
  end;
  { Its files are gone (or are overwritten now): nothing left to uninstall }
  RegDeleteKeyIncludingSubkeys(HKCU, NsisUninstKey);
  RegDeleteKeyIncludingSubkeys(HKCU, NsisProductKey);
  RegDeleteKeyIfEmpty(HKCU, NsisManuKey);
end;

function WebView2Present: Boolean;
var
  Version: String;
begin
  Result := (RegQueryStringValue(HKLM32, WebView2Key, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0')) or
            (RegQueryStringValue(HKCU, WebView2Key, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0'));
end;

{ The Evergreen bootstrapper: it downloads the runtime and asks for
  administrator rights itself }
procedure InstallWebView2;
var
  Code: Integer;
begin
  if WebView2Present then
    Exit;
  WizardForm.StatusLabel.Caption := CustomMessage('InstallingWebView2');
  ExtractTemporaryFile('MicrosoftEdgeWebview2Setup.exe');
  if ShellExec('', ExpandConstant('{tmp}\MicrosoftEdgeWebview2Setup.exe'), '/silent /install', '', SW_HIDE, ewWaitUntilTerminated, Code) then
    Log(Format('WebView2 bootstrapper: exit code %d', [Code]))
  else
    Log(Format('WebView2 bootstrapper did not start: %s', [SysErrorMessage(Code)]));
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  Result := PassiveMode;
end;

{ ...except the pages Setup shows anyway: Ready, the list of programs to
  close (closed, the default) and Finished. Next is clicked once the page
  is up (BM_CLICK posted: a click during the page change is lost). }
procedure CurPageChanged(CurPageID: Integer);
begin
  if PassiveMode and ((CurPageID = wpReady) or (CurPageID = wpPreparing) or (CurPageID = wpFinished)) then
    PostMessage(WizardForm.NextButton.Handle, $00F5, 0, 0);
end;

function InitializeSetup: Boolean;
begin
  UpdateMode := HasParam('/UPDATE');
  KeepV1 := HasParam('/KEEPV1');
  Result := True;
  { The updater of 2.0.2-beta.5 and earlier runs the new installer as an
    NSIS one: "/P /UPDATE /R /ARGS". Silent mode cannot be turned on here,
    and Setup's loader keeps its .exe locked, so it cannot start itself
    again with /SILENT either: every page is skipped instead (ShouldSkipPage),
    which leaves the progress, as NSIS's passive mode did. }
  PassiveMode := HasParam('/P') and not WizardSilent;
  if PassiveMode then
    Log('NSIS passive mode (/P): the pages are skipped');
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  NsisDir: String;
  V1Running: Boolean;
begin
  Result := '';
  CoreWasRunning := CloseCore(ExpandConstant('{app}'));
  if RegKeyExists(HKCU, NsisUninstKey) then begin
    NsisHadDesktopIcon := FileExists(ExpandConstant('{autodesktop}\{#AppName}.lnk'));
    if RegQueryStringValue(HKCU, NsisProductKey, '', NsisDir) and (NsisDir <> '') and
       (CompareText(NsisDir, ExpandConstant('{app}')) <> 0) and CloseCore(NsisDir) then
      CoreWasRunning := True;
  end;

  RunWasSet := False;
  if not KeepV1 then begin
    { Read before v1's uninstaller deletes its own Run value }
    RunWasSet := RegValueExists(HKCU, RunKey, RunValue);
    Result := WaitForV1Exit(V1Running);
    if Result <> '' then
      Exit;
    if V1Running then
      Log('A2DP Windows Bridge v1 is running: it stays installed')
    else begin
      UninstallV1(HKCU);
      UninstallV1(HKLM);
    end;
  end;
  UninstallNsis;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep <> ssPostInstall then
    Exit;
  if RunWasSet then
    RegWriteStringValue(HKCU, RunKey, RunValue, '"' + ExpandConstant('{app}\{#CoreExe}') + '" --background');
  InstallWebView2;
end;

{ Offers to give the adapters A2DPWB put on WinUSB back to Windows. The core
  keeps an assignment record per adapter in <settings>\driver\ (the result of
  `a2dpwb-driver assign`, app/src/core/src/driver.rs) and deletes it on
  "Restore driver"; `a2dpwb-driver restore --records` restores every recorded
  adapter with one UAC prompt and deletes the records of those given back.
  Silent mode keeps WinUSB: nobody to ask, and a silent uninstall must not
  raise a UAC prompt. The core must be closed already. }
procedure RestoreDrivers;
var
  Records, Msg: String;
  Find: TFindRec;
  V1Present: Boolean;
  Flags, Code: Integer;
begin
  Records := ConfigDir + '\driver';
  if not FindFirst(Records + '\*.json', Find) then
    Exit;
  FindClose(Find);
  if UninstallSilent or not FileExists(ExpandConstant('{app}\{#DriverExe}')) then begin
    Log('The adapters stay on WinUSB');
    Exit;
  end;
  { v1 (running, or installed) uses the adapter through WinUSB too: then No
    is the default }
  V1Present := CheckForMutexes(V1Mutex) or RegKeyExists(HKCU, V1UninstKey) or RegKeyExists(HKLM, V1UninstKey);
  Msg := CustomMessage('RestoreAsk');
  Flags := MB_YESNO;
  if V1Present then begin
    Msg := Msg + CustomMessage('RestoreV1Note');
    Flags := Flags or MB_DEFBUTTON2;
  end;
  if MsgBox(Msg, mbConfirmation, Flags) <> IDYES then begin
    Log('The adapters stay on WinUSB');
    Exit;
  end;
  Log('Restoring the drivers of the adapters recorded in ' + Records);
  if not ShellExec('runas', ExpandConstant('{app}\{#DriverExe}'), 'restore --records "' + Records + '"', '', SW_HIDE, ewWaitUntilTerminated, Code) then
    { 1223: UAC cancelled }
    Log(Format('%s did not start (%d): the adapters stay on WinUSB', ['{#DriverExe}', Code]))
  { 0: all given back (or nothing to do) }
  else if Code = 0 then
    Log('The adapters are back on their own drivers')
  else begin
    Log(Format('%s restore --records: exit code %d', ['{#DriverExe}', Code]));
    MsgBox(CustomMessage('RestoreFailed'), mbError, MB_OK);
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Cmd: String;
begin
  case CurUninstallStep of
    usUninstall:
      begin
        CloseCore(ExpandConstant('{app}'));
        WaitForAppExit;
        RestoreDrivers;
        { Only the settings folder itself, not one A2DPWB_CONFIG_DIR names }
        DeleteData := not UninstallSilent and
          (MsgBox(FmtMessage(CustomMessage('DeleteDataAsk'), [ExpandConstant('{userappdata}\A2DPWB')]),
                  mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES);
      end;
    usPostUninstall:
      begin
        if DeleteData then
          DelTree(ExpandConstant('{userappdata}\A2DPWB'), True, True, True);
        if RegQueryStringValue(HKCU, RunKey, RunValue, Cmd) and
           (Pos(Lowercase(ExpandConstant('{app}\{#CoreExe}')), Lowercase(Cmd)) > 0) then
          RegDeleteValue(HKCU, RunKey, RunValue);
      end;
  end;
end;

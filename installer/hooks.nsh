; A2DPWB v2 installer hooks for Tauri's NSIS bundler
; (app/src/ui/src-tauri/tauri.conf.json: bundle.windows.nsis.installerHooks).
; SPDX-License-Identifier: Apache-2.0 OR MIT
;
; Install:
;   - a running v1 (its mutex A2DPWB_SingleInstance) is asked to exit;
;   - a running v2 core is closed cleanly (`a2dpwb-core.exe --quit`: the
;     stream stops and the adapter is released) and started again after;
;   - v1 installed by Inno Setup is uninstalled. Its uninstaller removes the
;     program files only: the settings folder %APPDATA%\A2DPWB (profiles,
;     pairings, firmware) stays, and v2 reads it as it is;
;   - "start with Windows" (HKCU Run, value A2DPWB, shared with v1) is
;     pointed at `a2dpwb-core.exe --background` when it was on.
; Uninstall: the core is closed, and the Run value removed when it points at
; this installation.
;
; Command line: /KEEPV1 leaves an installed v1, a running v1 and the Run
; value alone (testing v2 beside v1; both use the same settings folder and
; cannot use the adapter at the same time).
;
; This file is included at the top of Tauri's installer.nsi, after
; LogicLib, FileFunc and StrFunc. Saved as UTF-8 with a BOM.

!define A2DPWB_RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define A2DPWB_RUN_VALUE "A2DPWB"
; v1's AppId (installer/A2DPWB.iss); Inno Setup adds "_is1"
!define A2DPWB_V1_UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\{4D8EA889-0FE6-4753-A25F-2C1A3E4BF658}_is1"
!define A2DPWB_V1_MUTEX "A2DPWB_SingleInstance"
!define A2DPWB_CORE "a2dpwb-core.exe"

Var A2dpwbKeepV1
Var A2dpwbCoreWasRunning
Var A2dpwbRunWasSet

; The installer's declarations of StrFunc are for the installer only
${UnStrLoc}
${UnStrCase}

; $OUT = message NAME in the installer's language (English otherwise)
!macro A2DPWB_Text OUT NAME
  !if "${NAME}" == "v1_running"
    StrCpy ${OUT} "A2DP Windows Bridge v1 is running. Exit it (right-click its tray icon > Exit), then click Retry.$\n$\nCancel stops the installation."
    ${If} $LANGUAGE = 1041
      StrCpy ${OUT} "A2DP Windows Bridge v1 が起動しています。タスクトレイのアイコンを右クリックして「終了」を選んでから、「再試行」を押してください。$\n$\n「キャンセル」でインストールを中止します。"
    ${ElseIf} $LANGUAGE = 1031
      StrCpy ${OUT} "A2DP Windows Bridge v1 läuft noch. Beenden Sie es (Rechtsklick auf das Taskleistensymbol > Beenden) und klicken Sie dann auf Wiederholen.$\n$\nAbbrechen beendet die Installation."
    ${ElseIf} $LANGUAGE = 1036
      StrCpy ${OUT} "A2DP Windows Bridge v1 est en cours d'exécution. Quittez-le (clic droit sur son icône dans la zone de notification > Quitter), puis cliquez sur Réessayer.$\n$\nAnnuler arrête l'installation."
    ${ElseIf} $LANGUAGE = 1034
      StrCpy ${OUT} "A2DP Windows Bridge v1 se está ejecutando. Ciérrelo (clic derecho en su icono de la bandeja > Salir) y pulse Reintentar.$\n$\nCancelar detiene la instalación."
    ${ElseIf} $LANGUAGE = 1042
      StrCpy ${OUT} "A2DP Windows Bridge v1이 실행 중입니다. 트레이 아이콘을 마우스 오른쪽 버튼으로 클릭하고 [종료]를 선택한 다음 [다시 시도]를 누르세요.$\n$\n[취소]를 누르면 설치를 중단합니다."
    ${ElseIf} $LANGUAGE = 2052
      StrCpy ${OUT} "A2DP Windows Bridge v1 正在运行。请右键单击其托盘图标并选择“退出”，然后单击“重试”。$\n$\n单击“取消”将停止安装。"
    ${ElseIf} $LANGUAGE = 1028
      StrCpy ${OUT} "A2DP Windows Bridge v1 正在執行。請以滑鼠右鍵按一下其系統匣圖示並選擇「結束」，然後按一下「重試」。$\n$\n按一下「取消」將停止安裝。"
    ${EndIf}
  !else if "${NAME}" == "v1_left"
    StrCpy ${OUT} "A2DP Windows Bridge v1 could not be uninstalled. You can remove it later in Settings > Apps; your settings, pairings and firmware stay in either case."
    ${If} $LANGUAGE = 1041
      StrCpy ${OUT} "A2DP Windows Bridge v1 をアンインストールできませんでした。あとで「設定」>「アプリ」から削除できます。設定、ペアリング、ファームウェアはどちらの場合も残ります。"
    ${ElseIf} $LANGUAGE = 1031
      StrCpy ${OUT} "A2DP Windows Bridge v1 konnte nicht deinstalliert werden. Sie können es später unter Einstellungen > Apps entfernen; Ihre Einstellungen, Kopplungen und Firmware bleiben in jedem Fall erhalten."
    ${ElseIf} $LANGUAGE = 1036
      StrCpy ${OUT} "A2DP Windows Bridge v1 n'a pas pu être désinstallé. Vous pourrez le supprimer plus tard dans Paramètres > Applications ; vos réglages, appairages et firmware sont conservés dans tous les cas."
    ${ElseIf} $LANGUAGE = 1034
      StrCpy ${OUT} "No se pudo desinstalar A2DP Windows Bridge v1. Puede quitarlo más tarde en Configuración > Aplicaciones; su configuración, emparejamientos y firmware se conservan en cualquier caso."
    ${ElseIf} $LANGUAGE = 1042
      StrCpy ${OUT} "A2DP Windows Bridge v1을 제거하지 못했습니다. 나중에 [설정] > [앱]에서 제거할 수 있습니다. 설정, 페어링, 펌웨어는 어느 경우에도 유지됩니다."
    ${ElseIf} $LANGUAGE = 2052
      StrCpy ${OUT} "无法卸载 A2DP Windows Bridge v1。您可以稍后在“设置”>“应用”中将其删除；无论如何，您的设置、配对和固件都会保留。"
    ${ElseIf} $LANGUAGE = 1028
      StrCpy ${OUT} "無法解除安裝 A2DP Windows Bridge v1。您可以稍後在「設定」>「應用程式」中將其移除；無論如何，您的設定、配對和韌體都會保留。"
    ${EndIf}
  !endif
!macroend

; Closes the running v2 core of this user: asks it to exit (the installed
; copy speaks to whichever core holds the user's pipe), then ends a copy
; that still runs from $INSTDIR. $A2dpwbCoreWasRunning = 1 if one ran.
!macro A2DPWB_CloseCore
  StrCpy $A2dpwbCoreWasRunning 0
  ${If} ${FileExists} "$INSTDIR\${A2DPWB_CORE}"
    ; Exit code 0: it ran and has exited; 2: none ran; 1: it still runs
    nsExec::Exec '"$INSTDIR\${A2DPWB_CORE}" --quit'
    Pop $0
    ${If} $0 = 0
    ${OrIf} $0 = 1
      StrCpy $A2dpwbCoreWasRunning 1
    ${EndIf}
    ; Tauri's check (utils.nsh): the processes using that file, ended
    ; after asking (without asking in passive and silent mode)
    !insertmacro CheckIfAppIsRunning "$INSTDIR\${A2DPWB_CORE}" "${PRODUCTNAME}"
  ${EndIf}
!macroend

; Waits until v1 has exited, asking the user to exit it. Silent mode: nobody
; to ask, $0 = 1 when it still runs.
!macro A2DPWB_WaitForV1Exit
  StrCpy $0 0
  ${Do}
    System::Call 'kernel32::OpenMutexW(i 0x00100000, i 0, w "${A2DPWB_V1_MUTEX}") i .r1'
    ${If} $1 = 0
      ${Break}
    ${EndIf}
    System::Call 'kernel32::CloseHandle(i r1)'
    ${If} ${Silent}
      StrCpy $0 1
      ${Break}
    ${EndIf}
    !insertmacro A2DPWB_Text $2 v1_running
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "$2" IDRETRY +2
    Abort "$2"
  ${Loop}
!macroend

; Uninstalls v1 registered under ROOT (HKCU: installed for the user, HKLM:
; for all users, which asks for administrator rights)
!macro A2DPWB_UninstallV1 ROOT
  ReadRegStr $1 ${ROOT} "${A2DPWB_V1_UNINST_KEY}" "UninstallString"
  ${If} $1 != ""
    ; "C:\...\unins000.exe" -> the path without the quotes
    StrCpy $2 $1 1
    ${If} $2 == '"'
      StrCpy $1 $1 "" 1
      ${StrLoc} $2 $1 '"' ">"
      ${If} $2 != ""
        StrCpy $1 $1 $2
      ${EndIf}
    ${EndIf}
    DetailPrint "Uninstalling A2DP Windows Bridge v1: $1"
    ${If} ${FileExists} "$1"
      ExecShellWait "open" "$1" "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART"
    ${EndIf}
    ; Inno Setup's uninstaller finishes in a copy of itself: wait for its
    ; registration to go (30 s at most)
    StrCpy $3 0
    ${Do}
      ReadRegStr $2 ${ROOT} "${A2DPWB_V1_UNINST_KEY}" "UninstallString"
      ${If} $2 == ""
        ${Break}
      ${EndIf}
      IntOp $3 $3 + 1
      ${If} $3 > 60
        !insertmacro A2DPWB_Text $2 v1_left
        DetailPrint "$2"
        MessageBox MB_OK|MB_ICONINFORMATION "$2" /SD IDOK
        ${Break}
      ${EndIf}
      Sleep 500
    ${Loop}
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  StrCpy $A2dpwbKeepV1 0
  ${GetOptions} $CMDLINE "/KEEPV1" $0
  ${IfNot} ${Errors}
    StrCpy $A2dpwbKeepV1 1
  ${EndIf}

  !insertmacro A2DPWB_CloseCore

  StrCpy $A2dpwbRunWasSet 0
  ${If} $A2dpwbKeepV1 = 0
    ; Read before v1's uninstaller deletes its own Run value
    ReadRegStr $0 HKCU "${A2DPWB_RUN_KEY}" "${A2DPWB_RUN_VALUE}"
    ${If} $0 != ""
      StrCpy $A2dpwbRunWasSet 1
    ${EndIf}
    !insertmacro A2DPWB_WaitForV1Exit
    ${If} $0 = 0
      !insertmacro A2DPWB_UninstallV1 HKCU
      !insertmacro A2DPWB_UninstallV1 HKLM
    ${Else}
      DetailPrint "A2DP Windows Bridge v1 is running: it stays installed"
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $A2dpwbRunWasSet = 1
    WriteRegStr HKCU "${A2DPWB_RUN_KEY}" "${A2DPWB_RUN_VALUE}" '"$INSTDIR\${A2DPWB_CORE}" --background'
  ${EndIf}
  ; As the user (not elevated), like Tauri's start of the app
  ${If} $A2dpwbCoreWasRunning = 1
    nsis_tauri_utils::RunAsUser "$INSTDIR\${A2DPWB_CORE}" "--background"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro A2DPWB_CloseCore
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Kept on updates (the next version's installer runs this uninstaller)
  ${If} $UpdateMode <> 1
    ReadRegStr $0 HKCU "${A2DPWB_RUN_KEY}" "${A2DPWB_RUN_VALUE}"
    ${UnStrCase} $0 $0 "L"
    ${UnStrCase} $1 "$INSTDIR\${A2DPWB_CORE}" "L"
    ${UnStrLoc} $2 $0 $1 ">"
    ${If} $2 != ""
      DeleteRegValue HKCU "${A2DPWB_RUN_KEY}" "${A2DPWB_RUN_VALUE}"
    ${EndIf}
  ${EndIf}
!macroend

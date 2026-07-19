!include LogicLib.nsh

!macro NSIS_HOOK_PREUNINSTALL
  ; Remove only DataNexa's value when it still points to this installation.
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "DataNexa"
  ${If} $0 != ""
    StrCpy $1 '$INSTDIR\datanexa.exe'
    ${If} $0 == '"$1" --autostart'
      DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "DataNexa"
    ${EndIf}
  ${EndIf}
  ; Stop only DataNexa processes whose executable belongs to this installation.
  nsExec::ExecToLog 'powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "& { param([string]$$target) Get-CimInstance Win32_Process | Where-Object { $$_.ExecutablePath -and [IO.Path]::GetFullPath($$_.ExecutablePath) -ieq [IO.Path]::GetFullPath($$target) } | ForEach-Object { Stop-Process -Id $$_.ProcessId -Force } }" "$INSTDIR\datanexa.exe"'
!macroend

; Lamp Bench NSIS installer hooks.
; Documented insertion points provided by Tauri 2's NSIS template.

!macro NSIS_HOOK_POSTINSTALL
  ; Grant the local Users group Full Control on the install dir so the
  ; running app can write runtime state (generated configs, MySQL data,
  ; log files, leaf SSL certs, the SQLite app DB, the htdocs folder)
  ; without admin privileges.
  ;
  ; We use icacls — built into Windows since Vista, no NSIS plugin required.
  ; Earlier attempts used `AccessControl::GrantOnFile` but that plugin isn't
  ; bundled with Tauri 2's NSIS distribution (CI fails at makensis with
  ; "Plugin not found"). The well-known SID `*S-1-5-32-545` is the local
  ; Users group regardless of OS language. (OI)(CI)F = Object Inherit +
  ; Container Inherit + Full control. /T applies recursively.
  nsExec::ExecToLog 'icacls "$INSTDIR" /grant "*S-1-5-32-545:(OI)(CI)F" /T /Q'
  Pop $0
!macroend

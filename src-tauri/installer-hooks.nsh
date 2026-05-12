; Lamp Bench NSIS installer hooks.
; Documented insertion points provided by Tauri 2's NSIS template.

!macro NSIS_HOOK_POSTINSTALL
  ; Lamp Bench is a self-contained install — runtime state (generated
  ; configs, MySQL data, log files, leaf SSL certs, the SQLite app DB and
  ; the user-facing htdocs) lives next to the bundled binaries inside
  ; $INSTDIR. The running app needs write access to all of it but does NOT
  ; run elevated. Grant the local Users group full control on the install
  ; dir so any user account can drive the app after install.
  ;
  ; AccessControl is bundled with Tauri's NSIS distribution.
  AccessControl::GrantOnFile "$INSTDIR" "(BU)" "FullAccess"
  Pop $0
!macroend

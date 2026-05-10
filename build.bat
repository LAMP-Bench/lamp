@echo off
setlocal enableextensions
title Lamp Bench — local build
cd /d "%~dp0"

echo === Checking prerequisites ===
where node >nul 2>&1 || (echo [x] node not found on PATH & exit /b 1)
where pnpm >nul 2>&1 || (echo [x] pnpm not found  -  run: npm i -g pnpm & exit /b 1)
where cargo >nul 2>&1 || (echo [x] cargo not found  -  install Rust via rustup & exit /b 1)
echo   node, pnpm, cargo all present

echo.
echo === Installing JS dependencies ===
call pnpm install --frozen-lockfile || (echo [x] pnpm install failed & exit /b 1)

echo.
echo === Fetching bundled service binaries (~500 MB on first run) ===
call pnpm scripts:fetch-binaries || (echo [x] fetch-binaries failed & exit /b 1)

echo.
echo === Building Tauri app (this can take 10+ minutes on first build) ===
call pnpm tauri build || (echo [x] tauri build failed & exit /b 1)

echo.
echo === Done ===
echo Installers are under: src-tauri\target\release\bundle\
echo   - nsis\          .exe setup
echo   - msi\           .msi installer
echo Opening the bundle folder...
start "" "src-tauri\target\release\bundle"

endlocal

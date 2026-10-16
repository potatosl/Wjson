@echo off
rem Double-click to install missing tools and build/test wjson on Windows.
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0setup-windows.ps1"
echo.
echo Log files: setup-log.txt, run-log.txt
pause

@echo off
rem Lance install.ps1 depuis l'Explorateur ou cmd.exe (memes options : -Deps, -System, -Uninstall...)
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1" %*
if errorlevel 1 pause

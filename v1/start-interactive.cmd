@echo off
setlocal
cd /d "%~dp0"

where pwsh.exe >nul 2>nul
if errorlevel 1 (
  echo PowerShell 7 was not found.
  echo.
  echo This launcher uses pwsh.exe, not Windows PowerShell 5.1.
  echo Install PowerShell 7 or run this command manually after adding pwsh to PATH:
  echo   pwsh -File "%~dp0backup-env.ps1" -Interactive
  echo.
  pause
  exit /b 1
)

pwsh.exe -NoProfile -File "%~dp0backup-env.ps1" -Interactive

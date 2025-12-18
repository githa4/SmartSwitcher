@echo off
chcp 65001 >nul
echo.
echo ========================================
echo   SmartSwitcher
echo ========================================
echo.
echo HOW TO TEST:
echo   1. Open Notepad
echo   2. Switch to EN layout
echo   3. Type: ghbdtn + space
echo   4. Should autocorrect to: privet (in Russian)
echo.
echo   Alt+Shift = manual layout switch
echo.
echo   Ctrl+C to stop
echo ========================================
echo.
cargo run -p smart_switcher

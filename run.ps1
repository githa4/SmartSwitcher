# SmartSwitcher - quick start
# Run: .\run.ps1

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  SmartSwitcher" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "HOW TO TEST:" -ForegroundColor Yellow
Write-Host "  1. Open Notepad"
Write-Host "  2. Switch to EN layout"
Write-Host "  3. Type: ghbdtn + space"
Write-Host "  4. Should autocorrect to Russian"
Write-Host ""
Write-Host "  Alt+Shift = manual layout switch"
Write-Host ""
Write-Host "Ctrl+C to stop" -ForegroundColor Gray
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

Get-Process smart_switcher -ErrorAction SilentlyContinue | Stop-Process -Force

cargo run -p smart_switcher

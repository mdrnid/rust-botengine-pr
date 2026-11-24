# Brave example. Change the path if Brave is installed elsewhere.
$brave = "$env:ProgramFiles\BraveSoftware\Brave-Browser\Application\brave.exe"

if (!(Test-Path $brave)) {
    $brave = "${env:ProgramFiles(x86)}\BraveSoftware\Brave-Browser\Application\brave.exe"
}
if (!(Test-Path $brave)) {
    throw "Brave executable not found. Edit the `$brave path in this script."
}

# Permanent profile directory preserves cookies, login sessions, and history
$profileDir = "$HOME\.browser-automation-profile"

Write-Host "Starting Brave with permanent profile: $profileDir" -ForegroundColor Cyan

& $brave `
  --remote-debugging-port=9222 `
  --remote-debugging-address=127.0.0.1 `
  --user-data-dir="$profileDir" `
  --disable-blink-features=AutomationControlled `
  --no-first-run `
  --no-default-browser-check `
  "https://www.pr.social"

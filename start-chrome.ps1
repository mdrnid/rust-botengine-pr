$chrome = "$env:ProgramFiles\Google\Chrome\Application\chrome.exe"
if (!(Test-Path $chrome)) {
    $chrome = "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe"
}
if (!(Test-Path $chrome)) {
    throw "Chrome executable not found. Edit the `$chrome path in this script."
}

# Permanent profile directory preserves cookies, login sessions, and history
$profileDir = "$HOME\.browser-automation-profile"

Write-Host "Starting Chrome with permanent profile: $profileDir" -ForegroundColor Cyan

& $chrome `
  --remote-debugging-port=9222 `
  --remote-debugging-address=127.0.0.1 `
  --user-data-dir="$profileDir" `
  --disable-blink-features=AutomationControlled `
  --disable-features=IsolateOrigins,site-per-process `
  --disable-infobars `
  --flag-switches-begin --flag-switches-end `
  --disable-background-timer-throttling `
  --disable-backgrounding-occluded-windows `
  --disable-renderer-backgrounding `
  --no-first-run `
  --no-default-browser-check `
  "https://www.pr.social"

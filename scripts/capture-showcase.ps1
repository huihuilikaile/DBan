param(
  [string]$Browser = ""
)

$browserCandidates = @(
  $Browser
  "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
  "C:\Progra~2\Microsoft\Edge\Application\msedge.exe"
  "C:\Program Files\Google\Chrome\Application\chrome.exe"
  "C:\Progra~1\Google\Chrome\Application\chrome.exe"
) | Where-Object { $_ }

$Browser = $browserCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $Browser) {
  throw "Microsoft Edge or Google Chrome was not found. Pass its path with -Browser."
}

$root = Split-Path -Parent $PSScriptRoot
$page = (New-Object System.Uri((Join-Path $root "docs\showcase.html"))).AbsoluteUri
$outputDir = Join-Path $root "docs\images"
$runId = [Guid]::NewGuid().ToString("N")
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null

foreach ($view in @("todo", "apps", "history", "capsule")) {
  $output = Join-Path $outputDir "dban-$view.png"
  $profileDir = Join-Path $env:TEMP "dban-showcase-browser-$runId-$view"
  New-Item -ItemType Directory -Force -Path $profileDir | Out-Null
  Remove-Item -LiteralPath $output -Force -ErrorAction SilentlyContinue
  $browserArgs = @(
    "--headless=new"
    "--disable-gpu"
    "--disable-extensions"
    "--hide-scrollbars"
    "--no-first-run"
    "--force-device-scale-factor=1"
    "--user-data-dir=$profileDir"
    "--window-size=980,640"
    "--screenshot=$output"
    "${page}#$view"
  )
  & $Browser @browserArgs
  for ($attempt = 0; $attempt -lt 100 -and -not (Test-Path -LiteralPath $output); $attempt++) {
    Start-Sleep -Milliseconds 100
  }
  if (-not (Test-Path -LiteralPath $output) -or (Get-Item -LiteralPath $output).Length -eq 0) {
    throw "Screenshot failed for $view"
  }
}

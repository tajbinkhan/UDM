# Builds UDM's Windows installer, dist\UDM-Setup-<version>.exe.
#
#   .\installer\windows\build.ps1              # build the programs, then the installer
#   .\installer\windows\build.ps1 -SkipBuild   # the programs in target\release are current
#
# Needs Inno Setup 6 (winget install JRSoftware.InnoSetup). The version comes
# from Cargo.toml.

param([switch]$SkipBuild)

$ErrorActionPreference = "Stop"
$root = Split-Path (Split-Path $PSScriptRoot)

$match = Select-String -Path (Join-Path $root "Cargo.toml") -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
if (-not $match) { throw "no version in Cargo.toml" }
$version = $match.Matches[0].Groups[1].Value

if (-not $SkipBuild) {
    Push-Location $root
    try {
        # The window embeds the built frontend (AGENTS.md section 13).
        & pnpm --dir frontend install --frozen-lockfile
        if ($LASTEXITCODE -ne 0) { throw "pnpm install failed" }
        & pnpm --dir frontend build
        if ($LASTEXITCODE -ne 0) { throw "the frontend did not build" }
        & cargo build --release --locked
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    } finally {
        Pop-Location
    }
}

# Microsoft's WebView2 bootstrapper, which Setup runs where the runtime is
# missing. It is small and fetches the runtime itself.
$webview = Join-Path $PSScriptRoot "MicrosoftEdgeWebview2Setup.exe"
if (-not (Test-Path $webview)) {
    Invoke-WebRequest -UseBasicParsing -Uri "https://go.microsoft.com/fwlink/p/?LinkId=2124703" -OutFile $webview
}

# Setup installs extension\firefox as it is, so refuse a stale copy.
& (Join-Path $PSScriptRoot "make-firefox.ps1") -Check (Join-Path $root "extension\firefox")
if ($LASTEXITCODE -ne 0) { throw "extension\firefox is out of date: run node installer/make-firefox.js" }

$iscc = Get-Command ISCC.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
if (-not $iscc) {
    $iscc = @(
        (Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 6\ISCC.exe"),
        (Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6\ISCC.exe"),
        (Join-Path $env:ProgramFiles "Inno Setup 6\ISCC.exe")
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $iscc) { throw "Inno Setup 6 is not installed (winget install JRSoftware.InnoSetup)" }

& $iscc /Q "/DAppVersion=$version" (Join-Path $PSScriptRoot "udm.iss")
if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }
Write-Host "Built $(Join-Path $root "dist\UDM-Setup-$version.exe")"

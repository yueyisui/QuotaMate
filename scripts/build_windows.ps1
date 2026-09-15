param(
    [switch]$SkipInstall
)

$ErrorActionPreference = "Stop"

if ($env:OS -ne "Windows_NT") {
    throw "This build script must run on Windows."
}

$rootDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$releaseDir = Join-Path $rootDir "src-tauri\target\release"
$bundleDir = Join-Path $releaseDir "bundle\nsis"
$outputDir = Join-Path $rootDir "artifacts\windows-x64"

function Invoke-Pnpm {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$CommandArgs)

    $pnpmCommand = Get-Command pnpm -ErrorAction SilentlyContinue
    if ($pnpmCommand) {
        & $pnpmCommand.Source @CommandArgs
    } elseif (Get-Command corepack -ErrorAction SilentlyContinue) {
        & corepack pnpm @CommandArgs
    } else {
        throw "pnpm was not found. Install Node.js with Corepack or install pnpm first."
    }

    if ($LASTEXITCODE -ne 0) {
        throw "pnpm $($CommandArgs -join ' ') failed with exit code $LASTEXITCODE."
    }
}

$runningApp = Get-Process -Name "quotamate" -ErrorAction SilentlyContinue
if ($runningApp) {
    throw "QuotaMate is still running. Exit it from the system tray (and stop any dev session with Ctrl+C), then run this script again."
}

Set-Location $rootDir

if (-not $SkipInstall) {
    Write-Host "[1/3] Installing locked dependencies..." -ForegroundColor Cyan
    Invoke-Pnpm install --frozen-lockfile
} else {
    Write-Host "[1/3] Dependency installation skipped." -ForegroundColor DarkGray
}

Write-Host "[2/3] Building the Windows release and NSIS installer..." -ForegroundColor Cyan
Invoke-Pnpm tauri build --bundles nsis

$portableExe = Join-Path $releaseDir "quotamate.exe"
$installer = Get-ChildItem -LiteralPath $bundleDir -Filter "*-setup.exe" -File |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

if (-not (Test-Path -LiteralPath $portableExe)) {
    throw "The portable executable was not generated: $portableExe"
}
if (-not $installer) {
    throw "The NSIS installer was not generated in: $bundleDir"
}

Write-Host "[3/3] Collecting release files..." -ForegroundColor Cyan
New-Item -ItemType Directory -Path $outputDir -Force | Out-Null
Copy-Item -LiteralPath $portableExe -Destination (Join-Path $outputDir "quotamate.exe") -Force
Copy-Item -LiteralPath $installer.FullName -Destination (Join-Path $outputDir $installer.Name) -Force

Write-Host "Windows packages are ready:" -ForegroundColor Green
Get-ChildItem -LiteralPath $outputDir -File | ForEach-Object {
    $sizeMb = [math]::Round($_.Length / 1MB, 2)
    Write-Host "  $($_.FullName) ($sizeMb MB)"
}

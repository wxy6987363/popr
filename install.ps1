# popr installer for Windows
# Usage:
#   irm https://raw.githubusercontent.com/wxy6987363/popr/main/install.ps1 | iex
#   or with mirror:
#   irm https://ghproxy.net/https://raw.githubusercontent.com/wxy6987363/popr/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "wxy6987363/popr"
$Bin  = "popr.exe"

# ---------- 检测架构 ----------
$arch = $env:PROCESSOR_ARCHITECTURE
switch ($arch) {
    "AMD64" { $ArchTag = "x86_64" }
    "ARM64" { Write-Error "Windows ARM64 is not supported yet"; exit 1 }
    default { Write-Error "unsupported arch: $arch"; exit 1 }
}

$File = "popr-windows-$ArchTag.exe"

# ---------- 拿最新版本 ----------
Write-Host "fetching latest version..." -ForegroundColor Cyan

$Latest = $env:VERSION
if (-not $Latest) {
    # 走 github.com 的 releases/latest 重定向
    $resp = Invoke-WebRequest -Uri "https://github.com/$Repo/releases/latest" `
        -MaximumRedirection 0 -ErrorAction SilentlyContinue -SkipHttpErrorCheck

    if ($resp.Headers.Location) {
        $Latest = ($resp.Headers.Location -split '/tag/')[-1]
    }
}

if (-not $Latest) {
    Write-Error "cannot detect latest version. Try: `$env:VERSION='v1.4.2'; irm ... | iex"
    exit 1
}

Write-Host "version: $Latest" -ForegroundColor Green

# ---------- 下载 ----------
$Url = "https://github.com/$Repo/releases/download/$Latest/$File"
$Tmp = Join-Path $env:TEMP "popr-download.exe"

Write-Host "downloading: $Url" -ForegroundColor Cyan

try {
    Invoke-WebRequest -Uri $Url -OutFile $Tmp -UseBasicParsing
} catch {
    Write-Error "download failed: $_"
    exit 1
}

# ---------- 选安装目录 ----------
$Dest = Join-Path $env:USERPROFILE "bin"
if (-not (Test-Path $Dest)) {
    New-Item -ItemType Directory -Path $Dest | Out-Null
}

$Target = Join-Path $Dest $Bin

if (Test-Path $Target) {
    # 被占用时换个名字
    $inUse = $false
    try {
        Remove-Item $Target -Force -ErrorAction Stop
    } catch {
        $inUse = $true
    }
    if ($inUse) {
        $Target = Join-Path $Dest "popr-new.exe"
        Write-Host "existing popr.exe is locked, installing as popr-new.exe" -ForegroundColor Yellow
    }
}

Move-Item -Path $Tmp -Destination $Target -Force
Write-Host "installed: $Target" -ForegroundColor Green

# ---------- 加到 PATH ----------
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$Dest*") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$Dest", "User")
    Write-Host ""
    Write-Host "added to PATH (current session may need restart)" -ForegroundColor Yellow
    Write-Host "  $Dest"
    # 让当前 session 也能用
    $env:Path = "$env:Path;$Dest"
}

# ---------- 验证 ----------
Write-Host ""
Write-Host "verifying..." -ForegroundColor Cyan
try {
    & $Target --version
} catch {
    Write-Host "run failed: $_" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "next:" -ForegroundColor Cyan
Write-Host "  popr config <your-api-key>"

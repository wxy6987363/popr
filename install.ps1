# popr installer for Windows
# Usage:
#   irm https://ghproxy.net/https://raw.githubusercontent.com/wxy6987363/popr/main/install.ps1 | iex
#   irm https://github.com/wxy6987363/popr/releases/latest/download/install.ps1 | iex

$ErrorActionPreference = 'Stop'

[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$Repo = 'wxy6987363/popr'
$Bin  = 'popr.exe'

# ---------- 检查 curl.exe ----------
$curl = Get-Command curl.exe -ErrorAction SilentlyContinue
if (-not $curl) {
    Write-Host "error: curl.exe not found (need Windows 10 1803+)" -ForegroundColor Red
    exit 1
}

# ---------- 平台检测 ----------
$Arch = $env:PROCESSOR_ARCHITECTURE
switch ($Arch) {
    'AMD64' { $ArchTag = 'x86_64' }
    'ARM64' { $ArchTag = 'aarch64' }
    default {
        Write-Host "error: unsupported architecture: $Arch" -ForegroundColor Red
        exit 1
    }
}

$File = "popr-windows-${ArchTag}.exe"

# ---------- 拿最新版本（用 curl.exe 拿 302 Location） ----------
Write-Host "fetching latest version..."

function Get-LatestTag($repo) {
    $url = "https://github.com/$repo/releases/latest"
    # curl.exe -sI 拿头部，-o - 输出到 stdout
    $headers = & curl.exe -sI "$url" 2>$null
    foreach ($line in $headers) {
        if ($line -match '^location:\s*(.+)$') {
            $loc = $matches[1].Trim()
            return ($loc -split '/tag/')[-1]
        }
    }
    return $null
}

if ($env:VERSION) {
    $Latest = $env:VERSION
} else {
    $Latest = Get-LatestTag $Repo
}

if (-not $Latest -or $Latest -eq 'latest') {
    Write-Host "error: cannot detect latest version." -ForegroundColor Red
    Write-Host "       try: `$env:VERSION='v1.4.3'; irm ... | iex"
    exit 1
}

Write-Host "version: $Latest"

# ---------- 下载 ----------
$Url = "https://github.com/$Repo/releases/download/$Latest/$File"
$InstallDir = Join-Path $env:USERPROFILE 'bin'
$Dest = Join-Path $InstallDir $Bin

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

Write-Host "downloading: $Url"
& curl.exe -fL --progress-bar -o "$Dest" "$Url"

if (-not (Test-Path $Dest)) {
    Write-Host "download failed" -ForegroundColor Red
    exit 1
}

$size = (Get-Item $Dest).Length
if ($size -lt 100000) {
    # 二进制应该至少几百 KB
    Write-Host "downloaded file too small ($size bytes), probably an error page" -ForegroundColor Red
    Remove-Item $Dest -Force
    exit 1
}

Write-Host "installed: $Dest ($size bytes)"

# ---------- PATH（用户级，安全追加） ----------
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not $userPath) { $userPath = '' }

if ($userPath -notlike "*$InstallDir*") {
    $newPath = if ($userPath) { "$userPath;$InstallDir" } else { $InstallDir }

    # 安全检查：PATH 总长别超 2000
    if ($newPath.Length -gt 2000) {
        Write-Host ""
        Write-Host "warning: PATH too long, skip auto-add." -ForegroundColor Yellow
        Write-Host "manually add: $InstallDir"
    } else {
        [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
        Write-Host ""
        Write-Host "added to PATH: $InstallDir"
        Write-Host "restart your terminal for it to take effect."
    }
} else {
    Write-Host "PATH already contains: $InstallDir"
}

# ---------- 当前会话也能用 ----------
if ($env:Path -notlike "*$InstallDir*") {
    $env:Path += ";$InstallDir"
}

Write-Host ""
Write-Host "next:"
Write-Host "  popr config <your-api-key>"

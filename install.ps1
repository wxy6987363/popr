# popr installer for Windows
# Usage (PowerShell 5.1+):
#   irm https://ghproxy.net/https://raw.githubusercontent.com/wxy6987363/popr/main/install.ps1 | iex
#   irm https://github.com/wxy6987363/popr/releases/latest/download/install.ps1 | iex

$ErrorActionPreference = 'Stop'

# PS 5.1 默认 TLS 1.0，强制 TLS 1.2
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$Repo = 'wxy6987363/popr'
$Bin  = 'popr.exe'

# ---------- 平台检测 ----------
$Arch = $env:PROCESSOR_ARCHITECTURE
switch ($Arch) {
    'AMD64' { $ArchTag = 'x86_64' }
    'ARM64' { $ArchTag = 'aarch64' }
    default {
        Write-Error "unsupported architecture: $Arch"
        exit 1
    }
}

$File = "popr-windows-${ArchTag}.exe"

# ---------- 拿最新版本（PS 5.1 兼容） ----------
Write-Host "fetching latest version..."

function Get-LatestTag($repo) {
    $url = "https://github.com/$repo/releases/latest"
    $req = [System.Net.HttpWebRequest]::Create($url)
    $req.AllowAutoRedirect = $false
    $req.Method = "HEAD"
    $req.UserAgent = "popr-installer"
    try {
        $resp = $req.GetResponse()
        $loc = $resp.Headers['Location']
        $resp.Close()
        if ($loc) { return ($loc -split '/tag/')[-1] }
    } catch [System.Net.WebException] {
        $resp = $_.Exception.Response
        if ($resp -and $resp.Headers['Location']) {
            return ($resp.Headers['Location'] -split '/tag/')[-1]
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
try {
    Invoke-WebRequest -Uri $Url -OutFile $Dest -UseBasicParsing
} catch {
    Write-Host "download failed: $_" -ForegroundColor Red
    exit 1
}

Write-Host "installed: $Dest"

# ---------- PATH ----------
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath -notlike "*$InstallDir*") {
    $newPath = if ($userPath) { "$userPath;$InstallDir" } else { $InstallDir }
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    Write-Host ""
    Write-Host "added to PATH: $InstallDir"
    Write-Host "restart your terminal for it to take effect."
} else {
    Write-Host "PATH already contains: $InstallDir"
}

Write-Host ""
Write-Host "next:"
Write-Host "  popr config <your-api-key>"

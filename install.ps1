# popr installer for Windows
#
# 用法（PowerShell 5.1+）:
#   irm https://github.com/wxy6987363/popr/releases/latest/download/install.ps1 | iex
#   $s = irm https://github.com/wxy6987363/popr/releases/latest/download/install.ps1
#   & ([scriptblock]::Create($s)) uninstall
#
# 本地用法:
#   powershell -File install.ps1
#   powershell -File install.ps1 uninstall

$ErrorActionPreference = 'Stop'

# PS 5.1 默认 TLS 1.0，强制 TLS 1.2
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$Repo = 'wxy6987363/popr'
$Bin  = 'popr.exe'

# ---------- 版本（发版时由 push.sh 自动改） ----------
$Version = 'v2.4.0'

# ---------- 解析参数 ----------
$Action = 'install'
if ($args.Count -gt 0) {
    switch -Regex ($args[0]) {
        '^(uninstall|--uninstall|-u)$' { $Action = 'uninstall' }
        '^(--version|-v)$'             { $Action = 'version' }
        '^(--help|-h)$'                { $Action = 'help' }
        '^install$'                    { $Action = 'install' }
    }
}

# ---------- 找安装位置 ----------
function Find-InstallPath {
    $candidates = @(
        (Join-Path $env:USERPROFILE 'bin\popr.exe'),
        (Join-Path $env:LOCALAPPDATA 'popr\popr.exe')
    )
    foreach ($p in $candidates) {
        if (Test-Path $p) { return $p }
    }
    $cmd = Get-Command $Bin -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    return $null
}

# ---------- help ----------
function Show-Help {
    @"
popr installer

Usage:
  install                       install popr $Version
  uninstall                     remove popr
  --version                     show installed version
  --help                        this help

Examples:
  irm https://github.com/wxy6987363/popr/releases/latest/download/install.ps1 | iex
  `$s = irm https://github.com/wxy6987363/popr/releases/latest/download/install.ps1
  & ([scriptblock]::Create(`$s)) uninstall
"@ | Write-Host
}

# ---------- version ----------
function Show-Version {
    $p = Find-InstallPath
    if (-not $p) {
        Write-Host "popr is not installed"
        exit 1
    }
    Write-Host "path: $p"
    try { & $p --version } catch { Write-Host "version unknown" }
}

# ---------- uninstall ----------
function Do-Uninstall {
    $p = Find-InstallPath
    if (-not $p) {
        Write-Host "popr is not installed"
        return
    }

    Write-Host "removing: $p"
    try {
        Remove-Item -Force $p
        Write-Host "removed: $p"
    } catch {
        Write-Host "error: cannot remove $p : $_" -ForegroundColor Red
        exit 1
    }

    # 尝试从用户 PATH 里移除目录（可选）
    $binDir = Split-Path $p
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($userPath -and $userPath -like "*$binDir*") {
        Write-Host ""
        Write-Host "note: '$binDir' is still in your user PATH."
        Write-Host "      Remove it manually if you want:"
        Write-Host "      Settings > Environment Variables > User PATH"
    }

    Write-Host ""
    Write-Host "note: config file kept:"
    Write-Host "  $env:USERPROFILE\.pylindrc"
    Write-Host "remove manually if you want."
}

# ---------- install ----------
function Do-Install {
    # 平台检测
    $Arch = $env:PROCESSOR_ARCHITECTURE
    switch ($Arch) {
        'AMD64' { $ArchTag = 'x86_64' }
        'ARM64' { $ArchTag = 'aarch64' }
        default {
            Write-Host "error: unsupported architecture: $Arch" -ForegroundColor Red
            exit 1
        }
    }

    $File = "windows-${ArchTag}.exe"
    $Url = "https://github.com/$Repo/releases/download/$Version/$File"

    $InstallDir = Join-Path $env:USERPROFILE 'bin'
    $Dest = Join-Path $InstallDir $Bin

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

    Write-Host "installing popr $Version for windows-${ArchTag}..."
    Write-Host "downloading: $Url"

    try {
        Invoke-WebRequest -Uri $Url -OutFile $Dest -UseBasicParsing
    } catch {
        Write-Host "error: download failed: $_" -ForegroundColor Red
        exit 1
    }

    # 验证大小（小于 100KB 大概率是错误页）
    $size = (Get-Item $Dest).Length
    if ($size -lt 100000) {
        Write-Host "error: downloaded file too small ($size bytes)" -ForegroundColor Red
        Remove-Item -Force $Dest
        exit 1
    }

    Write-Host "installed: $Dest ($size bytes)"

    # PATH
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not $userPath) { $userPath = '' }

    if ($userPath -notlike "*$InstallDir*") {
        $newPath = if ($userPath) { "$userPath;$InstallDir" } else { $InstallDir }

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

    # 当前会话也能用
    if ($env:Path -notlike "*$InstallDir*") {
        $env:Path += ";$InstallDir"
    }

    Write-Host ""
    Write-Host "next:"
    Write-Host "  popr config <your-api-key>"
}

# ---------- 分发 ----------
switch ($Action) {
    'install'   { Do-Install }
    'uninstall' { Do-Uninstall }
    'version'   { Show-Version }
    'help'      { Show-Help }
    default     { Do-Install }
}

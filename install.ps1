# popr installer for Windows
#
# 用法:
#   irm https://github.com/wxy6987363/popr/releases/latest/download/install.ps1 | iex
#   irm .../install.ps1 | iex  然后脚本会读 $env:ACTION
#
# 或者本地跑:
#   powershell -File install.ps1                 # 安装
#   powershell -File install.ps1 uninstall       # 卸载
#   powershell -File install.ps1 --version       # 看版本
#   powershell -File install.ps1 --help
#
# 环境变量:
#   $env:VERSION  指定版本安装
#   $env:ACTION   动作：install / uninstall / version / help
#                 （远程 irm | iex 方式用这个传参）

$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$Repo = 'wxy6987363/popr'
$Bin  = 'popr.exe'

# ---------- 解析参数 ----------
# 本地 powershell -File 时用 $args；远程 irm|iex 时用 $env:ACTION
$Action = 'install'

if ($args -and $args.Count -gt 0) {
    $a = $args[0].ToLower()
    switch ($a) {
        'uninstall' { $Action = 'uninstall' }
        '-u'        { $Action = 'uninstall' }
        '--version' { $Action = 'version' }
        '-v'        { $Action = 'version' }
        '--help'    { $Action = 'help' }
        '-h'        { $Action = 'help' }
        'install'   { $Action = 'install' }
    }
} elseif ($env:ACTION) {
    $Action = $env:ACTION.ToLower()
}

# ---------- curl.exe ----------
$curl = Get-Command curl.exe -ErrorAction SilentlyContinue

# ---------- 找已安装位置 ----------
function Find-InstallPath {
    # 常见位置
    $candidates = @(
        (Join-Path $env:USERPROFILE 'bin\popr.exe'),
        'C:\Program Files\popr\popr.exe',
        'C:\Program Files (x86)\popr\popr.exe'
    )
    foreach ($p in $candidates) {
        if (Test-Path $p) { return $p }
    }

    # 从 PATH 里找
    $cmd = Get-Command popr -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }

    return $null
}

# ---------- help ----------
function Show-Help {
    Write-Host @"
popr installer (Windows)

Usage:
  powershell -File install.ps1                  install latest
  powershell -File install.ps1 uninstall        remove popr
  powershell -File install.ps1 --version        show installed version
  powershell -File install.ps1 --help           this help

Remote usage:
  irm <url>/install.ps1 | iex                   install
  `$env:ACTION='uninstall'; irm <url>/install.ps1 | iex

Environment:
  `$env:VERSION='v1.4.2'    install specific version
  `$env:ACTION='uninstall'  action when using irm | iex
"@
}

# ---------- version ----------
function Do-Version {
    $p = Find-InstallPath
    if (-not $p) {
        Write-Host "popr is not installed"
        exit 1
    }
    Write-Host "path: $p"
    try {
        & $p --version
    } catch {
        Write-Host "version unknown"
    }
}

# ---------- uninstall ----------
function Do-Uninstall {
    $p = Find-InstallPath
    if (-not $p) {
        Write-Host "popr is not installed"
        exit 0
    }

    Write-Host "removing: $p"
    try {
        Remove-Item -Path $p -Force
        Write-Host "removed: $p"
    } catch {
        Write-Host "error removing: $_" -ForegroundColor Red
        exit 1
    }

    # 清理空目录
    $parent = Split-Path $p
    if ((Test-Path $parent) -and -not (Get-ChildItem $parent -Force)) {
        # 只有当目录是 %USERPROFILE%\bin 且为空才删
        if ($parent -eq (Join-Path $env:USERPROFILE 'bin')) {
            Remove-Item $parent -Force -Recurse
            Write-Host "removed empty dir: $parent"
        }
    }

    # 检查 PATH 里还有没有残留
    $cmd = Get-Command popr -ErrorAction SilentlyContinue
    if ($cmd) {
        Write-Host ""
        Write-Host "note: another 'popr' still in PATH: $($cmd.Source)"
    }

    Write-Host ""
    Write-Host "note: config kept:"
    Write-Host "  $env:USERPROFILE\.pylindrc"
    Write-Host "  $env:ProgramData\pylind\popr.json"
    Write-Host "remove manually if you want."
}

# ---------- install ----------
function Do-Install {
    if (-not $curl) {
        Write-Host "error: curl.exe not found (need Windows 10 1803+)" -ForegroundColor Red
        exit 1
    }

    # 平台
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

    # 拿最新版本
    Write-Host "fetching latest version..."

    function Get-LatestTag($repo) {
        $url = "https://github.com/$repo/releases/latest"
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
        Write-Host "       try: `$env:VERSION='v1.4.2'; irm ... | iex"
        exit 1
    }

    Write-Host "version: $Latest"

    # 下载
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
        Write-Host "downloaded file too small ($size bytes)" -ForegroundColor Red
        Remove-Item $Dest -Force
        exit 1
    }

    Write-Host "installed: $Dest ($size bytes)"

    # PATH（用户级）
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
    'version'   { Do-Version }
    'help'      { Show-Help }
    default     { Show-Help }
}

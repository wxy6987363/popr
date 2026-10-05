#!/bin/sh
# popr installer
#
# 用法:
#   sh install.sh                # 安装最新版
#   sh install.sh uninstall      # 卸载
#   sh install.sh --version      # 显示已装版本
#   sh install.sh --help
#
# 环境变量:
#   VERSION=v1.4.2   指定版本安装
#
# 网络安装:
#   curl -fsSL https://ghproxy.net/https://raw.githubusercontent.com/wxy6987363/popr/main/install.sh | sh
#   curl -fsSL https://github.com/wxy6987363/popr/releases/latest/download/install.sh | sh -s -- uninstall

set -e

REPO="wxy6987363/popr"
BIN="popr"

# ---------- 解析参数 ----------
ACTION="install"

for arg in "$@"; do
    case "$arg" in
        uninstall|--uninstall|-u) ACTION="uninstall" ;;
        --version|-v)             ACTION="version" ;;
        --help|-h)                ACTION="help" ;;
        install)                  ACTION="install" ;;
        *)                        ;; # 忽略未知参数
    esac
done

# ---------- 平台检测 ----------
detect_platform() {
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)

    case "$OS" in
        linux*)  PLATFORM="linux" ;;
        darwin*) PLATFORM="macos" ;;
        *) echo "unsupported OS: $OS" >&2; exit 1 ;;
    esac

    case "$ARCH" in
        x86_64|amd64)  ARCH_TAG="x86_64" ;;
        aarch64|arm64) ARCH_TAG="aarch64" ;;
        *) echo "unsupported arch: $ARCH" >&2; exit 1 ;;
    esac

    FILE="popr-${PLATFORM}-${ARCH_TAG}"
}

# ---------- 找安装位置 ----------
find_install_path() {
    # 优先查 PATH 里已有的 popr
    FOUND="$(command -v "$BIN" 2>/dev/null || true)"
    if [ -n "$FOUND" ]; then
        echo "$FOUND"
        return
    fi

    # 常见位置
    for d in /usr/local/bin /usr/bin /bin "$HOME/.local/bin"; do
        if [ -e "$d/$BIN" ]; then
            echo "$d/$BIN"
            return
        fi
    done

    echo ""
}

# ---------- help ----------
do_help() {
    cat <<EOF
popr installer

Usage:
  sh install.sh                install latest version
  sh install.sh uninstall      remove popr
  sh install.sh --version      show installed version
  sh install.sh --help         this help

Environment:
  VERSION=v1.4.2               install specific version

Install examples:
  curl -fsSL https://github.com/wxy6987363/popr/releases/latest/download/install.sh | sh
  curl -fsSL https://github.com/wxy6987363/popr/releases/latest/download/install.sh | sh -s -- uninstall
EOF
}

# ---------- version ----------
do_version() {
    P=$(find_install_path)
    if [ -z "$P" ]; then
        echo "popr is not installed"
        exit 1
    fi
    echo "path: $P"
    "$P" --version 2>/dev/null || echo "version unknown"
}

# ---------- uninstall ----------
do_uninstall() {
    P=$(find_install_path)
    if [ -z "$P" ]; then
        echo "popr is not installed"
        exit 0
    fi

    echo "removing: $P"

    # 尝试直接删
    if [ -w "$P" ]; then
        rm -f "$P"
        echo "removed: $P"
    elif command -v sudo >/dev/null 2>&1; then
        echo "requires sudo..."
        sudo rm -f "$P"
        echo "removed: $P"
    else
        echo "error: no write permission to $P and sudo not available" >&2
        exit 1
    fi

    # 检查是否还在 PATH（提示用户清理）
    if command -v "$BIN" >/dev/null 2>&1; then
        echo ""
        echo "note: '$BIN' still resolvable in PATH. You may have another copy:"
        command -v "$BIN"
    fi

    echo ""
    echo "note: config files are kept:"
    echo "  ~/.pylindrc"
    echo "  /etc/pylind/popr.json"
    echo "remove manually if you want."
}

# ---------- install ----------
do_install() {
    detect_platform

    # 拿最新版本
    echo "fetching latest version..."

    if [ -n "$VERSION" ]; then
        LATEST="$VERSION"
    else
        LATEST=$(curl -fsSL -o /dev/null -w '%{url_effective}' \
            "https://github.com/${REPO}/releases/latest" \
            | sed -E 's#.*/tag/##')
    fi

    if [ -z "$LATEST" ] || [ "$LATEST" = "latest" ]; then
        echo "error: cannot detect latest version." >&2
        echo "       try: VERSION=v1.4.2 sh install.sh" >&2
        exit 1
    fi

    echo "version: $LATEST"

    # 下载
    URL="https://github.com/${REPO}/releases/download/${LATEST}/${FILE}"
    TMP=$(mktemp)

    echo "downloading: $URL"
    if ! curl -fsSL "$URL" -o "$TMP"; then
        echo "error: download failed" >&2
        rm -f "$TMP"
        exit 1
    fi

    chmod +x "$TMP"

    # 找安装目录
    if [ -w /usr/local/bin ]; then
        DEST_DIR="/usr/local/bin"
    elif [ -w /bin ]; then
        DEST_DIR="/bin"
    else
        DEST_DIR="$HOME/.local/bin"
        mkdir -p "$DEST_DIR"
    fi

    DEST="$DEST_DIR/$BIN"

    # 如果已存在，先删旧的（防止 "text file busy"）
    if [ -e "$DEST" ]; then
        rm -f "$DEST" 2>/dev/null || sudo rm -f "$DEST"
    fi

    mv "$TMP" "$DEST" 2>/dev/null || {
        sudo mv "$TMP" "$DEST"
    }

    echo "installed: $DEST"

    # PATH 提示
    case ":$PATH:" in
        *":$DEST_DIR:"*) ;;
        *)
            echo ""
            echo "add to PATH:"
            echo "  export PATH=\"$DEST_DIR:\$PATH\""
            ;;
    esac

    echo ""
    echo "next:"
    echo "  popr config <your-api-key>"
}

# ---------- 分发 ----------
case "$ACTION" in
    install)   do_install ;;
    uninstall) do_uninstall ;;
    version)   do_version ;;
    help)      do_help ;;
esac

#!/bin/sh
# popr installer
#
# 用法:
#   sh install.sh                # 安装
#   sh install.sh uninstall      # 卸载
#   sh install.sh --version      # 显示已装版本
#   sh install.sh --help
#
# 网络安装:
#   curl -fsSL https://github.com/wxy6987363/popr/releases/latest/download/install.sh | sh
#   curl -fsSL https://github.com/wxy6987363/popr/releases/latest/download/install.sh | sh -s -- uninstall

set -e

REPO="wxy6987363/popr"
BIN="popr"

# ---------- 版本（发版时由 push.sh 自动改） ----------
VERSION="v2.0.0"

# ---------- 解析参数 ----------
ACTION="install"

for arg in "$@"; do
    case "$arg" in
        uninstall|--uninstall|-u) ACTION="uninstall" ;;
        --version|-v)             ACTION="version" ;;
        --help|-h)                ACTION="help" ;;
        install)                  ACTION="install" ;;
        *)                        ;;
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

    FILE="${PLATFORM}-${ARCH_TAG}"
}

# ---------- 找安装位置 ----------
find_install_path() {
    FOUND="$(command -v "$BIN" 2>/dev/null || true)"
    if [ -n "$FOUND" ]; then
        echo "$FOUND"
        return
    fi

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
  sh install.sh                install popr $VERSION
  sh install.sh uninstall      remove popr
  sh install.sh --version      show installed version
  sh install.sh --help         this help

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

    echo "installing popr $VERSION for ${PLATFORM}-${ARCH_TAG}..."

    URL="https://github.com/${REPO}/releases/download/${VERSION}/${FILE}"
    TMP=$(mktemp)

    echo "downloading: $URL"
    if ! curl -fsSL "$URL" -o "$TMP"; then
        echo "error: download failed" >&2
        rm -f "$TMP"
        exit 1
    fi

    chmod +x "$TMP"

    if [ -w /usr/local/bin ]; then
        DEST_DIR="/usr/local/bin"
    elif [ -w /bin ]; then
        DEST_DIR="/bin"
    else
        DEST_DIR="$HOME/.local/bin"
        mkdir -p "$DEST_DIR"
    fi

    DEST="$DEST_DIR/$BIN"

    if [ -e "$DEST" ]; then
        rm -f "$DEST" 2>/dev/null || sudo rm -f "$DEST"
    fi

    mv "$TMP" "$DEST" 2>/dev/null || {
        sudo mv "$TMP" "$DEST"
    }

    echo "installed: $DEST"

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

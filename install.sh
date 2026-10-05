#!/bin/sh
# popr installer
# curl -fsSL https://ghproxy.net/https://raw.githubusercontent.com/wxy6987363/popr/main/install.sh | sh

set -e

REPO="wxy6987363/popr"
BIN="popr"

# ---------- 平台检测 ----------
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
    linux*)  PLATFORM="linux" ;;
    darwin*) PLATFORM="macos" ;;
    *) echo "unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
    x86_64|amd64) ARCH_TAG="x86_64" ;;
    aarch64|arm64) ARCH_TAG="aarch64" ;;
    *) echo "unsupported arch: $ARCH"; exit 1 ;;
esac

FILE="popr-${PLATFORM}-${ARCH_TAG}"

# ---------- 拿最新版本号（走 github.com 重定向，不用 api.github.com） ----------
echo "fetching latest version..."

if [ -n "$VERSION" ]; then
    LATEST="$VERSION"
else
    LATEST=$(curl -fsSL -o /dev/null -w '%{url_effective}' \
        "https://github.com/${REPO}/releases/latest" \
        | sed -E 's#.*/tag/##')
fi

if [ -z "$LATEST" ] || [ "$LATEST" = "latest" ]; then
    echo "error: cannot detect latest version."
    echo "       try: VERSION=v1.4.2 sh install.sh"
    exit 1
fi

echo "version: $LATEST"

# ---------- 下载二进制 ----------
URL="https://github.com/${REPO}/releases/download/${LATEST}/${FILE}"
TMP=$(mktemp)

echo "downloading: $URL"
if ! curl -fsSL "$URL" -o "$TMP"; then
    echo "error: download failed"
    rm -f "$TMP"
    exit 1
fi

chmod +x "$TMP"

# ---------- 安装 ----------
if [ -w /usr/local/bin ]; then
    DEST="/usr/local/bin"
elif [ -w /bin ]; then
    DEST="/bin"
else
    DEST="$HOME/.local/bin"
    mkdir -p "$DEST"
fi

mv "$TMP" "$DEST/$BIN"
echo "installed: $DEST/$BIN"

# ---------- PATH 提示 ----------
case ":$PATH:" in
    *":$DEST:"*) ;;
    *)
        echo ""
        echo "add to PATH:"
        echo "  export PATH=\"$DEST:\$PATH\""
        ;;
esac

echo ""
echo "next:"
echo "  popr config <your-api-key>"

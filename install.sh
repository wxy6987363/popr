#!/bin/sh
# popr installer
# Usage: curl -fsSL https://raw.githubusercontent.com/你的用户名/popr/main/install.sh | sh

set -e

REPO="你的用户名/popr"
BIN="popr"

# 检测平台
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

# 拿最新版本
echo "fetching latest release..."
LATEST=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' | head -1 | cut -d'"' -f4)

if [ -z "$LATEST" ]; then
    echo "failed to fetch latest release"; exit 1
fi

echo "latest: $LATEST"

URL="https://github.com/${REPO}/releases/download/${LATEST}/${FILE}"
TMP=$(mktemp)

echo "downloading ${URL}..."
curl -fsSL "$URL" -o "$TMP"
chmod +x "$TMP"

# 选安装目录
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

# 提示 PATH
case ":$PATH:" in
    *":$DEST:"*) ;;
    *) echo ""
       echo "add to PATH:"
       echo "  export PATH=\"$DEST:\$PATH\"" ;;
esac

echo ""
echo "next:"
echo "  popr config <your-api-key>"

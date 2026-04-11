#!/usr/bin/env bash
# ForgeScript インストールスクリプト
# 使用方法: curl -sSf https://raw.githubusercontent.com/kazuma0606/rvm/master/install.sh | sh

set -euo pipefail

REPO="kazuma0606/rvm"
VERSION="${FORGE_VERSION:-latest}"
INSTALL_DIR="${FORGE_INSTALL_DIR:-/usr/local/bin}"
BINARY_NAME="forge"

# --- OS / アーキテクチャ判定 ---
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  os_name="linux" ;;
    Darwin) os_name="darwin" ;;
    *)
        echo "エラー: 非対応 OS: $OS" >&2
        echo "サポート対象: Linux, macOS" >&2
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64)          arch_name="x86_64" ;;
    aarch64|arm64)   arch_name="aarch64" ;;
    *)
        echo "エラー: 非対応アーキテクチャ: $ARCH" >&2
        echo "サポート対象: x86_64, aarch64/arm64" >&2
        exit 1
        ;;
esac

BINARY_FILE="forge-${os_name}-${arch_name}"

# --- ダウンロード URL 解決 ---
if [ "$VERSION" = "latest" ]; then
    BASE_URL="https://github.com/${REPO}/releases/latest/download"
else
    BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"
fi

DOWNLOAD_URL="${BASE_URL}/${BINARY_FILE}"

# --- インストール ---
echo "ForgeScript をインストールしています..."
echo "  対象: ${BINARY_FILE}"
echo "  URL:  ${DOWNLOAD_URL}"
echo "  先:   ${INSTALL_DIR}/${BINARY_NAME}"
echo ""

TMP_FILE="$(mktemp)"
trap 'rm -f "$TMP_FILE"' EXIT

if ! curl -sSfL "$DOWNLOAD_URL" -o "$TMP_FILE"; then
    echo "エラー: ダウンロードに失敗しました" >&2
    echo "  URL: $DOWNLOAD_URL" >&2
    echo "  リリースページを確認してください: https://github.com/${REPO}/releases" >&2
    exit 1
fi

chmod +x "$TMP_FILE"

# インストール先への書き込み権限確認
if [ -w "$INSTALL_DIR" ]; then
    mv "$TMP_FILE" "${INSTALL_DIR}/${BINARY_NAME}"
else
    echo "  (sudo が必要です)"
    sudo mv "$TMP_FILE" "${INSTALL_DIR}/${BINARY_NAME}"
fi

# --- 確認 ---
if command -v forge > /dev/null 2>&1; then
    echo "インストール完了: $(forge --version)"
else
    echo "インストール完了: ${INSTALL_DIR}/${BINARY_NAME}"
    echo ""
    echo "PATH に ${INSTALL_DIR} が含まれていない場合は追加してください:"
    echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
fi

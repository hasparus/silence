#!/bin/sh
# silence installer — https://github.com/hasparus/silence
#
# Usage:
#   curl -sSf https://raw.githubusercontent.com/hasparus/silence/main/install.sh | sh
#
# Honors:
#   BIN_DIR   install location (default: $HOME/.local/bin)
#   VERSION   pin to a specific tag instead of latest (e.g. VERSION=v0.2.2)

set -eu

REPO="hasparus/silence"
BIN_NAME="silence"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"

err() { echo "silence installer: $*" >&2; exit 1; }

# --- detect OS / arch ---
OS=""
case "$(uname -s)" in
  Linux)  OS="linux" ;;
  Darwin) OS="darwin" ;;
  *)      err "unsupported OS: $(uname -s)" ;;
esac

ARCH=""
case "$(uname -m)" in
  x86_64|amd64)   ARCH="x86_64" ;;
  aarch64|arm64)  ARCH="aarch64" ;;
  *)              err "unsupported architecture: $(uname -m)" ;;
esac

# --- pick version ---
if [ -z "${VERSION:-}" ]; then
  echo "fetching latest release..."
  VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep -m1 '"tag_name":' \
    | cut -d'"' -f4)
  [ -n "$VERSION" ] || err "could not determine the latest version"
fi
echo "version: $VERSION"

# --- download ---
ASSET="${BIN_NAME}-${OS}-${ARCH}"
URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET}"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "downloading $URL"
curl -fsSL "$URL" -o "$TMP/$BIN_NAME" \
  || err "download failed (asset $ASSET may not exist for this release)"
chmod +x "$TMP/$BIN_NAME"

# --- install ---
mkdir -p "$BIN_DIR"
mv "$TMP/$BIN_NAME" "$BIN_DIR/$BIN_NAME"

echo ""
echo "installed $BIN_NAME $VERSION to $BIN_DIR"

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) echo "note: $BIN_DIR is not in your PATH; add it to your shell rc." ;;
esac

echo ""
"$BIN_DIR/$BIN_NAME" hooks install

echo ""
echo "✔︎ hook configured, run \`$BIN_NAME --help\` to learn more"

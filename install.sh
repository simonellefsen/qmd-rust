#!/bin/sh
#
# install.sh - secure one-line installer for qmd (Rust port)
#
# Usage (as documented):
#   curl -fsSL https://raw.githubusercontent.com/simonellefsen/qmd-rust/main/install.sh | sh
#
# Advanced:
#   ./install.sh --version v0.2.0          # specific release tag
#   ./install.sh --version 0.2.0-test      # prerelease
#   ./install.sh --to ~/.local/bin         # custom install dir
#   ./install.sh --help
#
# It downloads the prebuilt tarball + sha256 from GitHub Releases (produced by cargo-dist),
# verifies the checksum, extracts, and installs the `qmd` binary.
#
# Supports: macOS (arm64/x86_64), Linux x86_64 (musl build for portability)
# Requires: curl, tar, shasum or sha256sum (for verify)
#
# This script is intentionally small, POSIX-ish, and auditable.
# Review it before piping from the internet.

set -e

REPO="simonellefsen/qmd-rust"
BINARY="qmd"

VERSION=""
INSTALL_DIR=""

# Detect lingering Node.js / npm version of qmd (published as @tobilu/qmd).
# This is the #1 cause of "wrong qmd in PATH" and uninstall headaches when
# switching to the Rust binary. We warn early so users can clean it up before
# the new binary is installed (data in ~/.cache/qmd and ~/.config/qmd is shared
# and safe).
if command -v npm >/dev/null 2>&1; then
  if npm list -g --depth=0 2>/dev/null | grep -q '@tobilu/qmd'; then
    echo "⚠️  Old Node.js version (@tobilu/qmd) detected in global npm packages." >&2
    echo "   This frequently leaves a stale 'qmd' launcher in PATH and makes uninstall tricky." >&2
    echo "   Recommended fix (run this, then re-run the installer if desired):" >&2
    echo "     npm uninstall -g @tobilu/qmd" >&2
    echo "   Your collections, index (~/.cache/qmd/index.sqlite), and config are preserved." >&2
    echo "" >&2
  fi
fi

usage() {
  echo "qmd installer"
  echo "Usage: $0 [--version <tag>] [--to <dir>] [--help]"
  echo "  --version, -v   Release tag (e.g. v0.2.0 or 0.2.0-test). Defaults to latest."
  echo "  --to            Destination directory for the binary (default: /usr/local/bin or ~/.local/bin)"
  echo "  --help, -h      Show this help"
  exit 0
}

# Very simple arg parser (no getopt for portability)
while [ $# -gt 0 ]; do
  case "$1" in
    --version|-v)
      VERSION="$2"
      shift 2
      ;;
    --to)
      INSTALL_DIR="$2"
      shift 2
      ;;
    --help|-h)
      usage
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      ;;
  esac
done

# Detect OS / ARCH
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin|darwin) OS="macos" ;;
  Linux|linux)   OS="linux" ;;
  *)
    echo "Unsupported operating system: $OS" >&2
    echo "Supported: macOS, Linux" >&2
    exit 1
    ;;
esac

case "$ARCH" in
  x86_64|amd64)  ARCH="x86_64" ;;
  arm64|aarch64) ARCH="aarch64" ;;
  *)
    echo "Unsupported architecture: $ARCH" >&2
    exit 1
    ;;
esac

if [ "$OS" = "macos" ]; then
  TARGET="${ARCH}-apple-darwin"
elif [ "$OS" = "linux" ]; then
  if [ "$ARCH" != "x86_64" ]; then
    echo "qmd-rust currently only provides x86_64 Linux builds (gnu + musl)." >&2
    exit 1
  fi
  # musl build is more portable (fewer glibc version issues)
  TARGET="x86_64-unknown-linux-musl"
fi

# Resolve version / tag
if [ -z "$VERSION" ]; then
  echo "Fetching latest release tag..."
  # GitHub API (no auth needed for public repo rate limits)
  VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name":' | head -1 | sed -E 's/.*"([^"]+)".*/\1/')
  if [ -z "$VERSION" ]; then
    echo "Failed to detect latest version from GitHub API." >&2
    echo "Pass --version explicitly." >&2
    exit 1
  fi
fi

# Normalize to have leading v for the download URL (GitHub releases use v-prefixed tags)
TAG="$VERSION"
case "$TAG" in
  v*) ;;
  *) TAG="v${TAG}" ;;
esac

echo "→ Installing ${BINARY} ${TAG} for ${TARGET}"

BASE_URL="https://github.com/${REPO}/releases/download/${TAG}"
ASSET="${BINARY}-${TARGET}.tar.xz"
ASSET_URL="${BASE_URL}/${ASSET}"
CHECKSUM_URL="${BASE_URL}/${ASSET}.sha256"

TMPDIR="$(mktemp -d 2>/dev/null || mktemp -d -t qmd-install)"
# shellcheck disable=SC2064
trap "rm -rf '$TMPDIR'" EXIT INT TERM

cd "$TMPDIR"

echo "↓ Downloading ${ASSET}"
curl -fL --progress-bar "$ASSET_URL" -o "$ASSET" || { echo "Download failed: $ASSET_URL" >&2; exit 1; }

echo "↓ Downloading checksum"
curl -fL "$CHECKSUM_URL" -o "${ASSET}.sha256" || {
  echo "Warning: could not download .sha256 (proceeding without verification)" >&2
}

# Verify checksum when possible
if [ -f "${ASSET}.sha256" ]; then
  echo "→ Verifying SHA-256 checksum..."
  if command -v sha256sum >/dev/null 2>&1; then
    if ! sha256sum -c "${ASSET}.sha256" >/dev/null 2>&1; then
      echo "Checksum verification FAILED!" >&2
      exit 1
    fi
  elif command -v shasum >/dev/null 2>&1; then
    if ! shasum -a 256 -c "${ASSET}.sha256" >/dev/null 2>&1; then
      echo "Checksum verification FAILED!" >&2
      exit 1
    fi
  else
    echo "Warning: neither sha256sum nor shasum found; skipping verification."
    echo "         Please verify manually or install one of the tools."
  fi
fi

echo "→ Extracting archive..."
tar -xJf "$ASSET"

# The tarball contains the bare binary (and possibly LICENSE/readme from auto-includes)
BIN_PATH="$(find . -type f -perm -111 -name "$BINARY" | head -n 1)"
if [ -z "$BIN_PATH" ]; then
  # fallback: any executable named qmd
  BIN_PATH="$(find . -type f -name "$BINARY" | head -n 1)"
fi
if [ -z "$BIN_PATH" ]; then
  echo "Could not locate ${BINARY} binary inside the archive." >&2
  ls -la
  exit 1
fi

chmod +x "$BIN_PATH"

# Decide install location (prefer /usr/local/bin when writable, else ~/.local/bin)
if [ -z "$INSTALL_DIR" ]; then
  if [ -w /usr/local/bin ] && [ -d /usr/local/bin ]; then
    INSTALL_DIR="/usr/local/bin"
  else
    INSTALL_DIR="${HOME}/.local/bin"
    mkdir -p "$INSTALL_DIR"
  fi
else
  mkdir -p "$INSTALL_DIR"
fi

DEST="${INSTALL_DIR}/${BINARY}"

echo "→ Installing to ${DEST}"
if [ -f "$DEST" ]; then
  echo "   (overwriting existing ${DEST})"
fi
mv "$BIN_PATH" "$DEST"
chmod +x "$DEST"

echo
echo "✅ Successfully installed ${BINARY} ${TAG}"
echo "   Location : ${DEST}"
echo "   Version  : $("$DEST" --version 2>/dev/null || echo 'run it to check')"
echo
echo "   Add ${INSTALL_DIR} to your PATH if it isn't already:"
echo "     export PATH=\"${INSTALL_DIR}:\$PATH\""
echo
echo "   Try it   : ${BINARY} --help"
echo "   Uninstall: rm ${DEST}"
echo

exit 0

#!/usr/bin/env bash
set -euo pipefail

REPO="2yuri/lazyreq"
INSTALL_DIR="${LAZYREQ_INSTALL_DIR:-$HOME/.local/bin}"

fail() {
  echo "error: $1" >&2
  exit 1
}

latest_version() {
  curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep '"tag_name"' \
    | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/' \
    || true
}

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1"
  else
    shasum -a 256 "$1"
  fi
}

case "$(uname -s)" in
  Darwin) asset="lazyreq_Darwin_all.tar.gz" ;;
  Linux)
    case "$(uname -m)" in
      x86_64 | amd64) asset="lazyreq_Linux_x86_64.tar.gz" ;;
      arm64 | aarch64) asset="lazyreq_Linux_arm64.tar.gz" ;;
      *) fail "unsupported architecture: $(uname -m)" ;;
    esac
    ;;
  *) fail "unsupported OS: $(uname -s) — grab a build from https://github.com/$REPO/releases/latest" ;;
esac

version="${VERSION:-$(latest_version)}"
[ -n "$version" ] || fail "could not determine the latest release (set VERSION=vX.Y.Z to pin one)"

base="https://github.com/$REPO/releases/download/$version"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "downloading lazyreq $version ($asset)..."
curl -fsSL "$base/$asset" -o "$tmp/$asset" \
  || fail "download failed: $base/$asset"

checksums="lazyreq_${version#v}_checksums.txt"
if curl -fsSL "$base/$checksums" -o "$tmp/$checksums" 2>/dev/null; then
  expected="$(grep " $asset\$" "$tmp/$checksums" | awk '{print $1}')"
  actual="$(sha256 "$tmp/$asset" | awk '{print $1}')"
  [ "$expected" = "$actual" ] || fail "checksum mismatch for $asset"
  echo "checksum ok"
else
  echo "warning: checksums not found, skipping verification" >&2
fi

tar -xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$INSTALL_DIR"
install -m 755 "$tmp/lazyreq" "$INSTALL_DIR/lazyreq"

echo "installed $("$INSTALL_DIR/lazyreq" --version) to $INSTALL_DIR/lazyreq"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo
    echo "$INSTALL_DIR is not on your PATH — add this to your shell profile:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac

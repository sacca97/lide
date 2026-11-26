#!/usr/bin/env bash
#
# Fetch a prebuilt PDFium shared library for the PDF renderer.
#
# The editor loads PDFium at runtime rather than linking against it, so the
# library is not part of the source tree. This script downloads the official
# prebuilt binary from the pdfium-binaries project and verifies it against a
# pinned SHA-256 checksum.
#
# Usage:
#   scripts/fetch-pdfium.sh [version]
#
# The version and its checksum are pinned in docs/adr/0004-pdf-rendering.md.
# The library is written to third_party/pdfium/lib/ and is not committed.

set -euo pipefail

# macOS ships `shasum`, most Linux systems `sha256sum`.
sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    else
        echo "no sha256 tool found (need sha256sum or shasum)" >&2
        return 1
    fi
}

REPO="bblanchon/pdfium-binaries"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${ROOT}/third_party/pdfium"

# version:sha256 of the asset for this platform, pinned.
# Populate with: scripts/fetch-pdfium.sh --print-checksums
case "$(uname -s)/$(uname -m)" in
    Linux/x86_64)
        ASSET="pdfium-linux-x64.tgz"
        VERSION="${1:-8179}"
        EXPECTED="${PDFIUM_SHA256:-b4766a6bf2856efbcc6c3cc7d6ae9022bdc6bb36ddc1acdd92d1f1873f773f4b}"
        ;;
    Linux/aarch64)
        ASSET="pdfium-linux-arm64.tgz"
        VERSION="${1:-8179}"
        EXPECTED="${PDFIUM_SHA256:-}"
        ;;
    Darwin/x86_64)
        ASSET="pdfium-mac-x64.tgz"
        VERSION="${1:-8179}"
        EXPECTED="${PDFIUM_SHA256:-b29bbfe37efcad5b443fa3556c0485ec96fc1992d5b19d84df49d48fbed1d0d2}"
        ;;
    Darwin/arm64)
        ASSET="pdfium-mac-arm64.tgz"
        VERSION="${1:-8179}"
        EXPECTED="${PDFIUM_SHA256:-44b7c6f55dd33454d0909728bd0adf6c86a8e141b36f07a6b6f50e228e1dfb46}"
        ;;
    *)
        echo "unsupported platform: $(uname -s)/$(uname -m)" >&2
        exit 1
        ;;
esac

if [ "${1:-}" = "--print-checksums" ]; then
    for platform in "pdfium-linux-x64.tgz" "pdfium-linux-arm64.tgz" "pdfium-mac-x64.tgz" "pdfium-mac-arm64.tgz"; do
        printf '%s  ' "${platform}"
        curl --fail --location --silent --show-error \
            "https://github.com/${REPO}/releases/download/chromium%2F${VERSION}/${platform}" \
            | { if command -v sha256sum >/dev/null 2>&1; then sha256sum; else shasum -a 256; fi; } | cut -d' ' -f1
    done
    exit 0
fi

BASE="https://github.com/${REPO}/releases/download/chromium%2F${VERSION}"
TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

echo "downloading ${ASSET} (chromium/${VERSION})"
curl --fail --location --silent --show-error \
    --output "${TMP}/${ASSET}" "${BASE}/${ASSET}"

if [ -n "${EXPECTED}" ]; then
    ACTUAL="$(sha256 "${TMP}/${ASSET}")"
    if [ "${EXPECTED}" != "${ACTUAL}" ]; then
        echo "checksum mismatch for ${ASSET}" >&2
        echo "  expected ${EXPECTED}" >&2
        echo "  actual   ${ACTUAL}" >&2
        exit 1
    fi
    echo "checksum ok"
else
    echo "warning: no pinned checksum for ${ASSET}; set PDFIUM_SHA256 to verify" >&2
fi

mkdir -p "${DEST}"
tar xzf "${TMP}/${ASSET}" -C "${DEST}"

echo "installed to ${DEST}/lib"
echo
LIBRARY="libpdfium.so"
if [ "$(uname -s)" = "Darwin" ]; then
    LIBRARY="libpdfium.dylib"
fi

echo "The next native build embeds this library in the editor. Set"
echo "LIDE_EMBED_PDFIUM=0 to keep it external, or point that variable at a"
echo "different library. At runtime LIDE_PDFIUM_PATH and a library beside the"
echo "binary still take precedence."
echo
echo "  export LIDE_PDFIUM_PATH=${DEST}/lib/${LIBRARY}"

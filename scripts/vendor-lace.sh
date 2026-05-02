#!/usr/bin/env bash
#
# Update the vendored Lace C sources from a local checkout.
#
# Usage:
#   ./scripts/vendor-lace.sh /path/to/lace
#   LACE_DIR=/path/to/lace ./scripts/vendor-lace.sh
#
# This copies lace.h and lace.c into lace-native/vendor/ and
# updates the VERSION file with the git commit and version.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
VENDOR_DIR="$REPO_ROOT/lace-native/vendor"

# Find Lace source
LACE_DIR="${1:-${LACE_DIR:-}}"
if [ -z "$LACE_DIR" ]; then
    echo "Usage: $0 /path/to/lace"
    echo "   or: LACE_DIR=/path/to/lace $0"
    exit 1
fi

# Resolve src directory
if [ -f "$LACE_DIR/src/lace.h" ]; then
    LACE_SRC="$LACE_DIR/src"
elif [ -f "$LACE_DIR/lace.h" ]; then
    LACE_SRC="$LACE_DIR"
else
    echo "Error: cannot find lace.h in $LACE_DIR or $LACE_DIR/src"
    exit 1
fi

# Copy files
cp "$LACE_SRC/lace.h" "$VENDOR_DIR/lace.h"
cp "$LACE_SRC/lace.c" "$VENDOR_DIR/lace.c"

# Extract version info
VERSION="unknown"
COMMIT="unknown"
DATE="$(date +%Y-%m-%d)"

if [ -d "$LACE_DIR/.git" ]; then
    COMMIT="$(git -C "$LACE_DIR" rev-parse --short HEAD 2>/dev/null || echo "unknown")"
    # Try to get version from git tag or CMakeLists.txt
    TAG="$(git -C "$LACE_DIR" describe --tags --exact-match 2>/dev/null || true)"
    if [ -n "$TAG" ]; then
        VERSION="$TAG"
    elif [ -f "$LACE_DIR/CMakeLists.txt" ]; then
        VERSION="$(grep -oP 'project\(lace VERSION \K[0-9.]+' "$LACE_DIR/CMakeLists.txt" 2>/dev/null || echo "unknown")"
    fi
elif [ -f "$LACE_DIR/CMakeLists.txt" ]; then
    VERSION="$(grep -oP 'project\(lace VERSION \K[0-9.]+' "$LACE_DIR/CMakeLists.txt" 2>/dev/null || echo "unknown")"
fi

# Write VERSION file
cat > "$VENDOR_DIR/VERSION" << EOF
Vendored from: https://github.com/trolando/lace
Version: $VERSION
Commit: $COMMIT
Date: $DATE

To update:
  ./scripts/vendor-lace.sh /path/to/lace
EOF

echo "Vendored Lace $VERSION ($COMMIT) from $LACE_SRC"
echo "  -> $VENDOR_DIR/lace.h ($(wc -l < "$VENDOR_DIR/lace.h") lines)"
echo "  -> $VENDOR_DIR/lace.c ($(wc -l < "$VENDOR_DIR/lace.c") lines)"
echo "  -> $VENDOR_DIR/VERSION"

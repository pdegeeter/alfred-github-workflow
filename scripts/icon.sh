#!/usr/bin/env bash
#
# Regenerate the Alfred workflow icon (workflow/icon.png) from scripts/icon.svg.
# Alfred uses a PNG, so the SVG is rasterized at 512x512.
#
# Requires: rsvg-convert (brew install librsvg).
#
# Usage: scripts/icon.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

if ! command -v rsvg-convert >/dev/null 2>&1; then
  echo "error: rsvg-convert not found. Install it with: brew install librsvg" >&2
  exit 1
fi

rsvg-convert -w 512 -h 512 scripts/icon.svg -o workflow/icon.png

echo "==> Wrote workflow/icon.png ($(sips -g pixelWidth -g pixelHeight workflow/icon.png | awk '/pixel/{print $2}' | paste -sd x -))"

#!/usr/bin/env bash
#
# Regenerate the Alfred icons from scripts/icon.svg (the GitHub mark):
#
#   * workflow/icon.png       — the workflow's icon;
#   * workflow/<uid>.png      — one per script filter of workflow/info.plist,
#                               which Alfred shows for that keyword (and its
#                               items, which carry no icon of their own).
#
# Requires: rsvg-convert (brew install librsvg), plutil (ships with macOS).
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
echo "==> Wrote workflow/icon.png"

# Script filter uids, read from the plist so a new keyword gets its icon.
uids="$(plutil -convert json -o - workflow/info.plist | python3 -c '
import json, sys
for o in json.load(sys.stdin)["objects"]:
    if o["type"] == "alfred.workflow.input.scriptfilter":
        print(o["uid"])
')"

for uid in $uids; do
  cp workflow/icon.png "workflow/$uid.png"
  echo "==> Wrote workflow/$uid.png"
done

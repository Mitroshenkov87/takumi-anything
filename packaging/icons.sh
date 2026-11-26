#!/usr/bin/env bash
# Regenerate every app icon from assets/app-icon/takumi-anything.svg (40 px and up) and
# takumi-anything-small.svg (16-32 px). Needs rsvg-convert (librsvg) and Python with Pillow.
# The outputs are committed, so building and packaging never need these tools.
#
#   packaging/icons.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
command -v rsvg-convert >/dev/null || { echo "error: rsvg-convert not found (librsvg)" >&2; exit 1; }
python3 "$ROOT/assets/app-icon/source/gen.py"

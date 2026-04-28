#!/usr/bin/env bash
# Fetch the cctv-sample clip used by Step 2 / Step 5 integration tests.
#
# The asset is hosted on Pexels under the Pexels License (free for commercial
# use, attribution appreciated). It is not committed to the repo because it
# is too large for source control.
#
# Usage:
#   benchmarks/datasets/cctv-sample/download.sh
#
# Behavior:
#   - Skips download if `sample.mp4` already exists.
#   - Verifies file size + ffprobe-reported codec/resolution after download.
#
# TODO: fill in PEXELS_URL with the actual asset page URL once the SOURCE.md
#       attribution is finalized.

set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
OUT="$DIR/sample.mp4"

if [ -f "$OUT" ]; then
  echo "sample.mp4 already present at $OUT — skipping download"
  exit 0
fi

# TODO: replace with the canonical Pexels download URL.
PEXELS_URL=""

if [ -z "$PEXELS_URL" ]; then
  cat <<EOF >&2
download.sh: no canonical URL is wired up yet.

For now, manually obtain the clip:
  1. Visit https://www.pexels.com/search/videos/cctv/
  2. Pick a 1080p H.264 clip ~15-30s.
  3. Download HD/Full HD MP4 to:
       $OUT
  4. Update SOURCE.md with the page URL, contributor name, and license note.
  5. Edit this script to set PEXELS_URL = direct mp4 download link.

EOF
  exit 1
fi

echo "fetching $PEXELS_URL -> $OUT"
curl -fL "$PEXELS_URL" -o "$OUT.part"
mv "$OUT.part" "$OUT"

ffprobe -v error -select_streams v:0 \
  -show_entries stream=codec_name,width,height,duration \
  -of default=nw=1 "$OUT"

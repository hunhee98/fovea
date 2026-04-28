#!/usr/bin/env bash
# Generate synthetic camera-pan clips for global-motion correction benchmarks.
#
# Inputs:  benchmarks/datasets/cctv-sample/sample.mp4
# Outputs: benchmarks/datasets/synthetic-ptz/data/{pan_pure,static}.mp4
#
# Encoding profile matches seattle-dot DOT camera fingerprint:
#   H.264 High profile, Level 4.0, GOP 15, no B-frames, ~3 Mbps target.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
SRC="$ROOT/benchmarks/datasets/cctv-sample/sample.mp4"
OUT_DIR="$ROOT/benchmarks/datasets/synthetic-ptz/data"

if [[ ! -f "$SRC" ]]; then
    echo "ERROR: cctv-sample/sample.mp4 not found at $SRC" >&2
    echo "Run benchmarks/datasets/cctv-sample/download.sh first." >&2
    exit 2
fi

mkdir -p "$OUT_DIR"
TMP_FRAME="$OUT_DIR/_frame0.jpg"

# Extract frame 0 as a JPEG.
ffmpeg -hide_banner -loglevel error -y -i "$SRC" -frames:v 1 "$TMP_FRAME"

# Surveillance encoder profile (matches seattle-dot fingerprint).
SURV_OPTS=(
    -c:v libx264
    -profile:v high
    -level:v 4.0
    -preset medium
    -pix_fmt yuv420p
    -g 15
    -bf 0
    -b:v 3M
    -maxrate 3M
    -bufsize 6M
)

# Source still is 1080×1920 (portrait Pexels). We crop a 540×960 window
# that slides horizontally over 10 seconds.
# Pan distance: 0 → 540 px over 10 s ≈ 54 px/s ≈ 1.8 px per 30-fps frame.
ffmpeg -hide_banner -loglevel error -y \
    -loop 1 -framerate 30 -i "$TMP_FRAME" \
    -t 10 \
    -vf "crop=540:960:'min(540, t*54)':0" \
    "${SURV_OPTS[@]}" \
    "$OUT_DIR/pan_pure.mp4"

# Control: same source frame, no pan. All triggers are FP by construction.
ffmpeg -hide_banner -loglevel error -y \
    -loop 1 -framerate 30 -i "$TMP_FRAME" \
    -t 10 \
    -vf "crop=540:960:0:0" \
    "${SURV_OPTS[@]}" \
    "$OUT_DIR/static.mp4"

rm -f "$TMP_FRAME"

echo "Generated:"
ls -la "$OUT_DIR"/*.mp4

#!/usr/bin/env bash
# Capture a short clip from a public Seattle DOT traffic camera HLS feed.
#
# These cameras are operated by the Seattle Department of Transportation and
# served via Wowza streamlock.net, the same infrastructure used by many North
# American DOTs (Caltrans D7, Florida 511, etc.). The HLS playlist is open
# (no auth, CORS-allowed) and `ffmpeg -c copy` preserves the original H.264
# bitstream — encoder fingerprints (GOP structure, slice layout, MV partition
# choices) are not lost to a re-encode.
#
# Confirmed bitstream characteristics (2026-04-29):
#   - 1920 × 1080, H.264 High@4.0, ~3 Mbps, ~22 fps effective
#   - GOP ≈ 15 frames (I-frame every ~0.5 s)
#   - No B-frames (typical low-latency surveillance encoder profile)
#
# Usage:
#   benchmarks/datasets/seattle-dot/download.sh                # 60s default
#   DURATION_S=600 benchmarks/datasets/seattle-dot/download.sh # 10 minutes
#   STREAM=15_NW_85_NS benchmarks/datasets/seattle-dot/download.sh
#
# Behavior:
#   - Skips download if the target file already exists.
#   - Uses `-c copy` so no encoder is invoked locally.
#   - Verifies codec / resolution / GOP after download.

set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$DIR/data"

STREAM="${STREAM:-24_NW_Market_EW}"
DURATION_S="${DURATION_S:-60}"

PLAYLIST="https://61e0c5d388c2e.streamlock.net/live/${STREAM}.stream/playlist.m3u8"
OUT="$DIR/data/${STREAM}_${DURATION_S}s.mp4"

if [ -f "$OUT" ]; then
  echo "$OUT already present — skipping download"
  exit 0
fi

if ! command -v ffmpeg >/dev/null 2>&1; then
  echo "ffmpeg not found on PATH" >&2
  exit 1
fi

echo "playlist : $PLAYLIST"
echo "output   : $OUT"
echo "duration : ${DURATION_S}s"
echo

# `-c copy` keeps the original H.264 bitstream untouched. `-bsf:v
# h264_mp4toannexb` is needed only when remuxing into formats that don't
# carry length-prefixed NALs; mp4 handles avcC fine on macOS / Linux ffmpeg.
ffmpeg -y -loglevel error \
  -i "$PLAYLIST" \
  -t "$DURATION_S" \
  -c copy \
  -movflags +faststart \
  "$OUT.part"
mv "$OUT.part" "$OUT"

echo
echo "--- ffprobe ---"
ffprobe -v error -select_streams v:0 \
  -show_entries stream=codec_name,profile,level,width,height,r_frame_rate,bit_rate,pix_fmt \
  -show_entries format=duration,size \
  -of default=noprint_wrappers=1 "$OUT"

echo
echo "--- frame-type counts (first ${DURATION_S}s) ---"
ffprobe -v error -select_streams v:0 \
  -show_entries frame=pict_type -of csv=p=0 "$OUT" \
  | sort | uniq -c

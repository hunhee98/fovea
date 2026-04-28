#!/usr/bin/env bash
# Fetch and re-encode CDnet 2014 cameraJitter scenes to surveillance-profile MP4.
#
# Source: http://jacarini.dinf.usherbrooke.ca/static/dataset/cameraJitter.zip
# Output: benchmarks/datasets/cdnet-camerajitter/data/<scene>.mp4

set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
DATA_DIR="$DIR/data"
ZIP_PATH="$DATA_DIR/cameraJitter.zip"
URL="http://jacarini.dinf.usherbrooke.ca/static/dataset/cameraJitter.zip"

mkdir -p "$DATA_DIR"

if [[ ! -f "$ZIP_PATH" ]]; then
    echo "fetching $URL"
    curl -fL "$URL" -o "$ZIP_PATH.part"
    mv "$ZIP_PATH.part" "$ZIP_PATH"
fi

if [[ ! -d "$DATA_DIR/cameraJitter" ]]; then
    echo "unpacking..."
    (cd "$DATA_DIR" && unzip -q cameraJitter.zip)
fi

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

# Each scene is a directory of `inNNNNNN.jpg` images at 30 fps.
for scene_dir in "$DATA_DIR/cameraJitter"/*/; do
    scene=$(basename "$scene_dir")
    out="$DATA_DIR/$scene.mp4"
    if [[ -f "$out" ]]; then
        echo "$scene: already encoded — skipping"
        continue
    fi
    input_dir="$scene_dir/input"
    if [[ ! -d "$input_dir" ]]; then
        echo "$scene: no input/ directory, skipping"
        continue
    fi
    echo "encoding $scene..."
    ffmpeg -hide_banner -loglevel error -y \
        -framerate 30 -i "$input_dir/in%06d.jpg" \
        "${SURV_OPTS[@]}" \
        "$out"
done

echo "Encoded:"
ls -la "$DATA_DIR"/*.mp4

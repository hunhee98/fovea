# youtube-cctv-2 / sample.mp4

## File
- Path: `benchmarks/datasets/youtube-cctv-2/sample.mp4`
- Codec: H.264
- Resolution: 640 × 360
- Frame rate: 30 fps
- Duration: 55.57 s

## Source
- Origin: YouTube — https://youtu.be/UZFm-kg3PaE
- Downloaded via `yt-dlp -f "best[ext=mp4][vcodec^=avc1]/best[ext=mp4]/best"`
  on 2026-04-28. yt-dlp was the only available format (itag=18) without
  a JS runtime; this is YouTube's compatibility 360p MP4.
- Content: fixed-camera CCTV footage of an underground parking garage.
  A white SUV maneuvers out of a stall and leaves the frame.

## Use
- Step 5 benchmark Clip B. Hand-labeled active interval lives in
  `events.csv`.

## Notes
- Resolution is low (360p) compared to the canonical 1080p target.
  Motion-energy peaks are correspondingly small (max 3910), so the
  motion-trigger threshold for this clip differs from clips at higher
  resolution. A future `motion_energy_per_mb` normalization in
  fovea-trigger-core would let users specify thresholds in resolution-
  independent units.

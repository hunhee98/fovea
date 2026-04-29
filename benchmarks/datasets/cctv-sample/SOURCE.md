# cctv-sample / sample.mp4

## File
- Path: `benchmarks/datasets/cctv-sample/sample.mp4`
- Size: 12.6 MB
- Codec: H.264 (avc1)
- Resolution: 1080 × 1920 (portrait orientation)
- Frame rate: 30 fps
- Duration: 15.23 s
- Pixel format: yuv420p

## Source
- Origin: Pexels (filename `15537026_1080_1920_30fps.mp4` matches Pexels asset ID convention).
- URL: TODO — fill in the Pexels page URL.
- Author: TODO — fill in the Pexels contributor name.
- License: Pexels License (free for commercial use, attribution appreciated, no resale of unaltered file).
- License URL: https://www.pexels.com/license/

## Use
- Step 2 development & integration tests for `fovea-trigger-stream` MV extraction.
- Step 5 partial benchmark coverage. Note: portrait orientation (1080×1920), not the typical 1080p (1920×1080) landscape used in canonical 1080p benchmarks. Bench results derived from this clip must declare the orientation.

## Notes
- Continuous motion content (vehicles/pedestrians) — exercises P-frame MV extraction well, but provides few static intervals to validate `IntervalTrigger` heartbeat behavior. A second clip with quiet periods will be added in Step 5 if needed.

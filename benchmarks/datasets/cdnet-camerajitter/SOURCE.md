# cdnet-camerajitter — CDnet 2014 cameraJitter category

## What this is

The `cameraJitter` category from CDnet 2014, an academic change-detection
benchmark hosted by Université de Sherbrooke. Four scenes captured with a
camera that vibrates / shakes — the closest CDnet analogue to real fixed
surveillance with mounting vibration, wind sway, or HVAC interference.

The full PTZ category lives behind a different URL that no longer resolves
(2026-04-29). cameraJitter exercises the same mechanism we care about
(uniform global motion → false positives) and is what we use here.

`data/` is gitignored.

## Why this dataset

- Real (not synthetic) camera-induced global motion across the frame.
- Public, citable, well-known in the change-detection literature.
- Small (~130 MB compressed) — runs on a MacBook Air without thermal load.

## Limitations

- CDnet sequences are PNG image streams, not native H.264 bitstreams.
  `download.sh` re-encodes them with libx264 in a surveillance-like profile
  (High profile, GOP 15, no B-frames, ~3 Mbps). The resulting bitstream is
  closer to real CCTV than libx264 defaults but is not produced by an Axis /
  Bosch / Pelco encoder.
- Numbers from this dataset prove **mechanism** (correction reduces FP under
  real camera-jitter motion). Real-encoder validation is pending a future
  RTSP capture from a Hikvision/Dahua/Axis class camera.

## Citation

> Wang, Y., Jodoin, P.-M., Porikli, F., Konrad, J., Benezeth, Y., Ishwar, P.
> "CDnet 2014: An Expanded Change Detection Benchmark Dataset"
> CVPR Workshop, 2014. https://changedetection.net/

## Files

- `download.sh` — fetches `cameraJitter.zip` from
  `jacarini.dinf.usherbrooke.ca`, unpacks each scene's PNGs, re-encodes each
  to a surveillance-profile MP4 under `data/<scene>.mp4`.

Run from the repo root:

```sh
benchmarks/datasets/cdnet-camerajitter/download.sh
```

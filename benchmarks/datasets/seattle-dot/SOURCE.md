# seattle-dot — public Seattle Department of Transportation traffic cameras

## What this is

A capture script for live HLS feeds from Seattle DOT intersection cameras.
These are real production CCTV cameras operated by a US municipality, served
via Wowza streamlock.net at `61e0c5d388c2e.streamlock.net`. The same
infrastructure backs many North American DOTs (Caltrans D7, Florida 511,
Texas DOT subsystems), so the encoder fingerprint here is representative of
production ITS deployments — not consumer phone cameras, not academic test
encoders.

This dataset is **not committed**. `download.sh` fetches it on demand.
`data/` is gitignored.

## Why this dataset matters

fovea-trigger reads patterns the encoder leaves in the H.264 bitstream — motion
vectors, intra-coded blocks, slice structure. Those patterns are encoder-
specific. To make claims like "this works on real CCTV", we need bitstreams
produced by real CCTV encoders, not by `ffmpeg -i in.png -c:v libx264` with
default settings (which is what the academic CDnet 2014 dataset gives you
when you re-encode its image sequences).

The Seattle DOT feed is, as of 2026-04-29:
- 1920 × 1080
- H.264 High Profile @ Level 4.0
- ~3 Mbps target bitrate
- GOP ≈ 15 (I-frame every ~0.5 s)
- **No B-frames** — classic low-latency surveillance encoder configuration
- Effective frame rate ≈ 22 fps (the source declares 30, but HLS chunk
  boundaries lose a few)

This matches typical municipal CCTV hardware (Axis / Bosch / Pelco class
encoders behind a Wowza re-streamer).

## Confirmed working stream IDs (2026-04-29)

| `STREAM=` | Location | Resolution |
|---|---|---|
| `24_NW_Market_EW` | 24th Ave NW & NW Market St | 1920×1080 |
| `15_NW_85_NS` | 15th Ave NW & NW 85th St | 1280×720 |

The full per-intersection list lives in
[wttdotm/traffic_cam_photobooth — seattleSources.json](https://github.com/wttdotm/traffic_cam_photobooth/blob/main/seattleSources.json).

Streams listed in that file may go offline; verify with `curl -I` before
relying on a particular ID for a long-running bench.

## License / attribution

Live public traffic cameras operated by Seattle DOT, broadcast on the open
Internet without authentication. We treat them under "fair use for research /
benchmarking on a public live feed", consistent with how OSS NVR projects
(Frigate, Viseron) document live test sources. We **do not redistribute**
captured footage — `data/` is gitignored, and result files reference the
dataset by its `download.sh` rather than embedding clips.

## Usage

```sh
# Default — 60 s capture from 24th & Market
benchmarks/datasets/seattle-dot/download.sh

# Longer capture (10 minutes) for soak / density work
DURATION_S=600 benchmarks/datasets/seattle-dot/download.sh

# Different intersection
STREAM=15_NW_85_NS benchmarks/datasets/seattle-dot/download.sh

# Cleanup
rm -rf benchmarks/datasets/seattle-dot/data/
```

## What's been validated against this source so far

- 2026-04-29 — fovea-trigger on a 30-second 1080p capture: 17.2× realtime parse
  on M3 8-core, P-frame `intra_ratio` distribution mean 0.079 with max
  0.180 (i.e. the new CB-stats accessor returns meaningful, non-degenerate
  values on real traffic CCTV). Captured ad-hoc during the 004 wedge-proof
  scoping — not yet a committed bench result. Formal results land via the
  P0.x sequence in `docs/05.exec-plans/004-positioning.md`.

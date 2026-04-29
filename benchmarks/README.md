# Fovea Benchmarks

Benchmarks are first-class in fovea (see `CLAUDE.md` → "Evidence rules"). Every performance claim must point to a result file here.

## Layout

```
benchmarks/
├── datasets/              # download.sh per dataset; no media committed
├── baselines/             # non-fovea reference implementations (e.g. pixel-diff)
├── runners/               # fovea-trigger runs
├── analysis/              # comparison + plotting
└── results/               # dated, committed: <YYYY-MM-DD>-<slug>.md
```

## Baselines

Non-fovea-trigger comparison points used by the result files:

- [`baselines/pixel_diff/`](baselines/pixel_diff/density.py) — classical
  decode + grayscale-absdiff motion detector (the Frigate / Viseron /
  Shinobi pattern). Same multiprocess shape as
  [`runners/density.py`](runners/density.py); used for
  `2026-04-29-pixeldiff-vs-fovea-density.md` and
  `2026-04-29-seattle-pixeldiff-vs-fovea.md`.

A real `mv-extractor` baseline is deferred to the next cloud-bench
session — the upstream package has no Apple Silicon wheels and the
Docker image is amd64-only, so it doesn't run cleanly on the dev box.
See 004 P0.2 for the rescoped capability-matrix framing of that
comparison.

## Reproducing

Each result file at `results/<date>-<slug>.md` declares:

- Commit hash of fovea
- Hardware (CPU/RAM/GPU/OS)
- Dataset and download command
- Exact command line used
- Numbers + plot link

Anyone with the same dataset and a clean clone runs the same command and gets the same numbers (within hardware noise).

## Datasets

Public datasets only. We never commit media — only `datasets/<name>/download.sh` plus a `SOURCE.md` explaining where the asset lives upstream and what license applies.

Currently committed:

- **`cctv-sample/`** — short Pexels-licensed CCTV-style clip kept as a quick sanity dataset for unit-tests and density smokes. Tiny, mp4 only.
- **`cctv-sample-hevc/`** — HEVC variant of the same clip; used by HEVC accessor tests.
- **`seattle-dot/`** — `download.sh` that captures from a public Seattle DOT
  traffic-camera HLS feed via `ffmpeg -c copy`, preserving the original
  H.264 NALs. Closest free path to a real production CCTV bitstream.
  Used by `2026-04-29-seattle-pixeldiff-vs-fovea.md`.
- **`youtube-cctv-30s/`** / **`youtube-cctv*`** — older yt-dlp captures kept for legacy benches; `seattle-dot` is the preferred replacement.
- **`kaggle-n001`**, **`kaggle-n014`** — early Kaggle samples kept as unit-test fixtures.

Each dataset carries a `download.sh` + `SOURCE.md`. `data/` is
gitignored.

## Methodology

See `docs/03.methodology/` for:
- How we measure latency (p50/p99, what we exclude)
- How we measure recall@event (with VLM-as-oracle)
- How we attribute decode CPU%
- Confidence intervals and noise floor

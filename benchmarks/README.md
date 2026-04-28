# Fovea Benchmarks

Benchmarks are first-class in fovea (see `CLAUDE.md` → "Evidence rules"). Every performance claim must point to a result file here.

## Layout

```
benchmarks/
├── datasets/              # download.sh per dataset; no media committed
├── baselines/             # uniform-sampling, mv-extractor, etc.
├── runners/               # fovea-mv runs
├── analysis/              # comparison + plotting
└── results/               # dated, committed: <YYYY-MM-DD>-<slug>.md
```

## Reproducing

Each result file at `results/<date>-<slug>.md` declares:

- Commit hash of fovea
- Hardware (CPU/RAM/GPU/OS)
- Dataset and download command
- Exact command line used
- Numbers + plot link

Anyone with the same dataset and a clean clone runs the same command and gets the same numbers (within hardware noise).

## Datasets

Public datasets only. We never commit media — only `datasets/<name>/download.sh`.

Initial datasets (planned):
- **pexels-cctv** — small set of free CCTV-style clips from Pexels (Pexels License).
- **pexels-traffic** — traffic / dashcam clips from Pexels.
- **bigbuckbunny** — Big Buck Bunny (CC-BY 3.0) for codec correctness baselines.
- **personal** — user's own short captures (gitignored, optional).

## Methodology

See `docs/03.methodology/` for:
- How we measure latency (p50/p99, what we exclude)
- How we measure recall@event (with VLM-as-oracle)
- How we attribute decode CPU%
- Confidence intervals and noise floor

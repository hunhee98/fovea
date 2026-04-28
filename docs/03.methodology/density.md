# Density

How many concurrent streams a single box can carry while idle. The structural claim of fovea — "no decode in the idle path" — is verified here.

## What we measure

Per-stream resource consumption while the trigger is *not* firing (idle window). For the active window we report end-to-end latency (see `latency.md`); density is about idle.

| metric | how |
|--------|-----|
| streams sustained | run N fovea sessions in parallel against synthetic or recorded RTSP sources, increase N until any of the bounds below is breached |
| idle CPU% per stream | `psutil` 100 ms sampling, averaged over a 60 s idle window |
| RSS per stream | `psutil.memory_info().rss`, mean over the idle window |
| trigger latency p95 | as defined in `latency.md`, must stay under target |

## 0.2 acceptance bounds (target — actual numbers land when bench runs)

- Idle CPU per stream: target ≤ 5%.
- Trigger fire latency p95: target ≤ 100 ms (clip-dependent — declare GOP).
- Sustained streams: target ≥ 100 on commodity hardware.

These are targets. The bench result file records the actual numbers, the hardware they were measured on, and any failure to hit them.

## Comparison baselines

The density story is meaningful relative to a "decode everything" pipeline. Suggested baselines for the next bench round:

1. **OpenCV motion detection** — `cv2.absdiff` on decoded frames.
2. **Frigate-style** — full software decode, motion mask, no MV.

Run all three at the same N and report idle CPU%. The fovea claim is a measurable gap, not a percentage.

## Hardware

Density numbers are meaningless without hardware declaration. See `hardware.md`. Repeat the bench on each target hardware class (M-series, x86 8-core, etc.).

## What we do not measure here

- Network bandwidth — RTSP is TCP-bound and flow-controlled by the source. Not a fovea property.
- VLM cost during active windows — see `cost.md`.

# 001 — fovea-mv MVP

Status: parked — 0.1.0 prototype, see 003 for roadmap
Owner: @hunhee98
Subproject: `fovea-mv`

## Outcome

Working prototype of a compressed-domain MV trigger engine. Public branch:
`feat/mv-step2-h264-extraction` on github.com/hunhee98/fovea (22+ commits).
27 commits include vendored libde265.

### Done

- H.264 MV extraction via FFmpeg `+export_mvs` (`fovea-mv-stream`).
- HEVC MV extraction via vendored libde265 + `de265_internals` accessor we added on top.
- Per-MB threshold mode on `MotionTrigger` (resolution-independent).
- `MotionTrigger`, `IntervalTrigger`, `SceneChangeTrigger`, `RegionMask`.
- File source (mp4 + raw .h264 + raw .h265) and RTSP source (TCP/UDP, timeout, reconnect on Disconnected + ReadTimeout).
- A' decoder skip-flags (`skip_loop_filter=all`, `skip_idct=all`) — measured 1.20× decode speedup with bit-exact MV output.
- Python bindings via PyO3 (`Stream.from_file`, `Stream.from_url`, `Event.decode` returning `numpy.uint8 (H,W,3)`).
- 30 tests (17 Rust suites + 13 pytest) green.
- Live-tested against an iPhone IP-camera app over Wi-Fi (~7 min, no memory growth).

### Not done — see 003

- exec-plan 001 acceptance bars (≤15% calls, ≥95% recall) never both hit on the same clip.
- HEVC RGB decode (`event.decode()` errors for HEVC sources).
- HEVC PTS pass-through (`ts_us=0` for HEVC).
- Onset/offset trigger.
- AV1 / VP9 codec coverage.
- 24h+ soak (only ran for ~7 min).
- NVDEC / VideoToolbox GPU decode (multi-stream density).
- Multi-stream concurrency.
- CI + cross-platform wheels.

## History

| Date | Event |
|---|---|
| 2026-04-28 | Plan written + Step 1 scaffolded |
| 2026-04-28 | Step 2 (H.264 MV extraction via FFmpeg) — A' decision |
| 2026-04-28 | Step 3 (file source polish + raw .h264 + fast_decode toggle) |
| 2026-04-28 | Step 4a (triggers in mv-core) + Step 4b (PyO3 bindings) |
| 2026-04-28 | Step 5 (bench infra + 2 clips) — partial; acceptance not met on the available clips |
| 2026-04-28 | Per-MB threshold added |
| 2026-04-28 | exec-plan 002 (RTSP) implemented end-to-end |
| 2026-04-28 | HEVC support via libde265 vendoring (Steps B.1–B.6) |
| 2026-04-28 | Production validation: phone IP-camera RTSP H.264, ~7 min, no leak |
| 2026-04-28 | reconnect-on-ReadTimeout patch |
| 2026-04-28 | Public push to github.com/hunhee98/fovea |

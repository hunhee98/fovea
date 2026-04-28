# 002 — RTSP / URL source

Status: completed 2026-04-28 — branch `feat/mv-step2-h264-extraction`
Owner: @hunhee98
Subproject: `fovea-mv` (extends 001)

## Outcome

`Stream.from_url(...)` works end-to-end against real RTSP cameras.

- `FfmpegSource::open_url(url, NetworkOptions)` and `open_url_with` accept
  rtsp://, rtsps://, rtmp://, http(s)://, file://, bare paths.
- `NetworkOptions { transport, open_timeout_ms, read_timeout_ms, max_reconnects }`.
  FFmpeg 8 needed `timeout` (not legacy `stimeout`) — verified by
  500 ms timeout failing in 529 ms vs 75 s with `stimeout`.
- Live-source EOF distinction: `SourceError::Disconnected` for live
  streams, `Ok(None)` for file EOF. `SourceError::ReadTimeout` for
  ETIMEDOUT/EAGAIN. Both auto-recovered via internal reconnect when
  `max_reconnects > 0`.
- Python: `Stream.from_url(url, *, transport, open_timeout_s,
  read_timeout_s, max_reconnects, fast_decode)`. Errors map to
  `TimeoutError` / `ConnectionError` / `RuntimeError`.
- Verification recipe (MediaMTX Docker container loop) used during
  Step 002.5; live phone test confirmed in Step C of follow-up work.

## History

| Date | Event |
|---|---|
| 2026-04-28 | Plan + Steps 002.1–002.5 implemented |
| 2026-04-28 | Phone IP-camera live test successful |

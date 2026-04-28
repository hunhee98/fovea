# Fovea

Helpers for sending video to VLMs (Vision-Language Models) without paying
to look at every frame.

I'm trying this out as a side project. The pieces below are at very
different levels of done-ness — please read the status column.

## Subprojects

| Crate / Package | Status | What it does |
|---|---|---|
| `fovea-mv` | working prototype | Reads H.264 / HEVC motion vectors and fires events on motion / scene change / heartbeat. |
| `fovea-pick` | planned | Pick K representative frames out of N inside a token budget. Hybrid of MV + image embeddings. |
| `fovea-stream` | planned | Streaming wrapper around an HF VLM. KV-cache reuse for long videos. |

## Why bother

Most "video → VLM" pipelines just push 1 frame per second to the model
and wait. On a single 24 / 7 camera that ends up being thousands of
calls per day, most of which look nearly identical to each other.

Compressed video already carries cheap signals about *where* and *when*
something changed (motion vectors, intra-coding ratios, frame types).
You can use those signals to drop ≥ 90 % of frames before they reach
the VLM.

This project is mostly an experiment in stitching that idea into a
small library. The numbers below are from prior work that motivated
the design — none of them are *our* numbers yet.

## Prior work this builds on

- **CoViAR** — Wu et al., CVPR 2018, [arXiv:1712.00636](https://arxiv.org/abs/1712.00636).
  Showed that compressed-domain action recognition can be **4.6× faster
  than Res3D** at comparable accuracy.
- **Towards Scalable Modeling of Compressed Videos** — Biswas et al.,
  Purdue, [arXiv:2503.13724](https://arxiv.org/abs/2503.13724).
  Reports **56× inference speedup and 330× cost reduction** over
  pixel-domain baselines on K-400 / K-600 / SS-v2.
- **LLaVA-Mini** — Zhang et al., [arXiv:2501.03895](https://arxiv.org/abs/2501.03895).
  Compresses image-token count from **576 → 1** while preserving most
  of the quality.
- **StreamingVLM** — Xu et al., MIT Han Lab,
  [arXiv:2510.09608](https://arxiv.org/abs/2510.09608).
  Real-time long-video understanding with KV-cache reuse, **up to 8 FPS
  on a single H100**.

## What's actually working today (`fovea-mv`)

- H.264 motion-vector extraction via FFmpeg's `+export_mvs`.
- HEVC motion-vector extraction via a vendored libde265 + a small
  `de265_internals` accessor we added on top.
- File and RTSP sources. RTSP includes timeout / reconnect handling.
- Triggers: `MotionTrigger`, `IntervalTrigger`, `SceneChangeTrigger`,
  with a per-macroblock threshold mode that's resolution-independent.
- Python bindings via PyO3:

  ```python
  from fovea_mv import Stream, MotionTrigger, IntervalTrigger

  stream = Stream.from_url(
      "rtsp://USER:PASS@CAMERA_IP:8554/live",
      transport="tcp",
      max_reconnects=3,
  )
  for ev in stream.events([
      MotionTrigger.per_mb(5.0),
      IntervalTrigger(10_000),
  ]):
      rgb = ev.decode()  # numpy uint8 (H, W, 3)
      # ... feed `rgb` to your VLM ...
  ```

- 30 tests (Rust + pytest) passing on macOS arm64 / Python 3.14.

I tested it against an iPhone running an IP-camera app over Wi-Fi for
a few minutes and the trigger reacted to me waving at the phone. That's
the level of "real-world tested" we're at.

## Status

Pre-alpha. No release. Things will move and break. The fovea-pick and
fovea-stream subprojects don't exist yet — only the road map does.

## License

Apache-2.0 — see [LICENSE](LICENSE).

The vendored libde265 (HEVC decoder) is LGPL-3.0-or-later; we link to
it dynamically. See [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md)
for the LGPL §4 source-shipping arrangement.

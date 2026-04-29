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
something changed (motion vectors, intra-coding ratios, frame types,
encoder MODE_SKIP markers). You can use those signals to drop most
frames before they reach the VLM.

## What `fovea-mv` is doing differently

The classical OSS motion detector — Frigate, Viseron, Shinobi — decodes
every frame, sums frame-to-frame absolute pixel differences, fires when
the sum crosses a threshold. It works, but the decode cost is paid on
*every* frame, including idle ones.

`fovea-mv` does the gate **before** decode. It reads motion vectors and
intra / skip block ratios out of the H.264 / HEVC bitstream, runs
trigger logic on those numbers, and only decodes RGB when the trigger
fires. The structural claim is that idle hours cost almost nothing.

Numbers from our own measurements, on Apple M3 (8-core, MBA), parsing
1080p H.264:

| source | per-realtime-stream CPU cost | vs pixel-diff |
|---|---:|---:|
| academic test clip ([cctv-sample.mp4](benchmarks/datasets/cctv-sample/SOURCE.md), 30 fps) | 14.9 % of one core | **~2.0× cheaper** ([result](benchmarks/results/2026-04-29-pixeldiff-vs-fovea-density.md)) |
| Seattle DOT public traffic camera (real CCTV bitstream, GOP 15, no B-frames) | 5.3 % of one core | **~2.9× cheaper** ([result](benchmarks/results/2026-04-29-seattle-pixeldiff-vs-fovea.md)) |

Both runs use the same hardware, same clip, same number of streams as
the pixel-diff baseline; the comparator is `cv2.absdiff` over decoded
grayscale frames, the same pattern Frigate's motion module uses. The
gap *grows* on real CCTV bitstreams (long GOP, no B-frames) — those
encoder choices are the case the parse path is cheapest on.

What `fovea-mv` is **not** claiming: faster than mv-extractor or other
compressed-domain MV libraries. They call the same `libavcodec` API
under the hood, so single-stream raw-MV-extraction throughput is
approximately equal. The advantage is *what each library exposes* —
HEVC, MODE_SKIP ratio, global-motion correction, spatial-cluster
reasoning, RTSP reconnect, and trigger primitives. See
[`docs/01.architecture/comparison-matrix.md`](docs/01.architecture/comparison-matrix.md)
for the side-by-side.

## Prior work this builds on

- **CoViAR** — Wu et al., CVPR 2018, [arXiv:1712.00636](https://arxiv.org/abs/1712.00636).
  Showed that compressed-domain action recognition can be **4.6× faster
  than Res3D** at comparable accuracy.
- **DMC-Net** — Shou et al., 2019, [arXiv:1901.03460](https://arxiv.org/abs/1901.03460).
  Validates the MV + residual fusion idea this project's CB-stats
  accessor surfaces as a trigger primitive.
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
- HEVC motion-vector extraction via a vendored libde265 plus a small
  `de265_internals` accessor that exposes per-frame intra / inter / skip
  ratios. The accessor is additive — it reads decoder state already
  populated during a normal HEVC decode, with zero changes to the
  hot path.
- File and RTSP sources. RTSP includes timeout / reconnect handling.
- Trigger primitives in `fovea-mv-core`:
  - `MotionTrigger` — energy-threshold trigger with optional
    per-macroblock (resolution-independent) mode, sliding-window
    smoothing, and median global-motion correction for PTZ cameras.
  - `IntervalTrigger` — heartbeat.
  - `SceneChangeTrigger` — fires on intra-block-ratio bursts.
  - `SpatialClusterTrigger` — fires only on motion concentrated in
    one region of the frame; suppresses uniformly scattered motion
    (rain, wind in foliage).
  - `FusionTrigger` — combines motion + intra + skip in a single
    decision, with an explicit `skip_suppress` idle gate.
- ~50-line cascade demo in
  [`examples/mini_nvr.py`](examples/mini_nvr.py) — open one source,
  run `FusionTrigger`, only on fire decode the frame and call your
  downstream model.
- Python bindings via PyO3:

  ```python
  from fovea_mv import Stream, FusionTrigger

  stream = Stream.from_url(
      "rtsp://USER:PASS@CAMERA_IP:8554/live",
      transport="tcp",
      max_reconnects=3,
  )
  trigger = FusionTrigger(
      "any",
      motion_threshold=200_000,
      intra_threshold=0.05,
      skip_suppress=0.95,   # encoder said "this didn't change" → skip
  )
  for ev in stream.events([trigger]):
      rgb = ev.decode()  # numpy uint8 (H, W, 3)
      # ... feed `rgb` to your VLM ...
  ```

- 42 Rust unit tests + 13 Python smoke tests passing on macOS arm64 /
  Python 3.14.
- A 1.7-hour soak against an iPhone IP-camera over Wi-Fi held flat
  RSS (46.3 MB) with zero RTSP timeouts / disconnects ([result](benchmarks/results/2026-04-28-rtsp-soak-1h7.md)).

## What's not done yet

- **Recall on public anomaly datasets.** No Avenue / ShanghaiTech /
  UCF-Crime numbers committed yet. The "drops 90% of frames" claim is
  motivated by prior work, not measured on our pipeline. Tracked as
  P0.7 in [docs/05.exec-plans/004-positioning.md](docs/05.exec-plans/004-positioning.md).
- **Multi-stream Tokio scheduler.** Today the density bench forks a
  Python worker per stream; the no-GIL claim against mv-extractor's
  multi-stream model is honest in shape but not yet measured. Tracked
  as P0.3.
- **24h+ soak on a real CCTV camera.** The 1.7h iPhone-IP-cam run is
  a smoke test, not the canonical acceptance bar.
- **AV1 / VP9.** Out of scope for 0.2; explicit error per
  [`CLAUDE.md`](CLAUDE.md).

## Status

Pre-alpha. No release. Things will move and break. The fovea-pick and
fovea-stream subprojects don't exist yet — only the road map does.

## License

Apache-2.0 — see [LICENSE](LICENSE).

The vendored libde265 (HEVC decoder) is LGPL-3.0-or-later; we link to
it dynamically. See [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md)
for the LGPL §4 source-shipping arrangement.

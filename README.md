# Fovea

Helpers for sending video to VLMs (Vision-Language Models) without
paying to look at every frame.

I'm trying this out as a side project. `main` is mostly empty
scaffolding. The actual work-in-progress lives on the
[`feat/mv-step2-h264-extraction`](https://github.com/hunhee98/fovea/tree/feat/mv-step2-h264-extraction)
branch and will land here only when it's stable enough.

## Subprojects (planned)

| Crate / Package | Status | What it would do |
|---|---|---|
| `fovea-mv` | working prototype on the feat branch | Reads H.264 / HEVC motion vectors and fires events on motion / scene change / heartbeat. |
| `fovea-pick` | planned | Pick K representative frames out of N inside a token budget. |
| `fovea-stream` | planned | Streaming wrapper around an HF VLM with KV-cache reuse. |

## Why bother

Most "video → VLM" pipelines just push 1 frame per second to the model
and wait. On a single 24/7 camera that ends up being thousands of
calls a day, most of which look nearly identical to each other.

Compressed video already carries cheap signals about *where* and
*when* something changed (motion vectors, intra-coding ratios, frame
types). You can use those signals to drop most frames before they
reach the VLM. This project is an experiment in stitching that idea
into a small library.

The numbers below are from prior work that motivated the design —
none of them are *our* numbers yet.

## Prior work this builds on

- **CoViAR** — Wu et al., CVPR 2018, [arXiv:1712.00636](https://arxiv.org/abs/1712.00636).
  Compressed-domain action recognition can be **4.6× faster than
  Res3D** at comparable accuracy.
- **Towards Scalable Modeling of Compressed Videos** — Biswas et al.,
  Purdue, [arXiv:2503.13724](https://arxiv.org/abs/2503.13724).
  **56× inference speedup, 330× cost reduction** vs pixel-domain on
  K-400 / K-600 / SS-v2.
- **LLaVA-Mini** — Zhang et al., [arXiv:2501.03895](https://arxiv.org/abs/2501.03895).
  Image-token count from **576 → 1** while preserving most of the
  quality.
- **StreamingVLM** — Xu et al., MIT Han Lab, [arXiv:2510.09608](https://arxiv.org/abs/2510.09608).
  Real-time long-video understanding with KV-cache reuse, **up to
  8 FPS on a single H100**.

## Status

Pre-alpha. No release. Things will move and break.

## License

Apache-2.0 — see [LICENSE](LICENSE).

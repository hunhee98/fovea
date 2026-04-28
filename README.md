# Fovea

Evidence-driven infrastructure OSS for video → VLM pipelines.

## Subprojects

| Crate / Package | Status | Role |
|---|---|---|
| `fovea-mv` | wip | H.264 / HEVC motion-vector trigger engine. Decides which frames deserve downstream processing. |
| `fovea-pick` | planned | Hybrid frame sampler (MV + CLIP/DINOv2 + token-budget). |
| `fovea-stream` | planned | Streaming VLM wrapper over HuggingFace VLMs. |

## Why this exists

Most "video → VLM" workflows naively send 1 frame per second to expensive VLM endpoints. 99% of those frames are redundant. Fovea uses cheap signals already present in compressed video (motion vectors, frame types) to gate downstream compute.

Cited prior work: see [docs/02.papers/](docs/02.papers/).

## Status

Pre-alpha. No release yet. See [docs/05.exec-plans/](docs/05.exec-plans/) for active work.

## License

Apache-2.0. See [LICENSE](LICENSE).

Vendored libde265 (HEVC decoder) is LGPL-3.0-or-later. See
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md) for details and the
LGPL §4 source-shipping requirement.

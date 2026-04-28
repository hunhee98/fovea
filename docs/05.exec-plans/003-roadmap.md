# 003 — fovea-mv roadmap (deferred items)

Status: roadmap — items pulled out of 001/002 that didn't ship in 0.1.0
Owner: @hunhee98

## What's done before this plan

See 001/002. fovea-mv 0.1.0 prototype works end-to-end on H.264 + HEVC,
file + RTSP.

## Tier 1 — small wins

| Item | Effort | Why |
|---|---|---|
| 24h+ soak run | overnight + analysis | only ~7 min run so far |
| Real IP camera (Hikvision/Dahua/Axis) | dependent on hardware | phone-cam is approximation |
| HEVC RGB decode (`event.decode()`) | ~1 day | currently errors for HEVC sources |
| HEVC PTS pass-through | ~few hrs | currently `ts_us=0` |

## Tier 2 — codec coverage

| Item | Effort | Notes |
|---|---|---|
| AV1 MV extraction | ~weeks | likely needs custom path; FFmpeg AV1 decoder doesn't expose MVs |
| VP9 MV extraction | similar | same gap as AV1 |

## Tier 3 — performance / scale

| Item | Effort | Notes |
|---|---|---|
| NVDEC / VideoToolbox GPU decode | ~2 weeks | for >30 streams/box |
| Multi-stream concurrency (Tokio) | ~1 week | currently caller manages threads |
| Onset/offset trigger | ~3-5 days | closes Step 5 recall gap |
| Trigger confidence score | ~1 day | event.confidence for downstream filtering |

## Tier 4 — release prep (only if going public)

| Item | Effort | Notes |
|---|---|---|
| GitHub Actions CI | ~half-day | cargo test + clippy + pytest + maturin wheels |
| Cross-platform wheels | ~1 day | linux x86_64 + aarch64 + macOS arm64 |
| Crates.io / PyPI publish | hours | only after stable claim |

## Phase 2 — `fovea-pick`

Hybrid frame sampler. CLIP/DINOv2 embedding diff + token-budget
selection. New domain. ~1-2 weeks.

## Phase 3 — `fovea-stream`

HF VLM streaming wrapper, KV-cache reuse (StreamingVLM pattern). ~2-3
weeks. Biggest piece. Real differentiator.

## Recommended sequence

1. (optional) overnight soak + log analysis
2. Phase 3 `fovea-stream` — maximum value
3. Phase 2 `fovea-pick` if/when needed for Phase 3
4. fovea-mv tier 3 if/when stream density forces it

Tiers 1, 2, 4 can be slotted opportunistically.

# Architecture overview

Fovea is a layered set of libraries for video → VLM pipelines. Each layer has a distinct role and ships as an independent crate / package.

## Layers

```
                     ┌────────────────────────────────────┐
   user code  ────▶  │  fovea-stream (Phase 3, planned)   │  Streaming VLM wrapper
                     └────────────────┬───────────────────┘
                                      │
                     ┌────────────────▼───────────────────┐
                     │  fovea-pick   (Phase 2, planned)   │  Hybrid frame picker
                     └────────────────┬───────────────────┘
                                      │
                     ┌────────────────▼───────────────────┐
                     │  fovea-trigger     (Phase 1, active)    │  Compressed-domain trigger
                     │  ┌──────────────────────────────┐  │
                     │  │ fovea-trigger-stream (sources)    │  │
                     │  │ fovea-trigger-core   (parse + MV) │  │
                     │  │ fovea-trigger-py     (PyO3)       │  │
                     │  └──────────────────────────────┘  │
                     └────────────────────────────────────┘
                                      │
                     [H.264 / H.265 packets from RTSP / file / HTTP]
```

## Layer roles

### fovea-trigger (Phase 1)

**Role:** decide which video frames deserve downstream compute.
**Input:** raw H.264 packets.
**Output:** events — timestamps + lazily-decodable frame handles.
**Cost:** sub-millisecond per packet on consumer CPUs (no decode in idle path).
**Key insight:** motion vectors are already computed by the encoder; we just read them.

### fovea-pick (Phase 2)

**Role:** select the best frames within a triggered window for a constrained token budget.
**Input:** events from fovea-trigger.
**Output:** ordered list of frames, capped by user-specified budget.
**Stages:** cheap (MV) → middle (CLIP/DINOv2 diff) → budget-aware selection.

### fovea-stream (Phase 3)

**Role:** wrap a HuggingFace VLM with streaming inference (sustained throughput on long video).
**Input:** frames from fovea-pick (or any source).
**Output:** VLM responses, query-time interactive.
**Key technique:** two-process split + KV cache reuse.

## Why the separation

Each layer is independently useful:
- A NVR project may use only `fovea-trigger` (no VLM).
- A research project may use only `fovea-stream` (frames already picked elsewhere).
- The VLM cost-reduction pitch is `fovea-trigger` + `fovea-pick` together.

This separation is enforced by crate boundaries — `fovea-trigger-core` does not depend on torch/onnx/candle (see `CLAUDE.md` → "Code rules").

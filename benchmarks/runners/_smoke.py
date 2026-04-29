"""Smoke-test the VLM client by calling it on a single frame from the sample.

Run from repo root: `python -m benchmarks.runners._smoke`
"""

from __future__ import annotations

import sys

from fovea_trigger import IntervalTrigger, Stream

from .vlm import VlmClient


def main() -> int:
    client = VlmClient()
    sample = "benchmarks/datasets/cctv-sample/sample.mp4"
    stream = Stream.from_file(sample)
    ev = next(iter(stream.events([IntervalTrigger(100_000)])))
    rgb = ev.decode()
    print(f"frame ts={ev.timestamp_s:.3f} shape={rgb.shape}")
    resp = client.call(rgb)
    print(f'people={resp.people} vehicles={resp.vehicles} event="{resp.event}"')
    print(f"tokens in={resp.input_tokens} out={resp.output_tokens} cost=${resp.cost_usd():.6f}")
    print(f"latency={resp.latency_ms:.1f}ms")
    return 0


if __name__ == "__main__":
    sys.exit(main())

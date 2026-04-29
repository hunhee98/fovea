"""Frame sampling strategies for the Step 5 benchmark.

Each strategy is a generator of `FrameSample(ts_us, rgb)` — RGB is
materialized eagerly inside the iteration (the underlying decoder advances
on every `__next__`, so lazy decode is not safe across calls).
"""

from __future__ import annotations

import dataclasses
from typing import Iterator

import numpy as np

from fovea_trigger import (
    IntervalTrigger,
    MotionTrigger,
    SceneChangeTrigger,
    Stream,
)


@dataclasses.dataclass
class FrameSample:
    ts_us: int
    rgb: np.ndarray
    trigger_name: str


# ----- Oracle -----

def oracle(stream: Stream, interval_ms: int = 1000) -> Iterator[FrameSample]:
    """Sample every `interval_ms` of clip time. Default 1 fps."""
    iv = IntervalTrigger(interval_ms)
    for ev in stream.events([iv]):
        yield FrameSample(ts_us=ev.ts_us, rgb=ev.decode(), trigger_name=ev.trigger_name)


# ----- Uniform -----

def uniform_fps(stream: Stream, fps: float) -> Iterator[FrameSample]:
    interval_ms = max(1, int(round(1000.0 / fps)))
    iv = IntervalTrigger(interval_ms)
    for ev in stream.events([iv]):
        yield FrameSample(ts_us=ev.ts_us, rgb=ev.decode(), trigger_name=ev.trigger_name)


# ----- fovea-trigger triggers -----

@dataclasses.dataclass
class FoveaMvConfig:
    motion_energy_threshold: int = 20_000
    motion_min_duration_ms: int = 0
    interval_max_gap_ms: int = 10_000
    scene_change_threshold: float = 0.6


def fovea_trigger(stream: Stream, cfg: FoveaMvConfig | None = None) -> Iterator[FrameSample]:
    cfg = cfg or FoveaMvConfig()
    triggers = [
        MotionTrigger(cfg.motion_energy_threshold, min_duration_ms=cfg.motion_min_duration_ms),
        IntervalTrigger(cfg.interval_max_gap_ms),
        SceneChangeTrigger(cfg.scene_change_threshold),
    ]
    for ev in stream.events(triggers):
        yield FrameSample(ts_us=ev.ts_us, rgb=ev.decode(), trigger_name=ev.trigger_name)

"""Smoke tests for the Python bindings.

Runs the public API end-to-end on the committed CCTV sample. Skipped when
the sample is not present (clean clones without datasets).
"""

from __future__ import annotations

import os
from pathlib import Path

import numpy as np
import pytest

import fovea_mv
from fovea_mv import (
    Event,
    IntervalTrigger,
    MotionTrigger,
    RegionMask,
    SceneChangeTrigger,
    Stream,
)

REPO_ROOT = Path(__file__).resolve().parents[3]
SAMPLE = REPO_ROOT / "benchmarks" / "datasets" / "cctv-sample" / "sample.mp4"


def _require_sample() -> Path:
    if not SAMPLE.exists():
        pytest.skip(f"sample missing at {SAMPLE}")
    return SAMPLE


def test_module_metadata() -> None:
    assert isinstance(fovea_mv.__version__, str)
    assert fovea_mv.core_version() == fovea_mv.__version__


def test_stream_open_and_info() -> None:
    path = _require_sample()
    s = Stream.from_file(str(path))
    info = s.info()
    assert info.width == 1080
    assert info.height == 1920
    assert abs(info.frame_rate - 30.0) < 0.01
    assert info.duration_s is not None
    assert 14.5 <= info.duration_s <= 15.5


def test_events_match_rust_counts() -> None:
    """Mirrors the Rust integration test trigger counts on the same clip."""
    path = _require_sample()
    s = Stream.from_file(str(path))
    triggers = [
        MotionTrigger(20_000),
        IntervalTrigger(2_000),
        SceneChangeTrigger(0.6),
    ]
    counts = {"motion": 0, "interval": 0, "scene_change": 0}
    for ev in s.events(triggers):
        assert isinstance(ev, Event)
        counts[ev.trigger_name] = counts.get(ev.trigger_name, 0) + 1
    assert counts["motion"] >= 1
    assert 7 <= counts["interval"] <= 9
    assert counts["scene_change"] <= 5


def test_decode_returns_uint8_rgb() -> None:
    path = _require_sample()
    s = Stream.from_file(str(path))
    triggers = [IntervalTrigger(100_000)]  # fire only on the very first frame
    it = iter(s.events(triggers))
    ev = next(it)
    rgb = ev.decode()
    info = s.info()
    assert rgb.dtype == np.uint8
    assert rgb.shape == (info.height, info.width, 3)
    assert rgb.size == info.height * info.width * 3
    # Sanity: not all zero / all 255.
    assert int(rgb.min()) < int(rgb.max())


def test_decode_after_advance_raises() -> None:
    path = _require_sample()
    s = Stream.from_file(str(path))
    it = iter(s.events([IntervalTrigger(50)]))
    first = next(it)
    next(it)  # advance past `first`
    with pytest.raises(RuntimeError):
        first.decode()


def test_fast_decode_kwarg_accepted() -> None:
    """`fast_decode=True` enables A' skip flags. MV output unchanged."""
    path = _require_sample()
    triggers_factory = lambda: [
        MotionTrigger(20_000),
        IntervalTrigger(2_000),
    ]
    a = sum(
        1 for _ in Stream.from_file(str(path), fast_decode=False).events(triggers_factory())
    )
    b = sum(
        1 for _ in Stream.from_file(str(path), fast_decode=True).events(triggers_factory())
    )
    assert a == b, f"event count diverged: default={a}, fast={b}"


def test_region_mask_filters() -> None:
    """Mask covering ~nothing (1×1 pixel) should suppress the motion trigger."""
    path = _require_sample()
    s = Stream.from_file(str(path))
    info = s.info()
    tiny = RegionMask(0, 0, 1, 1)
    triggers = [MotionTrigger(20_000, mask=tiny)]
    fires = sum(1 for _ in s.events(triggers))
    assert fires == 0, f"masked trigger fired {fires} times — mask ignored?"
    # Sanity: a mask covering the full frame should fire >= 1.
    s2 = Stream.from_file(str(path))
    full = RegionMask(0, 0, info.width, info.height)
    triggers_full = [MotionTrigger(20_000, mask=full)]
    fires_full = sum(1 for _ in s2.events(triggers_full))
    assert fires_full >= 1

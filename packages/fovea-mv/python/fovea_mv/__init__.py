"""fovea-mv: H.264 motion-vector trigger engine.

Open a video, iterate the events that the trigger pipeline fires, and forward
those events to a downstream model (e.g. a VLM) only when they fire.

Quick start::

    from fovea_mv import Stream, MotionTrigger, IntervalTrigger

    stream = Stream.from_file("clip.mp4")
    triggers = [
        MotionTrigger(energy_threshold=20_000),
        IntervalTrigger(max_gap_ms=10_000),
    ]
    for event in stream.events(triggers):
        rgb = event.decode()  # numpy uint8 (H, W, 3)
        # forward to VLM, save clip, ...

See `docs/05.exec-plans/001-mvtrigger-mvp.md` for design notes.
"""

from ._fovea_mv import (  # type: ignore[import-not-found]
    Event,
    EventIterator,
    FusionTrigger,
    IntervalTrigger,
    MotionTrigger,
    RegionMask,
    SceneChangeTrigger,
    SpatialClusterTrigger,
    Stream,
    VideoInfo,
    core_version,
)

__all__ = [
    "Event",
    "EventIterator",
    "FusionTrigger",
    "IntervalTrigger",
    "MotionTrigger",
    "RegionMask",
    "SceneChangeTrigger",
    "SpatialClusterTrigger",
    "Stream",
    "VideoInfo",
    "core_version",
]

__version__ = core_version()

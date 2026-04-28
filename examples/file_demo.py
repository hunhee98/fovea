"""Open a video file, run the trigger pipeline, save the first decoded frame.

Run from the repo root::

    python examples/file_demo.py path/to/clip.mp4

The script does not call any external service. It demonstrates the public
fovea-mv API end-to-end: open → events → decode → save.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from fovea_mv import (
    IntervalTrigger,
    MotionTrigger,
    SceneChangeTrigger,
    Stream,
)


def main() -> int:
    parser = argparse.ArgumentParser(description="fovea-mv file demo")
    parser.add_argument("video", type=Path, help="Path to an H.264 mp4 file")
    parser.add_argument(
        "--max-events", type=int, default=10, help="Stop after N events"
    )
    parser.add_argument(
        "--save-first-frame",
        type=Path,
        default=None,
        help="Optional path to save the first event's RGB as a PNG (requires Pillow)",
    )
    parser.add_argument(
        "--motion-threshold", type=int, default=20_000, help="MotionTrigger energy threshold"
    )
    parser.add_argument(
        "--interval-ms", type=int, default=10_000, help="IntervalTrigger gap (ms)"
    )
    parser.add_argument(
        "--scene-threshold",
        type=float,
        default=0.5,
        help="SceneChangeTrigger intra-block ratio threshold",
    )
    args = parser.parse_args()

    if not args.video.exists():
        print(f"video not found: {args.video}", file=sys.stderr)
        return 2

    stream = Stream.from_file(str(args.video))
    info = stream.info()
    print(
        f"opened {args.video.name}: "
        f"{info.width}x{info.height} @ {info.frame_rate:.2f} fps, duration={info.duration_s:.2f}s"
    )

    triggers = [
        MotionTrigger(args.motion_threshold),
        IntervalTrigger(args.interval_ms),
        SceneChangeTrigger(args.scene_threshold),
    ]
    print(f"triggers: {triggers}")

    saved_first = False
    for i, ev in enumerate(stream.events(triggers)):
        print(
            f"  [{i:03d}] t={ev.timestamp_s:7.3f}s  type={ev.frame_type}  trigger={ev.trigger_name:<13}"
            f"  energy={ev.energy:>10}  intra_ratio={ev.intra_ratio:.3f}  mvs={ev.mv_count}"
        )

        if not saved_first and args.save_first_frame is not None:
            try:
                from PIL import Image  # type: ignore[import-not-found]
            except ImportError:
                print("  (Pillow not installed; skipping save)")
            else:
                rgb = ev.decode()
                Image.fromarray(rgb).save(args.save_first_frame)
                print(f"  saved first frame → {args.save_first_frame}")
                saved_first = True

        if i + 1 >= args.max_events:
            print(f"  reached --max-events={args.max_events}")
            break

    return 0


if __name__ == "__main__":
    raise SystemExit(main())

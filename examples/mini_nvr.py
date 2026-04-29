"""Minimal NVR-style cascade in ~50 lines.

Open one RTSP / file source → run the FusionTrigger → only on fire,
decode the RGB frame and hand it to a downstream model. This is the
canonical "what does fovea-trigger buy me" demo: most frames never get
decoded, never reach the model. The model only sees what the
compressed-domain trigger said is worth a second look.

The downstream `process_frame` here is a stub that prints the frame's
shape — replace it with a YOLO call, a VLM POST, a CLIP embedding,
whatever. The important part is **how few times** it's called relative
to how many frames the source produced.

Run::

    # File source
    python examples/mini_nvr.py benchmarks/datasets/cctv-sample/sample.mp4

    # RTSP (live or MediaMTX loop)
    python examples/mini_nvr.py rtsp://localhost:8554/cctv-loop --max-seconds 60
"""

from __future__ import annotations

import argparse
import sys
import time

from fovea_trigger import FusionTrigger, IntervalTrigger, Stream


def process_frame(rgb, ts_s: float, reason: str) -> None:
    """Stand-in for whatever expensive thing wants the pixels.

    Replace this with a YOLO inference call, a CLIP embedding, a VLM
    HTTP request, etc. The point of fovea-trigger is that this function is
    invoked only on frames worth processing.
    """
    h, w, _ = rgb.shape
    print(f"  → process_frame  t={ts_s:6.2f}s  {w}x{h}  trigger={reason}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Mini NVR cascade demo")
    parser.add_argument("source", help="File path or rtsp:// URL")
    parser.add_argument("--max-seconds", type=float, default=30.0)
    parser.add_argument("--motion-threshold", type=int, default=200_000)
    parser.add_argument("--intra-threshold", type=float, default=0.05)
    parser.add_argument("--skip-suppress", type=float, default=0.95)
    args = parser.parse_args()

    if args.source.startswith(("rtsp://", "rtsps://", "http://", "https://")):
        stream = Stream.from_url(args.source, max_reconnects=3)
    else:
        stream = Stream.from_file(args.source)
    info = stream.info()
    print(f"opened: {info.width}x{info.height} @ {info.frame_rate:.1f} fps")

    trigger = FusionTrigger(
        "any",
        motion_threshold=args.motion_threshold,
        intra_threshold=args.intra_threshold,
        skip_suppress=args.skip_suppress,
    )
    heartbeat = IntervalTrigger(60_000)  # one event per minute even when idle

    t_start = time.time()
    fires = 0
    for event in stream.events([trigger, heartbeat]):
        if event.trigger_name == "interval":
            print(f"  · idle heartbeat at t={event.timestamp_s:6.2f}s")
            continue
        try:
            rgb = event.decode()
        except RuntimeError:
            # Iterator advanced past the event before we could decode.
            # Tighten the cooldown or pull events faster if this fires.
            continue
        process_frame(rgb, event.timestamp_s, event.trigger_name)
        fires += 1
        if time.time() - t_start >= args.max_seconds:
            break

    elapsed = time.time() - t_start
    print(f"\nfired {fires} events over {elapsed:.1f}s wall — "
          f"that's how many times the downstream model was called.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

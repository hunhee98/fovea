"""Open an RTSP stream, run the trigger pipeline, print events.

Usage::

    # Real camera
    python examples/rtsp_demo.py --rtsp rtsp://user:pass@cam.local:554/stream

    # Local MediaMTX server (see docs/05.exec-plans/002-rtsp-source.md)
    python examples/rtsp_demo.py --rtsp rtsp://localhost:8554/cctv-loop

The demo prints one line per event. Live sources never reach EOF;
press Ctrl-C to stop, or pass --max-events / --max-seconds.
"""

from __future__ import annotations

import argparse
import sys
import time
from pathlib import Path

from fovea_mv import (
    IntervalTrigger,
    MotionTrigger,
    SceneChangeTrigger,
    Stream,
)


def main() -> int:
    parser = argparse.ArgumentParser(description="fovea-mv RTSP demo")
    parser.add_argument("--rtsp", required=True, help="RTSP URL to open")
    parser.add_argument("--transport", default="tcp", choices=["tcp", "udp"])
    parser.add_argument("--open-timeout-s", type=float, default=5.0)
    parser.add_argument("--read-timeout-s", type=float, default=10.0)
    parser.add_argument("--max-reconnects", type=int, default=3)
    parser.add_argument("--fast-decode", action="store_true")
    parser.add_argument("--max-events", type=int, default=20)
    parser.add_argument(
        "--max-seconds",
        type=float,
        default=60.0,
        help="Stop after this much wall-clock time even if events keep flowing.",
    )
    parser.add_argument(
        "--motion-per-mb",
        type=float,
        default=20.0,
        help="Per-macroblock motion-energy threshold (resolution-independent).",
    )
    parser.add_argument("--interval-ms", type=int, default=10_000)
    parser.add_argument("--scene-threshold", type=float, default=0.5)
    parser.add_argument(
        "--save-first-frame",
        type=Path,
        default=None,
        help="Optional path to save the first event's RGB frame as PNG (requires Pillow).",
    )
    args = parser.parse_args()

    print(f"opening {args.rtsp}", file=sys.stderr)
    stream = Stream.from_url(
        args.rtsp,
        transport=args.transport,
        open_timeout_s=args.open_timeout_s,
        read_timeout_s=args.read_timeout_s,
        max_reconnects=args.max_reconnects,
        fast_decode=args.fast_decode,
    )
    info = stream.info()
    print(
        f"opened: {info.width}x{info.height} @ {info.frame_rate:.2f} fps",
        file=sys.stderr,
    )

    triggers = [
        MotionTrigger.per_mb(args.motion_per_mb),
        IntervalTrigger(args.interval_ms),
        SceneChangeTrigger(args.scene_threshold),
    ]
    print(f"triggers: {triggers}", file=sys.stderr)

    saved_first = False
    start = time.time()

    try:
        for i, ev in enumerate(stream.events(triggers)):
            elapsed = time.time() - start
            print(
                f"[{i:03d}] t={ev.timestamp_s:8.3f}s  type={ev.frame_type}  "
                f"trigger={ev.trigger_name:<13}  energy={ev.energy:>10}  "
                f"intra_ratio={ev.intra_ratio:.3f}  mvs={ev.mv_count}",
                flush=True,
            )

            if not saved_first and args.save_first_frame is not None:
                try:
                    from PIL import Image  # type: ignore[import-not-found]
                except ImportError:
                    print("  (Pillow not installed; skipping save)", file=sys.stderr)
                else:
                    rgb = ev.decode()
                    Image.fromarray(rgb).save(args.save_first_frame)
                    print(
                        f"  saved first frame → {args.save_first_frame}",
                        file=sys.stderr,
                    )
                    saved_first = True

            if i + 1 >= args.max_events:
                print(f"reached --max-events={args.max_events}", file=sys.stderr)
                break
            if elapsed >= args.max_seconds:
                print(f"reached --max-seconds={args.max_seconds}", file=sys.stderr)
                break
    except KeyboardInterrupt:
        print("interrupted", file=sys.stderr)
    except ConnectionError as e:
        print(f"disconnected: {e}", file=sys.stderr)
        return 1
    except TimeoutError as e:
        print(f"read timeout: {e}", file=sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())

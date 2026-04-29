"""Live RTSP demo with on-screen trigger feedback.

Opens an RTSP URL, runs the trigger pipeline, displays each decoded
frame in a window, and overlays a red banner when a MotionTrigger or
SceneChangeTrigger fires.

Run on your local machine — the OpenCV window pops up on your desktop:

    python examples/live_view.py \
        --rtsp rtsp://USER:PASS@CAMERA_IP:8554/live

Press ESC or `q` to quit. Press `r` to reset the trigger banner timeout.
"""

from __future__ import annotations

import argparse
import time

import cv2  # type: ignore[import-not-found]
import numpy as np

from fovea_trigger import (
    IntervalTrigger,
    MotionTrigger,
    SceneChangeTrigger,
    Stream,
)


BANNER_COLOURS = {
    "motion": (0, 0, 255),       # red (BGR)
    "scene_change": (0, 165, 255),  # orange
    "interval": (200, 200, 200),    # grey (heartbeat — calm)
}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--rtsp", required=True)
    parser.add_argument("--transport", default="tcp", choices=["tcp", "udp"])
    parser.add_argument("--motion-per-mb", type=float, default=5.0)
    parser.add_argument("--interval-ms", type=int, default=10_000)
    parser.add_argument("--scene-threshold", type=float, default=0.5)
    parser.add_argument(
        "--banner-ms",
        type=int,
        default=1500,
        help="How long the trigger banner stays on screen after a fire.",
    )
    args = parser.parse_args()

    print(f"opening {args.rtsp}")
    stream = Stream.from_url(
        args.rtsp,
        transport=args.transport,
        open_timeout_s=5.0,
        read_timeout_s=10.0,
        max_reconnects=3,
    )
    info = stream.info()
    print(f"stream: {info.width}x{info.height} @ {info.frame_rate:.1f} fps")

    triggers = [
        MotionTrigger.per_mb(args.motion_per_mb),
        IntervalTrigger(args.interval_ms),
        SceneChangeTrigger(args.scene_threshold),
    ]

    win = "fovea-trigger live"
    cv2.namedWindow(win, cv2.WINDOW_NORMAL)
    cv2.resizeWindow(win, info.width // 2, info.height // 2)

    last_fire_at = 0.0
    last_fire_msg = ""
    last_fire_colour = (200, 200, 200)
    banner_seconds = args.banner_ms / 1000.0

    fired_n = 0
    started = time.time()

    try:
        for ev in stream.events(triggers):
            now = time.time()
            fired_n += 1
            last_fire_at = now
            last_fire_msg = (
                f"{ev.trigger_name.upper()}  "
                f"e={ev.energy}  ratio={ev.intra_ratio:.2f}  mvs={ev.mv_count}"
            )
            last_fire_colour = BANNER_COLOURS.get(ev.trigger_name, (200, 200, 200))

            rgb = ev.decode()  # numpy uint8 (H, W, 3) RGB
            bgr = cv2.cvtColor(rgb, cv2.COLOR_RGB2BGR)

            # Banner if recent.
            if (now - last_fire_at) < banner_seconds:
                h, w = bgr.shape[:2]
                cv2.rectangle(bgr, (0, 0), (w, 60), last_fire_colour, -1)
                cv2.putText(
                    bgr,
                    last_fire_msg,
                    (16, 42),
                    cv2.FONT_HERSHEY_SIMPLEX,
                    1.0,
                    (255, 255, 255),
                    2,
                    cv2.LINE_AA,
                )

            # Footer with running counters.
            elapsed = now - started
            footer = (
                f"events={fired_n}  elapsed={elapsed:.1f}s  "
                f"frame_t={ev.timestamp_s:.2f}s  type={ev.frame_type}"
            )
            h, w = bgr.shape[:2]
            cv2.rectangle(bgr, (0, h - 36), (w, h), (0, 0, 0), -1)
            cv2.putText(
                bgr,
                footer,
                (12, h - 12),
                cv2.FONT_HERSHEY_SIMPLEX,
                0.6,
                (255, 255, 255),
                1,
                cv2.LINE_AA,
            )

            cv2.imshow(win, bgr)
            key = cv2.waitKey(1) & 0xFF
            if key in (27, ord("q")):  # ESC or 'q'
                break
            if key == ord("r"):
                last_fire_at = 0.0
    except KeyboardInterrupt:
        pass
    finally:
        cv2.destroyAllWindows()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

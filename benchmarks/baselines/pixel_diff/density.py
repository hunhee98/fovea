"""Pixel-diff baseline density runner.

Mirror image of `benchmarks/runners/density.py` but with the fovea-mv
trigger pipeline replaced by a classical pixel-diff motion detector
operating on fully-decoded frames. Same hardware capture, same CPU /
RSS / throughput / latency instrumentation, same multiprocessing
structure, same `--source` / `--streams` / `--duration-s` CLI shape.
The only thing that changes is what runs inside the worker.

What "pixel-diff" means here:

    decode each frame  →  convert to grayscale  →  absdiff vs previous frame
    sum the absolute differences over the whole frame
    fire the trigger if the sum is above `--threshold`

This is the textbook OSS first-stage motion detector — Frigate's
`motion-mask + frame_diff` path, Shinobi's `pam`, Viseron's
`background_subtractor`. We implement it once here so fovea-mv's
"compressed-domain, no-decode-in-idle-path" claim has something to
beat (or be beaten by) on identical hardware.

Usage::

    python -m benchmarks.baselines.pixel_diff.density \\
        --source benchmarks/datasets/cctv-sample/sample.mp4 \\
        --streams 1,4,16 \\
        --duration-s 30
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import multiprocessing as mp
import os
import platform
import subprocess
import sys
import time
from pathlib import Path

import cv2  # type: ignore[import-untyped]
import numpy as np
import psutil


# ---------------------------------------------------------------------------
# Worker.
# ---------------------------------------------------------------------------


@dataclasses.dataclass
class WorkerSamples:
    pid: int
    cpu_pct: list[float]
    rss_mb: list[float]
    trigger_latencies_ms: list[float]
    packets: int                     # frames consumed
    fires: int                       # times the diff threshold was crossed
    wall_s: float
    clip_duration_s: float
    loops: int


def _worker(
    source: str,
    duration_s: float,
    threshold: float,
    sample_period_s: float,
    out_q: "mp.Queue[WorkerSamples]",
) -> None:
    """Decode every frame of `source`, compute consecutive-frame
    absolute-diff sum, fire when it exceeds `threshold`. Loop the clip
    until `duration_s` of wall time has elapsed."""
    p = psutil.Process()
    p.cpu_percent(None)

    cpu_samples: list[float] = []
    rss_samples: list[float] = []
    latencies_ms: list[float] = []

    # Capture native clip duration via OpenCV's metadata.
    probe = cv2.VideoCapture(source)
    if not probe.isOpened():
        out_q.put(
            WorkerSamples(
                pid=os.getpid(),
                cpu_pct=[],
                rss_mb=[],
                trigger_latencies_ms=[],
                packets=0,
                fires=0,
                wall_s=0.0,
                clip_duration_s=0.0,
                loops=0,
            )
        )
        return
    fps = probe.get(cv2.CAP_PROP_FPS) or 0.0
    n_frames = probe.get(cv2.CAP_PROP_FRAME_COUNT) or 0.0
    clip_duration_s = (n_frames / fps) if (fps > 0 and n_frames > 0) else 0.0
    probe.release()

    t_start = time.time()
    t_end = t_start + duration_s
    next_sample = t_start
    packets = 0
    fires = 0
    last_event_wall = t_start
    loops = 0

    while time.time() < t_end:
        cap = cv2.VideoCapture(source)
        if not cap.isOpened():
            break
        loops += 1
        prev_gray: np.ndarray | None = None
        while True:
            now = time.time()
            if now > t_end:
                break

            ok, frame = cap.read()
            if not ok or frame is None:
                # End of clip — break to outer loop to re-open and continue.
                break
            packets += 1

            gray = cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY)
            if prev_gray is not None:
                # `cv2.absdiff` returns a uint8 ndarray of |a-b| pixel-wise.
                # `.sum()` gives a single u64; for a 1080p frame the maximum
                # possible value is ~ 255 * 1920 * 1080 ≈ 5.3e8.
                diff_sum = float(np.sum(cv2.absdiff(gray, prev_gray)))
                if diff_sum >= threshold:
                    latencies_ms.append((now - last_event_wall) * 1000.0)
                    last_event_wall = now
                    fires += 1
            prev_gray = gray

            if now >= next_sample:
                cpu_samples.append(p.cpu_percent(None))
                rss_samples.append(p.memory_info().rss / (1024 * 1024))
                next_sample = now + sample_period_s
        cap.release()

    if not cpu_samples:
        cpu_samples.append(p.cpu_percent(None))
        rss_samples.append(p.memory_info().rss / (1024 * 1024))

    out_q.put(
        WorkerSamples(
            pid=os.getpid(),
            cpu_pct=cpu_samples,
            rss_mb=rss_samples,
            trigger_latencies_ms=latencies_ms,
            packets=packets,
            fires=fires,
            wall_s=time.time() - t_start,
            clip_duration_s=clip_duration_s,
            loops=loops,
        )
    )


# ---------------------------------------------------------------------------
# Aggregation — mirrors benchmarks/runners/density.py.
# ---------------------------------------------------------------------------


def _percentile(values: list[float], pct: float) -> float:
    if not values:
        return 0.0
    s = sorted(values)
    idx = max(0, min(len(s) - 1, int(round(pct / 100.0 * (len(s) - 1)))))
    return s[idx]


@dataclasses.dataclass
class StageResult:
    n_streams: int
    cpu_mean_per_stream: float
    cpu_p95_per_stream: float
    cpu_realtime_per_stream: float
    rss_mean_per_stream_mb: float
    fires_per_stream: float
    latency_p50_ms: float
    latency_p95_ms: float
    latency_p99_ms: float
    wall_mean_s: float
    throughput_x: float


def _aggregate(samples: list[WorkerSamples], n: int) -> StageResult:
    all_cpu = [c for s in samples for c in s.cpu_pct]
    all_lat = [ms for s in samples for ms in s.trigger_latencies_ms]
    rss_means = [sum(s.rss_mb) / len(s.rss_mb) for s in samples if s.rss_mb]
    cpu_means = [sum(s.cpu_pct) / len(s.cpu_pct) for s in samples if s.cpu_pct]
    cpu_mean = (sum(cpu_means) / len(cpu_means)) if cpu_means else 0.0

    throughputs = [
        (s.clip_duration_s * s.loops) / s.wall_s
        for s in samples
        if s.wall_s > 0 and s.clip_duration_s > 0 and s.loops > 0
    ]
    throughput_x = (sum(throughputs) / len(throughputs)) if throughputs else 0.0
    cpu_realtime = (cpu_mean / throughput_x) if throughput_x > 0 else 0.0

    return StageResult(
        n_streams=n,
        cpu_mean_per_stream=cpu_mean,
        cpu_p95_per_stream=_percentile(all_cpu, 95.0),
        cpu_realtime_per_stream=cpu_realtime,
        rss_mean_per_stream_mb=(sum(rss_means) / len(rss_means)) if rss_means else 0.0,
        fires_per_stream=sum(s.fires for s in samples) / max(1, n),
        latency_p50_ms=_percentile(all_lat, 50.0),
        latency_p95_ms=_percentile(all_lat, 95.0),
        latency_p99_ms=_percentile(all_lat, 99.0),
        wall_mean_s=sum(s.wall_s for s in samples) / max(1, n),
        throughput_x=throughput_x,
    )


# ---------------------------------------------------------------------------
# Hardware capture — copy of density.py's helper so this baseline can run
# without depending on benchmarks.runners.* internals.
# ---------------------------------------------------------------------------


def _hardware_line() -> str:
    cpu = platform.processor() or platform.machine()
    cores = os.cpu_count() or 0
    ram_gb = round(psutil.virtual_memory().total / (1024**3))
    osver = f"{platform.system()} {platform.release()}"
    if platform.system() == "Darwin":
        try:
            brand = subprocess.run(
                ["sysctl", "-n", "machdep.cpu.brand_string"],
                capture_output=True,
                text=True,
                timeout=2,
            ).stdout.strip()
            if brand:
                cpu = brand
        except (FileNotFoundError, subprocess.TimeoutExpired):
            pass
    return f"{cpu} ({cores}-core), {ram_gb} GB, GPU none (CPU-only path), {osver}, opencv {cv2.__version__}"


# ---------------------------------------------------------------------------
# Driver.
# ---------------------------------------------------------------------------


def run_stage(source: str, n: int, duration_s: float, threshold: float) -> StageResult:
    ctx = mp.get_context("spawn")
    q: "mp.Queue[WorkerSamples]" = ctx.Queue()
    procs: list[mp.Process] = []
    for _ in range(n):
        p = ctx.Process(
            target=_worker,
            args=(source, duration_s, threshold, 0.1, q),
            daemon=False,
        )
        p.start()
        procs.append(p)

    samples: list[WorkerSamples] = []
    deadline = time.time() + duration_s + 30.0
    for _ in procs:
        remaining = max(1.0, deadline - time.time())
        try:
            samples.append(q.get(timeout=remaining))
        except Exception:
            break
    for p in procs:
        p.join(timeout=10.0)
        if p.is_alive():
            p.terminate()

    return _aggregate(samples, n)


def main() -> int:
    parser = argparse.ArgumentParser(description="pixel-diff density baseline")
    parser.add_argument("--source", required=True, help="Path to a sample clip (any codec OpenCV can decode).")
    parser.add_argument("--streams", default="1,4,16", help="Comma-separated stream counts to sweep.")
    parser.add_argument("--duration-s", type=float, default=30.0)
    parser.add_argument(
        "--threshold",
        type=float,
        default=5_000_000.0,
        help="Sum-of-abs-diff threshold for a fire. ~255*W*H is the hard cap; "
        "5e6 is a sane default for 1080p quiet scenes.",
    )
    parser.add_argument("--run-id", default=time.strftime("pixeldiff-%Y%m%d-%H%M%S"))
    args = parser.parse_args()

    src = Path(args.source).resolve()
    if not src.exists():
        print(f"source not found: {src}", file=sys.stderr)
        return 2

    stages = [int(x) for x in args.streams.split(",") if x.strip()]
    runs_dir = Path(__file__).resolve().parents[3] / "benchmarks" / "_runs"
    out_dir = runs_dir / args.run_id
    out_dir.mkdir(parents=True, exist_ok=True)

    hardware = _hardware_line()
    print(f"hardware: {hardware}")
    print(f"source:   {src} ({src.stat().st_size / (1024 * 1024):.1f} MB)")
    print(f"params:   duration={args.duration_s}s threshold={args.threshold:.0f}")
    print()
    print(f"{'streams':>8} | {'cpu%(flat)':>10} | {'cpu%(rt est)':>12} | {'thrput x':>8} | {'rss MB':>7} | {'fires':>5}")
    print("-" * 70)

    results: list[StageResult] = []
    for n in stages:
        r = run_stage(str(src), n, args.duration_s, args.threshold)
        results.append(r)
        print(
            f"{r.n_streams:>8} | "
            f"{r.cpu_mean_per_stream:>10.1f} | "
            f"{r.cpu_realtime_per_stream:>12.1f} | "
            f"{r.throughput_x:>8.2f} | "
            f"{r.rss_mean_per_stream_mb:>7.1f} | "
            f"{r.fires_per_stream:>5.0f}"
        )

    summary = {
        "hardware": hardware,
        "source": str(src),
        "duration_s": args.duration_s,
        "threshold": args.threshold,
        "stages": [dataclasses.asdict(r) for r in results],
    }
    (out_dir / "summary.json").write_text(json.dumps(summary, indent=2))
    print(f"\nsummary: {out_dir / 'summary.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

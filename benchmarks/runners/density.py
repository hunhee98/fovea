"""Density benchmark — sustained CPU% / RSS / trigger latency at N concurrent streams.

Measures the structural claim: "fovea-trigger does not decode in the idle path,
so per-stream cost stays roughly flat as N grows."

Each stream is a separate process running fovea-trigger with a high motion
threshold and a long interval (so the trigger rarely fires; we are
measuring the cheap parse path). psutil samples each worker at 100ms
and aggregates over the run window.

Usage::

    python -m benchmarks.runners.density \\
        --source benchmarks/datasets/cctv-sample/sample.mp4 \\
        --streams 1,4,16 \\
        --duration-s 30

The runner does not write to benchmarks/results/ automatically — that is
left to the caller (or to bench-runner agent), so the user can decide
which result file to commit.
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

import psutil

from fovea_trigger import IntervalTrigger, MotionTrigger, Stream

from .env import RUNS_DIR, macmon_sample, macmon_summary


# ---------------------------------------------------------------------------
# Worker — runs in each subprocess.
# ---------------------------------------------------------------------------


@dataclasses.dataclass
class WorkerSamples:
    pid: int
    cpu_pct: list[float]            # one sample per 100 ms
    rss_mb: list[float]             # same cadence
    trigger_latencies_ms: list[float]  # per trigger fire
    packets: int
    fires: int
    wall_s: float
    clip_duration_s: float          # native clip duration (for realtime extrapolation)
    loops: int                      # how many times the clip was looped during the run


def _worker(
    source: str,
    duration_s: float,
    motion_threshold: int,
    interval_ms: int,
    sample_period_s: float,
    out_q: "mp.Queue[WorkerSamples]",
) -> None:
    """Run one fovea Stream in this process, sampling self CPU% / RSS.

    Loops the clip until duration_s of wall time is reached. With a
    typical short bench clip (~15 s) this gives stable samples.
    """
    p = psutil.Process()
    p.cpu_percent(None)  # prime the counter; first call returns 0.0

    cpu_samples: list[float] = []
    rss_samples: list[float] = []
    latencies_ms: list[float] = []

    triggers_factory = lambda: [
        MotionTrigger(motion_threshold),
        IntervalTrigger(interval_ms),
    ]

    # Capture native clip duration once.
    info = Stream.from_file(source).info()
    clip_duration_s = float(info.duration_s) if info.duration_s is not None else 0.0

    t_start = time.time()
    t_end = t_start + duration_s
    next_sample = t_start
    packets = 0
    fires = 0
    last_event_wall = t_start
    loops = 0

    while time.time() < t_end:
        stream = Stream.from_file(source)
        loops += 1
        for ev in stream.events(triggers_factory()):
            now = time.time()
            packets += 1
            latencies_ms.append((now - last_event_wall) * 1000.0)
            last_event_wall = now
            fires += 1

            if now >= next_sample:
                cpu_samples.append(p.cpu_percent(None))
                rss_samples.append(p.memory_info().rss / (1024 * 1024))
                next_sample = now + sample_period_s

            if now > t_end:
                break
        # Sample at clip boundary even if no fires this loop.
        now = time.time()
        if now >= next_sample:
            cpu_samples.append(p.cpu_percent(None))
            rss_samples.append(p.memory_info().rss / (1024 * 1024))
            next_sample = now + sample_period_s

    # Flush a final sample window if we exited without one.
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
# Aggregation.
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
    cpu_mean_per_stream: float          # observed mean while running flat-out
    cpu_p95_per_stream: float
    cpu_realtime_per_stream: float      # extrapolated to 1× playback (see note in result file)
    rss_mean_per_stream_mb: float
    fires_per_stream: float
    latency_p50_ms: float
    latency_p95_ms: float
    latency_p99_ms: float
    wall_mean_s: float
    throughput_x: float                  # how many × realtime each worker processed


def _aggregate(samples: list[WorkerSamples], n: int) -> StageResult:
    all_cpu = [c for s in samples for c in s.cpu_pct]
    all_lat = [ms for s in samples for ms in s.trigger_latencies_ms]
    rss_means = [sum(s.rss_mb) / len(s.rss_mb) for s in samples if s.rss_mb]
    cpu_means = [sum(s.cpu_pct) / len(s.cpu_pct) for s in samples if s.cpu_pct]
    cpu_mean = (sum(cpu_means) / len(cpu_means)) if cpu_means else 0.0

    # Throughput = (clip_duration × loops) / wall_s, averaged across workers.
    throughputs = [
        (s.clip_duration_s * s.loops) / s.wall_s
        for s in samples
        if s.wall_s > 0 and s.clip_duration_s > 0 and s.loops > 0
    ]
    throughput_x = (sum(throughputs) / len(throughputs)) if throughputs else 0.0

    # Realtime-equivalent CPU = observed_cpu / throughput_x. If the worker
    # processed 6× realtime at 95% CPU, then at 1× realtime it would use
    # ~16% CPU (assuming the parse path is the dominant cost, which is the
    # whole point of fovea-trigger).
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
# Hardware capture.
# ---------------------------------------------------------------------------


def _hardware_line() -> str:
    cpu = platform.processor() or platform.machine()
    cores = os.cpu_count() or 0
    ram_gb = round(psutil.virtual_memory().total / (1024**3))
    osver = f"{platform.system()} {platform.release()}"
    rust = "?"
    try:
        rust = (
            subprocess.run(
                ["rustc", "--version"], capture_output=True, text=True, timeout=2
            ).stdout.strip()
            or "?"
        )
    except (FileNotFoundError, subprocess.TimeoutExpired):
        pass

    # Try sysctl for the actual brand on macOS.
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

    return f"{cpu} ({cores}-core), {ram_gb} GB, GPU none (CPU-only path), {osver}, {rust}"


# ---------------------------------------------------------------------------
# Driver.
# ---------------------------------------------------------------------------


def run_stage(source: str, n: int, duration_s: float, motion_threshold: int, interval_ms: int) -> StageResult:
    ctx = mp.get_context("spawn")
    q: mp.Queue[WorkerSamples] = ctx.Queue()
    procs: list[mp.Process] = []
    for _ in range(n):
        p = ctx.Process(
            target=_worker,
            args=(source, duration_s, motion_threshold, interval_ms, 0.1, q),
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
    parser = argparse.ArgumentParser(description="fovea density bench")
    parser.add_argument("--source", required=True, help="Path to a sample clip (H.264 mp4 or HEVC).")
    parser.add_argument("--streams", default="1,4,16", help="Comma-separated stream counts to sweep.")
    parser.add_argument("--duration-s", type=float, default=30.0)
    parser.add_argument("--motion-threshold", type=int, default=10_000_000,
                        help="High default — minimize trigger fires so we measure the parse path.")
    parser.add_argument("--interval-ms", type=int, default=60_000,
                        help="Long heartbeat interval — same reason.")
    parser.add_argument("--run-id", default=time.strftime("density-%Y%m%d-%H%M%S"))
    args = parser.parse_args()

    src = Path(args.source).resolve()
    if not src.exists():
        print(f"source not found: {src}", file=sys.stderr)
        return 2

    stages = [int(x) for x in args.streams.split(",") if x.strip()]
    out_dir = RUNS_DIR / args.run_id
    out_dir.mkdir(parents=True, exist_ok=True)

    hardware = _hardware_line()
    macmon_pre = macmon_sample()
    pre_summary = macmon_summary(macmon_pre)
    print(f"hardware: {hardware}")
    print(f"source:   {src} ({src.stat().st_size / (1024 * 1024):.1f} MB)")
    print(f"params:   duration={args.duration_s}s motion_th={args.motion_threshold} interval={args.interval_ms}ms")
    if pre_summary:
        print(f"thermals (pre-run): {pre_summary}")
    print()
    print(f"{'streams':>8} | {'cpu%(flat)':>10} | {'cpu%(rt est)':>12} | {'thrput x':>8} | {'rss MB':>7} | {'thermals':>30}")
    print("-" * 90)

    results: list[StageResult] = []
    macmon_per_stage: list[dict | None] = []
    for n in stages:
        r = run_stage(
            str(src),
            n,
            args.duration_s,
            args.motion_threshold,
            args.interval_ms,
        )
        # Sample macmon right after the stage finishes — captures the
        # thermal state the stage drove the box to. Cheap (one CLI call).
        post = macmon_sample()
        macmon_per_stage.append(post)
        results.append(r)
        print(
            f"{r.n_streams:>8} | "
            f"{r.cpu_mean_per_stream:>10.1f} | "
            f"{r.cpu_realtime_per_stream:>12.1f} | "
            f"{r.throughput_x:>8.1f} | "
            f"{r.rss_mean_per_stream_mb:>7.1f} | "
            f"{macmon_summary(post):>30}"
        )

    print()
    print("note: cpu%(flat) = observed while parsing as fast as possible.")
    print("      cpu%(rt est) = cpu%(flat) / throughput_x — extrapolated 1× playback cost.")
    print("      Trigger latency is intentionally omitted; current API only exposes fires,")
    print("      not per-packet timing. For latency see latency.md (criterion micro-bench).")

    summary = {
        "hardware": hardware,
        "source": str(src),
        "duration_s": args.duration_s,
        "motion_threshold": args.motion_threshold,
        "interval_ms": args.interval_ms,
        "stages": [dataclasses.asdict(r) for r in results],
        "macmon_pre": macmon_pre,
        "macmon_per_stage": macmon_per_stage,
    }
    (out_dir / "summary.json").write_text(json.dumps(summary, indent=2))
    print()
    print(f"raw: {out_dir}/summary.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())

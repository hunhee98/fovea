"""Compare strategy outputs from a benchmark run.

Loads the JSON files produced by `benchmarks.runners.run`, defines an event
ground truth from the Oracle, then scores each baseline against it.

Event ground-truth definition (this clip / round):
- A 1-second window in clip time is an "event window" iff the Oracle's call
  inside that window has `people=True` OR `vehicles=True`.
- A baseline call "covers" an event window if the call's `ts_us` falls
  within ±tolerance_ms of the window center.

Reported per baseline:
- call count
- total cost
- coverage = fraction of event windows that have at least one baseline call within tolerance
- precision = fraction of baseline calls that landed inside an event window
- mean call latency

Usage:
    python -m benchmarks.analysis.compare benchmarks/_runs/<run-id>
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import sys
from pathlib import Path
from typing import Dict, List


@dataclasses.dataclass
class Call:
    ts_us: int
    trigger_name: str
    people: bool
    vehicles: bool
    event: str
    input_tokens: int
    output_tokens: int
    latency_ms: float
    cost_usd: float


def load_strategy(path: Path) -> tuple[str, List[Call], float]:
    d = json.loads(path.read_text())
    calls = [Call(**c) for c in d["calls"]]
    return d["name"], calls, d.get("wall_clock_s", 0.0)


def windows_from_oracle(oracle_calls: List[Call], window_ms: int) -> List[tuple[int, bool]]:
    """One window per oracle call. Returns (center_ts_us, is_event)."""
    out = []
    for c in oracle_calls:
        is_event = bool(c.people or c.vehicles)
        out.append((c.ts_us, is_event))
    return out


def score_baseline(
    baseline: List[Call],
    windows: List[tuple[int, bool]],
    tolerance_ms: int,
) -> Dict[str, float]:
    tol_us = tolerance_ms * 1_000
    event_windows = [w for w in windows if w[1]]

    # Coverage of event windows.
    covered = 0
    for cw, _ in event_windows:
        if any(abs(c.ts_us - cw) <= tol_us for c in baseline):
            covered += 1
    coverage = covered / len(event_windows) if event_windows else float("nan")

    # Precision: how many baseline calls hit some event window.
    inside = 0
    for c in baseline:
        if any(w[1] and abs(c.ts_us - w[0]) <= tol_us for w in windows):
            inside += 1
    precision = inside / len(baseline) if baseline else float("nan")

    return {"coverage": coverage, "precision": precision}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("run_dir", type=Path)
    parser.add_argument("--tolerance-ms", type=int, default=1500)
    args = parser.parse_args()

    if not args.run_dir.is_dir():
        print(f"not a directory: {args.run_dir}", file=sys.stderr)
        return 2

    summary = json.loads((args.run_dir / "summary.json").read_text())
    print(f"run: {args.run_dir.name}  clip: {summary['clip']}")
    print(f"fovea config: {summary['fovea_config']}")
    print()

    strategies: Dict[str, tuple[List[Call], float]] = {}
    for f in sorted(args.run_dir.glob("*.json")):
        if f.name == "summary.json":
            continue
        name, calls, wall = load_strategy(f)
        strategies[name] = (calls, wall)

    if "oracle_1fps" not in strategies:
        print("oracle_1fps missing", file=sys.stderr)
        return 2
    oracle_calls, _ = strategies["oracle_1fps"]
    windows = windows_from_oracle(oracle_calls, window_ms=1000)
    n_windows = len(windows)
    n_events = sum(1 for _, e in windows if e)
    print(f"oracle windows: {n_windows} total, {n_events} event-positive (people OR vehicles)")
    print()

    headers = ["strategy", "calls", "vs oracle", "cost USD", "coverage", "precision", "mean lat ms"]
    rows = []
    oracle_n = len(oracle_calls)
    for name in ["oracle_1fps", "uniform_1fps", "uniform_0.2fps", "fovea_mv"]:
        if name not in strategies:
            continue
        calls, wall = strategies[name]
        scores = score_baseline(calls, windows, args.tolerance_ms)
        cost = sum(c.cost_usd for c in calls)
        mean_lat = (sum(c.latency_ms for c in calls) / len(calls)) if calls else float("nan")
        rows.append([
            name,
            str(len(calls)),
            f"{len(calls) / oracle_n:.0%}",
            f"${cost:.5f}",
            f"{scores['coverage']:.0%}" if scores['coverage'] == scores['coverage'] else "n/a",
            f"{scores['precision']:.0%}" if scores['precision'] == scores['precision'] else "n/a",
            f"{mean_lat:.0f}",
        ])

    widths = [max(len(h), *(len(r[i]) for r in rows)) for i, h in enumerate(headers)]
    fmt = " | ".join(f"{{:<{w}}}" for w in widths)
    print(fmt.format(*headers))
    print("-+-".join("-" * w for w in widths))
    for r in rows:
        print(fmt.format(*r))

    print()
    print("definitions:")
    print(f"- coverage: fraction of oracle event-windows ({n_events}) whose center is")
    print(f"  within ±{args.tolerance_ms} ms of at least one baseline call.")
    print("- precision: fraction of baseline calls that fall within ±tolerance")
    print("  of an oracle event-window.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

"""Run all four sampling strategies on a clip and persist per-call results.

Outputs one JSON file per (clip, strategy) under `benchmarks/_runs/<run_id>/`,
plus a manifest `summary.json` aggregating call counts, costs, and timings.

Re-running is idempotent up to the VLM cache: identical (frame, prompt,
model) tuples reuse the cached response.

Usage::

    python -m benchmarks.runners.run benchmarks/datasets/cctv-sample/sample.mp4
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import sys
import time
from pathlib import Path
from typing import Callable, Iterator, List

from fovea_trigger import Stream

from .env import RUNS_DIR
from .sample import FoveaMvConfig, FrameSample, fovea_trigger, oracle, uniform_fps
from .vlm import VlmClient, VlmResponse


@dataclasses.dataclass
class CallRecord:
    ts_us: int
    trigger_name: str
    people: bool
    vehicles: bool
    event: str
    input_tokens: int
    output_tokens: int
    latency_ms: float
    cost_usd: float


@dataclasses.dataclass
class StrategyRecord:
    name: str
    calls: List[CallRecord]
    wall_clock_s: float

    def total_cost(self) -> float:
        return sum(c.cost_usd for c in self.calls)


def run_strategy(
    name: str,
    sampler: Callable[[Stream], Iterator[FrameSample]],
    clip: Path,
    client: VlmClient,
) -> StrategyRecord:
    print(f"  ▶ {name}")
    stream = Stream.from_file(str(clip))
    calls: List[CallRecord] = []
    start = time.time()
    for sample in sampler(stream):
        resp: VlmResponse = client.call(sample.rgb)
        calls.append(
            CallRecord(
                ts_us=sample.ts_us,
                trigger_name=sample.trigger_name,
                people=resp.people,
                vehicles=resp.vehicles,
                event=resp.event,
                input_tokens=resp.input_tokens,
                output_tokens=resp.output_tokens,
                latency_ms=resp.latency_ms,
                cost_usd=resp.cost_usd(),
            )
        )
    elapsed = time.time() - start
    print(f"    {len(calls)} calls in {elapsed:.1f}s, cost ${sum(c.cost_usd for c in calls):.5f}")
    return StrategyRecord(name=name, calls=calls, wall_clock_s=elapsed)


def main() -> int:
    parser = argparse.ArgumentParser(description="Step 5 benchmark runner")
    parser.add_argument("clip", type=Path)
    parser.add_argument("--run-id", type=str, default=time.strftime("%Y%m%d-%H%M%S"))
    parser.add_argument("--motion-threshold", type=int, default=20_000)
    parser.add_argument("--interval-ms", type=int, default=10_000)
    parser.add_argument("--scene-threshold", type=float, default=0.6)
    parser.add_argument("--oracle-interval-ms", type=int, default=1000)
    args = parser.parse_args()

    if not args.clip.exists():
        print(f"clip not found: {args.clip}", file=sys.stderr)
        return 2

    out_dir = RUNS_DIR / args.run_id
    out_dir.mkdir(parents=True, exist_ok=True)

    client = VlmClient()

    fovea_cfg = FoveaMvConfig(
        motion_energy_threshold=args.motion_threshold,
        interval_max_gap_ms=args.interval_ms,
        scene_change_threshold=args.scene_threshold,
    )

    records: List[StrategyRecord] = []
    print(f"clip: {args.clip}")
    records.append(run_strategy("oracle_1fps", lambda s: oracle(s, args.oracle_interval_ms), args.clip, client))
    records.append(run_strategy("uniform_1fps", lambda s: uniform_fps(s, 1.0), args.clip, client))
    records.append(run_strategy("uniform_0.2fps", lambda s: uniform_fps(s, 0.2), args.clip, client))
    records.append(run_strategy("fovea_trigger", lambda s: fovea_trigger(s, fovea_cfg), args.clip, client))

    # Persist per-strategy JSON.
    summary = {
        "run_id": args.run_id,
        "clip": str(args.clip),
        "fovea_config": dataclasses.asdict(fovea_cfg),
        "strategies": [],
    }
    for r in records:
        path = out_dir / f"{r.name}.json"
        path.write_text(json.dumps(
            {
                "name": r.name,
                "wall_clock_s": r.wall_clock_s,
                "calls": [dataclasses.asdict(c) for c in r.calls],
            },
            indent=2,
        ))
        summary["strategies"].append({
            "name": r.name,
            "calls": len(r.calls),
            "cost_usd": r.total_cost(),
            "wall_clock_s": r.wall_clock_s,
        })
    (out_dir / "summary.json").write_text(json.dumps(summary, indent=2))

    print()
    print(f"results: {out_dir}")
    print()
    print("strategy           | calls | cost (USD)  | wall (s)")
    print("-------------------+-------+-------------+---------")
    for s in summary["strategies"]:
        print(f"{s['name']:<18} | {s['calls']:>5} | ${s['cost_usd']:>10.5f} | {s['wall_clock_s']:>6.1f}")

    return 0


if __name__ == "__main__":
    sys.exit(main())

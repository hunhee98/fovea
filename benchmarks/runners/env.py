"""Repo-wide environment helpers for benchmark scripts."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DATASETS = REPO_ROOT / "benchmarks" / "datasets"
RUNS_DIR = REPO_ROOT / "benchmarks" / "_runs"
CACHE_DIR = RUNS_DIR / "cache"
RESULTS_DIR = REPO_ROOT / "benchmarks" / "results"


def load_env() -> None:
    """Load `.env` at the repo root if present. Idempotent."""
    try:
        from dotenv import load_dotenv  # type: ignore[import-not-found]
    except ImportError as e:
        raise RuntimeError(
            "python-dotenv is required. pip install python-dotenv"
        ) from e
    load_dotenv(REPO_ROOT / ".env")


def require_env(name: str) -> str:
    """Return the env var or raise with a helpful message."""
    val = os.environ.get(name)
    if not val:
        raise RuntimeError(
            f"environment variable {name} is not set. "
            f"Either export it or put it in {REPO_ROOT}/.env"
        )
    return val


# ---------------------------------------------------------------------------
# Apple Silicon thermal / power telemetry via macmon.
# ---------------------------------------------------------------------------
#
# macmon is a sudoless CLI that reads SoC temperature, power, and per-cluster
# usage on Apple Silicon. Install:
#
#   brew install macmon
#
# We invoke it in `pipe -i 1000 -s 1` mode, which prints exactly one JSON
# line and exits — cheap enough to call between bench stages, and self-
# contained enough that bench callers don't need to manage a streaming
# subprocess.


def macmon_available() -> bool:
    """`True` if the `macmon` CLI is on PATH (Apple Silicon Mac with brew)."""
    return shutil.which("macmon") is not None


def macmon_sample(timeout_s: float = 5.0) -> dict | None:
    """Return one macmon JSON sample as a dict, or `None` if unavailable.

    Captures CPU / GPU temperature, package power, RAM usage, and per-cluster
    utilization. See `macmon pipe --help` for field semantics. Robust to
    missing CLI / parse errors / timeouts — bench code can call this
    unconditionally and just store `None` when telemetry isn't available.
    """
    if not macmon_available():
        return None
    try:
        out = subprocess.run(
            ["macmon", "pipe", "-i", "1000", "-s", "1"],
            capture_output=True,
            text=True,
            timeout=timeout_s,
        )
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return None
    if out.returncode != 0:
        return None
    line = out.stdout.strip().splitlines()[0] if out.stdout.strip() else ""
    if not line:
        return None
    try:
        return json.loads(line)
    except json.JSONDecodeError:
        return None


def macmon_summary(sample: dict | None) -> str:
    """Format a macmon dict as a single-line summary suitable for result.md.

    Returns `""` when telemetry is unavailable so callers can string-concat
    without conditional logic.
    """
    if not sample:
        return ""
    temp = sample.get("temp", {}) or {}
    cpu_t = temp.get("cpu_temp_avg")
    gpu_t = temp.get("gpu_temp_avg")
    cpu_p = sample.get("cpu_power")
    all_p = sample.get("all_power")
    parts = []
    if cpu_t is not None:
        parts.append(f"CPU {cpu_t:.1f}°C")
    if gpu_t is not None:
        parts.append(f"GPU {gpu_t:.1f}°C")
    if cpu_p is not None:
        parts.append(f"CPU {cpu_p:.1f} W")
    if all_p is not None:
        parts.append(f"pkg {all_p:.1f} W")
    return " | ".join(parts)

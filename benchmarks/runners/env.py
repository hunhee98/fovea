"""Repo-wide environment helpers for benchmark scripts."""

from __future__ import annotations

import os
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

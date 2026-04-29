"""Gemini Flash client adapted for the fovea-trigger benchmark.

Each call:
- takes an RGB ndarray and a prompt
- returns a structured JSON-ish dict the analysis layer can compare across
  approaches

We use a tight, structured prompt that asks for one-line agreement on a
fixed schema (presence flags + a free-form one-liner). This keeps the
output comparable across approaches without over-engineering grading.

Pricing (April 2026 reference, for cost reporting only):
- Gemini 1.5 Flash: input $0.075/1M tokens, output $0.30/1M tokens.
- A 1080×1920 image is encoded by the API at a fixed image-token rate;
  Google's docs treat each image as ~258 tokens regardless of resolution.
- We approximate per-call cost by summing token counts the SDK reports.
"""

from __future__ import annotations

import dataclasses
import hashlib
import io
import json
import os
import time
from pathlib import Path
from typing import Optional

import numpy as np
from PIL import Image

from .env import CACHE_DIR, load_env, require_env

# Public schema we ask Gemini to emit.
PROMPT = """\
You are inspecting a single video frame from a fixed-position street/CCTV
camera. Reply with ONE JSON object on a single line, no prose, matching
exactly:

{"people":bool,"vehicles":bool,"event":string}

Fields:
- "people": true iff at least one person is plainly visible in the frame
- "vehicles": true iff at least one vehicle (car/truck/bus/scooter/etc) is plainly visible
- "event": at most 12 words describing the most salient action or static
  scene state (e.g. "two pedestrians cross bridge", "white truck on overpass",
  "scene quiet").

Output exactly one JSON object. Do not include backticks, headers, or any
other text.
"""

PRICE_INPUT_PER_1M = 0.075
PRICE_OUTPUT_PER_1M = 0.30


@dataclasses.dataclass
class VlmResponse:
    """Structured Gemini reply for one frame."""
    people: bool
    vehicles: bool
    event: str
    raw: str
    input_tokens: int
    output_tokens: int
    latency_ms: float

    def cost_usd(self) -> float:
        in_cost = self.input_tokens / 1_000_000 * PRICE_INPUT_PER_1M
        out_cost = self.output_tokens / 1_000_000 * PRICE_OUTPUT_PER_1M
        return in_cost + out_cost

    def to_dict(self) -> dict:
        return dataclasses.asdict(self)


class VlmClient:
    """Gemini Flash wrapper with on-disk cache.

    Cache key = sha1(rgb_bytes_downscaled + prompt + model). Repeat runs
    over the same frames are free.
    """

    def __init__(self, model: str = "gemini-flash-latest", downscale_long_side: int = 768):
        load_env()
        api_key = require_env("GEMINI_API_KEY")
        from google import genai  # type: ignore[import-not-found]

        self._client = genai.Client(api_key=api_key)
        self._model = model
        self._downscale = downscale_long_side
        CACHE_DIR.mkdir(parents=True, exist_ok=True)

    # ---- caching ----

    def _key(self, image_bytes: bytes) -> str:
        h = hashlib.sha1()
        h.update(self._model.encode())
        h.update(b"\0")
        h.update(PROMPT.encode())
        h.update(b"\0")
        h.update(image_bytes)
        return h.hexdigest()

    def _cache_path(self, key: str) -> Path:
        return CACHE_DIR / f"vlm-{key}.json"

    def _load_cache(self, key: str) -> Optional[VlmResponse]:
        p = self._cache_path(key)
        if not p.exists():
            return None
        d = json.loads(p.read_text())
        return VlmResponse(**d)

    def _store_cache(self, key: str, resp: VlmResponse) -> None:
        self._cache_path(key).write_text(json.dumps(resp.to_dict()))

    # ---- main entry ----

    def call(self, rgb: np.ndarray) -> VlmResponse:
        if rgb.dtype != np.uint8 or rgb.ndim != 3 or rgb.shape[2] != 3:
            raise ValueError("rgb must be (H, W, 3) uint8")
        # Downscale to keep per-image token usage bounded and consistent.
        h, w = rgb.shape[:2]
        long_side = max(h, w)
        if long_side > self._downscale:
            scale = self._downscale / long_side
            new_h = int(round(h * scale))
            new_w = int(round(w * scale))
            img = Image.fromarray(rgb).resize((new_w, new_h), Image.BILINEAR)
        else:
            img = Image.fromarray(rgb)

        buf = io.BytesIO()
        img.save(buf, format="JPEG", quality=85)
        image_bytes = buf.getvalue()

        key = self._key(image_bytes)
        if cached := self._load_cache(key):
            return cached

        # Retry on 5xx and rate-limit. Gemini occasionally returns 503 under
        # load — exponential backoff up to ~60s total.
        from google.genai import errors as genai_errors  # type: ignore[import-not-found]

        delays = [2, 5, 10, 20, 30]
        last_err: Exception | None = None
        start = time.time()
        resp = None
        for delay in [0, *delays]:
            if delay:
                time.sleep(delay)
            try:
                resp = self._client.models.generate_content(
                    model=self._model,
                    contents=[
                        {"role": "user", "parts": [
                            {"text": PROMPT},
                            {"inline_data": {"mime_type": "image/jpeg", "data": image_bytes}},
                        ]},
                    ],
                )
                break
            except (genai_errors.ServerError, genai_errors.ClientError) as e:
                code = getattr(e, "code", 0) or 0
                if code in (429, 500, 502, 503, 504):
                    last_err = e
                    continue
                raise
        if resp is None:
            assert last_err is not None
            raise last_err
        latency_ms = (time.time() - start) * 1000.0
        text = (resp.text or "").strip()

        # Strip code fences if model added them.
        if text.startswith("```"):
            text = text.strip("`").lstrip("json").strip()
        try:
            parsed = json.loads(text)
        except json.JSONDecodeError:
            parsed = {"people": False, "vehicles": False, "event": text[:80]}

        usage = getattr(resp, "usage_metadata", None)
        in_tok = int(getattr(usage, "prompt_token_count", 0) or 0) if usage else 0
        out_tok = int(getattr(usage, "candidates_token_count", 0) or 0) if usage else 0

        out = VlmResponse(
            people=bool(parsed.get("people", False)),
            vehicles=bool(parsed.get("vehicles", False)),
            event=str(parsed.get("event", ""))[:200],
            raw=text,
            input_tokens=in_tok,
            output_tokens=out_tok,
            latency_ms=latency_ms,
        )
        self._store_cache(key, out)
        return out

"""Pixel-diff motion-detection baseline for fovea-trigger comparisons.

The classical, decode-based motion detector that NVR-class OSS projects
(Frigate, Viseron, Shinobi) use as their first-stage filter. We
implement it here so the fovea-trigger density / FP / latency claims have a
real baseline to compare against on the same hardware and clip.

Not part of the public fovea-trigger API — this is benchmark infrastructure.
"""

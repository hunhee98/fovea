"""Pixel-diff motion-detection baseline for fovea-mv comparisons.

The classical, decode-based motion detector that NVR-class OSS projects
(Frigate, Viseron, Shinobi) use as their first-stage filter. We
implement it here so the fovea-mv density / FP / latency claims have a
real baseline to compare against on the same hardware and clip.

Not part of the public fovea-mv API — this is benchmark infrastructure.
"""

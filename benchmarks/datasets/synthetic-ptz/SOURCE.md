# synthetic-ptz — synthetic camera-motion clips

## What this is

Synthetic clips that exercise global-motion correction on real-CCTV-encoded
bitstreams. The visual content is a single still frame from `cctv-sample`;
camera motion is added via FFmpeg `crop` with a time-varying offset, then
the result is re-encoded with libx264 in a surveillance-like profile (High
profile, GOP 15, no B-frames, ~3 Mbps). This gives us:

- A bitstream encoded with libx264 (not a real surveillance encoder), but
  with a profile that matches the seattle-dot DOT camera fingerprint.
- A scene with **only** camera motion (no in-frame object motion), so any
  trigger fired by a motion-energy threshold is a false positive caused by
  global camera motion.

`data/` is gitignored.

## Limitations

- libx264 with surveillance settings is closer to real CCTV than libx264
  defaults, but it is still not a real Axis / Bosch / Pelco encoder.
- The "no object motion" assumption holds because the source is one still
  frame replicated. Real PTZ surveillance always has both camera and
  object motion; this clip isolates camera motion alone.
- Numbers from this dataset prove **mechanism** (correction zeroes out
  uniform pan), not real-world deployment gain. Real-PTZ validation is
  pending.

## Files

- `generate.sh` — extracts frame 0 from `cctv-sample/sample.mp4`, then
  generates two clips:
  - `pan_pure.mp4` — 10 s, horizontal pan, ~30 fps, 540×960 crop.
  - `static.mp4` — 10 s, no motion at all (control). All triggers on
    this clip are by definition false positives.

Run from the repo root:

```sh
benchmarks/datasets/synthetic-ptz/generate.sh
```

## License

Source still frame is one snapshot of the Pexels-licensed `cctv-sample`
clip. The synthetic clips inherit the Pexels License.

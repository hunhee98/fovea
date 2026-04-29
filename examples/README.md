# Examples

Short scripts that exercise `fovea-trigger` end-to-end. Each is independent
— pick one that matches your input source.

| Script | Use it when you have… | Demonstrates |
|---|---|---|
| [`mini_nvr.py`](mini_nvr.py) | An RTSP camera **or** a local mp4 and you want the canonical "what does fovea-trigger buy me" cascade | `FusionTrigger` (motion + intra + skip) → decode only on fire → hand pixels to a stub `process_frame` you replace with your VLM / YOLO call |
| [`file_demo.py`](file_demo.py) | A local mp4 file | Plain `MotionTrigger` + `IntervalTrigger` + `SceneChangeTrigger` on a file source, optional first-frame PNG dump |
| [`rtsp_demo.py`](rtsp_demo.py) | An RTSP camera (real device or local MediaMTX loop) | Live `Stream.from_url` with reconnect, prints one line per event, supports `--max-events` / `--max-seconds` |
| [`live_view.py`](live_view.py) | An RTSP camera + a desktop session | Same as `rtsp_demo` but pops up an OpenCV window with a red overlay on each fire (developer-friendly visualization) |

## Quick start

Most readers will want `mini_nvr.py` first:

```sh
# File source (any clip in benchmarks/datasets/ works)
python examples/mini_nvr.py benchmarks/datasets/cctv-sample/sample.mp4

# RTSP source (e.g. an iPhone IP-cam app on the same Wi-Fi)
python examples/mini_nvr.py rtsp://USER:PASS@CAMERA_IP:8554/live --max-seconds 60
```

The script prints one line per `process_frame` invocation. Replace the
stub with your VLM / detector call.

## Adapting an example

Each script is short on purpose. The pattern is always:

```python
from fovea_trigger import Stream, FusionTrigger    # or any other trigger

stream = Stream.from_file("clip.mp4")          # or .from_url("rtsp://...")
trigger = FusionTrigger("any", motion_threshold=200_000, intra_threshold=0.05)

for ev in stream.events([trigger]):
    rgb = ev.decode()                          # only on fire — idle frames cost ~nothing
    your_model_call(rgb)
```

See [`docs/01.architecture/comparison-matrix.md`](../docs/01.architecture/comparison-matrix.md)
for which trigger primitive solves which problem.

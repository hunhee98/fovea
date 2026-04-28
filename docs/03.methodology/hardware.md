# Hardware declaration

Every result file under `benchmarks/results/` must declare hardware. Without it the number is unreproducible.

## Required line

```
Hardware: <CPU model>, <RAM GB>, <GPU model or "none">, <OS version>, Rust <version>
```

Example:

```
Hardware: Apple M3 Pro 12-core, 36 GB, Apple GPU 18-core (Metal), macOS 25.3.0, Rust 1.80
```

For Linux:

```
Hardware: Intel Xeon E5-2680 v4 @ 2.40GHz (14-core), 64 GB, NVIDIA RTX 3080 (10GB), Ubuntu 22.04, Rust 1.80
```

## What to include / exclude

Include:
- CPU model and core count.
- RAM in GB.
- GPU model **only if it was used** (NVDEC, VideoToolbox). Otherwise `none`.
- OS major.minor.
- Rust version (matters for codegen).

Exclude:
- Disk model (rarely the bottleneck for our workloads).
- Network card (the network is upstream).

## Hardware classes we target

Density numbers should be reported per class:

| class | example | why |
|-------|---------|-----|
| Apple Silicon | M-series MacBook | dev hardware, ubiquitous |
| x86 server CPU | Xeon / EPYC | self-hosted users |
| x86 + NVIDIA GPU | desktop / server with NVDEC | GPU decode path |

A "100 streams/box" claim is meaningless without the box. The bench result file declares the box; the README headline number cites the result file.

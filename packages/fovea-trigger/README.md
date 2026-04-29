# fovea-trigger

H.264 motion-vector trigger engine. Decides which video frames deserve downstream VLM processing.

Part of the [Fovea](https://github.com/hunhee98/fovea) monorepo. See the root README for context.

## Status

Pre-alpha scaffolding. No public API yet. See `docs/05.exec-plans/001-mvtrigger-mvp.md`.

## Build (development)

Requires Rust 1.80+ and Python 3.10+.

```bash
pip install maturin
cd packages/fovea-trigger
maturin develop
python -c "import fovea_trigger; print(fovea_trigger.core_version())"
```

## License

Apache-2.0.

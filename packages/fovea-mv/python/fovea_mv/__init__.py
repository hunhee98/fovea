"""fovea-mv: H.264 motion-vector trigger engine.

Status: scaffolding. Public API lands in MVP step 4 (see docs/05.exec-plans/001-mvtrigger-mvp.md).
"""

from ._fovea_mv import core_version  # type: ignore[import-not-found]

__all__ = ["core_version"]

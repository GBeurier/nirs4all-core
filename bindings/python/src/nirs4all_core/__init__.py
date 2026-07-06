"""``nirs4all_core`` -- core-contract facade of the aggregate.

RC V1 topology: the portable nirs4all aggregate ships as the
``nirs4all-core`` distribution (renamed from ``nirs4all-lite``). The canonical
import root stays ``nirs4all_lite`` for compatibility; this additive
``nirs4all_core`` import alias matches the distribution name so downstream
code can standardize on it without breakage.

The public ``nirs4all_core`` contract advertises only inspection, validation,
capability, release-topology, and facade APIs. Execution helpers from
``nirs4all_lite`` remain reachable through the compatibility passthrough, but
they are deliberately outside ``nirs4all_core.__all__`` so ``import *`` and
release manifest checks do not treat core as an execution engine.

Core-style imports are stable::

    import nirs4all_core as n4a_core

    n4a_core.upstream_status()
    n4a_core.load_pipeline_definition(config)

The legacy ``nirs4all_lite`` import surface is unchanged and fully supported.
"""

from __future__ import annotations

from typing import Any

import nirs4all_lite as _aggregate

CORE_FACADE_EXPORTS = _aggregate.CORE_FACADE_EXPORTS
TOPOLOGY_EXPORTS = _aggregate.TOPOLOGY_EXPORTS

#: Import package currently backing this alias (``nirs4all_lite``). The
#: ``nirs4all-core`` distribution keeps ``nirs4all_lite`` as its canonical
#: import root; this alias forwards to that shipped aggregate.
__aggregate_import__ = _aggregate.__name__
__version__ = _aggregate.__version__

__all__ = list(CORE_FACADE_EXPORTS + TOPOLOGY_EXPORTS)

for _name in __all__:
    globals()[_name] = getattr(_aggregate, _name)


def __getattr__(name: str) -> Any:
    """Forward any non-re-exported attribute to the backing aggregate package."""

    return getattr(_aggregate, name)


def __dir__() -> list[str]:
    """Return the advertised core facade surface plus normal module globals."""

    return sorted(set(__all__) | set(globals()))

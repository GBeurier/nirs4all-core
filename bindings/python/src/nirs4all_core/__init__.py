"""``nirs4all_core`` -- release-gated core facade for the aggregate.

LOCK-GOV direction (target-state): the portable nirs4all aggregate -- shipped
today as the ``nirs4all-lite`` distribution (import ``nirs4all_lite``) -- is to
be promoted to ``nirs4all-core``. The actual distribution rename is gated on the
release lock (``LOCK-REL``) and is intentionally **not** performed in this
slice: the project keeps publishing the aggregate as ``nirs4all-lite`` and only
adds this additive ``nirs4all_core`` import alias so downstream code can adopt
the target import name early, without breakage.

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
#: distribution rename ``nirs4all-lite`` -> ``nirs4all-core`` is release-gated
#: (LOCK-REL); this alias forwards to the shipped aggregate until then.
__aggregate_import__ = _aggregate.__name__

__all__ = list(CORE_FACADE_EXPORTS + TOPOLOGY_EXPORTS)

for _name in __all__:
    globals()[_name] = getattr(_aggregate, _name)


def __getattr__(name: str) -> Any:
    """Forward any non-re-exported attribute to the backing aggregate package."""

    return getattr(_aggregate, name)


def __dir__() -> list[str]:
    """Return the advertised core facade surface plus normal module globals."""

    return sorted(set(__all__) | set(globals()))

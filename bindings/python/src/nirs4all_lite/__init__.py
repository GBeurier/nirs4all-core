"""Python surface for the nirs4all-lite aggregate distribution."""

from ._upstreams import (
    LazyUpstream,
    Upstream,
    available_upstreams,
    import_upstream,
    require_upstream,
    upstream_status,
    upstreams,
)

dag_ml = LazyUpstream("dag_ml")
dag_ml_data = LazyUpstream("dag_ml_data")
datasets = LazyUpstream("datasets")
formats = LazyUpstream("formats")
io = LazyUpstream("io")
methods = LazyUpstream("methods")

__all__ = [
    "LazyUpstream",
    "Upstream",
    "available_upstreams",
    "dag_ml",
    "dag_ml_data",
    "datasets",
    "formats",
    "import_upstream",
    "io",
    "methods",
    "require_upstream",
    "upstream_status",
    "upstreams",
]

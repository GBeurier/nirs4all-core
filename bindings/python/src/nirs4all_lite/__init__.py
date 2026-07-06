"""Python surface for the nirs4all-core aggregate distribution (import root kept as ``nirs4all_lite``)."""

__version__ = "0.2.6"

from ._execution import PortableDataset, parse_execution_plan, run_portable_pipeline
from ._pipeline import (
    PORTABLE_OPERATOR_CLASSES,
    PipelineDefinition,
    load_pipeline_definition,
    portable_class_names,
)
from ._topology import (
    CORE_FACADE_EXPORTS,
    EXECUTION_ENGINE_EXPORTS,
    TOPOLOGY_EXPORTS,
    core_facade_exports,
    execution_engine_exports,
    release_topology_manifest,
    validate_core_facade,
)
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
    "PORTABLE_OPERATOR_CLASSES",
    "PortableDataset",
    "PipelineDefinition",
    "CORE_FACADE_EXPORTS",
    "EXECUTION_ENGINE_EXPORTS",
    "TOPOLOGY_EXPORTS",
    "Upstream",
    "available_upstreams",
    "core_facade_exports",
    "dag_ml",
    "dag_ml_data",
    "datasets",
    "execution_engine_exports",
    "formats",
    "import_upstream",
    "io",
    "load_pipeline_definition",
    "methods",
    "parse_execution_plan",
    "portable_class_names",
    "release_topology_manifest",
    "require_upstream",
    "run_portable_pipeline",
    "upstream_status",
    "upstreams",
    "validate_core_facade",
]

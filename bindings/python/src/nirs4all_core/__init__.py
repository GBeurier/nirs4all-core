"""Python surface for the nirs4all-core aggregate distribution."""

__version__ = "0.5.0"

from ._archive import (
    NativeArchiveUnavailableError,
    inspect_methods_archive_v2_predictors,
    open_native_package,
    predict_methods_archive_v2_matrix,
    read_archive_v2_payloads,
    read_archive_v3_view,
    read_native_package_view,
    read_portable_predictor_package_v2,
    read_portable_refit_package_v3,
    replay_methods_archive_v2,
    replay_methods_archive_v2_conformal_presentation_v1,
    replay_methods_archive_v2_conformal_presentation_v2,
    replay_methods_archive_v3,
    write_native_predictor_package,
    write_native_refit_package,
)
from ._browser_tuning import BrowserTuningResult, load_browser_tuning
from ._capabilities import (
    capability_manifest,
    execution_profiles,
    runtime_surfaces,
)
from ._conformal import (
    CalibratedWorkflow,
    calibrate,
    conformal_metrics,
    export_calibrated,
    load_calibrated,
    predict_calibrated,
)
from ._dataset import Dataset, dataset
from ._execution import PortableDataset, parse_execution_plan, run_portable_pipeline
from ._native_pipeline import NativePipeline, run_pipeline
from ._native_multimodal import NativeMultimodal, run_multimodal
from ._multimodal import MultimodalPredictor
from ._multimodal_archive import predict_multimodal_archive
from ._n4a import (
    N4aContainer,
    N4aFile,
    N4aMemberStream,
    inspect_n4a_bytes,
    open_n4a_container,
    open_n4a_file,
    predict_n4a_native,
    produce_n4a_native,
    retrain_n4a_native,
    write_n4a_container,
    write_n4a_file,
)
from ._n4m_roles import (
    N4M_ROLE_PREFIX,
    N4M_TRAINED_PIPELINE_SCHEMA,
    N4mRolePipeline,
    n4m_role_capabilities,
)
from ._pipeline import (
    PipelineDefinition,
    load_pipeline_definition,
    portable_class_names,
)
from ._results import Experiment, open_experiment, save_experiment
from ._robustness import robustness
from ._topology import (
    CORE_FACADE_EXPORTS,
    EXECUTION_ENGINE_EXPORTS,
    TOPOLOGY_EXPORTS,
    core_facade_exports,
    execution_engine_exports,
    release_topology_manifest,
    validate_core_facade,
)
from ._tuning import TuningResult, generate, load_tuning, resume_tuning, tune
from ._upstreams import (
    LazyUpstream,
    Upstream,
    available_upstreams,
    import_upstream,
    local_implementation_registry,
    require_upstream,
    upstream_status,
    upstreams,
)
from ._workflow import Workflow, export, load, predict, retrain, run
from ._workspace import Workspace, import_workspace, open_workspace, save_workspace

dag_ml = LazyUpstream("dag_ml")
dag_ml_data = LazyUpstream("dag_ml_data")
datasets = LazyUpstream("datasets")
formats = LazyUpstream("formats")
io = LazyUpstream("io")
methods = LazyUpstream("methods")

__aggregate_import__ = __name__

__all__ = [
    "NativePipeline",
    "NativeMultimodal",
    "run_multimodal",
    "run_pipeline",
    "CORE_FACADE_EXPORTS",
    "EXECUTION_ENGINE_EXPORTS",
    "N4M_ROLE_PREFIX",
    "N4M_TRAINED_PIPELINE_SCHEMA",
    "TOPOLOGY_EXPORTS",
    "BrowserTuningResult",
    "CalibratedWorkflow",
    "Dataset",
    "Experiment",
    "LazyUpstream",
    "MultimodalPredictor",
    "N4mRolePipeline",
    "NativeArchiveUnavailableError",
    "PipelineDefinition",
    "PortableDataset",
    "TuningResult",
    "Upstream",
    "Workflow",
    "Workspace",
    "available_upstreams",
    "calibrate",
    "capability_manifest",
    "conformal_metrics",
    "execution_profiles",
    "core_facade_exports",
    "dag_ml",
    "dag_ml_data",
    "dataset",
    "datasets",
    "execution_engine_exports",
    "export",
    "export_calibrated",
    "formats",
    "generate",
    "import_upstream",
    "import_workspace",
    "inspect_methods_archive_v2_predictors",
    "N4aContainer",
    "N4aFile",
    "N4aMemberStream",
    "inspect_n4a_bytes",
    "io",
    "load",
    "load_browser_tuning",
    "load_calibrated",
    "load_pipeline_definition",
    "load_tuning",
    "local_implementation_registry",
    "methods",
    "n4m_role_capabilities",
    "open_experiment",
    "open_n4a_container",
    "open_n4a_file",
    "open_workspace",
    "parse_execution_plan",
    "portable_class_names",
    "predict",
    "predict_calibrated",
    "predict_methods_archive_v2_matrix",
    "predict_multimodal_archive",
    "predict_n4a_native",
    "produce_n4a_native",
    "open_native_package",
    "read_archive_v2_payloads",
    "read_archive_v3_view",
    "read_native_package_view",
    "read_portable_predictor_package_v2",
    "read_portable_refit_package_v3",
    "release_topology_manifest",
    "replay_methods_archive_v2",
    "replay_methods_archive_v2_conformal_presentation_v1",
    "replay_methods_archive_v2_conformal_presentation_v2",
    "replay_methods_archive_v3",
    "require_upstream",
    "resume_tuning",
    "retrain",
    "retrain_n4a_native",
    "write_n4a_container",
    "write_n4a_file",
    "robustness",
    "run",
    "run_portable_pipeline",
    "runtime_surfaces",
    "save_experiment",
    "save_workspace",
    "tune",
    "upstream_status",
    "upstreams",
    "validate_core_facade",
    "write_native_predictor_package",
    "write_native_refit_package",
]

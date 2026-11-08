//! Native DAG Package2/3 over neutral Core V1 storage.
//!
//! DAG owns scientific closure and ABI requirements. This separate operation
//! profile never changes the closed canonical IO/Ridge action admission.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use dag_ml_core::{
    build_native_predictor_facets, build_native_refit_facets, validate_selected_native_closure,
    ArtifactFacetExport, ArtifactFacetGraph, FacetMemberSource, FacetRefusal, FacetResult,
    NativeMethodsRequirement, NativePackageClosure, PortablePredictorPackage,
    PortableRefitPackageV3,
};
use serde_json::{json, Value};

use crate::archive_codec::sha256;
use crate::n4a::*;
use crate::n4a_physical::{try_copy, Ledger};

pub const NATIVE_PACKAGE_FEATURE: &str = "native-methods-package@1";
pub const NATIVE_PACKAGE_PROFILE: &str = "native-methods-package";

fn refusal(detail: impl Into<String>) -> N4aError {
    N4aError::Manifest(detail.into())
}
fn owner_error(error: impl std::fmt::Display) -> N4aError {
    refusal(format!("DAG native package closure refused: {error}"))
}

/// Compatibility result keys identify the neutral container and full file hash.
#[derive(Clone, Debug)]
pub struct NativePackageReference {
    archive_id: String,
    archive_sha256: String,
    package_version: u32,
}
impl NativePackageReference {
    pub fn archive_id(&self) -> &str {
        &self.archive_id
    }
    pub fn archive_sha256(&self) -> &str {
        &self.archive_sha256
    }
    /// The package version, independent of the physical container version.
    pub fn schema_version(&self) -> u32 {
        self.package_version
    }
    pub fn profile(&self) -> &str {
        NATIVE_PACKAGE_PROFILE
    }
}

/// Invocation-local selected proof and exact closure bytes, never a new wire format.
#[derive(Clone, Debug)]
pub struct LoadedNativePackage {
    reference: NativePackageReference,
    manifest: N4aManifest,
    receipt: N4aReceipt,
    entrypoint: N4aEntrypoint,
    closure: NativePackageClosure,
    members: BTreeMap<String, Vec<u8>>,
    working_bytes: u64,
}
impl LoadedNativePackage {
    /// Re-admit retained Core buffers under a caller's explicit budget without
    /// re-reading storage or repeating owner closure validation.
    pub fn admit_memory_budget(&self, budget: N4aMemoryBudget) -> Result<(), N4aError> {
        budget.admit(self.working_bytes)
    }
    pub fn reference(&self) -> &NativePackageReference {
        &self.reference
    }
    pub fn manifest(&self) -> &N4aManifest {
        &self.manifest
    }
    pub fn members(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.members
    }
    pub fn entrypoint(&self) -> &N4aEntrypoint {
        &self.entrypoint
    }
    pub fn closure(&self) -> &NativePackageClosure {
        &self.closure
    }
    pub fn methods_requirements(&self) -> &[NativeMethodsRequirement] {
        self.closure.methods_requirements()
    }
    pub fn member(&self, path: &str) -> Result<&[u8], N4aError> {
        self.members.get(path).map(Vec::as_slice).ok_or_else(|| {
            refusal(format!(
                "member `{path}` is outside the selected native closure"
            ))
        })
    }
    pub fn predictor_package(&self) -> Result<&PortablePredictorPackage, N4aError> {
        self.closure
            .root()
            .predictor_package()
            .ok_or_else(|| refusal("selected root is not Package2"))
    }
    pub fn refit_package(&self) -> Result<&PortableRefitPackageV3, N4aError> {
        self.closure
            .root()
            .refit_package()
            .ok_or_else(|| refusal("selected root is not Package3"))
    }
    pub fn portable_predictor_package(&self) -> Result<&[u8], N4aError> {
        self.predictor_package()?;
        Ok(self.closure.root().original_bytes())
    }
    pub fn portable_refit_package(&self) -> Result<&[u8], N4aError> {
        self.refit_package()?;
        Ok(self.closure.root().original_bytes())
    }
    pub fn training_outcome_bytes(&self) -> Result<&[u8], N4aError> {
        self.closure
            .evidence()
            .map(|proof| proof.original_bytes())
            .ok_or_else(|| {
                refusal("selected child package has no complete parent/training outcome")
            })
    }
    pub fn training_outcome(&self) -> Result<&dag_ml_core::TrainingOutcome, N4aError> {
        self.closure
            .evidence()
            .and_then(|proof| proof.training_outcome())
            .ok_or_else(|| refusal("selected child package has no complete training outcome"))
    }
    /// Physical member selected by the owner mapping; never the logical URI.
    pub fn state_member(&self, artifact_id: &str) -> Result<&[u8], N4aError> {
        let state = self
            .closure
            .state_members()
            .iter()
            .find(|state| state.artifact_id.as_str() == artifact_id)
            .ok_or_else(|| refusal("native state member mapping absent"))?;
        self.member(&state.member_path)
    }
    pub fn view(&self) -> Value {
        let package_member = self
            .manifest
            .artifacts
            .iter()
            .find(|artifact| artifact.id == self.entrypoint.root_artifact)
            .and_then(|artifact| match &artifact.storage {
                N4aStorage::Embedded { root_member, .. } => Some(root_member.as_str()),
                _ => None,
            });
        json!({
            "archive_id": self.reference.archive_id(),
            "archive_sha256": self.reference.archive_sha256(),
            "schema_version": 1, "profile": NATIVE_PACKAGE_PROFILE,
            "package_schema_version": self.reference.package_version,
            "entrypoint": self.entrypoint.id, "operation": self.entrypoint.operation,
            "root_artifact": self.entrypoint.root_artifact,
            "package_member": package_member, "receipt": {
                "container_sha256": self.receipt.container_sha256,
                "manifest_sha256": self.receipt.manifest_sha256,
                "byte_length": self.receipt.byte_length,
                "members": self.receipt.members,
            },
            "manifest": self.manifest,
        })
    }
    /// Separate operation admission: a predictor proof cannot authorize calibration.
    pub fn require_operation(&self, operation: &str) -> Result<(), N4aError> {
        if self.entrypoint.operation != operation {
            return Err(refusal(format!(
                "selected entrypoint does not declare `{operation}`"
            )));
        }
        Ok(())
    }
}

struct Source<'a, C> {
    container: &'a C,
    ledger: RefCell<Ledger>,
    members: RefCell<BTreeMap<String, Vec<u8>>>,
}
impl<C: N4aMemberBytes> FacetMemberSource for Source<'_, C> {
    fn read_member(&self, path: &str) -> FacetResult<Vec<u8>> {
        let read = || -> Result<Vec<u8>, N4aError> {
            let mut members = self.members.borrow_mut();
            if !members.contains_key(path) {
                let location = self
                    .container
                    .metadata()
                    .member_location(path)
                    .ok_or_else(|| refusal("selected closure member absent"))?;
                self.ledger.borrow_mut().charge(location.byte_length)?;
                let bytes = self.container.member_bytes(path)?;
                let bytes = match bytes {
                    std::borrow::Cow::Owned(bytes) => bytes,
                    std::borrow::Cow::Borrowed(bytes) => try_copy(bytes)?,
                };
                members.insert(path.to_owned(), bytes);
            }
            // DAG's neutral source API owns its input. Charge the handed-over
            // copy as well; owner-internal allocations remain owner-managed.
            let bytes = &members[path];
            self.ledger.borrow_mut().charge(bytes.len() as u64)?;
            try_copy(bytes)
        };
        read().map_err(|error| FacetRefusal::InvalidStructure {
            detail: error.to_string(),
        })
    }
}

fn select_entry<'a>(
    manifest: &'a N4aManifest,
    selected: Option<&str>,
    operation: &str,
) -> Result<&'a N4aEntrypoint, N4aError> {
    let matching = |entry: &&N4aEntrypoint| {
        entry.owner == "dag-ml"
            && entry.operation == operation
            && manifest.artifacts.iter().any(|artifact| {
                artifact.id == entry.root_artifact
                    && artifact.owner == "dag-ml"
                    && matches!(
                        (
                            artifact.content_type.id.as_str(),
                            artifact.content_type.version
                        ),
                        ("dagml.portable_predictor_package", 2)
                            | ("dagml.portable_refit_package", 3)
                    )
            })
    };
    let mut candidates = manifest.entrypoints.iter().filter(matching);
    let entry = if let Some(selected) = selected {
        candidates
            .find(|entry| entry.id == selected)
            .ok_or_else(|| refusal("selected native entrypoint/operation is unavailable"))?
    } else {
        let entry = candidates
            .next()
            .ok_or_else(|| refusal("native entrypoint is unavailable"))?;
        if candidates.next().is_some() {
            return Err(refusal(
                "ambiguous native entrypoints; select an explicit entrypoint",
            ));
        }
        entry
    };
    if !entry.input_artifacts.is_empty()
        || entry
            .requirements
            .as_ref()
            .is_some_and(|r| operation != "calibrate-external" || r.artifact_id != "result")
        || (operation == "calibrate-external" && entry.requirements.is_none())
    {
        return Err(refusal(
            "native external operation has unsupported inputs/requirements",
        ));
    }
    Ok(entry)
}

fn graph(manifest: &N4aManifest) -> Result<ArtifactFacetGraph, N4aError> {
    // Translate descriptors only. Foreign bytes are never parsed or loaded.
    let artifacts = serde_json::from_value(json!(manifest.artifacts)).map_err(owner_error)?;
    let links = serde_json::from_value(json!(manifest
        .links
        .iter()
        .filter(|link| link.owner == "dag-ml")
        .collect::<Vec<_>>()))
    .map_err(owner_error)?;
    Ok(ArtifactFacetGraph { artifacts, links })
}

/// Resolve one declared operation, then ask DAG to reconstruct its strict closure.
pub fn read_native_package_for_operation<C: N4aMemberBytes>(
    container: &C,
    entrypoint: Option<&str>,
    operation: &str,
    budget: N4aMemoryBudget,
) -> Result<LoadedNativePackage, N4aError> {
    if !matches!(operation, "predict-external" | "calibrate-external") {
        return Err(refusal("unsupported native package operation"));
    }
    let metadata = container.metadata();
    let manifest = metadata.manifest();
    let entry = select_entry(manifest, entrypoint, operation)?;
    if !manifest
        .required_features
        .iter()
        .chain(&entry.required_features)
        .any(|feature| feature == NATIVE_PACKAGE_FEATURE)
    {
        return Err(refusal("native package feature declaration absent"));
    }
    let mut ledger = Ledger::new(budget);
    ledger.charge(metadata.retained_bytes())?;
    ledger.charge(container.resident_bytes())?;
    let source = Source {
        container,
        ledger: RefCell::new(ledger),
        members: RefCell::new(BTreeMap::new()),
    };
    let closure =
        validate_selected_native_closure(&graph(manifest)?, &entry.root_artifact, &source)
            .map_err(owner_error)?;
    let binding = metadata.operation_binding(&entry.id).map_err(owner_error)?;
    metadata
        .require_operation(
            &entry.id,
            Some(&OwnerAdmission {
                binding,
                status: OwnerAdmissionStatus::Verified,
                supported_features: vec![NATIVE_PACKAGE_FEATURE.into()],
            }),
        )
        .map_err(owner_error)?;
    let package_version = closure.root().content_type().version;
    if operation == "calibrate-external"
        && closure
            .root()
            .predictor_package()
            .is_none_or(|package| package.conformal_calibration.is_some())
    {
        return Err(refusal(
            "calibration requires an uncalibrated Package2 base",
        ));
    }
    let working_bytes = source.ledger.borrow().used();
    Ok(LoadedNativePackage {
        reference: NativePackageReference {
            archive_id: manifest.container_id.clone(),
            archive_sha256: metadata.receipt().container_sha256.clone(),
            package_version,
        },
        manifest: manifest.clone(),
        receipt: metadata.receipt().clone(),
        entrypoint: entry.clone(),
        closure,
        working_bytes,
        members: source.members.into_inner(),
    })
}

pub fn read_native_package<C: N4aMemberBytes>(
    container: &C,
    entrypoint: Option<&str>,
) -> Result<LoadedNativePackage, N4aError> {
    read_native_package_for_operation(
        container,
        entrypoint,
        "predict-external",
        container.metadata().memory_budget(),
    )
}

pub fn load_native_package_bytes(
    bytes: &[u8],
    entrypoint: Option<&str>,
    budget: N4aMemoryBudget,
) -> Result<LoadedNativePackage, N4aError> {
    let container = open_n4a_bytes(bytes, budget).map_err(native_package_transport_error)?;
    read_native_package_for_operation(&container, entrypoint, "predict-external", budget)
}

pub fn native_package_transport_error(error: N4aError) -> N4aError {
    match error {
        N4aError::UnsupportedFormat(detail) => N4aError::UnsupportedFormat(format!(
            "{detail}; legacy physical archives are refused; re-export from the producing run with the current native Core container writer")),
        other => other,
    }
}

fn external_entrypoints(root: &str, calibrate: bool) -> Vec<N4aEntrypoint> {
    let mut entrypoints = vec![N4aEntrypoint {
        id: "predict-external".into(),
        owner: "dag-ml".into(),
        operation: "predict-external".into(),
        root_artifact: root.into(),
        input_artifacts: vec![],
        required_features: vec![NATIVE_PACKAGE_FEATURE.into()],
        requirements: None,
    }];
    if calibrate {
        entrypoints.push(N4aEntrypoint {
            id: "calibrate-external".into(),
            owner: "dag-ml".into(),
            operation: "calibrate-external".into(),
            root_artifact: root.into(),
            input_artifacts: vec![],
            required_features: vec![NATIVE_PACKAGE_FEATURE.into()],
            requirements: Some(RequirementRef {
                artifact_id: "result".into(),
            }),
        });
    }
    entrypoints
}

fn manifest_from_export(
    export: &ArtifactFacetExport,
    container_id: &str,
    root: &str,
) -> Result<N4aManifest, N4aError> {
    let entrypoints = external_entrypoints(
        root,
        export
            .context
            .owner_proofs
            .get(root)
            .and_then(|proof| proof.predictor_package())
            .is_some_and(|package| package.conformal_calibration.is_none()),
    );
    Ok(N4aManifest {
        schema: N4A_SCHEMA.into(),
        schema_version: 1,
        minimum_reader_version: 1,
        container_id: container_id.into(),
        physical_profile: N4A_PHYSICAL_PROFILE.into(),
        required_features: vec![],
        members: export
            .members
            .iter()
            .map(|(path, bytes)| N4aMember {
                path: path.clone(),
                byte_length: bytes.len() as u64,
                sha256: sha256(bytes),
            })
            .collect(),
        artifacts: serde_json::from_value(json!(export.graph.artifacts)).map_err(owner_error)?,
        links: serde_json::from_value(json!(export.graph.links)).map_err(owner_error)?,
        entrypoints,
    })
}

fn export_package(
    outcome: Option<&[u8]>,
    package: &[u8],
) -> Result<(ArtifactFacetExport, &'static str), N4aError> {
    match outcome {
        Some(outcome) => Ok((
            build_native_predictor_facets(outcome, package).map_err(owner_error)?,
            "model",
        )),
        None => Ok((
            build_native_refit_facets(package).map_err(owner_error)?,
            "full_refit",
        )),
    }
}

/// Seal raw owner documents, never an old ZIP or a persisted transport descriptor.
pub fn native_package_container(
    outcome: Option<&[u8]>,
    package: &[u8],
    container_id: &str,
    budget: N4aMemoryBudget,
) -> Result<OwnedN4a, N4aError> {
    let (export, root) = export_package(outcome, package)?;
    write_n4a_owned(
        N4aWriteRequest {
            manifest: manifest_from_export(&export, container_id, root)?,
            members: export
                .members
                .iter()
                .map(|(path, bytes)| N4aMemberInput { path, bytes })
                .collect(),
        },
        budget,
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub fn open_native_package(
    path: &std::path::Path,
    entrypoint: Option<&str>,
    budget: N4aMemoryBudget,
) -> Result<LoadedNativePackage, N4aError> {
    let file =
        crate::n4a_file::open_n4a_file(path, budget).map_err(native_package_transport_error)?;
    read_native_package_for_operation(&file, entrypoint, "predict-external", budget)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_native_predictor_package(
    path: &std::path::Path,
) -> Result<LoadedNativePackage, N4aError> {
    let loaded = open_native_package(path, None, N4aMemoryBudget::host_allocator())?;
    loaded.predictor_package()?;
    Ok(loaded)
}
#[cfg(not(target_arch = "wasm32"))]
pub fn load_native_refit_package(path: &std::path::Path) -> Result<LoadedNativePackage, N4aError> {
    let loaded = open_native_package(path, None, N4aMemoryBudget::host_allocator())?;
    loaded.refit_package()?;
    Ok(loaded)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn write_native_package_file(
    path: &std::path::Path,
    outcome: Option<&[u8]>,
    package: &[u8],
    container_id: &str,
    budget: N4aMemoryBudget,
) -> Result<NativePackageReference, N4aError> {
    use crate::n4a_file::{write_n4a_file, N4aStreamInput, N4aStreamRequest};
    let (export, root) = export_package(outcome, package)?;
    let receipt = write_n4a_file(
        path,
        N4aStreamRequest {
            manifest: manifest_from_export(&export, container_id, root)?,
            members: export
                .members
                .iter()
                .map(|(path, bytes)| N4aStreamInput {
                    path,
                    source: bytes,
                })
                .collect(),
            base: None,
        },
        budget,
    )?;
    Ok(NativePackageReference {
        archive_id: container_id.into(),
        archive_sha256: receipt.container_sha256,
        package_version: if outcome.is_some() { 2 } else { 3 },
    })
}

/// Construct a calibration update against the admitted immutable base. Fixed
/// IDs cannot be rebound through retained links/entrypoints, including foreign ones.
pub fn calibration_update<C: N4aMemberBytes>(
    base: &C,
    before: &LoadedNativePackage,
    outcome: &[u8],
    package: &[u8],
    container_id: &str,
) -> Result<(N4aManifest, BTreeMap<String, Vec<u8>>), N4aError> {
    before.require_operation("calibrate-external")?;
    if before.receipt != *base.metadata().receipt()
        || before.manifest != *base.metadata().manifest()
    {
        return Err(refusal(
            "calibration base differs from its admitted receipt",
        ));
    }
    let fresh = build_native_predictor_facets(outcome, package).map_err(owner_error)?;
    let old_package = before.predictor_package()?;
    let new_package = fresh
        .context
        .owner_proofs
        .get("model")
        .and_then(|proof| proof.predictor_package())
        .ok_or_else(|| refusal("calibration package absent"))?;
    // DAG has already validated this replay/calibration context against the
    // new package. Bind its exact source reference to the known immutable base.
    if new_package.conformal_calibration.is_none()
        || new_package
            .conformal_calibration_replay
            .as_ref()
            .is_none_or(|replay| replay.source_training_outcome != old_package.training_outcome)
    {
        return Err(refusal(
            "calibration does not derive from the admitted frozen base",
        ));
    }
    if old_package.execution_bundle.raw_artifact_payloads
        != new_package.execution_bundle.raw_artifact_payloads
    {
        return Err(refusal("calibration changed fitted-state bytes"));
    }
    for state in before.closure.state_members() {
        if fresh.members.get(&state.member_path).map(Vec::as_slice)
            != Some(before.member(&state.member_path)?)
        {
            return Err(refusal(
                "calibration changed fitted-state member bytes/path",
            ));
        }
    }
    // Reuse the already admitted source projection. Fresh owner facets and
    // the physically read source members identify replaced fixed-ID facets;
    // no old package/outcome is rebuilt or scientifically validated again.
    let mut manifest = manifest_from_export(&fresh, container_id, "model")?;
    let mut replaced: BTreeSet<String> = before
        .manifest
        .artifacts
        .iter()
        .filter(|artifact| {
            artifact.owner == "dag-ml"
                && match &artifact.storage {
                    N4aStorage::Embedded { member_paths, .. } => member_paths
                        .iter()
                        .any(|path| before.members.contains_key(path)),
                    _ => false,
                }
        })
        .map(|artifact| artifact.id.clone())
        .collect();
    // DAG already proved every link touching this closure. Project those
    // validated links to include reference facets without rebuilding the source.
    loop {
        let count = replaced.len();
        for link in &before.manifest.links {
            if link.owner == "dag-ml"
                && (replaced.contains(&link.source_artifact)
                    || replaced.contains(&link.target_artifact))
            {
                replaced.insert(link.source_artifact.clone());
                replaced.insert(link.target_artifact.clone());
            }
        }
        if replaced.len() == count {
            break;
        }
    }
    let changed: BTreeSet<_> = before
        .manifest
        .artifacts
        .iter()
        .filter(|old| {
            replaced.contains(old.id.as_str())
                && manifest.artifacts.iter().find(|new| new.id == old.id) != Some(*old)
        })
        .map(|a| a.id.as_str())
        .collect();
    let old_native_entries = external_entrypoints("model", true);
    for artifact in &before.manifest.artifacts {
        if replaced.contains(artifact.id.as_str()) {
            continue;
        }
        if manifest.artifacts.iter().any(|a| a.id == artifact.id)
            || artifact
                .requirements
                .as_ref()
                .is_some_and(|r| changed.contains(r.artifact_id.as_str()))
        {
            return Err(refusal(
                "calibration fixed-ID collision or retained artifact reference rebinding",
            ));
        }
        manifest.artifacts.push(artifact.clone());
    }
    let old_link_ids: BTreeSet<_> = before
        .manifest
        .links
        .iter()
        .filter(|link| {
            link.owner == "dag-ml"
                && (replaced.contains(link.source_artifact.as_str())
                    || replaced.contains(link.target_artifact.as_str()))
        })
        .map(|link| link.id.as_str())
        .collect();
    for link in &before.manifest.links {
        if link.owner == "dag-ml" && old_link_ids.contains(link.id.as_str()) {
            continue;
        }
        if changed.contains(link.source_artifact.as_str())
            || changed.contains(link.target_artifact.as_str())
            || manifest.links.iter().any(|l| l.id == link.id)
        {
            return Err(refusal("calibration would rebind a retained link"));
        }
        manifest.links.push(link.clone());
    }
    for entry in &before.manifest.entrypoints {
        let native = old_native_entries.contains(entry) || entry == &before.entrypoint;
        if native {
            continue;
        }
        if changed.contains(entry.root_artifact.as_str())
            || entry
                .input_artifacts
                .iter()
                .any(|id| changed.contains(id.as_str()))
            || entry
                .requirements
                .as_ref()
                .is_some_and(|r| changed.contains(r.artifact_id.as_str()))
            || manifest.entrypoints.iter().any(|e| e.id == entry.id)
        {
            return Err(refusal("calibration would rebind a retained entrypoint"));
        }
        manifest.entrypoints.push(entry.clone());
    }
    manifest.required_features = before.manifest.required_features.clone();
    let old_paths: BTreeSet<_> = before.members.keys().collect();
    for member in &before.manifest.members {
        if old_paths.contains(&member.path) {
            continue;
        }
        if fresh.members.contains_key(&member.path) {
            return Err(refusal("calibration fresh/retained member collision"));
        }
        manifest.members.push(member.clone());
    }
    Ok((manifest, fresh.members))
}

/// Apply an explicit operation budget without reparsing or reopening a file.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_native_package_from_file(
    file: &crate::n4a_file::N4aFile,
    entrypoint: Option<&str>,
    operation: &str,
    budget: N4aMemoryBudget,
) -> Result<LoadedNativePackage, N4aError> {
    struct BudgetedFile<'a> {
        file: &'a crate::n4a_file::N4aFile,
        budget: N4aMemoryBudget,
    }
    impl N4aMemberBytes for BudgetedFile<'_> {
        fn metadata(&self) -> &N4aMetadata {
            self.file.metadata()
        }
        fn member_bytes(&self, path: &str) -> Result<std::borrow::Cow<'_, [u8]>, N4aError> {
            self.file
                .read_member(path, self.budget)
                .map(std::borrow::Cow::Owned)
        }
        fn resident_bytes(&self) -> u64 {
            0
        }
    }
    read_native_package_for_operation(
        &BudgetedFile { file, budget },
        entrypoint,
        operation,
        budget,
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub fn write_calibrated_native_package_file(
    path: &std::path::Path,
    base: &crate::n4a_file::N4aFile,
    before: &LoadedNativePackage,
    outcome: &[u8],
    package: &[u8],
    container_id: &str,
    budget: N4aMemoryBudget,
) -> Result<NativePackageReference, N4aError> {
    use crate::n4a_file::{write_n4a_file, N4aStreamBase, N4aStreamInput, N4aStreamRequest};
    let (manifest, members) = calibration_update(base, before, outcome, package, container_id)?;
    let receipt = write_n4a_file(
        path,
        N4aStreamRequest {
            manifest,
            members: members
                .iter()
                .map(|(path, bytes)| N4aStreamInput {
                    path,
                    source: bytes,
                })
                .collect(),
            base: Some(N4aStreamBase::File(base)),
        },
        budget,
    )?;
    Ok(NativePackageReference {
        archive_id: container_id.into(),
        archive_sha256: receipt.container_sha256,
        package_version: 2,
    })
}

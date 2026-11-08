//! Typed IO `DatasetPackage` provider, free of any Methods runtime or file IO.
//!
//! IO remains the authority for package materialization and numeric buffers;
//! DAG-ML remains the authority for folds, training, selection and replay.
//! This adapter is compiled into the native crate and, through
//! `n4a_owner_portable`, into `wasm-native`; each crate uses a subset of it.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use dag_ml_core::{
    data_binding_requirement_key, BundleDataRequirement, ConformalCalibrationTruth, ControllerId,
    DataBinding, DataMaterializationRequest, DataProviderViewSpec, DataViewRequest,
    EntityUnitLevel, ExternalDataPlanEnvelope, HandleKind, HandleRef, MethodsPlsData,
    MethodsPlsDataRequest, MethodsPlsDataset, MethodsPlsMatrix, Phase, RuntimeDataProvider,
    SampleRelation, SampleRelationSet, TrainingDataIdentity, TrainingReplayOutcome,
};
use dag_ml_data_crate::{
    CoordinatorDataMaterializationRequest, CoordinatorHandleKind, DataView, SampleId as IoSampleId,
    SourceId as IoSourceId,
};
use dag_ml_data_provider_crate::DagMlDataProvider;
use nirs4all_io_dagml::{DatasetPackage, PackageProvider};

#[derive(Clone, Debug, PartialEq)]
struct ViewSlot {
    node_id: dag_ml_core::NodeId,
    input_name: String,
    phase: Phase,
    variant_id: Option<dag_ml_core::VariantId>,
    fold_id: Option<dag_ml_core::FoldId>,
    view: DataProviderViewSpec,
    handle: u64,
}

#[derive(Default)]
struct ProviderHandles {
    data: BTreeSet<u64>,
    views: Vec<ViewSlot>,
}

/// Complete row-major IO buffers of one training cohort, aligned by the caller's
/// sample identities. `y` holds one column per IO target, in target-name order.
pub(crate) struct ProviderTrainingMatrices {
    pub(crate) x: MethodsPlsMatrix,
    pub(crate) y: MethodsPlsMatrix,
}

/// Runtime adapter over one exact numeric source selected from an IO package.
///
/// The adapter does not own or reinterpret feature buffers. It forwards every
/// scheduler-created identity view to IO's `PackageProvider`, projects the
/// resulting typed `f64` block, and releases all provider handles explicitly.
pub struct DatasetPackageMethodsProvider {
    pub(crate) provider: PackageProvider,
    external_envelope: ExternalDataPlanEnvelope,
    pub(crate) relations: SampleRelationSet,
    source_id: String,
    target_masks: Option<BTreeMap<String, Vec<bool>>>,
    handles: Mutex<ProviderHandles>,
    #[cfg(test)]
    pub(crate) training_views: Mutex<Vec<(Phase, DataProviderViewSpec)>>,
}

impl DatasetPackageMethodsProvider {
    /// Select one source from a dense numeric package. Fusion, N-D tensors and
    /// named processing stacks remain fail-closed in the IO owner.
    pub fn new(package: &DatasetPackage, source_id: &str) -> Result<Self, String> {
        let result = Self::from_package(package, source_id, false, false)?;
        result.require_complete_targets()?;
        Ok(result)
    }

    pub(crate) fn require_complete_targets(&self) -> Result<(), String> {
        if self
            .target_masks
            .as_ref()
            .is_some_and(|rows| rows.values().any(|row| row.contains(&false)))
        {
            return Err("Legacy complete-target workflow refuses partial target masks".into());
        }
        Ok(())
    }

    pub(crate) fn for_product_pipeline(
        package: &DatasetPackage,
        source_id: &str,
        classification: bool,
    ) -> Result<Self, String> {
        Self::from_package(package, source_id, classification, true)
    }

    fn from_package(
        package: &DatasetPackage,
        source_id: &str,
        classification: bool,
        training_scope: bool,
    ) -> Result<Self, String> {
        if package.task_type
            != if classification {
                "classification"
            } else {
                "regression"
            }
        {
            return Err(format!(
                "Core native pipeline role/task_type disagree, got `{}`",
                package.task_type
            ));
        }
        let provider = PackageProvider::from_package_source(package, source_id)
            .map_err(|error| error.to_string())?;
        if provider.target_names().is_empty() || provider.target_ids().is_empty() {
            return Err("Core Methods package training requires numeric targets".to_string());
        }
        let mut relations = convert_relations(
            provider
                .envelope()
                .coordinator_relations
                .as_ref()
                .ok_or("Core Methods package training requires coordinator relations")?,
        )?;
        let mut target_masks = None;
        for audit in &package.audits {
            if let Some(mask) = audit.get("target_mask").filter(|mask| !mask.is_null()) {
                let ids: Vec<String> = serde_json::from_value(audit["input_sample_ids"].clone())
                    .map_err(|e| e.to_string())?;
                let rows = mask["values"]
                    .as_array()
                    .ok_or("Invalid native target mask rows")?;
                if rows.len() != ids.len() {
                    return Err("Native mask/sample dimensions disagree".into());
                }
                let mut keyed = BTreeMap::new();
                for (id, row) in ids.into_iter().zip(rows) {
                    let cells = if let Some(value) = row.as_bool() {
                        vec![value]
                    } else {
                        row.as_array()
                            .ok_or("Invalid native target mask row")?
                            .iter()
                            .map(|value| value.as_bool().ok_or("Invalid native target mask cell"))
                            .collect::<Result<Vec<_>, _>>()?
                    };
                    if cells.len() != provider.target_names().len()
                        || keyed.insert(id, cells).is_some()
                    {
                        return Err("Native target mask schema or identity mismatch".into());
                    }
                }
                if keyed.len() != relations.records.len()
                    || relations
                        .records
                        .iter()
                        .any(|r| !keyed.contains_key(r.sample_id.as_str()))
                {
                    return Err("Native target mask identities disagree with provider".into());
                }
                target_masks = Some(keyed);
            }
        }
        if training_scope {
            for record in &relations.records {
                let partitions: Vec<_> = record
                    .tags
                    .iter()
                    .filter(|tag| tag.starts_with("partition:"))
                    .map(String::as_str)
                    .collect();
                if !matches!(
                    partitions.as_slice(),
                    ["partition:train" | "partition:test" | "partition:predict"]
                ) {
                    return Err("Native training requires one train, test or predict partition tag per relation".into());
                }
            }
            // IO keeps the complete record. Only the external training contract
            // is scoped; mask identities were validated against all rows above.
            relations
                .records
                .retain(|record| record.tags.iter().any(|tag| tag == "partition:train"));
            if relations.records.is_empty() {
                return Err("Native training requires at least one train relation".into());
            }
            if let Some(masks) = &mut target_masks {
                let train_ids: BTreeSet<_> = relations
                    .records
                    .iter()
                    .map(|record| record.sample_id.as_str())
                    .collect();
                masks.retain(|id, _| train_ids.contains(id.as_str()));
            }
        }
        let relation_fingerprint = relations.fingerprint().map_err(|error| error.to_string())?;
        let mut external_envelope = ExternalDataPlanEnvelope {
            schema_version: 1,
            schema_fingerprint: provider.envelope().schema_fingerprint.clone(),
            plan_fingerprint: provider.envelope().plan_fingerprint.clone(),
            relation_fingerprint: Some(relation_fingerprint),
            data_content_fingerprint: provider.envelope().data_content_fingerprint.clone(),
            target_content_fingerprint: provider.envelope().target_content_fingerprint.clone(),
            coordinator_relations: Some(relations.clone()),
            predict_cohort: None,
        };
        if let Some(masks) = &target_masks {
            use sha2::{Digest, Sha256};
            let identity = serde_json::to_vec(&serde_json::json!({"target_content_fingerprint":external_envelope.target_content_fingerprint,"target_names":provider.target_names(),"observed_by_sample":masks})).map_err(|e|e.to_string())?;
            external_envelope.target_content_fingerprint =
                Some(format!("{:x}", Sha256::digest(identity)));
        }
        external_envelope
            .validate()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            provider,
            external_envelope,
            relations,
            source_id: source_id.to_string(),
            target_masks,
            handles: Mutex::new(ProviderHandles::default()),
            #[cfg(test)]
            training_views: Mutex::new(Vec::new()),
        })
    }

    pub fn external_envelope(&self) -> &ExternalDataPlanEnvelope {
        &self.external_envelope
    }

    pub fn relations(&self) -> &SampleRelationSet {
        &self.relations
    }

    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    pub(crate) fn bind_replay_requirement(
        &mut self,
        requirement: &BundleDataRequirement,
    ) -> Result<(), String> {
        requirement.validate().map_err(|error| error.to_string())?;
        if requirement.feature_set_id.as_deref() != Some(self.provider.feature_set_id())
            || requirement.output_representation
                != self.provider.envelope().plan.output_representation.as_str()
        {
            return Err(format!(
                "Core calibration source `{}` is incompatible with signed data requirement `{}`",
                self.source_id,
                requirement.key()
            ));
        }
        // The bundle owns the training-time structural contract. A held-out
        // package naturally has different concrete relation/fold metadata,
        // while IO remains authoritative for its buffers and content proofs.
        self.external_envelope.schema_fingerprint = requirement.schema_fingerprint.clone();
        self.external_envelope.plan_fingerprint = requirement.plan_fingerprint.clone();
        self.external_envelope
            .validate()
            .map_err(|error| error.to_string())
    }

    /// Explicitly release all IO provider handles. `Drop` invokes the same
    /// operation, so early-return and error paths are deterministic too.
    pub fn release_all(&self) {
        let Ok(mut handles) = self.handles.lock() else {
            return;
        };
        for slot in handles.views.drain(..).rev() {
            self.provider.release(slot.handle);
        }
        for handle in std::mem::take(&mut handles.data).into_iter().rev() {
            self.provider.release(handle);
        }
    }

    fn validate_binding(&self, binding: &DataBinding) -> Result<(), String> {
        binding
            .validate_envelope(&self.external_envelope)
            .map_err(|error| error.to_string())?;
        if binding.feature_set_id() != self.provider.feature_set_id()
            || binding.source_ids.as_slice() != [self.source_id.as_str()]
            || binding.output_representation
                != self.provider.envelope().plan.output_representation.as_str()
        {
            return Err(format!(
                "Core Methods package binding `{}` must select only IO source `{}` and its exact output representation",
                data_binding_requirement_key(&binding.node_id, &binding.input_name),
                self.source_id
            ));
        }
        Ok(())
    }

    fn view_handle(
        &self,
        request: &MethodsPlsDataRequest,
        view: &DataProviderViewSpec,
    ) -> Result<u64, String> {
        let handles = self
            .handles
            .lock()
            .map_err(|_| "Core IO provider handle registry is poisoned".to_string())?;
        handles
            .views
            .iter()
            .rev()
            .find(|slot| {
                slot.node_id == request.node_id
                    && slot.input_name == request.binding.input_name
                    && slot.phase == request.phase
                    && slot.variant_id == request.variant_id
                    && slot.fold_id == request.fold_id
                    && slot.view == *view
            })
            .map(|slot| slot.handle)
            .ok_or_else(|| {
                format!(
                    "Core IO provider has no scheduler-created view for `{}.{}` {:?}",
                    request.node_id, request.binding.input_name, request.phase
                )
            })
    }

    fn project_dataset(
        &self,
        request: &MethodsPlsDataRequest,
        view: &DataProviderViewSpec,
        require_targets: bool,
    ) -> Result<MethodsPlsDataset, String> {
        let view_handle = self.view_handle(request, view)?;
        self.project_view(view_handle, require_targets)
    }

    /// Project one IO view through the typed feature and target blocks.
    fn project_view(
        &self,
        view_handle: u64,
        require_targets: bool,
    ) -> Result<MethodsPlsDataset, String> {
        let feature = self
            .provider
            .feature_block_f64(view_handle)
            .map_err(|error| error.to_string())?;
        if feature.feature_set_id != self.provider.feature_set_id()
            || feature.observation_ids.len() != feature.sample_ids.len()
            || feature
                .sample_ids
                .len()
                .checked_mul(feature.feature_names.len())
                != Some(feature.values.len())
        {
            return Err("Core IO provider returned an invalid typed feature block".to_string());
        }
        let sample_ids = feature
            .sample_ids
            .iter()
            .map(|id| dag_ml_core::SampleId::new(id.as_str()).map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        let target_names = self.provider.target_names().to_vec();
        let y = if require_targets {
            Some(self.project_targets(view_handle, &feature.sample_ids, &target_names)?)
        } else {
            None
        };
        let y_validity_masks = if require_targets {
            self.target_masks
                .as_ref()
                .map(|masks| {
                    sample_ids
                        .iter()
                        .map(|id| {
                            masks.get(id.as_str()).cloned().ok_or_else(|| {
                                "Native target mask missing sample identity".to_string()
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
        } else {
            None
        };
        let dataset = MethodsPlsDataset {
            y_validity_masks,
            sample_ids,
            x: MethodsPlsMatrix {
                rows: feature.sample_ids.len(),
                cols: feature.feature_names.len(),
                values: feature.values,
            },
            y,
            target_names,
        };
        dataset
            .validate("IO DatasetPackage", require_targets)
            .map_err(|error| error.to_string())?;
        Ok(dataset)
    }

    fn project_targets(
        &self,
        view_handle: u64,
        sample_ids: &[IoSampleId],
        target_names: &[String],
    ) -> Result<MethodsPlsMatrix, String> {
        let target_ids = self.provider.target_ids();
        let values = if target_ids.len() == 1 && target_names.len() > 1 {
            let block = self
                .provider
                .target_block(view_handle, &target_ids[0])
                .map_err(|error| error.to_string())?;
            validate_target_samples(sample_ids, &block.sample_ids)?;
            let mut values = Vec::with_capacity(sample_ids.len() * target_names.len());
            for value in &block.values {
                let row = value
                    .as_array()
                    .ok_or("Core IO provider multi-target block contains a non-array row")?;
                if row.len() != target_names.len() {
                    return Err(
                        "Core IO provider multi-target block width differs from target names"
                            .to_string(),
                    );
                }
                for cell in row {
                    values.push(finite_number(cell, "multi-target cell")?);
                }
            }
            values
        } else {
            if target_ids.len() != target_names.len() {
                return Err(
                    "Core IO provider target identifiers differ from target-column names"
                        .to_string(),
                );
            }
            let blocks = target_ids
                .iter()
                .map(|target_id| {
                    self.provider
                        .target_block(view_handle, target_id)
                        .map_err(|error| error.to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            for block in &blocks {
                validate_target_samples(sample_ids, &block.sample_ids)?;
            }
            let mut values = Vec::with_capacity(sample_ids.len() * target_names.len());
            for row in 0..sample_ids.len() {
                for block in &blocks {
                    values.push(finite_number(&block.values[row], "target cell")?);
                }
            }
            values
        };
        Ok(MethodsPlsMatrix {
            values,
            rows: sample_ids.len(),
            cols: target_names.len(),
        })
    }

    /// Read every requested training row once, through the same typed IO view
    /// projection scheduler-created views use, and return it in `sample_ids`
    /// order. Temporary handles are released on every path. This reads buffers
    /// only; it derives no identity and never touches a fold or a partition.
    pub(crate) fn training_matrices(
        &self,
        run_id: &str,
        sample_ids: &[String],
    ) -> Result<ProviderTrainingMatrices, String> {
        let view_ids = sample_ids
            .iter()
            .map(|id| IoSampleId::new(id.as_str()).map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        let source_id = IoSourceId::new(&self.source_id).map_err(|error| error.to_string())?;
        let envelope = self.provider.envelope();
        let mut handles = Vec::new();
        let result = (|| {
            let data = self
                .provider
                .materialize(&CoordinatorDataMaterializationRequest {
                    run_id: run_id.to_string(),
                    node_id: "training_matrices".into(),
                    input_name: "x".into(),
                    phase: Phase::Refit.as_str().into(),
                    variant_id: None,
                    fold_id: None,
                    request_id: run_id.to_string(),
                    schema_fingerprint: envelope.schema_fingerprint.clone(),
                    plan_fingerprint: envelope.plan_fingerprint.clone(),
                    relation_fingerprint: envelope.relation_fingerprint.clone(),
                    output_representation: envelope.plan.output_representation.clone(),
                    source_ids: vec![source_id.clone()],
                    require_relations: true,
                    predict_cohort: None,
                })
                .map_err(|error| error.to_string())?;
            handles.push(data.handle.handle);
            if data.handle.kind != CoordinatorHandleKind::Data {
                return Err("Core training IO returned a non-data handle".to_string());
            }
            let view = self
                .provider
                .make_view(
                    data.handle.handle,
                    &DataView {
                        sample_ids: Some(view_ids.clone()),
                        source_ids: Some(vec![source_id]),
                        include_augmented: false,
                        include_excluded: false,
                        ..Default::default()
                    },
                )
                .map_err(|error| error.to_string())?;
            handles.push(view.handle.handle);
            if view.handle.kind != CoordinatorHandleKind::View
                || view.parent_handle != data.handle
                || view.sample_count != sample_ids.len()
            {
                return Err("Core training IO view differs from its sample identities".to_string());
            }
            self.project_view(view.handle.handle, true)
        })();
        let mut released = true;
        for handle in handles.into_iter().rev() {
            released &= self.provider.release(handle);
        }
        if !released {
            return Err("Core training IO failed to release temporary handles".into());
        }
        let dataset = result?;
        let y = dataset
            .y
            .as_ref()
            .ok_or("Core training IO returned no target block")?;
        // Rows are joined by identity: the view may deliver its own order.
        let rows = dataset
            .sample_ids
            .iter()
            .enumerate()
            .map(|(row, id)| (id.as_str(), row))
            .collect::<BTreeMap<_, _>>();
        let order = sample_ids
            .iter()
            .map(|id| {
                rows.get(id.as_str())
                    .copied()
                    .ok_or("Core training IO block omits a requested sample")
            })
            .collect::<Result<Vec<_>, _>>()?;
        if order.len() != dataset.sample_ids.len() {
            return Err("Core training IO block row count differs from its identities".into());
        }
        let select = |matrix: &MethodsPlsMatrix| MethodsPlsMatrix {
            values: order
                .iter()
                .flat_map(|row| {
                    matrix.values[row * matrix.cols..(row + 1) * matrix.cols]
                        .iter()
                        .copied()
                })
                .collect(),
            rows: matrix.rows,
            cols: matrix.cols,
        };
        Ok(ProviderTrainingMatrices {
            x: select(&dataset.x),
            y: select(y),
        })
    }

    /// Read complete regression truth from this package in replay row and
    /// selected binding target order. Construct this adapter with `new` over
    /// the same immutable package retained for the admitted replay.
    ///
    /// These IO handles belong only to this read. Admitted replay uses DAG's
    /// private provider, so no scheduler-created Core PREDICT view is required.
    pub(crate) fn conformal_truth_by_name(
        &self,
        replay: &TrainingReplayOutcome,
        binding_id: &str,
    ) -> Result<ConformalCalibrationTruth, String> {
        replay.validate().map_err(|error| error.to_string())?;
        if replay.phase != Phase::Predict {
            return Err("Core named calibration truth requires a PREDICT replay".into());
        }
        let output = replay
            .outputs
            .iter()
            .find(|output| output.binding.binding_id == binding_id)
            .ok_or("Core calibration replay has no requested output binding")?;
        if output.binding.prediction_kind != dag_ml_core::PredictionKind::RegressionPoint {
            return Err("Core named calibration truth requires regression point output".into());
        }
        let [point] = output.predictions.as_slice() else {
            return Err(
                "Core calibration replay requires one terminal point-prediction block".into(),
            );
        };
        let width = point
            .validate_content()
            .map_err(|error| error.to_string())?;
        let selected_names = &output.binding.target_names;
        if point.sample_ids.is_empty()
            || selected_names.is_empty()
            || point.target_names != *selected_names
            || width != selected_names.len()
            || selected_names.iter().collect::<BTreeSet<_>>().len() != selected_names.len()
        {
            return Err("Core calibration point identities or target schema are invalid".into());
        }
        if self
            .target_masks
            .as_ref()
            .is_some_and(|rows| rows.values().any(|row| row.contains(&false)))
        {
            return Err("Core named calibration truth refuses partial target masks".into());
        }

        let package_names = self.provider.target_names();
        if package_names.is_empty()
            || package_names.iter().any(|name| name.trim().is_empty())
            || package_names.iter().collect::<BTreeSet<_>>().len() != package_names.len()
        {
            return Err("Core calibration IO target names are empty or ambiguous".into());
        }
        let columns = selected_names
            .iter()
            .map(|name| {
                package_names
                    .iter()
                    .position(|candidate| candidate == name)
                    .ok_or_else(|| format!("Core calibration IO package has no target `{name}`"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        // IO emits homogeneous regression columns as one matrix-target table,
        // or a scalar table for one column. Never zip sorted target IDs with
        // column names or reproduce IO's target-ID sanitization here.
        let [target_id] = self.provider.target_ids() else {
            return Err(
                "Core calibration requires one IO scalar or matrix regression target table".into(),
            );
        };
        let sample_ids = point
            .sample_ids
            .iter()
            .map(|id| IoSampleId::new(id.as_str()).map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        let requested = sample_ids.iter().cloned().collect::<BTreeSet<_>>();
        let source_id = IoSourceId::new(&self.source_id).map_err(|error| error.to_string())?;
        let envelope = self.provider.envelope();

        // Keep temporary handles out of the replay cache, and release every
        // handle even when view construction or target validation fails.
        let mut handles = Vec::new();
        let result = (|| {
            let data = self
                .provider
                .materialize(&CoordinatorDataMaterializationRequest {
                    run_id: replay.run_id.to_string(),
                    node_id: output.binding.node_id.to_string(),
                    input_name: "calibration_truth".into(),
                    phase: Phase::Predict.as_str().into(),
                    variant_id: None,
                    fold_id: None,
                    request_id: replay.replay_request_id.clone(),
                    schema_fingerprint: envelope.schema_fingerprint.clone(),
                    plan_fingerprint: envelope.plan_fingerprint.clone(),
                    relation_fingerprint: envelope.relation_fingerprint.clone(),
                    output_representation: envelope.plan.output_representation.clone(),
                    source_ids: vec![source_id.clone()],
                    require_relations: true,
                    predict_cohort: None,
                })
                .map_err(|error| error.to_string())?;
            handles.push(data.handle.handle);
            if data.handle.kind != CoordinatorHandleKind::Data {
                return Err("Core calibration IO returned a non-data handle".into());
            }
            let view = self
                .provider
                .make_view(
                    data.handle.handle,
                    &DataView {
                        sample_ids: Some(sample_ids.clone()),
                        source_ids: Some(vec![source_id]),
                        include_augmented: false,
                        include_excluded: false,
                        ..Default::default()
                    },
                )
                .map_err(|error| error.to_string())?;
            handles.push(view.handle.handle);
            if view.handle.kind != CoordinatorHandleKind::View
                || view.parent_handle != data.handle
                || view.sample_count != sample_ids.len()
            {
                return Err(
                    "Core calibration IO view differs from replay sample identities".into(),
                );
            }
            let block = self
                .provider
                .target_block(view.handle.handle, target_id)
                .map_err(|error| error.to_string())?;
            if block.target_id != *target_id
                || block.sample_ids.len() != sample_ids.len()
                || block.values.len() != sample_ids.len()
            {
                return Err("Core calibration IO target block is incomplete or mismatched".into());
            }
            let mut keyed = BTreeMap::new();
            for (id, value) in block.sample_ids.into_iter().zip(block.values) {
                let row = if package_names.len() == 1 {
                    vec![finite_number(&value, "calibration target cell")?]
                } else {
                    let cells = value
                        .as_array()
                        .filter(|cells| cells.len() == package_names.len())
                        .ok_or("Core calibration IO target row differs from named columns")?;
                    cells
                        .iter()
                        .map(|cell| finite_number(cell, "calibration target cell"))
                        .collect::<Result<Vec<_>, _>>()?
                };
                if !requested.contains(&id) || keyed.insert(id, row).is_some() {
                    return Err(
                        "Core calibration IO target samples are unexpected or duplicated".into(),
                    );
                }
            }
            let values = sample_ids
                .iter()
                .map(|id| {
                    let row = keyed
                        .get(id)
                        .ok_or("Core calibration IO target sample is missing")?;
                    Ok(columns.iter().map(|column| row[*column]).collect())
                })
                .collect::<Result<Vec<Vec<f64>>, String>>()?;
            Ok(ConformalCalibrationTruth {
                sample_ids: point.sample_ids.clone(),
                values,
            })
        })();
        let mut released = true;
        for handle in handles.into_iter().rev() {
            released &= self.provider.release(handle);
        }
        if !released {
            return Err("Core calibration IO failed to release temporary truth handles".into());
        }
        result
    }

    pub(crate) fn conformal_truth(
        &self,
        replay: &TrainingReplayOutcome,
        binding_id: &str,
    ) -> Result<ConformalCalibrationTruth, String> {
        let output = replay
            .outputs
            .iter()
            .find(|output| output.binding.binding_id == binding_id)
            .ok_or("Core calibration replay has no requested output binding")?;
        if output.binding.target_names != self.provider.target_names() {
            return Err(
                "Core calibration target order differs from the replay output binding".to_string(),
            );
        }
        let [point] = output.predictions.as_slice() else {
            return Err(
                "Core calibration replay requires one terminal point-prediction block".to_string(),
            );
        };
        let handles = self
            .handles
            .lock()
            .map_err(|_| "Core IO provider handle registry is poisoned".to_string())?;
        let predict_views = handles
            .views
            .iter()
            .filter(|slot| slot.phase == Phase::Predict)
            .collect::<Vec<_>>();
        let [slot] = predict_views.as_slice() else {
            return Err(format!(
                "Core calibration replay requires exactly one IO PREDICT view, got {}",
                predict_views.len()
            ));
        };
        let feature = self
            .provider
            .feature_block_f64(slot.handle)
            .map_err(|error| error.to_string())?;
        let sample_ids = feature
            .sample_ids
            .iter()
            .map(|id| dag_ml_core::SampleId::new(id.as_str()).map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        if sample_ids != point.sample_ids {
            return Err(
                "Core calibration truth rows differ from replay point-prediction identities"
                    .to_string(),
            );
        }
        let targets = self.project_targets(
            slot.handle,
            &feature.sample_ids,
            self.provider.target_names(),
        )?;
        let values = targets
            .values
            .chunks_exact(targets.cols)
            .map(|row| row.to_vec())
            .collect();
        Ok(ConformalCalibrationTruth { sample_ids, values })
    }
}

impl Drop for DatasetPackageMethodsProvider {
    fn drop(&mut self) {
        self.release_all();
    }
}

impl RuntimeDataProvider for DatasetPackageMethodsProvider {
    fn materialize(&self, request: &DataMaterializationRequest) -> dag_ml_core::Result<HandleRef> {
        self.validate_binding(&request.binding)
            .map_err(dag_ml_core::DagMlError::RuntimeValidation)?;
        let io_request = CoordinatorDataMaterializationRequest {
            run_id: request.run_id.to_string(),
            node_id: request.node_id.to_string(),
            input_name: request.input_name.clone(),
            phase: request.phase.as_str().to_string(),
            variant_id: request.variant_id.as_ref().map(ToString::to_string),
            fold_id: request.fold_id.as_ref().map(ToString::to_string),
            request_id: request.binding.request_id.clone(),
            schema_fingerprint: self.provider.envelope().schema_fingerprint.clone(),
            plan_fingerprint: self.provider.envelope().plan_fingerprint.clone(),
            relation_fingerprint: self.provider.envelope().relation_fingerprint.clone(),
            output_representation: self.provider.envelope().plan.output_representation.clone(),
            source_ids: vec![IoSourceId::new(&self.source_id)
                .map_err(|error| dag_ml_core::DagMlError::RuntimeValidation(error.to_string()))?],
            require_relations: request.binding.require_relations,
            predict_cohort: request
                .predict_cohort
                .as_ref()
                .map(|cohort| {
                    serde_json::to_value(cohort)
                        .and_then(serde_json::from_value)
                        .map_err(|error| {
                            dag_ml_core::DagMlError::RuntimeValidation(error.to_string())
                        })
                })
                .transpose()?,
        };
        let record = self
            .provider
            .materialize(&io_request)
            .map_err(|error| dag_ml_core::DagMlError::RuntimeValidation(error.to_string()))?;
        if record.handle.kind != CoordinatorHandleKind::Data {
            return Err(dag_ml_core::DagMlError::RuntimeValidation(
                "IO PackageProvider returned a non-data materialization handle".to_string(),
            ));
        }
        self.handles
            .lock()
            .map_err(|_| {
                dag_ml_core::DagMlError::RuntimeValidation(
                    "Core IO provider handle registry is poisoned".to_string(),
                )
            })?
            .data
            .insert(record.handle.handle);
        Ok(HandleRef {
            handle: record.handle.handle,
            kind: HandleKind::Data,
            owner_controller: ControllerId::new(record.handle.owner_controller)
                .map_err(|error| dag_ml_core::DagMlError::RuntimeValidation(error.to_string()))?,
        })
    }

    fn make_view(&self, request: &DataViewRequest) -> dag_ml_core::Result<HandleRef> {
        self.validate_binding(&request.binding)
            .map_err(dag_ml_core::DagMlError::RuntimeValidation)?;
        let mut handles = self.handles.lock().map_err(|_| {
            dag_ml_core::DagMlError::RuntimeValidation(
                "Core IO provider handle registry is poisoned".to_string(),
            )
        })?;
        if !handles.data.contains(&request.data_handle.handle) {
            return Err(dag_ml_core::DagMlError::RuntimeValidation(format!(
                "Core IO provider does not own data handle {}",
                request.data_handle.handle
            )));
        }
        if request.view.branch_view.is_some() {
            return Err(dag_ml_core::DagMlError::RuntimeValidation(
                "Core DATA-002 Methods training does not support branch/fusion views".to_string(),
            ));
        }
        let view = DataView {
            sample_ids: request
                .view
                .sample_ids
                .as_ref()
                .map(|ids| {
                    ids.iter()
                        .map(|id| IoSampleId::new(id.as_str()).map_err(|error| error.to_string()))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()
                .map_err(dag_ml_core::DagMlError::RuntimeValidation)?,
            partition: None,
            fold_id: None,
            source_ids: Some(vec![IoSourceId::new(&self.source_id).map_err(|error| {
                dag_ml_core::DagMlError::RuntimeValidation(error.to_string())
            })?]),
            columns: request.view.columns.clone(),
            include_augmented: request.view.include_augmented,
            include_excluded: request.view.include_excluded,
            branch_view: None,
            extra: request.view.extra.clone(),
        };
        let record = self
            .provider
            .make_view(request.data_handle.handle, &view)
            .map_err(|error| dag_ml_core::DagMlError::RuntimeValidation(error.to_string()))?;
        if record.handle.kind != CoordinatorHandleKind::View {
            return Err(dag_ml_core::DagMlError::RuntimeValidation(
                "IO PackageProvider returned a non-view handle".to_string(),
            ));
        }
        handles.views.push(ViewSlot {
            node_id: request.node_id.clone(),
            input_name: request.input_name.clone(),
            phase: request.phase,
            variant_id: request.variant_id.clone(),
            fold_id: request.fold_id.clone(),
            view: request.view.clone(),
            handle: record.handle.handle,
        });
        #[cfg(test)]
        self.training_views
            .lock()
            .unwrap()
            .push((request.phase, request.view.clone()));
        Ok(HandleRef {
            handle: record.handle.handle,
            kind: HandleKind::DataView,
            owner_controller: ControllerId::new(record.handle.owner_controller)
                .map_err(|error| dag_ml_core::DagMlError::RuntimeValidation(error.to_string()))?,
        })
    }

    fn training_data_identity(
        &self,
        binding: &DataBinding,
    ) -> dag_ml_core::Result<Option<TrainingDataIdentity>> {
        self.validate_binding(binding)
            .map_err(dag_ml_core::DagMlError::RuntimeValidation)?;
        TrainingDataIdentity::from_binding_envelope(binding, &self.external_envelope).map(Some)
    }

    fn coordinator_relations(
        &self,
        binding: &DataBinding,
    ) -> dag_ml_core::Result<Option<SampleRelationSet>> {
        self.validate_binding(binding)
            .map_err(dag_ml_core::DagMlError::RuntimeValidation)?;
        Ok(Some(self.relations.clone()))
    }

    fn methods_pls_capability(&self) -> dag_ml_core::Result<()> {
        Ok(())
    }

    fn preflight_methods_pls(&self, request: &MethodsPlsDataRequest) -> dag_ml_core::Result<()> {
        request.validate()?;
        self.validate_binding(&request.binding)
            .map_err(dag_ml_core::DagMlError::RuntimeValidation)
    }

    fn methods_pls_data(
        &self,
        request: &MethodsPlsDataRequest,
    ) -> dag_ml_core::Result<MethodsPlsData> {
        self.preflight_methods_pls(request)?;
        let fit = self
            .project_dataset(request, &request.fit_view, request.phase != Phase::Predict)
            .map_err(dag_ml_core::DagMlError::RuntimeValidation)?;
        let prediction = request
            .prediction_view
            .as_ref()
            .map(|view| self.project_dataset(request, view, true))
            .transpose()
            .map_err(dag_ml_core::DagMlError::RuntimeValidation)?;
        let data = MethodsPlsData { fit, prediction };
        data.validate_for(request)?;
        Ok(data)
    }
}

pub(crate) fn convert_relations(
    relations: &dag_ml_data_crate::CoordinatorRelationSet,
) -> Result<SampleRelationSet, String> {
    let records = relations
        .records
        .iter()
        .map(|record| {
            Ok(SampleRelation {
                unit_level: EntityUnitLevel::Observation,
                unit_id: None,
                observation_id: dag_ml_core::ObservationId::new(record.observation_id.as_str())
                    .map_err(|error| error.to_string())?,
                sample_id: dag_ml_core::SampleId::new(record.sample_id.as_str())
                    .map_err(|error| error.to_string())?,
                source_id: record.source_id.as_ref().map(ToString::to_string),
                rep_id: None,
                target_id: record
                    .target_id
                    .as_ref()
                    .map(|id| dag_ml_core::TargetId::new(id.as_str()))
                    .transpose()
                    .map_err(|error| error.to_string())?,
                group_id: record
                    .group_id
                    .as_ref()
                    .map(|id| dag_ml_core::GroupId::new(id.as_str()))
                    .transpose()
                    .map_err(|error| error.to_string())?,
                origin_sample_id: record
                    .origin_sample_id
                    .as_ref()
                    .map(|id| dag_ml_core::SampleId::new(id.as_str()))
                    .transpose()
                    .map_err(|error| error.to_string())?,
                derived_unit_id: None,
                component_observation_ids: Vec::new(),
                sample_influence_weight: None,
                quality_flag: None,
                is_augmented: record.is_augmented,
                excluded: record.excluded,
                metadata: record.metadata.clone(),
                tags: record.tags.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let converted = SampleRelationSet { records };
    converted.validate().map_err(|error| error.to_string())?;
    Ok(converted)
}

fn validate_target_samples(expected: &[IoSampleId], actual: &[IoSampleId]) -> Result<(), String> {
    if expected != actual {
        return Err(
            "Core IO provider target rows differ from feature sample identities".to_string(),
        );
    }
    Ok(())
}

fn finite_number(value: &serde_json::Value, label: &str) -> Result<f64, String> {
    let number = value
        .as_f64()
        .ok_or_else(|| format!("Core IO provider {label} is not numeric"))?;
    if !number.is_finite() {
        return Err(format!("Core IO provider {label} is non-finite"));
    }
    Ok(number)
}

pub(crate) fn validate_disjoint_calibration_relations(
    training: &SampleRelationSet,
    calibration: &SampleRelationSet,
) -> Result<(), String> {
    let training_ids = training
        .records
        .iter()
        .flat_map(|record| {
            std::iter::once(&record.sample_id).chain(record.origin_sample_id.as_ref())
        })
        .collect::<BTreeSet<_>>();
    if let Some(overlap) = calibration
        .records
        .iter()
        .flat_map(|record| {
            std::iter::once(&record.sample_id).chain(record.origin_sample_id.as_ref())
        })
        .find(|sample_id| training_ids.contains(sample_id))
    {
        return Err(format!(
            "Core conformal calibration identity `{overlap}` overlaps training"
        ));
    }
    let training_observations = training
        .records
        .iter()
        .map(|record| &record.observation_id)
        .collect::<BTreeSet<_>>();
    if let Some(overlap) = calibration
        .records
        .iter()
        .map(|record| &record.observation_id)
        .find(|observation_id| training_observations.contains(observation_id))
    {
        return Err(format!(
            "Core conformal calibration observation `{overlap}` overlaps training"
        ));
    }
    Ok(())
}

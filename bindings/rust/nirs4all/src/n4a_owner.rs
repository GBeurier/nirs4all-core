//! Report-only DAG semantic inspection of immutable canonical containers.
//!
//! Core translates neutral descriptors and supplies exact member bytes. DAG
//! owns parsing, identity, physical state closure and scientific link meaning.
//! No report is executable admission or native numerical-state validation.
//! Foreign payloads and links remain opaque; references are never fetched.

use std::borrow::Cow;
use std::collections::BTreeMap;

use dag_ml_core::artifact_facets::{
    inspect_facet_model_states, validate_artifact_facet_links, validate_facet_members,
    validate_owner_facet_bytes, ArtifactFacet, ArtifactFacetGraph, ArtifactFacetKind,
    ArtifactFacetLink, ArtifactFacetRelation, FacetLinkValidation, FacetModelStateInspection,
    FacetRefusal, FacetResult, FacetStateMember, FacetValidationContext,
};

use crate::n4a::{N4aError, N4aMemberBytes, N4aMemoryBudget, N4aStorage};
use crate::n4a_physical::{try_copy, Ledger};

/// Content disposition only. Opaque bytes remain available through storage APIs.
#[derive(Debug)]
pub enum OwnerContentInspection {
    /// Exact owner identity and member closure validated, without native loading.
    Validated {
        state_members: Vec<FacetStateMember>,
        model_states: Vec<FacetModelStateInspection>,
    },
    Opaque,
    Unresolved,
}

#[derive(Debug)]
pub struct ArtifactSemanticInspection {
    pub artifact_id: String,
    pub content: OwnerContentInspection,
}

/// Resolution of the declared root, inputs and requirement chain only.
/// Even `Ok(())` grants no operation, feature, trust or execution authority.
#[derive(Debug)]
pub struct EntrypointContentInspection {
    pub entrypoint_id: String,
    pub content: FacetResult<()>,
}

/// Bounded diagnostics, with no retained archive, owner proofs or copied members.
/// Malformed known content and mismatched links refuse the entire inspection.
#[derive(Debug)]
pub struct N4aSemanticInspection {
    pub artifacts: Vec<ArtifactSemanticInspection>,
    pub links: Vec<FacetLinkValidation>,
    pub opaque_link_ids: Vec<String>,
    pub entrypoints: Vec<EntrypointContentInspection>,
}

/// Inspect all applicable embedded DAG content, including non-selected facets.
///
/// Accepts any handle without cloning its archive: byte handles lend their
/// members, a file handle reads and verifies only the DAG-owned ones. Copies
/// required by DAG's current proof/member APIs are local to this call and dropped
/// before returning. Scientific JSON and fitted-state bytes are never rewritten.
/// No runtime, native loader, compiler, hydration, provider or callback is used.
pub fn inspect_n4a_semantics<C: N4aMemberBytes>(
    container: &C,
) -> FacetResult<N4aSemanticInspection> {
    let manifest = container.metadata().manifest();
    let OwnerSnapshot {
        graph,
        context,
        reports,
        opaque_link_ids,
    } = owner_snapshot(container, container.metadata().memory_budget())?;
    // No IO validator is fabricated: dataset_of/evaluates/residual_of retain
    // the owner's unverified verdicts, and absent evidence is never filtered.
    let links = validate_artifact_facet_links(&graph, &context)?;
    let entrypoints = manifest
        .entrypoints
        .iter()
        .map(|entry| {
            let content = (|| {
                let mut pending = vec![entry.root_artifact.as_str()];
                pending.extend(entry.input_artifacts.iter().map(String::as_str));
                if let Some(reference) = &entry.requirements {
                    pending.push(&reference.artifact_id);
                }
                let mut visited = std::collections::BTreeSet::new();
                while let Some(id) = pending.pop() {
                    if !visited.insert(id) {
                        continue;
                    }
                    let artifact =
                        manifest
                            .artifacts
                            .iter()
                            .find(|a| a.id == id)
                            .ok_or_else(|| FacetRefusal::UnresolvedOwnerContent {
                                artifact_id: id.into(),
                            })?;
                    if matches!(artifact.storage, N4aStorage::Reference { .. }) {
                        return Err(FacetRefusal::UnresolvedOwnerContent {
                            artifact_id: id.into(),
                        });
                    }
                    if !context.owner_proofs.contains_key(id) {
                        return Err(if artifact.owner == "dag-ml" {
                            FacetRefusal::UnsupportedPayload {
                                content_type: artifact.content_type.id.clone(),
                            }
                        } else {
                            FacetRefusal::UnverifiedEvidence {
                                detail: format!(
                                    "owner `{}` content for `{id}` remains opaque",
                                    artifact.owner
                                ),
                            }
                        });
                    }
                    if let Some(reference) = &artifact.requirements {
                        pending.push(&reference.artifact_id);
                    }
                }
                Ok(())
            })();
            EntrypointContentInspection {
                entrypoint_id: entry.id.clone(),
                content,
            }
        })
        .collect();
    Ok(N4aSemanticInspection {
        artifacts: reports,
        links,
        opaque_link_ids,
        entrypoints,
    })
}

/// An absent member is unresolved content. Anything else is a failed read, which
/// refuses the inspection rather than letting unread bytes pass as validated.
fn read_refusal(artifact_id: &str, error: N4aError) -> FacetRefusal {
    match error {
        N4aError::Manifest(_) => FacetRefusal::UnresolvedOwnerContent {
            artifact_id: artifact_id.into(),
        },
        other => FacetRefusal::InvalidStructure {
            detail: other.to_string(),
        },
    }
}

/// Portable invocation-local context; fitted member maps remain transient in
/// inspection. Native actions separately retain their complete member map.
pub(crate) struct OwnerSnapshot {
    pub(crate) graph: ArtifactFacetGraph,
    pub(crate) context: FacetValidationContext<'static>,
    pub(crate) reports: Vec<ArtifactSemanticInspection>,
    pub(crate) opaque_link_ids: Vec<String>,
}

/// Invocation-local complete owner context shared by inspection and native
/// actions. It confers no execution authority and imports no native services.
pub(crate) fn owner_snapshot<C: N4aMemberBytes>(
    container: &C,
    budget: N4aMemoryBudget,
) -> FacetResult<OwnerSnapshot> {
    let metadata = container.metadata();
    let manifest = metadata.manifest();
    let mut ledger = Ledger::new(budget);
    ledger
        .charge(metadata.retained_bytes())
        .and_then(|()| ledger.charge(container.resident_bytes()))
        .map_err(|error| FacetRefusal::InvalidStructure {
            detail: error.to_string(),
        })?;
    let retained = ledger.used();
    // Only neutral transport descriptors cross serde here, never owner payloads.
    // Preserve EVERY endpoint, including foreign and unresolved artifacts.
    let artifacts: Vec<ArtifactFacet> = manifest
        .artifacts
        .iter()
        .map(|artifact| Ok(serde_json::from_value(serde_json::to_value(artifact)?)?))
        .collect::<FacetResult<_>>()?;
    let mut graph = ArtifactFacetGraph {
        artifacts,
        links: Vec::new(),
    };
    let mut opaque_link_ids = Vec::new();
    for link in &manifest.links {
        if link.owner != "dag-ml" {
            opaque_link_ids.push(link.id.clone());
            continue;
        }
        let relation = match link.relation.as_str() {
            "recipe_of" => ArtifactFacetRelation::RecipeOf,
            "dataset_of" => ArtifactFacetRelation::DatasetOf,
            "produced_by" => ArtifactFacetRelation::ProducedBy,
            "evaluates" => ArtifactFacetRelation::Evaluates,
            "residual_of" => ArtifactFacetRelation::ResidualOf,
            "evidence_for" => ArtifactFacetRelation::EvidenceFor,
            "refit_of" => ArtifactFacetRelation::RefitOf,
            _ => {
                return Err(FacetRefusal::InvalidStructure {
                    detail: format!(
                        "unknown DAG-owned relation `{}` on `{}`",
                        link.relation, link.id
                    ),
                });
            }
        };
        graph.links.push(ArtifactFacetLink {
            id: link.id.clone(),
            owner: link.owner.clone(),
            relation,
            source_artifact: link.source_artifact.clone(),
            target_artifact: link.target_artifact.clone(),
        });
    }
    graph.validate_structure()?;

    let mut context = FacetValidationContext::default();
    let mut reports = Vec::with_capacity(graph.artifacts.len());
    for artifact in &graph.artifacts {
        let content = match &artifact.storage {
            _ if artifact.owner != "dag-ml" => OwnerContentInspection::Opaque,
            dag_ml_core::artifact_facets::ArtifactFacetStorage::Reference { .. } => {
                OwnerContentInspection::Unresolved
            }
            dag_ml_core::artifact_facets::ArtifactFacetStorage::Embedded {
                root_member,
                member_paths,
            } => {
                let root = container
                    .member_bytes(root_member)
                    .map_err(|error| read_refusal(&artifact.id, error))?;
                // A file read owns its root; a byte handle already charged it
                // as resident memory. Keep this charge while its map is live.
                if let Cow::Owned(bytes) = &root {
                    ledger
                        .charge(bytes.len() as u64)
                        .map_err(|error| read_refusal(&artifact.id, error))?;
                }
                if let Some(proof) = validate_owner_facet_bytes(&artifact.content_type, &root)? {
                    // DAG requires owned maps. Copy only this facet's exact members,
                    // transiently; no archive clone or idle-handle proof cache.
                    let members = member_paths
                        .iter()
                        .map(|path| {
                            let location = metadata.member_location(path).ok_or_else(|| {
                                FacetRefusal::UnresolvedOwnerContent {
                                    artifact_id: artifact.id.clone(),
                                }
                            })?;
                            ledger
                                .charge(location.byte_length)
                                .map_err(|error| read_refusal(&artifact.id, error))?;
                            let member = container
                                .member_bytes(path)
                                .map_err(|error| read_refusal(&artifact.id, error))?;
                            let bytes = match member {
                                Cow::Borrowed(bytes) => try_copy(bytes)
                                    .map_err(|error| read_refusal(&artifact.id, error))?,
                                Cow::Owned(bytes) => bytes,
                            };
                            Ok((path.clone(), bytes))
                        })
                        .collect::<FacetResult<BTreeMap<_, _>>>()?;
                    let state_members = validate_facet_members(artifact, &proof, &members)?;
                    let model_states = if matches!(
                        artifact.kind,
                        ArtifactFacetKind::Model | ArtifactFacetKind::FullRefit
                    ) {
                        inspect_facet_model_states(&proof)?
                    } else {
                        Vec::new()
                    };
                    context.owner_proofs.insert(artifact.id.clone(), proof);
                    OwnerContentInspection::Validated {
                        state_members,
                        model_states,
                    }
                } else {
                    OwnerContentInspection::Opaque
                }
            }
        };
        // This facet's root and map have been dropped; opaque attachments were
        // never read. Retain only the handle's metadata/resident charge.
        ledger.release(ledger.used() - retained);
        reports.push(ArtifactSemanticInspection {
            artifact_id: artifact.id.clone(),
            content,
        });
    }
    Ok(OwnerSnapshot {
        graph,
        context,
        reports,
        opaque_link_ids,
    })
}

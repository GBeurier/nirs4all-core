#![allow(dead_code)]
use dag_ml_core::artifact_facets::*;
use dag_ml_core::PortablePredictorPackage;
use nirs4all::n4a::*;
use nirs4all::n4a_owner_native::*;
use nirs4all::pipeline_workflow::NativePipelineRecipe;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
#[path = "data.rs"]
mod data;
pub use data::*;

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[cfg(target_os = "linux")]
pub fn assert_mapped_n4m_library() {
    use std::collections::BTreeSet;
    use std::io::Read;

    // Inspect this child's actual mappings: the runtime loads a private snapshot,
    // whose path differs from the controller's admitted library path.
    let maps = std::fs::read_to_string("/proc/self/maps").expect("read child mappings");
    let paths: BTreeSet<PathBuf> = maps
        .lines()
        // The five fixed fields are space-separated; preserve spaces in paths.
        .filter_map(|line| line.splitn(6, ' ').nth(5))
        .map(|path| PathBuf::from(path.trim_start_matches(' ')))
        .filter(|path| {
            path.is_absolute()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.starts_with("libn4m"))
                    .unwrap_or(false)
        })
        .collect();
    let mut hashes = Vec::new();
    for path in paths {
        let mut file = std::fs::File::open(&path)
            .unwrap_or_else(|error| panic!("open mapped libn4m {path:?}: {error}"));
        assert!(
            file.metadata().expect("stat mapped libn4m").is_file(),
            "mapped libn4m must be a regular file: {path:?}"
        );
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .unwrap_or_else(|error| panic!("read mapped libn4m {path:?}: {error}"));
        let sha = digest(&bytes);
        println!(
            "F10 mapped libn4m evidence: pid={} path={path:?} sha256={sha}",
            std::process::id()
        );
        hashes.push((path, sha));
    }
    assert_eq!(
        hashes.len(),
        1,
        "expected exactly one distinct mapped libn4m file: {hashes:?}"
    );
    let expected = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").expect("controller SHA required");
    assert_eq!(hashes[0].1, expected, "mapped libn4m SHA mismatch");
}
#[cfg(not(target_os = "linux"))]
pub fn assert_mapped_n4m_library() {
    println!(
        "F10 mapped libn4m check skipped: pid={} /proc/self/maps requires Linux; continuing test",
        std::process::id()
    );
}
pub fn library() -> PathBuf {
    let path =
        PathBuf::from(std::env::var_os("N4M_LIBRARY_PATH").expect("controller runtime required"));
    assert_eq!(
        std::env::var_os("N4M_LIB_PATH").as_deref(),
        Some(path.as_os_str())
    );
    let expected = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").expect("controller SHA required");
    assert_eq!(digest(&std::fs::read(&path).unwrap()), expected);
    path
}
pub fn source(selected: bool, embed: bool, label: &str) -> OwnedN4a {
    source_recipe(
        recipe_value(selected),
        record("train", 18, true, true),
        embed,
        label,
    )
}
pub fn source_recipe(recipe: Value, record: Value, embed: bool, label: &str) -> OwnedN4a {
    let library = library();
    let sha = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").unwrap();
    let recipe: NativePipelineRecipe = serde_json::from_value(recipe).unwrap();
    produce_n4a_native(NativeProduceRequest {
        record: &serde_json::to_vec(&record).unwrap(),
        source_id: "spectra",
        recipe: &recipe,
        runtime: NativeRuntime {
            library: &library,
            expected_sha256: &sha,
        },
        run_id: label,
        container_id: label,
        embed_training_record: embed,
        memory_budget: N4aMemoryBudget::default(),
    })
    .unwrap()
}
pub fn action<'a>(
    bytes: &'a [u8],
    operation: &'a str,
    library: &'a Path,
    sha: &'a str,
) -> NativeActionRequest<'a> {
    NativeActionRequest {
        entrypoint: operation,
        input: NativeInput::External(bytes),
        source_id: "spectra",
        partition: "predict",
        schema_policy: NativeSchemaPolicy::RequireTrainingSchema,
        request_id: "request:stage-d",
        run_id: "run:stage-d",
        memory_budget: N4aMemoryBudget::default(),
        runtime: NativeRuntime {
            library,
            expected_sha256: sha,
        },
    }
}
pub fn package(container: &OwnedN4a) -> PortablePredictorPackage {
    let bytes = root(container, "model");
    PortablePredictorPackage::from_json(std::str::from_utf8(bytes).unwrap()).unwrap()
}
pub fn root<'a>(container: &'a OwnedN4a, id: &str) -> &'a [u8] {
    let artifact = container
        .manifest()
        .artifacts
        .iter()
        .find(|a| a.id == id)
        .unwrap();
    let N4aStorage::Embedded { root_member, .. } = &artifact.storage else {
        panic!("embedded root")
    };
    container.member(root_member).unwrap().bytes
}
pub fn states(container: &OwnedN4a) -> BTreeMap<String, String> {
    let artifact = container
        .manifest()
        .artifacts
        .iter()
        .find(|a| a.id == "model")
        .unwrap();
    let facet: ArtifactFacet = serde_json::from_value(json!(artifact)).unwrap();
    let proof = validate_owner_facet_bytes(&facet.content_type, root(container, "model"))
        .unwrap()
        .unwrap();
    let members = members(container);
    validate_facet_members(&facet, &proof, &members)
        .unwrap()
        .into_iter()
        .map(|state| {
            (
                state.member_path.clone(),
                digest(&members[&state.member_path]),
            )
        })
        .collect()
}
pub fn members(container: &OwnedN4a) -> BTreeMap<String, Vec<u8>> {
    container
        .manifest()
        .members
        .iter()
        .map(|m| {
            (
                m.path.clone(),
                container.member(&m.path).unwrap().bytes.to_vec(),
            )
        })
        .collect()
}
pub fn reseal(mut manifest: N4aManifest, members: BTreeMap<String, Vec<u8>>) -> OwnedN4a {
    manifest.members = members
        .iter()
        .map(|(path, bytes)| N4aMember {
            path: path.clone(),
            byte_length: bytes.len() as u64,
            sha256: digest(bytes),
        })
        .collect();
    let (_, bytes) = write_n4a_bytes(
        N4aWriteRequest {
            manifest,
            members: members
                .iter()
                .map(|(path, bytes)| N4aMemberInput { path, bytes })
                .collect(),
        },
        N4aMemoryBudget::default(),
    )
    .unwrap();
    open_owned_n4a_bytes(bytes.into_boxed_slice(), N4aMemoryBudget::default()).unwrap()
}
pub fn owner_error(error: NativeActionError) -> FacetRefusal {
    match error {
        NativeActionError::Owner(error) => error,
        other => panic!("expected owner error, got {other}"),
    }
}
pub fn calibrate_error(container: &OwnedN4a, bytes: &[u8]) -> NativeActionError {
    match calibrate_n4a_native(
        container,
        action(
            bytes,
            "calibrate-external",
            Path::new("/missing/stage-d/libn4m.so"),
            &"0".repeat(64),
        ),
        NativeCalibrationOptions {
            coverages: vec![0.8],
            ..Default::default()
        },
        "refused",
    ) {
        Ok(_) => panic!("expected static refusal"),
        Err(error) => error,
    }
}

# n4a exchange and compatibility

`.n4a` is the ecosystem exchange container for pipeline definitions, fitted
models, datasets, predictions, residuals, scores and provenance. The suffix
alone is not a schema or an execution guarantee. Readers must identify the
container profile and each owner's artifact version before offering an
operation. Cross-platform storage and cross-language execution are separate
contracts.

## Clean V1 target

The maintainer's clean V1 baseline removes pre-V1 compatibility requirements.
The target is one n4a V1 container with typed owner artifacts and links for
recipes, datasets, models, runs, results and scores. Its versioning policy
governs future evolution from that baseline. Old readers, migration dispatch
and compatibility aliases are removed with their active caller migrations.

The existing families below are interim implementation, not supported legacy
profiles to preserve. Their useful current capabilities (training evidence,
frozen prediction and full refit) move into typed contents of the single
container; their historical wire identities do not need to survive.

## Owner-neutral storage exchange

The `n4a-container@1` execution profile (see `EXECUTION_PROFILES.md`) declares
only storage: inspect, read, write and update of a canonical V1 container whose
members are arbitrary owner or host bytes (a pickled or joblib model, recipes,
datasets, scores and results are all opaque). Core validates the physical
container and manifest once, returns exact bytes and derives member sizes and
SHA-256 values. It does not interpret, deserialize or execute a member and needs
no Methods library. A host model is therefore readable and exchangeable but not
executable through Core; running it is an explicit decision of its owner or the
SDK. Closed native-produced models keep their predict and retrain actions under
`native-n4a-canonical`, and a container that also holds a foreign model is
readable but refused by those actions.

V1 content has no intrinsic cap on entries, member size, total size or path
length. Only an optional working-memory budget, allocator failure, the ZIP and
host representation limits and integrity checks refuse; the
[container contract](contracts/n4a-container-v1/README.md) states them. The
`zip-stored.v1` profile uses ZIP64 fields exactly where a value requires them and
keeps ZIP32 bytes below every threshold. This is source support: no huge-file,
Windows, browser or MATLAB behavior is qualified by it. Archive V2/V3 keep their
declared wire limits on their own routes.

An update is a new container with a new digest; the base is immutable. Source
support does not mean qualified: the common write, read, update and refusal gate
over `tests/parity/fixtures/n4a_host_opaque_v1` is pending its first executed run.

## Interim archive families

| Family | Meaning | Compatibility boundary |
| --- | --- | --- |
| Historical pipeline bundle (`bundle_format_version: 1.0`) | Full Python pipeline bundle, handled by `BundleLoader` | Retire this reader in the clean V1 migration; preserve separately identified research provenance. |
| `nirs4all.archive_workspace.v1` | Pre-V1 native archive family | Retire the reader and its compatibility dispatch. Reserved schema fields are not executable capabilities. |
| `nirs4all.archive_workspace.v2` | Native predictor package and training evidence | Distinct graph, execution bundle, outcome, prediction-cache and score-set companions; bounded Methods artifact families. DAG-ML validates replay closure and controller trust. |
| `nirs4all.archive_workspace.v3` | Native target-bound full-refit child | Distinct refit package, graph, refit bundle and outcome. Source CV/selection scores and copied calibration state do not become child evidence. |

V2 and V3 currently express different operations. The clean container must
retain those operations without silently dropping training results. Core's
interim native readers check ZIP structure, budgets, declared member
closure, raw hashes and manifest dispatch. DAG-ML then validates graph,
package, result and replay semantics. Methods imports its own numerical state.
Bindings pass the original bytes to these owners.

The interim manifests and fixtures are owned by DAG-ML under
`docs/contracts/archive-v1/`, `archive-v2/` and `archive-v3/`. Core's readers
implement their bounded storage profiles. Language-specific support must be
qualified through the actual operation: a browser V2 reader is not evidence of
a browser V3 refit API, and a CLI bridge is not a Python-free workspace API.
The retained V3 schema covers fewer cases than Core's code: additional N4ME
and role-pipeline families, safe NFC Unicode paths and nested N4MM paths need
to converge in the canonical V1 contract. Passing interim storage tests does
not resolve those differences.

## Versioning rules

1. Dispatch using the exact manifest version, profile and writer contract
   before loading model payloads. Refuse unknown required versions/profiles.
   Do not fall back to a different archive family after validation fails.
2. Version container manifests, recipe languages, result schemas, model wire
   formats, ABI requirements and canonicalization algorithms independently.
   A package release number cannot substitute for any of them.
3. Reading must not rewrite an archive. Keep bytes of independently identified
   scientific provenance, but remove pre-V1 readers and migration dispatch.
   Do not build old-format compatibility into the clean V1 baseline.
4. Bind every referenced member by safe relative path, raw SHA-256 and size.
   Semantic fingerprints name their canonicalization profile separately. JSON
   decode/reencode, newline conversion and host-native serialization must not
   alter opaque model/package bytes.
5. Keep closed profiles closed. New mandatory content needs an owner schema
   and an explicit profile/version decision, with fixtures and old-reader
   refusal tests. Use extensions only where that exact profile defines them;
   optional display metadata cannot alter execution semantics.
6. Declare operations separately: inspect, retrieve, compile, fit, predict,
   retrain and export. Return an explicit refusal when the artifact, build,
   controller, provider or operation is unsupported. Inspection never executes
   host sidecars; code-bearing artifacts require the applicable trust policy.

## Complete exchange coverage

| Content | Definition owner | Required link and operation |
| --- | --- | --- |
| Pipeline JSON/YAML | DAG-ML compiler plus explicit host adapters | Recipe schema/hash and complete parameters, branches, target processing and search space; compile or explicit refusal. |
| Fitted state | Methods or the declared host controller | Wire schema, ABI/runtime, features, targets/classes, original recipe and fit lineage; frozen predict distinguished from a new fit. |
| Scores, predictions and residuals | DAG-ML | Run, variant, fold, sample/target identities, evaluation scope and scoring definition; trace results back to the exact fitted pipeline. |
| Dataset content | Formats decoding, IO assembly, DMD runtime identities | Exact sample/origin/group identities, axes/units, masks, targets, partitions and content fingerprint; embedded content distinguished from an external reference. |
| Aggregated runs and workspace snapshots | Core composition of owner records | Closed inventory, coherent checkpoint and owner references; no live transaction/lock state. |

The required clean V1 exchange scope includes every row. The interim native
V2/V3 profiles do **not** implement arbitrary recipe-only, dataset-only or
result-only containers, nor arbitrary host binary replay. Their nullable slots
and deferred references are not implementations of those operations. A general
exchange container is being implemented before these combinations can be
advertised across all languages. It must preserve links between datasets,
recipes, models and results, reuse the owner schemas above, and include directed
producer to consumer tests. Replace the old readers and do not extend their
compatibility branches.

Qualification records must distinguish executed interoperability, expected
unsupported operations and unavailable environments. Include artifact/build
digests, source revision and dirty-source identity, OS, binding, operation,
scenario, tolerances and evidence. Byte preservation, scientific equivalence
and successful negotiation are different claims. An Octave pass does not
certify MATLAB or Windows transport.

## Native Stage D source

The bounded native producer, stored/external prediction and external calibration
source is described in [Native n4a actions](n4a_native_actions.md). It uses the
accepted local owner cohort plus the separately authored DAG authority APIs;
frozen DAG414 alone cannot compile the new calls. No runtime/scientific or
package-reuse qualification follows from this source-only implementation.
Portable T1 inspection and WASM storage remain separate, loader-free surfaces.

The same source adds a read-only owner training view, a single-fit producer
variant and a thin Python bridge (`produce_n4a_native`, `inspect_n4a_bytes`,
`predict_n4a_native`) for the closed native-produced model profile only. Other
shapes are refused with no fallback. It is source only and unqualified: the
earlier bounded Rust cohort is not evidence for the Python bridge. A
coordinated qualification wave with the pending IO/FormatsN2 cohort is still
required.

The execution-profile registry (`compat/capabilities.toml`, see
[Execution profiles](EXECUTION_PROFILES.md)) declares this container as the
profile `native-n4a-canonical@1` with the real actions `produce`, `inspect`,
`predict`, `calibrate` (Rust only) and `retrain`. `inspect` is not `import` and
`calibrate` is not `fit`. That record states source support only: the portable,
retrain, WASM, CLI, MATLAB/Octave and R runtime gates are pending, and it is not
numerical certification or release approval.

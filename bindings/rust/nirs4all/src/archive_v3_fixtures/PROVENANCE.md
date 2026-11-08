# DAG-ML archive V3 owner contract mirror

These files are byte-for-byte test-data mirrors of the DAG-ML Archive V3
storage contract. They are checked into `nirs4all-core` so the published
crate's test gate never depends on a sibling checkout.

| Mirrored file | DAG-ML source file | SHA-256 |
| --- | --- | --- |
| `archive_workspace_manifest.v3.schema.json` | `dag-ml/docs/contracts/archive-v3/archive_workspace_manifest.v3.schema.json` | `bc972495f66cfa0c9443bfd515843f4231c6087284da38dd1e433ea006c5763d` |
| `positive/storage_only_native_refit.json` | `dag-ml/docs/contracts/archive-v3/fixtures/positive/storage_only_native_refit.json` | `788e2eecd728d426b86b07c54ae2916483f6f2ce84065fb10d2453919466eee5` |
| `positive/synthetic_members.v3.json` | `dag-ml/docs/contracts/archive-v3/fixtures/positive/synthetic_members.v3.json` | `a7c259ec63005ced761583a771daa62b81faed430abd3d3873fcf060ed8e6139` |
| `negative/refusals.v3.json` | `dag-ml/docs/contracts/archive-v3/fixtures/negative/refusals.v3.json` | `d5de313cd4a0119d90f980698f387584675415559c0d5f7c93a3df114f03a47c` |

Source revision: the current schema is unchanged since DAG-ML commit
`65decf3b6c683f8b82933b2f2774cd0cc407944d`; the synchronized DAG-ML checkout
`HEAD` at reconciliation was `963abc4531789d6483e9d60d8d2e30549fc5ca9e`.
The three fixture files are uncommitted implementation files. The revision is
context, not a content identifier: refreshes must verify the source-file
SHA-256 values above. They are validated by
`scripts/validate_archive_v3_contract.py` and `tests/test_archive_v3_contract.py`.

Phase 0 reconciliation adopts the owner's existing N4MM format 3 / ABI 2.20
schema support. The refusal corpus distinguishes format 3 below its required
ABI floor from an unknown format 4. This updates interim test evidence only;
it does not widen a product wire schema or approve the clean V1 design.

The Rust test `owner_fixture_mirror_matches_optional_sibling_dag_ml_checkout`
re-compares every mirror to a sibling `dag-ml` checkout (or `DAG_ML_REPO`) and
prints an explicit `executed` / `unavailable` status. Published tests never
require the sibling; set `NIRS4ALL_REQUIRE_OWNER_FIXTURE_DRIFT=1` to make an
unavailable checkout a failure.

The corpus is explicitly synthetic and storage-only. Member bytes are a marker
plus a hex body from `synthetic_members.v3.json`: they are neither DAG-ML
documents nor executable Methods models. Unicode, NUL, BOM/CRLF, non-UTF-8 and
all 256 byte values appear only in payload bytes; manifest paths and ids are
ASCII, as the owner schema requires. The owner schema covers N4MM only; Core
additionally stores N4ME and role-pipeline families, which have no owner
fixtures yet.

# `nirs4all-lite` → `nirs4all-core` rename runbook

This is the ordered, concrete procedure for promoting the aggregate from
`nirs4all-lite` to `nirs4all-core` **without breaking current production package
availability**. Originally drafted by the RC-E language-surface lane as a
rename-readiness record; topology/naming ownership and the ecosystem-level
moves belong to the coordinator / RC-A.

> **Status (2026-07-02, RC-A):** **Phase R1 is executed** on the RC V1 head
> (`rc/v1-full-refactor-core`) per the RC V1 control-board decision that RC is
> the final topology. Exception: Phase R1 item 2 (canonical/alias package
> inversion) is deliberately deferred — the canonical import stays
> `nirs4all_lite` because the ecosystem release-lock consumes
> `bindings/python/src/nirs4all_lite/_topology.py` by path and must move in
> lockstep with any such inversion. The topology schema id was bumped to
> `nirs4all-core.release-topology.v2` (the shape changed: `future_id` →
> `legacy_id`, `target_distribution` → `legacy_distribution`); no code consumer
> pinned the old id — only the intentionally-stale committed aggregation lock,
> which is regenerated at final head selection. Phase R2 is partially executed:
> the GitHub repository and Read the Docs slug are now `nirs4all-core`. The
> remaining admin blocker is the PyPI `nirs4all-core` Trusted Publisher plus the
> final `nirs4all-lite` alias release.

Governance context: `SW2_GOV_CORE_NAMING_spec.md` (GOV-003/GOV-004),
`A13_A13-core-release.md` (DEC-GOV-001/002, staged T0→T2), and
[`NAMING.md`](NAMING.md). The direction is a *concept* rename whose only Python
distribution name is actually in question — Rust/npm/R/MATLAB already ship the
bare `nirs4all` name and are unaffected.

## Invariants that must hold at every step

1. **PyPI `nirs4all-lite` stays installable** before, during, and after cutover.
   After the flip it becomes a thin alias/successor that depends on
   `nirs4all-core`; `pip install nirs4all-lite` must keep working.
2. **Rust / npm / R / MATLAB are untouched.** They already use the bare
   `nirs4all` name (crate / npm package / `library(nirs4all)` / `+nirs4all`).
   The rename is Python-distribution-only.
3. **`nirs4all` stays reserved as a Python import.** The full Python `nirs4all`
   modelling library owns it until an intentional core cutover; neither
   `n4a`, `nirs4all_core`, nor `nirs4all_lite` may define a top-level `nirs4all`
   module.
4. **No behaviour fork.** `nirs4all-core` is the same code lineage, published as
   an alias/successor — never a divergent re-implementation.

## Phase R1 — in-repo edits (EXECUTED by RC-A, except item 2)

1. `bindings/python/pyproject.toml` — **done**:
   - `name = "nirs4all-core"`;
   - self-referential extra flipped to `everything = ["nirs4all-core[all,datasets]"]`;
   - `[project.urls]` points to `GBeurier/nirs4all-core`;
   - the wheel `packages` list still ships `nirs4all_core`, `nirs4all_lite`,
     and `n4a` so invariant (1) holds (the old import keeps resolving).
2. Facade direction — **deferred** (see status note above): making
   `src/nirs4all_core` the canonical implementation and turning
   `src/nirs4all_lite/__init__.py` into the thin forwarding alias requires a
   lockstep update of the ecosystem release-lock artifact path
   (`aggregation-manifest.n4a.json` → `lite.contract_artifacts.release_topology_manifest.path`).
   `n4a` continues to re-export whichever package is canonical via
   `__aggregate_import__`.
3. `bindings/python/src/nirs4all_lite/_topology.py` — **done**:
   - `aggregate.id = "nirs4all-core"`, `legacy_id = "nirs4all-lite"`,
     `repo = "GBeurier/nirs4all-core"`, and
     `legacy_repo = "GBeurier/nirs4all-lite"`;
   - `python.distribution = "nirs4all-core"`, `canonical_import` unchanged
     (`nirs4all_lite`), `legacy_distribution_status = "superseded"`;
   - `install_distributions`: `nirs4all-core` → `status = "current"` with
     `workflow = "release-python.yml"`; `nirs4all-lite` kept as the legacy
     alias row (`status = "legacy-superseded"`, no workflow/artifact);
   - schema id bumped to `nirs4all-core.release-topology.v2` (not a silent
     rename: shape changed, consumers audited — none pinned the old id).
4. Guard tests in `test_release_topology.py` flipped **in the same change** —
   **done**: they now assert the new canonical name and the `nirs4all-lite`
   alias row. `test_facade.py` unchanged (canonical/alias `__name__`s did not
   swap; item 2 deferred).
5. Source/SBOM naming — **done**: `release-source.yml` prefix and the manifest
   row renamed `nirs4all-lite-source-sbom` → `nirs4all-core-source-sbom`
   (cosmetic; keep the old artifact name as an alias for one release if
   downstreams fetch it).
6. Cosmetic diagnostics sweep — **done**: the "outside the current
   nirs4all-lite portable subset" strings in `_pipeline.py`,
   `bindings/r/R/pipeline.R`, `bindings/rust/nirs4all/src/lib.rs`,
   `bindings/wasm/src/index.js`, and MATLAB now say `nirs4all-core`. These are
   diagnostics, not contracts.

## Phase R2 — coordinator / GitHub / registry actions (PARTIAL)

These require repository-admin or registry credentials and are recorded here:

- **GitHub repo**: `GBeurier/nirs4all-core` is the canonical repository. Keep
  `GBeurier/nirs4all-lite` only as a legacy redirect/alias reference.
- **Ecosystem submodule pin** (owned by RC-A / coordinator): update the
  `nirs4all-ecosystem` submodule path/URL for the renamed repo.
- **PyPI**: register/claim `nirs4all-core` and create its Trusted Publisher
  (owner `GBeurier`, repo `nirs4all-core`, workflow `release-python.yml`,
  environment `pypi`); publish `nirs4all-core` from `release-python.yml`; then
  publish a final `nirs4all-lite` release whose only dependency is
  `nirs4all-core` (alias wheel) so invariant (1) holds. Never yank existing
  `nirs4all-lite` versions.
- **Read the Docs**: `nirs4all-core` is the canonical slug; leave
  `nirs4all-lite` only as a legacy redirect/alias if it remains configured.
- **Docs site / `nirs4all.org`**: install snippets updated on the org RC head
  (RC-A); that head must not deploy before the PyPI `nirs4all-core` publish.

## Phase R3 — verification (after R1 + R2)

- `pip install nirs4all-core` and `pip install nirs4all-lite` both succeed;
  `import nirs4all_core`, `import n4a`, and the legacy `import nirs4all_lite` all
  work; `import nirs4all` still resolves to the full library (invariant 3).
- `make test-python-v1-surfaces` passes with the flipped guard tests.
- Rust/npm/R/MATLAB gates unchanged and green (invariant 2).
- `release_topology_manifest()` reports `nirs4all-core` as `current` and
  `nirs4all-lite` as the alias.

## One-line status

Phase R1 is executed on the RC V1 head (canonical-import inversion deferred
with its lockstep requirement recorded). Remaining work is the PyPI Trusted
Publisher plus the final `nirs4all-lite` alias release, then Phase R3
verification.

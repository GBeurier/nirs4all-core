# Core contributor instructions

## Aggregate ownership

- Core composes/re-exports Methods, DAG-ML, DMD, Formats, IO and Datasets. Never add a second parser, estimator/numerical kernel, dataset loader/catalog or DAG compiler/coordinator here: fix its owner, then expose the owner contract.
- The Python SDK builds on this aggregate plus Python controllers/extras; the older statement that it remains separate until a future replacement is superseded. This relationship does not certify every supported profile or language path.
- Core owns the directed `.n4a` byte container/storage/inventory and host facade.
- Do not impose intrinsic `.n4a` content-size or entry-count ceilings. Only host-authorized RAM may bound an operation; preserve structural/integrity checks and implement the requirement consistently in Core and its bindings.
- DAG owns recipe/run/result/score/replay semantics; IO owns dataset payloads; DMD owns identity; Methods owns fitted state; hosts retain serializers.
- Readability is not executability.
- Container, owner schema, ABI/wire, recipe-language and package-release versions are independent.
- Route by declared owner/schema/profile/operation, never suffix.
- Preserve explicit refusals and current supported low versions; no retired Lite alias.
- Owner entrypoints: [Methods](../nirs4all-methods/AGENTS.md), [DAG](../dag-ml/AGENTS.md), [DMD](../dag-ml-data/AGENTS.md), [Formats](../nirs4all-formats/AGENTS.md), [IO](../nirs4all-io/AGENTS.md), [Datasets](../nirs4all-datasets/AGENTS.md).
- Sibling links are optional navigation, not required policy inputs.

## Surface and parity

- Python distribution `nirs4all-core`, canonical import `nirs4all_core`, additive `n4a`; Rust/npm packages `nirs4all`; MATLAB/Octave `+nirs4all`. The end-user R package lives in `nirs4all-r`, not retired Core R bindings.
- Re-export upstream formats/io/datasets/methods/dag_ml/dag_ml_data domains.
- Keep APIs idiomatic: sklearn Python adapters, R formula/data-frame hosts, MATLAB arrays/tables, typed npm, Rust traits/results.
- Bindings translate, not reimplement algorithms.
- Before widening equivalent behavior, add native-vs-binding and Core-vs-SDK parity fixtures. Claims identify pipeline/profile/operation/platform and actual checks; local host usability is not universal DAG/portability parity.
- `bindings/` contains Python/Rust/WASM/MATLAB surfaces; `docs/` holds binding/parity/compatibility/release contracts and `compat/` the machine-readable upstream registry. Start with [README](README.md) and owner contracts.

## Checks

```bash
make test
cargo test --workspace
PYTHONPATH=bindings/python/src python -m unittest discover -s bindings/python/tests
npm test --prefix bindings/wasm
octave --quiet --eval "run('bindings/matlab/tests/smoke.m')"
```

`make build` produces Core-owned Python/npm/MATLAB/Rust artifacts. R is built/checked in its own repository. Run checks for touched bindings plus equivalent-pipeline parity before publishing; publishing still needs maintainer authority.

## Working rules

- Read applicable local and deeper instructions before editing. Preserve dirty/concurrent work; make authorized owner fixes and propagate affected contracts/bindings/consumers. Do not reset another author's work.
- Use CodeGraph FIRST for implementation navigation only if this checkout's root already has `.codegraph/`; use its exact projectPath or run the CLI here. Never auto-index or silently substitute an ancestor index.
- Unindexed code and documentation use ordinary source navigation. Explicit source, tests and build tools are final authority. Owner links are navigation; this policy works without a parent workspace.
- All application/project content is English: UI text/subtitles/tooltips/errors, docs/comments/docstrings, test/fixture prose, reports/issues/PRs, commits and generated material. Preserve identifiers, quotations and historical/provenance meaning; converse with the maintainer in French.
- Do not commit credentials, private data, generated workspaces, caches or build outputs. Follow local licensing/fixture restrictions and report actual commands/exits/skips; a command reference grants no execution/publication permission.

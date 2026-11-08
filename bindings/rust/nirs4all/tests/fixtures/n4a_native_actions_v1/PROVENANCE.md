# Stage D test source provenance

`data.rs` retains the literal unsorted sample-ID, two-target ridge fixture from
preserved `F10_REAL_RED_TEST.rs` (SHA-256
`dc803fc74a1f0ffdda652b8bcf3cf0c2525402c221058f171ab033af9e43d634`).
It contains additive test-only data construction, not a provider or numerical
implementation. `support.rs` calls production byte-facing actions and real
owner parsers/builders. `authority_controls.rs` is included only under the
library's `cfg(test)` and uses genuine IO packages and owner training to create
adversarial artifacts. There is no publicly exported test bridge.

The first real F10 runtime red and its exact original test/support bytes remain
outside this snapshot in the controller's Stage D evidence directory. The new
F10 test is a positive production-action/cold-reload test and does not relabel
that red as success. No test in this directory has been run in this authoring
turn; the controller must qualify exact source, owner cohort and runtime bytes.

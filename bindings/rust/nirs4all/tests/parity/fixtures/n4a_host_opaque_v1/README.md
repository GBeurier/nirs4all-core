# Opaque host `.n4a` exchange fixture

Synthetic and owner-neutral: it is separate from the immutable `n4a_v1` owner
corpus and contains no Methods state. It exists to prove storage exchange of
arbitrary bytes, never numerical behavior.

| Path | Meaning |
| --- | --- |
| `manifest.json` | Input manifest. `byte_length` and `sha256` are placeholders; the writer derives both. Six embedded artifacts (`recipe`, `dataset`, `model`, `foreign-model`, `scores`, `results`), one `reference` dataset, three links and a `host-predict` entrypoint owned by `host.example`. |
| `members.json` | The CLI member list: an array of `{path, file}`. Relative `file` values resolve against this directory. |
| `members/` | Raw member bytes. `model.bin` has an inert pickle protocol-4 prefix, NUL, 0xFF and CRLF; `data.csv` uses CRLF; `foreign-model.bin` belongs to owner `foreign.lab` with the unknown content type `urn:unknown:content-type`@7. |
| `expected.json` | Per-member length and SHA-256 (computed from the files). Container and manifest digests and lengths were recorded from real Core CLI create/update outputs, independently read and hashed with Python zipfile. Every binding must match these values. |
| `update/` | Update case: replaces `host/model.bin`, deletes `host/foreign-model.bin` with its artifact and link, and retains recipe, data, scores and results from the base. |

Every host runs the same five steps: write and assert the digest, open and assert
`executability: not_evaluated`, read each member byte-for-byte, update from the
base (base unchanged, retained members identical), and refuse a tampered byte, a
future schema version, an unknown path, an undeclared or duplicate input, and any
native predict on `host-predict`. Nothing here deserializes a member.

The exchange tests require every golden in `expected.json` (`container` and
`update.container`: `container_sha256`, `manifest_sha256`, `byte_length`).
If the fixture generator is rerun, record new values through the real owner
`n4a-write` create/update commands before running the binding gates; missing
values fail the tests.

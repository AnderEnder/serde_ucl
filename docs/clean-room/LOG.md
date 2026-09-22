# Clean-Room Provenance Log

## Before the clean room (oracle side)

- 2026-09-22 — REVIEW.md and PLAN.md written by participants who had read libucl source.
- 2026-09-22 — Phase 0 (conformance suite, `tools/ucl-dump`, `scripts/regen-golden.sh`, vendored
  upstream corpus): test-side work, allowed to be derived.
- 2026-09-22 — Phase 1 (fixes to the crate's existing parser and deserializer) and Phase 2 (value
  model, `src/de.rs`, `src/time.rs`) were written by oracle-side participants. The duplicate-key
  insertion in `src/value.rs` was a port of libucl logic. It was removed in the commit that added
  this log, leaving `todo!()` placeholders for C1. Comments that cited libucl internals were
  removed or made neutral.
- 2026-09-22 — A Phase 3 attempt that ported libucl's parser was stopped before any commit and
  moved to branch `quarantine/p3-source-port`. It must not be used.
- 2026-09-22 — `CLAUDE.md` (on this branch and in the main checkout) had its libucl implementation
  notes removed and the clean-room rules added.

- 2026-09-22 — Process changed to the classic two-team model: the spec team (which reads libucl
  source) writes `docs/spec/`, a reviewer checks it for prohibited content before release, and the
  implementation team (which has never read libucl source) works only from released spec versions.
  An earlier C0 attempt by a clean participant, using public docs and black-box runs only, was
  stopped before committing anything.

## Sessions

(Append entries here: date, role, work item, inputs consulted, commits, attestation.)

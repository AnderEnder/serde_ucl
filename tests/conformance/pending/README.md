# Pending cases

Cases that a released spec version added but the crate does not pass yet. The conformance runners
do not read this directory; `scripts/regen-golden.sh` regenerates the golden files here like the
others. When the crate passes a case, move its files unchanged to
`cases/spec/<same directory>/` (with the files it uses under `files/`).

## `13-inputs/` (spec-v10, spec §13)

101 cases for several inputs into one parser (§13.1, `inputs_*`) and macros registered by the
application (§13.2, `macro_registered_*`). They need two things the runners do not support yet:

- `<case>.inputs`: further inputs read by the same parser after the case's own file, one per line,
  `MODE PRIORITY STRATEGY PATH` (see `tests/conformance/README.md`). The further inputs and the
  files the cases include are under `13-inputs/files/`; paths are relative to `13-inputs/`.
- the flags `registered-macros` and `registered-priority-override`, which register the oracle's
  test macros `.emit`, `.seen`, `.fail` and `.ctx`. Spec §13.2, *The test macros*, gives their
  behaviour; the runner has to register macros that behave the same.

Cases whose name ends in `_error` are errors in libucl. The spec README's coverage table lists
every case under the path it will have, `cases/spec/13-inputs/<name>`.

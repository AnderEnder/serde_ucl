# Pending cases

The conformance runners do not read this directory; `scripts/regen-golden.sh` regenerates its
oracle goldens. Once the crate passes a case, move it and its golden files to
`tests/conformance/cases/spec/09-macros/`.

Two cases for question #86 (§9.4, draft for `spec-v20`) wait for implementation in the same way.
The fixture `files/v4/p1/pa.inc` is identical to the one in `cases/spec/09-macros/` and can be
dropped on the move; `files/v4` serves as the directory that the second case loads.

- `09-macros/include_path_first_miss_then_load_try_accepts_later`: a `.load(try=true)` of a
  missing file after a first-directory miss lets the document pass.
- `09-macros/include_path_first_miss_then_load_try_directory_accepts_later`: the same with a
  directory as the `.load` path.

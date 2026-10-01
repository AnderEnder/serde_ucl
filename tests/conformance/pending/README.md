# Pending cases

The conformance runners do not read this directory; `scripts/regen-golden.sh` regenerates its
oracle goldens. Once the crate passes a case, move it and its golden files to the same directory
under `tests/conformance/cases/spec/`.

Fourteen `spec-v21` cases, for question #89 (§9.2 and §13.1, a rejected argument document or a
first-directory miss and a later skip), wait for implementation, seven under `09-macros/` and
seven under `13-inputs/`. The fixtures `files/a.inc`, `files/v12/args_bad.inc` and
`files/v4/p1/pa.inc` under `09-macros/` are identical to the active ones and can be dropped on
the move; the `13-inputs/` cases with a first-directory miss read `files/v4/p1` through
`../09-macros/`.

- `09-macros/macro_args_rejected_then_load_try_accepts`,
  `09-macros/macro_args_rejected_then_url_try_accepts`: a later skip of either kind lets the
  document pass, the macro having run without ARGUMENTS.
- `09-macros/macro_args_rejected_unknown_macro_then_load_try_accepts`,
  `09-macros/macro_args_rejected_in_included_file_then_load_try_accepts`: the same for an
  unknown macro in the argument document and for a rejection in an included file.
- `09-macros/macro_args_rejected_include_runs_without_args`: the `.include` reads its file.
- `09-macros/macro_args_rejected_and_first_miss_then_one_skip_accepts`: one skip covers a
  first-directory miss and a rejection before it.
- `13-inputs/macro_registered_args_rejected_then_load_try_accepts`: a registered macro receives
  no ARGUMENTS and the VALUE as it stands after the `)`.
- `09-macros/macro_args_stopped_then_load_try_accepts`: the same for a silent stop in the
  argument document.
- `13-inputs/macro_registered_text_args_rejected_then_load_try_accepts`: the same for a rejection
  in text parsed in place.
- `13-inputs/inputs_args_rejected_then_skip_in_later_input`,
  `13-inputs/inputs_args_rejected_skip_two_inputs_later`,
  `13-inputs/inputs_args_rejected_skip_in_later_file_input`,
  `13-inputs/inputs_first_miss_then_url_skip_in_later_input`,
  `13-inputs/inputs_first_miss_then_load_skip_in_later_input`: a skip in a later input discards
  both errors; their further inputs are under `13-inputs/files/`.

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

Thirty-two `spec-v23` cases wait for implementation: three under `07-variables/` (§7.6,
question #97), twenty-eight under `09-macros/` and one under `13-inputs/` (§9.2 and §9.4,
questions #93, #98 and #99). Their fixtures are under `09-macros/files/v23/` and `13-inputs/files/`:
`first_miss.inc`, `miss_then_load_skip.inc` and `load_skip.inc` are identical to the active ones
and can be dropped on the move, and so can the copies of `files/v4/g/` and of `files/v23/gg/`
and `files/v23/gh/`; `url_skip.inc`,
`args_rejected_space_end.inc`, the file `v4` and the further inputs of
`13-inputs/inputs_args_rejected_value_at_end_of_input` move with their cases. They also read
`09-macros/files/v4/p1/pa.inc`, which the `spec-v21` cases share, so it stays until the last of
them moves.

Nine of them also need §9.2's rejected argument documents (`spec-v21`) before they can pass:
`macro_args_first_miss_inside_then_try_directory_then_skip`,
`macro_args_rejected_then_load_try_in_included_file`,
`macro_args_rejected_then_try_directory_error`,
`macro_args_rejected_then_try_directory_next_dir_missing`,
`macro_args_rejected_then_glob_try_directory_error`,
`macro_args_rejected_then_glob_try_directory_one_dir_error`,
`macro_args_rejected_then_glob_try_last_dir_no_match`,
`macro_args_rejected_value_at_end_of_file_and_text` and
`13-inputs/inputs_args_rejected_value_at_end_of_input`. The three rejection cases that end in
`_error` are errors in the oracle; the crate on `origin/main` gives an error too, but only because
it rejects every rejected argument document, so they wait here with the others until the crate
follows §9.2 and the rule of #98.

- `07-variables/quoted_unicode_dollar_not_expanded`,
  `07-variables/quoted_unicode_dollar_handler_not_expanded`,
  `07-variables/quoted_unicode_dollar_dollar_outside_string_not_counted`: a double-quoted string
  without a `$` as written is not expanded, whatever a `\u0024` in it or a `$` elsewhere gives.
- `09-macros/include_path_first_miss_and_load_try_in_included_file`: the miss and the skip in
  the same included file.
- `09-macros/include_path_first_miss_in_included_file_then_load_try`: the miss in an included
  file, the skip later in the document.
- `09-macros/include_path_first_miss_then_load_try_in_included_file`,
  `09-macros/include_path_first_miss_then_url_try_in_included_file`: the miss in the document,
  the skip (a `.load` or a URL include) in a file included after it.
- `09-macros/include_path_first_miss_then_load_try_in_text`,
  `09-macros/include_path_first_miss_and_load_try_in_text`,
  `09-macros/include_path_first_miss_in_text_then_load_try`: the miss or the skip, or both, in
  text parsed in place (`registered-macros`).
- `09-macros/include_path_skip_in_included_file_then_miss_then_skip`: a skip in an included file
  covers the miss before it; a later miss needs the later skip.
- `09-macros/macro_args_rejected_then_load_try_in_included_file`: a rejected argument document in
  the document, the skip in a file included after it.
- `09-macros/macro_args_first_miss_inside_no_error`,
  `09-macros/macro_args_first_miss_in_included_file_no_error`,
  `09-macros/macro_args_first_miss_inside_then_value`,
  `09-macros/macro_args_first_miss_inside_in_text`: a first-directory miss inside an argument
  document, or in a file it includes, is no error, and the macro keeps its ARGUMENTS.
- `09-macros/include_path_first_miss_then_try_directory_error`,
  `09-macros/macro_args_rejected_then_try_directory_error`,
  `09-macros/macro_args_rejected_then_glob_try_directory_error`: while an error waits for a skip,
  an include with `try=true` of a directory fails with it when no other directory is tried.
- `09-macros/include_path_first_miss_then_try_directory_every_dir_error`,
  `09-macros/include_path_first_miss_then_try_directory_next_dir_file`,
  `09-macros/macro_args_rejected_then_try_directory_next_dir_missing`: with a search list, the
  include goes on to the next directory, and fails only when the last one gives a directory too.
- `09-macros/include_path_first_miss_then_glob_try_directory_last_dir_error`,
  `09-macros/macro_args_rejected_then_glob_try_last_dir_no_match`,
  `09-macros/macro_args_rejected_then_glob_try_directory_one_dir_error`: with `glob=true`, the
  last directory decides.
- `09-macros/include_path_first_miss_then_glob_try_directory_match_skips_rest`,
  `09-macros/include_path_first_miss_then_glob_try_directory_match_every_dir_error`: the matches
  after a directory match are not included.
- `09-macros/macro_args_first_miss_inside_then_try_directory_then_skip`,
  `09-macros/macro_args_first_miss_inside_then_try_directory_next_dir_missing`,
  `09-macros/macro_args_nested_rejected_then_try_directory_error`: an argument document's own
  waiting error, a miss or a nested rejection, makes a later `try=true` directory include there
  reject the argument document, unless another directory is tried.
- `09-macros/macro_args_rejected_value_at_end_of_file_and_text`,
  `13-inputs/inputs_args_rejected_value_at_end_of_input`: after rejected ARGUMENTS at the end of
  a unit, any byte after the `)` is VALUE.

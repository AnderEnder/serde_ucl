# Pending cases

Cases from `spec-v13` that the crate does not pass yet. The conformance runners do not read this
directory; `scripts/regen-golden.sh` regenerates its golden files like any other case's. When the
crate passes a case, move its files unchanged to `tests/conformance/cases/spec/<same directory>/`,
with the fixtures it uses under `files/`. Fixtures copied here from `cases/spec/09-macros/files/`
(`a.inc`, `text.txt`, `num.txt`) are identical to the originals and can be dropped on the move.

By answer (`docs/clean-room/QUESTIONS.md`):

- #70 (§9.4, a file that ends right after its leading bracket, after a leading comment group):
  `09-macros/include_comment_then_open_brace_takes_nothing_over`,
  `09-macros/include_block_comment_then_open_brace_takes_nothing_over`,
  `09-macros/include_comment_then_open_bracket_adds_nothing`,
  `13-inputs/macro_registered_text_comment_then_open_brace`.
- #71 (§9.6, `.load` with a VALUE that starts with NUL): `09-macros/load_try_path_nul_first_skipped`.
- #72 (§9.4, a wildcard after the NUL): `09-macros/include_glob_wildcard_after_nul_no_match_stops`,
  `09-macros/include_glob_nul_first_wildcard_after_stops`.
- #74 and #76 (§13.2, text in place in a section object):
  `13-inputs/macro_registered_text_braced_after_name_keeps_section_open`,
  `13-inputs/macro_registered_text_braced_after_name_in_braced_root_error`,
  `13-inputs/macro_registered_text_keeps_section_open`,
  `13-inputs/macro_registered_text_keeps_section_chain_open`,
  `13-inputs/macro_registered_text_section_left_open_brace_error`,
  `13-inputs/macro_registered_text_later_section_closes`,
  `13-inputs/macro_registered_text_then_braced_file_keeps_section_open`.
- #75 (§5.2, a leading `-` when nothing is read after the `x`):
  `05-numbers/hex_after_fraction_negative_nothing_read_is_string`.
- #77 (§9.7, the first-value rule at every level): `09-macros/inherit_first_value_rule_at_every_level`.
- #78 (§9.2, string parameters end at their first NUL): `09-macros/include_key_param_ends_at_nul`,
  `09-macros/include_key_param_nul_first_empty_key`, `09-macros/include_path_param_entry_ends_at_nul`,
  `09-macros/include_duplicate_param_ends_at_nul`, `09-macros/include_target_param_ends_at_nul`,
  `09-macros/load_key_param_ends_at_nul`, `09-macros/load_key_param_nul_first_error`,
  `09-macros/load_target_param_ends_at_nul`.

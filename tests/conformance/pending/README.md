# Pending cases

Cases that a released spec version added but the crate does not pass yet. The conformance runners
do not read this directory; `scripts/regen-golden.sh` regenerates the golden files here like the
others. When the crate passes a case, move its files unchanged to `cases/spec/<same directory>/`,
with the files it uses under `files/`. Fixtures that also exist under `cases/spec/` (`files/a.inc`,
`files/text.txt`, `files/v12/args_bad.inc`) are identical copies; keep the one already there.

## spec-v12 (answers to QUESTIONS #60–#69)

29 cases. Names ending in `_error` are errors in libucl; the spec README's coverage table lists
every case under the path it will have, `cases/spec/<dir>/<name>`.

- `02-comments/hash_last_byte_after_*` (4): §2.2, a last-byte `#` after an entry once a VT or FF
  has come before it (#65).
- `06-strings/heredoc_opener_cut_by_end_*` (3): §6.3, `<<` and uppercase letters up to the end of
  a unit of four or more bytes (#61).
- `09-macros/macro_args_nested_*` (5): §9.2 *ARGUMENTS*, a rejected argument document inside an
  argument document (#60). Fixture `files/v12/args_bad.inc`, `files/a.inc`.
- `09-macros/macro_args_backslash_quote_opens_quoted_part`,
  `13-inputs/macro_registered_args_backslash_quote_opens_quoted_part`: §9.2, a `"` after `\`
  outside a quoted part in ARGUMENTS (#64).
- `09-macros/macro_paren_last_byte_is_value`, `13-inputs/macro_registered_paren_last_byte_is_value`:
  §9.2, a `(` that is the last byte of its unit (#63).
- `09-macros/include_*_open_brace*`, `09-macros/include_only_open_bracket_adds_nothing`,
  `13-inputs/macro_registered_text_only_open_brace`,
  `13-inputs/macro_registered_text_only_open_bracket`: §9.4 *Where the entries go*, a unit that
  ends right after its leading bracket (#67). Fixtures `files/v12/*brace*.inc`,
  `files/v12/only_bracket.inc`.
- `09-macros/macro_value_nul_*` (5): §9.2 *VALUE*, a NUL byte in a braced VALUE (#69). Fixtures
  `files/a.inc`, `files/text.txt`.
- `13-inputs/macro_registered_ctx_copy_keeps_root_priority`,
  `13-inputs/macro_registered_ctx_copy_priority_not_from_priority_macro`: §13.2, the copy of the
  root that `.ctx` adds keeps the root's priority (#62).

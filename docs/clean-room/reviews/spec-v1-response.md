# Response to the spec-v1 review

Review: `docs/clean-room/reviews/spec-v1.md` (verdict: release after fixes). Commits:

- `52ae390`: new conformance cases, with golden files from the oracle.
- `2ddb2d6`: spec text.

Every finding is fixed in this version; none is deferred. Where a fix states a rule that no
committed case can show, `docs/spec/README.md` (*Known gaps (fix in spec-v2)*) lists it.

| ID | Resolution | Commit(s) | Cases added or changed |
| --- | --- | --- | --- |
| A-1 | §10.3 is a table of mutually exclusive conditions → ISO C conversion | `2ddb2d6` | — |
| A-2 | §4.8 states what the escape covers and the resulting code point | `2ddb2d6` | — |
| A-3 | §7.3 states the kept text and inner expansion as results | `2ddb2d6` | `07-variables/unterminated_brace_inner_reference` |
| A-4 | §7.1 states the observable order of variables | `2ddb2d6` | — |
| A-5 | §3.4 states the name test as a condition on the rest of the line, per word | `2ddb2d6` | — |
| A-6 | §2.3 and §9.2 state what the input may contain, not what is skipped | `2ddb2d6` | — |
| A-7 | §4 intro, §4.4, §4.6 and the §7.2 table restated as consequences | `2ddb2d6` | — |
| B-1 | §10.2 and §10.8 write the six ASCII characters `\uFFFD` (and `\u000B`) | `2ddb2d6` | — |
| B-2 | Escape text restored in §3.2, §4.7, §6.1 (three places), §10.2, §10.8; all examples rechecked against case inputs | `2ddb2d6` | — |
| B-3 | §10.3 uses t rounded toward zero, the strict 10⁻⁷ bound, and lists `-inf` | `52ae390`, `2ddb2d6` | `10-output/floats_boundaries` |
| B-4 | §5.3: 127 or more characters is a string, 126 still a number (sign and suffix not counted; hex counts digits after `x`); §10.9 updated | `52ae390`, `2ddb2d6` | `05-numbers/number_length_limit` |
| B-5 | §9.7: `.inherit` may name the root key that holds the current object; first-value rule; nothing copied from itself; enclosing-object quirk | `52ae390`, `2ddb2d6` | `09-macros/inherit_own_object`, `inherit_own_key_earlier_value`, `inherit_enclosing_object` |
| B-6 | §8.4 and §11.1: under `merge`, an object followed by an array is an error | `52ae390`, `2ddb2d6` | `08-duplicates/strategy_merge_object_then_array_error` |
| B-7 | §8.4: arrays merge whatever the priorities; a scalar replacing a container keeps the container's priority; array case added | `52ae390`, `2ddb2d6` | `08-duplicates/strategy_merge_arrays_ignore_priority`, `strategy_merge_scalar_keeps_container_priority`, `strategy_merge_array_then_scalar` |
| B-8 | §1.6 restated: errors at end of input, the next-line value, the comment group, and when the value is `null` | `52ae390`, `2ddb2d6` | nine `01-structure/empty_value_*` cases |
| B-9 | §1.3: a terminator before the first root entry is an error | `52ae390`, `2ddb2d6` | `01-structure/root_leading_semicolon_error`, `root_leading_comma_error`, `root_leading_terminator_after_newline_error` |
| B-10 | §7.1 and README: re-registration keeps position; file variables come first; a `var:` entry for them has no effect for file input, and does for string input | `52ae390`, `2ddb2d6` | `07-variables/reregister_keeps_position`, `filevars_precede_registered`, `filename_reregistered_for_string_input`; the file-input case is listed in *Known gaps* |
| B-11 | §5.2: citations fixed; `-0x`, `0x-1`, `1x` and the signed form `-12x34` covered | `52ae390`, `2ddb2d6` | `05-numbers/hex_more_malformed` |
| B-12 | §4.8: one-, two- and three-digit prefixes | `52ae390`, `2ddb2d6` | `04-atoms/backslash_invalid_unicode_prefixes` |
| B-13 | §10.5: 80 bytes JSON form, 81 heredoc; a first line `EOD` does not trigger the exception; §6.3 states that the first heredoc line is always content | `52ae390`, `2ddb2d6` | `10-output/long_string_boundary`, `heredoc_first_line_eod`, `06-strings/heredoc_first_line_is_content` |
| B-14 | §10.5 and §10.6: empty elements in config arrays; YAML output of an empty root | `2ddb2d6` | — (cites `arrays`, `empty_document`) |
| B-15 | §9.7: copies keep the source priority; inherited values replaced whatever the priorities; `replace=true` copies not inherited | `52ae390`, `2ddb2d6` | `09-macros/inherit_keeps_source_priority`, `inherit_replaced_whatever_priority`, `inherit_replace_copies_not_inherited` |
| B-16 | §7.2: the braced macro value form is cited | `52ae390`, `2ddb2d6` | `09-macros/include_braces_variables` |
| B-17 | §3.4: brackets inside quotes, section objects left open, separators after names | `52ae390`, `2ddb2d6` | six `03-keys/section_path_*` cases |
| B-18 | §5.1 cites `floats`; `a02_bare_float_inside` moved to §5.7 | `2ddb2d6` | — |
| B-19 | §10.9: the extra final LF of each `.res` file | `2ddb2d6` | — |
| B-20 | `object_depth_limit_error` now nests exactly 1024 objects; §11.2 states the boundary for both container kinds | `52ae390`, `2ddb2d6` | `11-errors/object_depth_limit_error` changed; the 1023 boundary is listed in *Known gaps* |

Also added: `07-variables/dollar_escape_in_unquoted` (the reviewer's probe that §7.5 holds for
unquoted values).

Self-check greps after the fixes: no `ucl_` or `UCL_` outside the public flag names in §12, no
`.c` or `.h` references, no code blocks. Every citation resolves to a case, and every
`cases/spec/` case is cited.

`tests/conformance/xfail.txt` lists the 28 new cases that the pre-C1 parser (commit `03b2756`)
fails. At that commit, with the new cases: 757 cases, 357 pass, 400 expected failures, green.

Note on B-2's cause: while this spec was being written, the tool that wrote the files replaced every
backslash-`u` escape followed by four hex digits with the decoded character. The fixed text was
written with a placeholder for the backslash, and checked by scanning for such decoded characters
and for every backslash-`u` sequence.

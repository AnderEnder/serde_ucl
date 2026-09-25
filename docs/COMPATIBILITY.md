# Compatibility with libucl

This crate reads and writes the UCL format as libucl does, the C library used by FreeBSD. The
reference is libucl at commit `24c8b399062ae4691168c243e3b7345ef7f31956`. Compatibility is
checked, not assumed: every case under `tests/conformance/` is compared with libucl's own result,
and every value the crate parses is written in each output format and compared with libucl's
output byte for byte. The behaviour is described section by section in `docs/spec/`; this page
lists where the crate deliberately differs, and the libucl quirks it reproduces.

Case paths below are relative to `tests/conformance/`. Flag names are those of the spec: for
example `key-lowercase` is `ParserFlags::KEY_LOWERCASE` and `no-filevars` is
`ParserFlags::NO_FILEVARS`.

## Deliberate differences

| Where | libucl | This crate | Why | Case or test |
| --- | --- | --- | --- | --- |
| Bytes that are not UTF-8 in keys and strings, including strings made invalid by `\u` escapes of surrogate code points (spec §6.1) | accepted | error | keys and strings are Rust `String`s | `libucl/basic/22` |
| `.includes`, and `sign=true` on any include macro (§9.4) | checks a signature when built with support for it; the reference build ignores `sign` | "unsupported" error; `sign=false` is accepted | a configuration that asks for a signature check must not be accepted without one | `cases/spec/09-macros/includes_like_include`, `cases/spec/09-macros/include_sign_param_no_effect` |
| `.load` (§9.6) | always available | only with the Cargo feature `load`, off by default | it reads arbitrary files into values | `cases/spec/09-macros/load_*` (run with the feature) |
| Documents given as text: `from_str`, `from_slice`, `from_reader` and a parser without a file loader | `.include`, `.try_include` and `.load` read files relative to the working directory | no file is found, so they behave as for a missing file (§9.4, §9.6); file access is opt-in through `ParserBuilder` | parsing untrusted text must not read local files | `tests/api_tests.rs`: `text_input_reads_no_files`, `text_input_loads_no_files` |
| The working directory | relative include paths and `CURDIR` in macro argument lists (§9.2) use it | never used: for a document parsed from a file, that file's directory stands in for it, in included files too; for text, a configured base directory | results must not depend on where the program runs | `tests/api_tests.rs`: `from_file_resolves_includes_against_the_file_directory` |
| Macro argument lists nested inside argument lists (§9.2) | no limit of its own; very deep nesting crashes | at most 64 levels (`parse::MAX_ARGUMENT_DEPTH`), then an error | bounded stack use | `cases/spec/09-macros/macro_args_nested_100_levels` |
| Values that `.inherit` copies (§9.7, §11.2) | no limit: copies of copies can nest a value tens of thousands of levels deep | a copy that would nest a value more than 1024 containers deep, the root included, is an error (`parse::ErrorKind::NestingTooDeep`), the same limit as for containers open at once | bounded stack use: no parsed value is nested deeper than 1024 | `tests/stack_depth.rs`; no conformance case, because the golden file would be too deep for the runner |
| A variable handler's result in a string that also holds other text (§7.7) | depends on memory contents | the result is substituted in place | libucl's result is undefined | none (undefined in libucl) |
| An error or silent stop inside an included file when the same macro still has files to read: a glob of `.try_include`, or a search path with directories left (§9.4) | crashes | an error fails the document; a silent stop ends the parse | libucl's behaviour is a crash | none (libucl crashes) |
| An included file that starts with `[` (§9.4) | reads on and may stop or crash | error at the `[` | libucl's behaviour is undefined | none (undefined in libucl) |
| A `}` in an included file that closes an object that is an array element of the including document (§9.4) | reads keys into the array and crashes | parsing goes on inside the array | libucl's behaviour is a crash | none (libucl crashes) |
| A silent stop (§9.4, *Missing and unusable files*): `.try_include` of a missing file and similar | reports failure with no message and keeps the partial result | `UclError::Stopped` / `parse::ErrorKind::Stopped`, with the partial result available from the error | an application must be able to tell a stop from success | `cases/spec/09-macros/try_include_missing_stops_parsing` (partial result compared) |
| serde JSON output (`to_json_string`, `to_json_string_compact`) | writes `nan`, `inf` and times with a suffix, which is not JSON | valid JSON (RFC 8259): a time is its number of seconds; NaN and infinite floats and times are a serialization error | output of a JSON function must be JSON | `tests/serde_roundtrip.rs` |

These choices match libucl rather than differ from it:

- URLs are never fetched. An include with `url=true` and `://` in the path behaves as in a libucl
  built without URL support, as the reference build is (§9.4; `cases/spec/09-macros/include_url_param_error`).
- `path` search lists behave as in libucl, including that a list stays in effect for later
  includes (§9.4; `cases/spec/09-macros/include_path_persists`).
- Parsing a file by path defines `FILENAME` and `CURDIR` even with `no-filevars`, as libucl's
  file parsing does; parsing text honours the flag (§12.7).
- Saved comments are written in config output only when the caller asks for them, as in libucl
  (§10.10).

## libucl quirks the crate reproduces

libucl behaves in some places in ways a reader may not expect. The crate does the same, because
configurations written for libucl rely on it. The spec marks each as **Quirk**; notable ones:

| Behaviour | Spec | Case |
| --- | --- | --- |
| A leading `[` or `{` starts a bracketed root only after whitespace alone, or directly after comments; `# c⏎⏎{…}` is an error | §1.1 | `cases/spec/01-structure/root_bracket_after_comment_and_blank_line_error` |
| After the closing bracket of a bracketed root, the rest of the input is ignored | §1.1 | `cases/spec/01-structure/top_array_trailing_ignored` |
| Whitespace after comments before an array's first element is part of the element: `[ /* c */ 1]` → `[" 1"]` | §1.5 | `cases/spec/01-structure/array_first_element_after_comment_group` |
| VT and FF count as line breaks after a key | §1.6 | `cases/spec/01-structure/key_vertical_tab_is_line_break` |
| A `#` as the last byte of the input can be an error | §2.2 | `cases/spec/02-comments/hash_last_byte_after_leading_newline_error` |
| Quotes inside block comments are tracked; a `"` after a `\` never opens or closes one | §2.3 | `cases/spec/02-comments/block_comment_escaped_quote_outside_quotes` |
| A quoted key followed by a line break takes its value from the next line | §3.2 | `cases/spec/03-keys/quoted_newline_before_separator` |
| In section paths, a `=` or `:` after a name is ignored: `a b = c {…}` nests three objects | §3.4 | `cases/spec/03-keys/section_path_separator_is_ignored` |
| A short `\u` escape in an unquoted value keeps the `u` and drops the next byte | §4.8 | `cases/spec/04-atoms/backslash_short_unicode_at_end` |
| Digits before an `x` are ignored: `12x34` → `52` | §5.2 | `cases/spec/05-numbers/hex_digits_before_x_ignored` |
| After a fraction or exponent, `x` gives `0`: `1.5x10` → `0` | §5.2 | `cases/spec/05-numbers/hex_after_fraction_or_exponent` |
| Numbers of 127 or more characters are strings | §5.3 | `cases/spec/05-numbers/number_length_limit` |
| `kb`, `mb` and `gb` truncate floats: `1.5kb` → `1024` | §5.4 | `cases/spec/05-numbers/float_binary_multiplier_truncates` |
| Integer multipliers wrap around in 64 bits without error | §5.4 | `cases/spec/05-numbers/multiplier_overflow_wraps` |
| Time suffixes overflow to infinite times without error | §5.4 | `cases/spec/05-numbers/time_suffix_overflow_infinite` |
| A space or tab after a suffixed number makes the value a string: `30s # c` → `"30s"` | §5.5 | `cases/spec/05-numbers/suffix_then_space_comment_is_string` |
| A number followed by `/*` is a string | §5.6 | `cases/spec/05-numbers/number_then_block_comment` |
| An empty heredoc cannot be written; a heredoc with an empty name has its own end rule | §6.3 | `cases/spec/06-strings/heredoc_empty_error`, `cases/spec/06-strings/heredoc_empty_terminator` |
| `$$` is a literal `$` only in a string where some variable was replaced | §7.5 | `cases/spec/07-variables/dollar_escape_only_when_expanding` |
| Under `merge`, a scalar replaces an object or array whatever the priorities | §8.4 | `cases/spec/08-duplicates/strategy_merge_object_then_scalar` |
| Priorities are taken modulo 16 | §8.3 | `cases/spec/08-duplicates/priority_modulo_16` |
| A macro right after a section name makes the next key a name | §9.1 | `cases/spec/09-macros/macro_after_name_then_separator_key_is_name_error` |
| Macro argument lists see no application variables | §9.2 | `cases/spec/09-macros/macro_args_no_variables` |
| An include moves `FILENAME` and `CURDIR` to the end of the variable lookup order | §9.4 | `cases/spec/09-macros/include_moves_filevars_last` |
| Section objects left open by an included file stay open in the including file | §9.4 | `cases/spec/09-macros/include_left_open_section_persists` |
| `.inherit(replace=true)` appends copies instead of replacing | §9.7 | `cases/spec/09-macros/inherit_replace_appends` |
| Keys containing `;`, `}`, `#` or `,` are written unquoted in config output and cannot be read back | §10.1 | `cases/spec/10-output/keys_quoting` |
| Under `key-lowercase`, a quoted key is lowercased before its escapes are decoded | §12.1 | `cases/spec/12-flags/key_lowercase_before_escapes` |
| Under `no-time`, `ms`, `ks` and `gs` still give times | §12.3 | `cases/spec/12-flags/no_time` |
| A saved block comment includes the byte after its `*/` | §12.5 | `cases/spec/12-flags/comments_block_comment_saved_with_next_byte` |
| `no-filevars` still lets an include define `FILENAME` and `CURDIR` | §12.7 | `cases/spec/09-macros/no_filevars_include_defines_them` |

## Not supported

- Schema validation, MessagePack and signature checking, which libucl offers besides the format.
- Output formats other than JSON, compact JSON, the UCL config format and YAML.

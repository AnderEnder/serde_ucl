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
| The working directory | relative include paths and `CURDIR` in macro argument lists (§9.2) use it, in every input of a parser (§13.1) | never used: a configured base directory stands in for it; without one, a document parsed from a file, or an input given as a file, uses that file's directory, in included files too | results must not depend on where the program runs | `tests/api_tests.rs`: `from_file_resolves_includes_against_the_file_directory`; `tests/inputs_and_macros.rs`: `file_inputs_read_through_the_loader_and_set_the_file_variables` |
| Macro argument lists nested inside argument lists (§9.2) | no limit of its own; very deep nesting crashes | at most 64 levels (`parse::MAX_ARGUMENT_DEPTH`), then an error | bounded stack use | `cases/spec/09-macros/macro_args_nested_100_levels` |
| Values that `.inherit` copies (§9.7, §11.2) | no limit: copies of copies can nest a value tens of thousands of levels deep | a copy that would nest a value more than the parser's `.inherit` depth limit deep, the root included, is an error (`parse::ErrorKind::NestingTooDeep`, whose `limit` is the setting); the limit is 1024 by default, the same as for containers open at once, and can be set up to 2048 (`ParserBuilder::with_inherit_depth_limit`, `Parser::set_inherit_depth_limit`, `parse::MAX_INHERIT_DEPTH_LIMIT`) | bounded stack use: up to the largest setting, every entry point handles the parser's values on a 2 MiB stack in an unoptimised build | `tests/stack_depth.rs`; no conformance case, because the golden file would be too deep for the runner |
| A variable handler's result in a string that also holds other text (§7.7) | depends on memory contents | the result is substituted in place | libucl's result is undefined | none (undefined in libucl) |
| An error or silent stop inside an included file when the same macro still has files to read: a glob of `.try_include`, or a search path with directories left (§9.4) | crashes | an error fails the document; a silent stop ends the parse | libucl's behaviour is a crash | none (libucl crashes) |
| An included file that starts with `[` (§9.4) | reads on and may stop or crash | error at the `[` | libucl's behaviour is undefined | none (undefined in libucl) |
| A `}` in an included file that closes an object that is an array element of the including document (§9.4) | reads keys into the array and crashes | parsing goes on inside the array | libucl's behaviour is a crash | none (libucl crashes) |
| Keys and strings that come from variable-expanded `.emit` text of a registered macro under `zerocopy` (§12.2) | NUL or other bytes, which differ between runs | the expanded text | libucl's behaviour is undefined | none (undefined in libucl); the differential fuzzer skips these differences |
| Text that a registered macro has parsed in place and that starts with `[` followed by more, other than `[1]` and `[]` (§13.2) | writes the value after the `[` over the most recent value, or over the root | error at the `[`, as for an included file | libucl's behaviour is undefined | none (undefined) |
| Strings that hold a NUL byte, copied by `.inherit` (§9.7) | the copy keeps the length, but its bytes after the first NUL depend on memory contents | copied exactly | libucl's behaviour is undefined | `cases/spec/09-macros/inherit_copies_key_with_nul` (keys, which libucl copies exactly) |
| A silent stop (§9.4, *Missing and unusable files*): `.try_include` of a missing file and similar | reports failure with no message and keeps the partial result | `UclError::Stopped` / `parse::ErrorKind::Stopped`, with the partial result available from the error | an application must be able to tell a stop from success | `cases/spec/09-macros/try_include_missing_stops_parsing` (partial result compared) |
| A registered macro handler that fails (§13.2) | a handler can only succeed or fail; a failure is a silent stop with no message | a handler can stop silently (`MacroError::stop()`, reported as a stop) or fail with its own message (`MacroError::new`, `parse::ErrorKind::MacroFailed`) | an application must be able to say why its macro failed | `tests/inputs_and_macros.rs`: `handlers_fail_with_a_stop_or_a_message` |
| A later input that starts with `{` after a zero-byte first input (§13.1) | crashes | error, as for any later input with content there | libucl's behaviour is a crash | `tests/inputs_and_macros.rs`: `only_the_first_input_sets_up_the_root` |
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
- Registered macro handlers can do what libucl's can: add entries where the macro stands, and
  have text parsed in place of the macro (§13.2).

A limit of this crate's API, with no libucl counterpart: a deserialization error from a document
whose parse ran a registered macro carries no position, because positions are found by parsing
again and a handler is not run a second time.

### Behaviour that depends on the platform

libucl leaves glob matching, the base name of a path and the handling of a trailing `/` to the C
library and the operating system. The crate follows the reference platform (macOS) everywhere:
the glob rules of spec §9.4 (`^` in `[^…]` is an ordinary member, no character classes, a key from
the first matched file under `prefix=true`). With glibc, libucl gives other results for those
cases. Whether a pattern ending in `/` matches a symbolic link to a file, and whether a plain path
ending in `/` after a file name names that file, differ between macOS and Linux in libucl, and the
crate may follow either (§9.4, *Uncertain*). The crate matches directories only with a glob pattern
that ends in `/`, on every platform, so a symbolic link to a file is left out; a plain path that
ends in `/` after a file's name is looked up as the operating system does, which names the file on
macOS and nothing on Linux.

### NUL bytes, globs, copies and text in place (spec-v13)

- With a leading `-`, a number whose hex digits after an `x` that follows a fraction or exponent
  begin with a letter is a string: `-1.5xd` (§5.2;
  `cases/spec/05-numbers/hex_after_fraction_negative_nothing_read_is_string`).
- The string parameters of the include macros and `.load` end at their first NUL byte:
  `key="s\u0000t"` is `s` (§9.2; `cases/spec/09-macros/include_key_param_ends_at_nul`,
  `cases/spec/09-macros/load_key_param_ends_at_nul`).
- `.load(try=true)` with a VALUE that starts with a NUL byte skips the missing file; an empty VALUE
  is still an error (§9.2, §9.6; `cases/spec/09-macros/load_try_path_nul_first_skipped`).
- With `glob=true`, a `*` or `?` after a NUL byte in the VALUE makes the part before the NUL a
  pattern (§9.4; `cases/spec/09-macros/include_glob_wildcard_after_nul_no_match_stops`).
- An included file or text in place that is only a `{` or `[` after a leading comment group adds
  nothing (§9.4, §13.2; `cases/spec/09-macros/include_comment_then_open_brace_takes_nothing_over`).
- Text parsed in place while the innermost open object is a section object keeps that object open
  for the rest of the parse (§13.2; `cases/spec/13-inputs/macro_registered_text_keeps_section_open`).
- The first-value rule of `.inherit` copies applies at every level of the copy (§9.7;
  `cases/spec/09-macros/inherit_first_value_rule_at_every_level`).

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
| A `.include` whose search path lacks the file in the first directory is accepted, with a later directory's file, when a skipped URL include (`try=true, url=true` or `.try_include(url=true)`) follows in the same input | §9.4 | `cases/spec/09-macros/include_path_first_miss_then_url_try_accepts_later` |
| `.inherit(replace=true)` appends copies instead of replacing | §9.7 | `cases/spec/09-macros/inherit_replace_appends` |
| Keys containing `;`, `}`, `#` or `,` are written unquoted in config output and cannot be read back | §10.1 | `cases/spec/10-output/keys_quoting` |
| Under `key-lowercase`, a quoted key is lowercased before its escapes are decoded | §12.1 | `cases/spec/12-flags/key_lowercase_before_escapes` |
| Under `no-time`, `ms`, `ks` and `gs` still give times | §12.3 | `cases/spec/12-flags/no_time` |
| A saved block comment includes the byte after its `*/` | §12.5 | `cases/spec/12-flags/comments_block_comment_saved_with_next_byte` |
| `no-filevars` still lets an include define `FILENAME` and `CURDIR` | §12.7 | `cases/spec/09-macros/no_filevars_include_defines_them` |

### Several inputs into one parser (§13.1)

A parser that reads several inputs in turn keeps libucl's behaviour where they join:

- A parser takes at most 16 inputs. Each counts towards the include nesting limit for the rest of
  the parse, so later inputs may nest includes less deeply (`cases/spec/13-inputs/inputs_seventeen_inputs_error`,
  `cases/spec/13-inputs/inputs_include_depth_shared_error`).
- A zero-byte first input gives an empty root that later inputs cannot add to
  (`cases/spec/13-inputs/inputs_empty_first_then_entries_error`).
- The end of an input is not a separator.
  - An input that ends right after a value such as `x = 1`, or after an unquoted value and
    spaces, must be followed by one that starts with a line break, `;`, `,` or a comment
    (`cases/spec/13-inputs/inputs_value_at_end_then_entry_error`,
    `cases/spec/13-inputs/joins_bare_value_trailing_space_error`).
  - Spaces after a quoted value do separate
    (`cases/spec/13-inputs/joins_quoted_value_trailing_space`).
- A later input of whitespace alone makes the next input need a separator; a second one undoes
  that (`cases/spec/13-inputs/joins_whitespace_input_needs_separator_error`,
  `cases/spec/13-inputs/joins_two_whitespace_inputs_cancel`).
- A key followed by its separator and a line break at the end of an input takes its value from
  the next input (`cases/spec/13-inputs/joins_value_in_next_input`).
- After a closed root, a later input must start with a line break, `;`, `,` or a comment, and
  the rest of it is then ignored (`cases/spec/13-inputs/joins_closed_root_then_line_break_rest_ignored`,
  `cases/spec/13-inputs/joins_closed_root_then_space_entry_error`).
- An included file in which the parse stopped silently stays open for the rest of the parse. It
  keeps its `FILENAME` and counts towards the limits, and including it again is self-inclusion
  (`cases/spec/13-inputs/joins_stopped_include_again_is_self_inclusion_error`).
- A text input goes on in the file of the input before it, so after a file input it cannot
  include that file (`cases/spec/13-inputs/joins_text_input_continues_file_input_self_include_error`).

### Macro arguments, values and text in place (spec-v12)

- Inside an argument document, a macro whose own ARGUMENTS are rejected runs without them, its
  VALUE starting right after the `)`; the rejection is not an error (§9.2;
  `cases/spec/09-macros/macro_args_nested_rejected_dropped`).
- In ARGUMENTS, a `"` after a `\` outside quotes begins a quoted part (§9.2;
  `cases/spec/09-macros/macro_args_backslash_quote_opens_quoted_part`).
- A `(` that is the last byte of its unit is a macro's VALUE, not its ARGUMENTS (§9.2;
  `cases/spec/09-macros/macro_paren_last_byte_is_value`).
- A NUL byte in a braced VALUE ends a path or a priority, and an empty priority before it is 0
  (§9.2; `cases/spec/09-macros/macro_value_nul_ends_include_path`,
  `cases/spec/09-macros/macro_value_nul_first_priority_zero`).
- `<<` followed by uppercase letters up to the end of a unit of four or more bytes is an error
  (§6.3; `cases/spec/06-strings/heredoc_opener_cut_by_end_error`).
- After an entry, a VT or FF makes a last-byte `#` an error (§2.2;
  `cases/spec/02-comments/hash_last_byte_after_formfeed_after_entry_error`).
- An included file or text parsed in place that is only a `{` or `[` adds nothing and takes no
  brace over (§9.4; `cases/spec/09-macros/include_only_open_brace_takes_nothing_over`).
- A copy of the root that a registered context macro adds keeps the first input's priority
  (§13.2; `cases/spec/13-inputs/macro_registered_ctx_copy_keeps_root_priority`).

## Not supported

- Schema validation, MessagePack and signature checking, which libucl offers besides the format.
- Output formats other than JSON, compact JSON, the UCL config format and YAML.

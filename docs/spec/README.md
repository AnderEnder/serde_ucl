# UCL Behaviour Specification

This specification describes, feature by feature, how libucl (the reference implementation used
by FreeBSD, pinned at commit `24c8b399062ae4691168c243e3b7345ef7f31956`) reads UCL text and what
it produces. It is written for the implementation team and states **observable behaviour only**:
which inputs are accepted, which value each produces, which are rejected, and what the output
formats look like. It says nothing about how libucl is built internally.

Every rule cites conformance cases. A case is an input file under `tests/conformance/` whose
expected result (`<case>.golden.json`, and for every case that parses also its output in each
format, `<case>.<format>.golden`, §10) was produced by running libucl on it. **Where this text and a golden file disagree, the golden
file is right**; please report the discrepancy in `docs/clean-room/QUESTIONS.md`.

The project matches libucl's quirks exactly. Behaviour that looks accidental is still specified
and labelled **Quirk**. Behaviour that could not be pinned down is labelled **Uncertain**; the
implementation may choose, and should record its choice.

## Sections

| § | File | Topic |
| --- | --- | --- |
| 1 | [01-structure.md](01-structure.md) | Document structure, entries, separators, objects, arrays |
| 2 | [02-comments.md](02-comments.md) | Whitespace and comments |
| 3 | [03-keys.md](03-keys.md) | Keys and named sections |
| 4 | [04-unquoted-values.md](04-unquoted-values.md) | Unquoted values: extent, keywords, escapes |
| 5 | [05-numbers.md](05-numbers.md) | Numbers, multipliers, time suffixes |
| 6 | [06-strings.md](06-strings.md) | Double-quoted, single-quoted and heredoc strings |
| 7 | [07-variables.md](07-variables.md) | Variable expansion |
| 8 | [08-duplicates.md](08-duplicates.md) | Repeated keys, priorities, duplicate strategies |
| 9 | [09-macros.md](09-macros.md) | Macros: `.include`, `.try_include`, `.includes`, `.priority`, `.load`, `.inherit` |
| 10 | [10-output.md](10-output.md) | Output formats: config (UCL), JSON, compact JSON, YAML |
| 11 | [11-errors.md](11-errors.md) | Errors and limits |
| 12 | [12-flags.md](12-flags.md) | Parser flags |
| 13 | [13-inputs-and-macros.md](13-inputs-and-macros.md) | Several inputs into one parser; macros registered by the application |

## Conventions

### Case citations

A case ID is its path under `tests/conformance/` without the extension, for example
`cases/spec/05-numbers/hex` or `libucl/basic/2`. Inside a section, a bare name such as `hex`
means the case of that name in the section's own directory, `cases/spec/NN-topic/`.

### Result notation

| Notation | Meaning |
| --- | --- |
| `int 42` | 64-bit signed integer |
| `float 1.5` | double-precision float |
| `time 90` | time value: a float number of seconds, a distinct type from `float` |
| `"text"` | string (JSON escapes used for readability; the value holds the bytes) |
| `true`, `false`, `null` | boolean and null values |
| `[a, b]` | explicit array |
| `{ k: v, … }` | object; entries keep the order in which keys first appeared |
| `k: ⟨v1 \| v2⟩` | one key holding several values in order (a *multi-value* entry, libucl's "implicit array"); see §8 |
| `v @3` | value `v` carries priority 3 (priority 0 is not written); see §8 |
| **error** | the whole document is rejected |

Examples are written `input` → result. `⏎` marks a line feed where it matters, and `␍` a
carriage return.

### How the conformance oracle runs every case

These settings are part of every expected result:

- The variable `ABI` is registered with the value `unknown`. `.flags` entries `var:NAME=VALUE` register
  further variables after it, in file order. The file variables come before all of these (§7.1).
- The file variables `FILENAME` (the case's absolute path) and `CURDIR` (its directory) are set,
  unless the case's `.flags` contains `no-filevars`. Golden files never contain their values.
- Each case is parsed from its own directory, so relative file paths in macros resolve against it
  (§9).
- The input is one unit with priority 0 and the `append` duplicate strategy, unless `.flags` says
  `priority:N` or `strategy:NAME` (§8).
- `variable-handler` installs a test handler: every braced variable name starting with `H_`
  resolves to `[handled]`, any other name is refused (§7).
- `string-input` parses the case as if it were given as a string rather than a file (§7.8).
- Parser flags are off unless `.flags` names them: `key-lowercase`, `zerocopy`, `no-time`,
  `no-implicit-arrays`, `save-comments`, `disable-macro`, `no-filevars` (§12).
- `dump-comments` turns on `save-comments` and also records, in the golden file, the comments
  attached to each value: `"c"` for comments attached before it, `"ca"` for comments attached after
  it (§12.5).

Only whether a case fails is compared, never the wording of the error. **Error message wording is
not part of this specification** (§11.1).

The dumps also record two priorities that have no observable effect, that of the root object and
those of array elements (§8.7). The conformance runner ignores them.

### Terms

- **Document**: the input bytes given to the parser. A document produces one **root** value, an
  object or an array.
- **Entry**: a key and its value(s) inside an object.
- **Unquoted value**: a value written without quotes, such as `hello world`, `42`, `true` or
  `http://x/y` (§4). Its cases live in `cases/spec/04-atoms/`.
- **Input unit**: the main document, or a file brought in by a macro; each has its own priority
  and duplicate strategy (§8, §9).
- **Terminator characters**: LF, CR, NUL, `,`, `;` (§1).

## Divergences decided by the project

The project deliberately differs from libucl in these places. The spec still describes libucl's
behaviour, and each place says so. The decisions for §9, §10 and the API are recorded in
`docs/clean-room/WORKLIST.md` (C3, C4 and C5, *Project decisions*).

- Non-UTF-8 bytes in keys and strings are an **error** in the project (libucl accepts them;
  `libucl/basic/22`). This also covers strings made invalid by `\u` escapes of surrogate code
  points (§6).
- The project never verifies signatures: `.includes`, and `sign=true` on any include macro, return
  an "unsupported" error; `sign=false` is accepted. libucl built without signature support, as the
  oracle is, ignores `sign` (§9.4; `cases/spec/09-macros/includes_like_include`,
  `cases/spec/09-macros/include_sign_param_no_effect`).
- `.load` is available only behind a default-off feature in the project (§9.6).
- A document given as text, without a path, reads no files by default in the project: an
  `.include`, `.try_include` or `.load` in it finds no file and behaves as §9.4 and §9.6 describe
  for a missing file. libucl reads files relative to the working directory. A caller can give such
  a document file access; a document parsed from a file has it.
- The project never uses the process working directory. For a document parsed from a file, the
  file's directory stands in for it: relative include paths resolve against that directory, in
  included files too, and `CURDIR` in macro argument lists (§9.2) is that directory. For a document
  given as text, a configured base directory stands in for it.
- Macro argument documents nest at most 64 deep in the project, the document holding the outermost
  macro included; deeper nesting is an error. libucl sets no limit of its own (§9.2;
  `cases/spec/09-macros/macro_args_nested_100_levels`).
- A copy made by `.inherit` that would nest a value more than 1024 containers deep, the root
  included, is an error, the same limit as for containers open at once (§11.2). libucl sets no
  limit: copies of copies can nest a value tens of thousands of levels deep (§9.7). No case: the
  golden file would be deeper than the conformance runner reads.
- A variable-handler result that shares its string with other text is substituted in place in the
  project. libucl's result there depends on memory contents (§7.7).
- A failure inside an included file after which libucl goes on with the same macro, a glob of
  `.try_include` or a search path with directories left (§9.4, *Missing and unusable files*),
  crashes libucl; in the project an error there fails the document, and a silent stop ends the
  parse.

- A registered macro handler (§13.2) can also fail with an error and a message of its own, which
  libucl gives a handler no way to do; its silent failure stays a silent stop.
- Without a configured base directory, an input given as a file (§13.1) resolves relative include
  paths against its own directory, where libucl uses the working directory; with one, that
  directory stands in for the working directory for every input.

These are project decisions that match libucl's behaviour rather than differ from it:

- The project never fetches URLs; a URL include behaves as in a libucl built without URL support,
  which the oracle is (§9.4).
- `path` search lists are implemented as §9.4 describes, quirks included.
- Parsing a file by path defines `FILENAME` and `CURDIR` from that path whatever `no-filevars`
  says, as libucl's function for parsing a file does; parsing bytes honours the flag (§12.7).
- The API reports a silent stop (§9.4) as an error of its own kind, from which the partial result
  can be retrieved. Where relative include paths resolve, and what `CURDIR` is for a document given
  as bytes, are parser options; the conformance runner sets them to the case's directory.
- Saved comments are written in config output only when the caller asks for it, as libucl writes
  them only when the application asks (§10.10); the default config output has none.
- Several inputs (§13.1) keep libucl's quirks where inputs join: the limit of 16 inputs and the
  include depth they share, the zero-byte first input, and the end of an input not being a
  separator. Registered handlers can both add entries and have text parsed in place (§13.2).
- serde serialization need not use the default formats (§10.8). Its JSON output is valid JSON
  (RFC 8259), unlike libucl's JSON output of NaN and infinities (§10.3): a time is written as its
  number of seconds, and a NaN or infinite float or time cannot be serialized to JSON. The config
  and YAML forms of serde follow §10.8.

## Uncertain behaviour (summary)

Only behaviour that is undefined in libucl, because it depends on memory contents or on the
platform, is left uncertain:

- Variable-handler results combined with other text in one string (§7.7); the project's choice is
  listed under *Divergences decided by the project*.
- The byte saved after a block comment that ends the input, under `save-comments` (§12.5).
- Saved comments of a value that was replaced under §8, which can reappear on a later value
  (§12.5).
- A float outside the 64-bit range truncated for `kb`, `mb` or `gb` (§5.4).
- Macro argument documents nested so deep that libucl crashes (§9.2); the project's limit is
  listed under *Divergences decided by the project*.
- A failure inside an included file after which libucl goes on with the same macro: a glob of
  `.try_include`, or a search path with directories left (§9.4); the project's choice is listed
  under *Divergences decided by the project*.
- A macro directly after a name, followed only by comments to the end of its unit, when the value
  created most recently is not an object (§9.1).
- A `}` in a file included under a key, when the object where the macro stands has only its own
  bracket (§9.4, *Nesting under a key*).
- Whether a container opened by an included file that has ended counts as opened by a later
  included file, at the check at the end of that file (§9.4, *Where the entries go*).
- An included file that starts with `[`: libucl reads on instead of reporting the error at the
  `[`, and can stop silently or crash first; the project reports the error at the `[` (§9.4,
  *Where the entries go*).
- A `}` in an included file that closes an object which is an array element opened by the
  including unit: libucl then reads keys into the array and crashes; the project goes on inside
  the array (§9.4, *Where the entries go*).
- Text parsed in place that starts with `[` and holds more than the `[`, other than `[1]` and `[]`
  (§13.2, *What the handler can do*).
- The bytes after the first NUL of a string that `.inherit` or a test macro copies (§9.7), and the
  key of a collected array in the copy of ARGUMENTS that `.seen` makes (§13.2, *The test macros*).
- Under `zerocopy`, the keys and strings made from an expanded `.emit` VALUE and whatever depends
  on them after the macro, such as whether a later key finds their entry, and documents that
  include a file with entries or are given an input as a file, on which libucl crashes (§12.2).
  The project's result with `zerocopy` is the one this specification gives without it.
- A glob pattern that ends in `/` and a symbolic link to a regular file, and a plain path that ends
  in `/` after the name of a file: this depends on the operating system (§9.4, *Globs*).

## Known gaps

All findings of the `spec-v1` review (`docs/clean-room/reviews/spec-v1.md`) are fixed. `spec-v2`
answers the implementer questions in `docs/clean-room/QUESTIONS.md` #1–#4, `spec-v3` answers
#5–#16, `spec-v4` answers #17–#22 and completes §9 for the macro work, `spec-v5` answers
#23–#33, `spec-v6` answers #34–#49 and completes §10 for the output work: every case that
parses now has libucl's output in each format, §10.8 says what reads back exactly, and §10.10
specifies the config output with saved comments. `spec-v7` answers #50–#54, `spec-v8`
answers #55, `spec-v9` records the decision on #56, `spec-v10` answers #57–#58 and adds
§13 for several inputs and registered macros, `spec-v11` answers #59 (the rules where inputs
join, §13.1), `spec-v12` answers #60–#69, found by the differential fuzzer, and `spec-v13`
answers #70–#78, found by the fuzzer after `spec-v12`. `spec-v14` answers #79, found during
C14's differential fuzzing (§12.2 and §13.2). `spec-v15` answers #80 with a §9.4 exception.
`spec-v16` answers #81 by narrowing the existing §12.5 uncertainty.
`spec-v17` answers #82 by clarifying the §9.4 outcome for a file that cannot be opened for reading.
`spec-v18` answers #83 by saying which value a comment attaches to after the `}` that closes the
first name of a braced included file (§12.5, §9.4).
`spec-v19` answers #84 by extending the §12.2 uncertainty under `zerocopy` to what depends on the
changed keys and strings, and to included files.
A draft for #86 to #88 extends the §9.4 first-directory quirk to a later `.load(try=true)` that
skips its file, and says how an included file's brace is taken over again before its first key
and how the first name closes in a section object (§9.4, §12.5); it awaits independent review
for `spec-v20`.

The cases of `spec-v13`, `spec-v15` and `spec-v18` are all in `cases/spec/`; the three
`spec-v15` cases moved there when the crate followed §9.4 in C14, and the six `spec-v18` cases
when it followed §12.5.

Golden files that depend on the platform's C library are recorded per platform for the drift
check (`tests/conformance/README.md`, *Golden files per platform*). The files next to each case
come from the oracle platform and are the expectation on every platform.

The places where §9 and §10 left the project's behaviour open (the include parameters `sign`,
`url` and `path`, how the API reports a silent stop, parsing a file under `no-filevars`, whether
saved comments are written, and serde's JSON forms) are decided; see *Divergences decided by the
project*.

The places where §13 left the project's API open (which quirks of several inputs it keeps, how a
registered handler reports failure, what handlers can do) are decided; see *Divergences decided by
the project*.

Two §13 rules have no case: a `{` after a zero-byte first input crashes libucl, and the file
variables that an input given as a file sets would put the checkout path into the golden file.
The undefined results under `zerocopy` (§12.2) have no golden, because a golden file records
libucl's result with the case's flags, which is undefined there, while the project's expected
result is its result without `zerocopy`: the expanded `.emit` text, what depends on it, and
documents with included files or inputs given as files, on which libucl crashes. Their stable
boundaries have cases: `zerocopy_registered_macros_stable`,
`zerocopy_emit_repeated_key_in_text_error`, `zerocopy_include_empty_file`,
`zerocopy_include_comment_only_file` and `zerocopy_chunk_input`.
The §12.5 clarification for #81 has no golden because the replaced comment's later
appearance is uncertain in libucl.
The §9.4 access-failure rule for #82 has no portable golden because read access depends on
the checkout's permissions and the identity running the test.

Two rules are stated but have no committed case, because their golden files cannot be committed:

- B-10: a `var:` entry for `FILENAME` or `CURDIR` has no effect when the document comes from a
  file (§7.1). The golden file would contain the checkout path.
- B-20: 1023 nested arrays or objects inside the root parse (§11.2). The golden file is deeper
  than the conformance runner's JSON reader accepts.

## Coverage

Every case in `tests/conformance/` and the section(s) that explain it: 1686 cases in `cases/` and
`libucl/`, plus eighteen pending cases of the `spec-v20` draft.

- Cases under `cases/spec/NN-topic/` belong to section NN.
- Every case that parses also has output golden files (§10); the table lists §10 only for the
  cases in `cases/spec/10-output/` and the upstream cases.
- The mappings for `cases/review/`, `cases/additions/`, `cases/errors/` and `libucl/basic/` were
  assigned by hand. Every upstream case is also mapped to §10, because its `.res` file is
  config output.
- `cases/migrated/` holds inputs lifted from the crate's earlier test suites. Their sections were
  assigned from the features they use: comments (§2), quoted or heredoc strings (§6), variables
  (§7), macros (§9), repeated keys or priorities in the expected result (§8), numbers (§5),
  keywords or unquoted strings (§4), sections or quoted keys (§3), brackets (§1), rejection (§11)
  and flags (§12).
| Case | Section(s) |
| --- | --- |
| `cases/additions/a01_plus_concat` | §6 |
| `cases/additions/a02_bare_float_inside` | §4, §5 |
| `cases/additions/a03_two_strings` | §1, §6 |
| `cases/additions/a04_unbraced_var` | §7 |
| `cases/additions/a05_braced_var` | §7 |
| `cases/additions/a06_bare_var` | §7 |
| `cases/additions/a07_value_trailing_comment` | §2, §4 |
| `cases/additions/a08_array_no_commas` | §1, §4 |
| `cases/additions/a09_array_objs_no_comma` | §1 |
| `cases/additions/a10_numeric_key` | §3 |
| `cases/additions/a11_dash_key` | §3 |
| `cases/additions/a12_bool_prefix` | §4 |
| `cases/additions/a13_case_bool` | §4 |
| `cases/additions/a14_float_kb` | §5 |
| `cases/additions/a15_hex_suffix` | §5 |
| `cases/additions/a16_plus_num` | §5 |
| `cases/additions/a17_neg_inf` | §4 |
| `cases/additions/a18_empty_value` | §1 |
| `cases/additions/a19_key_only` | §1 |
| `cases/additions/a20_raw_newline_dq` | §6 |
| `cases/additions/a21_space_before_suffix` | §5 |
| `cases/additions/a22_time_h` | §5 |
| `cases/additions/a23_time_w` | §5 |
| `cases/additions/a24_int_float_exp_suffix` | §5 |
| `cases/additions/a25_top_multi_brace` | §1 |
| `cases/additions/a26_implicit_after_brace` | §1 |
| `cases/additions/a27_nested_dup_in_braces` | §3, §8 |
| `cases/additions/a28_value_semicolon_space` | §4 |
| `cases/additions/a29_single_quote_cont` | §6 |
| `cases/additions/a30_heredoc_empty` | §6 |
| `cases/additions/a31_heredoc_crlf` | §6 |
| `cases/additions/a32_key_colon_nospace` | §1 |
| `cases/additions/a33_url_in_array` | §1, §4 |
| `cases/additions/a34_string_escape_slash` | §6 |
| `cases/additions/a35_json_trailing_comma` | §1 |
| `cases/additions/a36_bare_dollar` | §4, §7 |
| `cases/additions/a37_leading_dot_value` | §5 |
| `cases/additions/a38_trailing_dot_num` | §5 |
| `cases/additions/a39_leading_zero` | §5 |
| `cases/additions/a40_underscore_num` | §5 |
| `cases/additions/a41_nested_obj_semicolons` | §1 |
| `cases/additions/a42_key_quoted_single` | §3 |
| `cases/additions/a43_comment_in_value` | §2 |
| `cases/additions/a44_hash_in_value` | §2, §4 |
| `cases/additions/a45_eq_in_value` | §4 |
| `cases/additions/a46_colon_in_value` | §4 |
| `cases/additions/a47_brace_in_value` | §4 |
| `cases/additions/a48_ms_uppercase` | §5 |
| `cases/additions/a49_min_upper` | §5 |
| `cases/additions/a50_b_suffix` | §5 |
| `cases/additions/v01_vars` | §7 |
| `cases/additions/v02_explicit_then_scalar` | §8 |
| `cases/additions/v03_prefix` | §7 |
| `cases/errors/e01_unterminated_string_first` | §6, §11 |
| `cases/errors/e02_invalid_escape_first` | §3, §11 |
| `cases/errors/e03_lone_backslash` | §3, §11 |
| `cases/errors/e04_nul_byte_first` | §3, §11 |
| `cases/errors/e05_unknown_macro` | §9, §11 |
| `cases/errors/e06_unknown_macro_with_args` | §9, §11 |
| `cases/errors/e07_unterminated_array` | §1, §11 |
| `cases/errors/e08_unterminated_object` | §1, §11 |
| `cases/errors/e09_short_unicode_escape` | §6, §11 |
| `cases/errors/e10_unterminated_comment` | §2, §11 |
| `cases/migrated/bare_word_tests__bare_word_edge_cases` | §4, §5, §6 |
| `cases/migrated/bare_word_tests__bare_word_error_suggestions` | §1 |
| `cases/migrated/bare_word_tests__bare_word_error_suggestions_2` | §2 |
| `cases/migrated/bare_word_tests__bare_word_error_suggestions_3` | §1 |
| `cases/migrated/bare_word_tests__bare_word_validation_errors` | §1, §11 |
| `cases/migrated/bare_word_tests__bare_word_validation_errors_2` | §2 |
| `cases/migrated/bare_word_tests__bare_word_validation_errors_3` | §1, §11 |
| `cases/migrated/bare_word_tests__bare_word_validation_errors_4` | §1, §11 |
| `cases/migrated/bare_word_tests__bare_word_validation_errors_5` | §1 |
| `cases/migrated/bare_word_tests__bare_word_validation_errors_6` | §1, §11 |
| `cases/migrated/bare_word_tests__bare_word_validation_errors_7` | §1 |
| `cases/migrated/bare_word_tests__bare_word_validation_errors_8` | §1 |
| `cases/migrated/bare_word_tests__bare_word_vs_quoted_string_distinction` | §4, §5, §6 |
| `cases/migrated/bare_word_tests__bare_word_with_numbers` | §4, §5 |
| `cases/migrated/bare_word_tests__bare_words_in_different_contexts` | §1, §3, §4, §5, §6, §7 |
| `cases/migrated/bare_word_tests__bare_words_with_unicode` | §4 |
| `cases/migrated/bare_word_tests__boolean_keyword_conversion` | §2, §4 |
| `cases/migrated/bare_word_tests__null_keyword_conversion` | §4 |
| `cases/migrated/bare_word_tests__reserved_keyword_handling` | §2, §4, §6 |
| `cases/migrated/bare_word_tests__special_float_values` | §4, §5 |
| `cases/migrated/bare_word_tests__unquoted_string_values` | §4 |
| `cases/migrated/c_libucl_behavior_validation__behavior_validation_tests__test_comment_format_compatibility` | §2, §6 |
| `cases/migrated/c_libucl_behavior_validation__behavior_validation_tests__test_comment_format_compatibility_2` | §6 |
| `cases/migrated/c_libucl_behavior_validation__behavior_validation_tests__test_comment_format_compatibility_3` | §2, §6 |
| `cases/migrated/c_libucl_behavior_validation__behavior_validation_tests__test_comment_format_compatibility_4` | §2, §6 |
| `cases/migrated/c_libucl_behavior_validation__behavior_validation_tests__test_comprehensive_compatibility_report` | §1, §2, §3, §4, §5, §6, §7, §11 |
| `cases/migrated/c_libucl_behavior_validation__behavior_validation_tests__test_number_format_compatibility` | §5 |
| `cases/migrated/c_libucl_behavior_validation__behavior_validation_tests__test_number_format_compatibility_2` | §5 |
| `cases/migrated/c_libucl_behavior_validation__behavior_validation_tests__test_number_format_compatibility_3` | §5 |
| `cases/migrated/c_libucl_behavior_validation__behavior_validation_tests__test_number_format_compatibility_4` | §5 |
| `cases/migrated/c_libucl_behavior_validation__behavior_validation_tests__test_number_format_compatibility_5` | §5 |
| `cases/migrated/c_libucl_behavior_validation__behavior_validation_tests__test_number_format_compatibility_6` | §5 |
| `cases/migrated/c_libucl_behavior_validation__behavior_validation_tests__test_number_format_compatibility_7` | §5 |
| `cases/migrated/c_libucl_behavior_validation__behavior_validation_tests__test_number_format_compatibility_8` | §5 |
| `cases/migrated/c_libucl_behavior_validation__behavior_validation_tests__test_number_format_compatibility_9` | §5 |
| `cases/migrated/c_libucl_behavior_validation__behavior_validation_tests__test_real_world_config_validation` | §1, §5 |
| `cases/migrated/c_libucl_behavior_validation__behavior_validation_tests__test_unicode_compatibility_validation` | §1, §6, §11 |
| `cases/migrated/c_libucl_behavior_validation__behavior_validation_tests__test_unicode_compatibility_validation_2` | §6 |
| `cases/migrated/c_libucl_behavior_validation__behavior_validation_tests__test_unicode_compatibility_validation_3` | §1, §6, §11 |
| `cases/migrated/c_libucl_compatibility__array_formats_compatibility` | §1, §4, §5, §6 |
| `cases/migrated/c_libucl_compatibility__bare_word_values` | §2, §4, §5 |
| `cases/migrated/c_libucl_compatibility__basic_ucl_syntax_compatibility` | §1, §2, §4, §5, §6 |
| `cases/migrated/c_libucl_compatibility__comment_formats_compatibility` | §2, §6 |
| `cases/migrated/c_libucl_compatibility__cpp_style_comments` | §2, §6, §8 |
| `cases/migrated/c_libucl_compatibility__duplicate_key_handling` | §1, §6, §8 |
| `cases/migrated/c_libucl_compatibility__error_handling_compatibility` | §1, §11 |
| `cases/migrated/c_libucl_compatibility__error_handling_compatibility_2` | §5 |
| `cases/migrated/c_libucl_compatibility__error_handling_compatibility_3` | §6, §11 |
| `cases/migrated/c_libucl_compatibility__error_handling_compatibility_4` | §1, §6, §11 |
| `cases/migrated/c_libucl_compatibility__error_handling_compatibility_5` | §6, §11 |
| `cases/migrated/c_libucl_compatibility__freebsd_pkg_style_configuration` | §1, §3, §6 |
| `cases/migrated/c_libucl_compatibility__nginx_style_configuration` | §1, §4, §5, §6 |
| `cases/migrated/c_libucl_compatibility__number_formats_compatibility` | §2, §5 |
| `cases/migrated/c_libucl_compatibility__real_world_nginx_config` | §1, §3, §4, §5, §6, §7, §8 |
| `cases/migrated/c_libucl_compatibility__string_formats_compatibility` | §6 |
| `cases/migrated/c_libucl_compatibility__unicode_escape_sequences` | §1, §6, §11 |
| `cases/migrated/c_libucl_compatibility_report__run_full_compatibility_report` | §6, §8 |
| `cases/migrated/c_libucl_compatibility_report__run_full_compatibility_report_10` | §2, §6 |
| `cases/migrated/c_libucl_compatibility_report__run_full_compatibility_report_11` | §1, §4, §5, §6 |
| `cases/migrated/c_libucl_compatibility_report__run_full_compatibility_report_2` | §6, §8 |
| `cases/migrated/c_libucl_compatibility_report__run_full_compatibility_report_3` | §1, §6, §11 |
| `cases/migrated/c_libucl_compatibility_report__run_full_compatibility_report_4` | §1, §3, §5 |
| `cases/migrated/c_libucl_compatibility_report__run_full_compatibility_report_5` | §1, §4, §5, §6 |
| `cases/migrated/c_libucl_compatibility_report__run_full_compatibility_report_6` | §6 |
| `cases/migrated/c_libucl_compatibility_report__run_full_compatibility_report_7` | §4, §5 |
| `cases/migrated/c_libucl_compatibility_report__run_full_compatibility_report_8` | §5 |
| `cases/migrated/c_libucl_compatibility_report__run_full_compatibility_report_9` | §6, §11 |
| `cases/migrated/compatibility_tests__bare_word_in_arrays` | §1, §4, §5, §6 |
| `cases/migrated/compatibility_tests__bare_word_validation_errors` | §1, §11 |
| `cases/migrated/compatibility_tests__bare_word_validation_errors_2` | §1 |
| `cases/migrated/compatibility_tests__bare_word_validation_errors_3` | §1 |
| `cases/migrated/compatibility_tests__bare_word_validation_errors_4` | §6 |
| `cases/migrated/compatibility_tests__bare_word_validation_errors_5` | §2 |
| `cases/migrated/compatibility_tests__bare_word_validation_errors_6` | §1 |
| `cases/migrated/compatibility_tests__bare_word_validation_errors_7` | §1 |
| `cases/migrated/compatibility_tests__bare_word_validation_errors_8` | §1, §11 |
| `cases/migrated/compatibility_tests__bare_word_values` | §2, §4, §5, §6 |
| `cases/migrated/compatibility_tests__debug_simple_bare_word` | §1 |
| `cases/migrated/compatibility_tests__debug_simple_bare_word_2` | §1 |
| `cases/migrated/compatibility_tests__error_handling_backward_compatibility` | §1, §6 |
| `cases/migrated/compatibility_tests__error_handling_backward_compatibility_2` | §1, §11 |
| `cases/migrated/compatibility_tests__error_handling_backward_compatibility_3` | §1, §6, §11 |
| `cases/migrated/compatibility_tests__error_handling_backward_compatibility_4` | §1, §6 |
| `cases/migrated/compatibility_tests__freebsd_ucl_compatibility` | §1, §2, §3, §5, §6 |
| `cases/migrated/compatibility_tests__heredoc_enhanced_error_messages` | §6, §11 |
| `cases/migrated/compatibility_tests__heredoc_enhanced_error_messages_2` | §1, §11 |
| `cases/migrated/compatibility_tests__heredoc_terminator_improvements` | §1, §11 |
| `cases/migrated/compatibility_tests__heredoc_terminator_improvements_2` | §6, §11 |
| `cases/migrated/compatibility_tests__heredoc_terminator_improvements_3` | §6, §11 |
| `cases/migrated/compatibility_tests__heredoc_terminator_improvements_4` | §6, §11 |
| `cases/migrated/compatibility_tests__libucl_basic_compatibility` | §1, §2, §3, §4, §5, §6, §8 |
| `cases/migrated/compatibility_tests__mixed_syntax_styles_compatibility` | §1, §2, §6 |
| `cases/migrated/compatibility_tests__nginx_style_backward_compatibility` | §1, §3, §5, §6 |
| `cases/migrated/compatibility_tests__nginx_ucl_compatibility` | §1, §2, §3, §4, §5, §6, §7, §8 |
| `cases/migrated/compatibility_tests__rspamd_ucl_compatibility` | §1, §2, §3, §4, §5, §6, §7, §9, §11 |
| `cases/migrated/compatibility_tests__ucl_array_formats_compatibility` | §1, §2, §3, §4, §5, §6, §8 |
| `cases/migrated/compatibility_tests__ucl_comments_compatibility` | §1, §2, §6 |
| `cases/migrated/compatibility_tests__ucl_number_formats_compatibility` | §1, §2, §3, §4, §5 |
| `cases/migrated/compatibility_tests__ucl_string_formats_compatibility` | §1, §2, §3, §6, §7, §11 |
| `cases/migrated/heredoc_tests__heredoc_empty_content` | §6, §11 |
| `cases/migrated/heredoc_tests__heredoc_error_messages` | §6, §11 |
| `cases/migrated/heredoc_tests__heredoc_error_messages_2` | §1 |
| `cases/migrated/heredoc_tests__heredoc_in_nested_structures` | §1, §3, §6 |
| `cases/migrated/heredoc_tests__heredoc_partial_terminator_matches` | §6 |
| `cases/migrated/heredoc_tests__heredoc_preserves_internal_whitespace` | §6 |
| `cases/migrated/heredoc_tests__heredoc_terminator_case_sensitivity` | §6 |
| `cases/migrated/heredoc_tests__heredoc_unterminated_error` | §6, §11 |
| `cases/migrated/heredoc_tests__heredoc_with_both_leading_and_trailing_whitespace` | §6 |
| `cases/migrated/heredoc_tests__heredoc_with_crlf_line_endings` | §1, §11 |
| `cases/migrated/heredoc_tests__heredoc_with_custom_terminators` | §2, §4, §6 |
| `cases/migrated/heredoc_tests__heredoc_with_leading_whitespace_terminator` | §6 |
| `cases/migrated/heredoc_tests__heredoc_with_lf_line_endings` | §6 |
| `cases/migrated/heredoc_tests__heredoc_with_mixed_line_endings` | §1, §11 |
| `cases/migrated/heredoc_tests__heredoc_with_only_whitespace_lines` | §6 |
| `cases/migrated/heredoc_tests__heredoc_with_special_characters_in_terminator` | §1, §11 |
| `cases/migrated/heredoc_tests__heredoc_with_trailing_whitespace_terminator` | §6 |
| `cases/migrated/heredoc_tests__multiple_heredocs_in_same_config` | §6 |
| `cases/migrated/implicit_array_tests__array_extension_behavior` | §1, §6, §8 |
| `cases/migrated/implicit_array_tests__automatic_array_creation_from_repeated_keys` | §5, §6, §8 |
| `cases/migrated/implicit_array_tests__complex_implicit_array_scenario` | §1, §2, §3, §4, §5, §7, §8 |
| `cases/migrated/implicit_array_tests__duplicate_key_error_when_disabled` | §6, §8 |
| `cases/migrated/implicit_array_tests__explicit_vs_implicit_arrays` | §1, §2, §4, §6, §8 |
| `cases/migrated/implicit_array_tests__implicit_array_edge_cases` | §2, §4, §6, §8 |
| `cases/migrated/implicit_array_tests__implicit_array_ordering` | §6, §8 |
| `cases/migrated/implicit_array_tests__implicit_arrays_in_nested_objects` | §1, §5, §6, §8 |
| `cases/migrated/implicit_array_tests__implicit_arrays_with_comments` | §2, §6, §8 |
| `cases/migrated/implicit_array_tests__implicit_arrays_with_nginx_syntax` | §1, §3, §4, §5, §8 |
| `cases/migrated/implicit_array_tests__implicit_arrays_with_objects` | §1, §3, §5, §6, §8 |
| `cases/migrated/implicit_array_tests__mixed_type_arrays` | §4, §5, §6, §8 |
| `cases/migrated/implicit_array_tests__single_to_array_conversion` | §5, §6, §8 |
| `cases/migrated/integration_tests__compatibility_with_existing_ucl_files` | §1, §2, §3, §5, §6, §7, §9, §11 |
| `cases/migrated/integration_tests__compatibility_with_existing_ucl_files_2` | §1, §3, §5, §6 |
| `cases/migrated/integration_tests__cpp_comments_lexing` | §6, §8 |
| `cases/migrated/integration_tests__cpp_comments_mixed_with_other_comments` | §2, §6, §8 |
| `cases/migrated/integration_tests__cpp_comments_preservation` | §1, §3, §6, §11 |
| `cases/migrated/integration_tests__performance_with_deeply_nested_structures` | §1, §3, §6 |
| `cases/migrated/integration_tests__real_world_ci_cd_pipeline_config` | §1, §3, §5, §6 |
| `cases/migrated/integration_tests__real_world_microservices_config` | §1, §2, §3, §4, §5, §6, §7 |
| `cases/migrated/integration_tests__real_world_monitoring_config` | §1, §2, §3, §4, §5, §6, §7 |
| `cases/migrated/libucl_compatibility_tests__libucl_basic_10_named_sections` | §1, §3, §6, §8 |
| `cases/migrated/libucl_compatibility_tests__libucl_basic_1_invalid_hex` | §6 |
| `cases/migrated/libucl_compatibility_tests__libucl_basic_1_invalid_hex_2` | §6 |
| `cases/migrated/libucl_compatibility_tests__libucl_basic_1_value_types` | §1, §3, §4, §5, §6, §8 |
| `cases/migrated/libucl_compatibility_tests__libucl_basic_2_escape_sequences` | §6 |
| `cases/migrated/libucl_compatibility_tests__libucl_basic_2_nested_sections` | §1, §3, §4, §5, §6 |
| `cases/migrated/libucl_compatibility_tests__libucl_basic_2_number_suffixes` | §5 |
| `cases/migrated/libucl_compatibility_tests__libucl_basic_2_path_strings` | §3, §4, §5, §6 |
| `cases/migrated/libucl_compatibility_tests__libucl_basic_2_variable_expansion` | §1, §6, §7, §8 |
| `cases/migrated/libucl_compatibility_tests__libucl_basic_3_duplicate_keys_in_object` | §1, §5, §6, §8 |
| `cases/migrated/libucl_compatibility_tests__libucl_basic_3_pkg_config` | §1, §2, §3, §6, §7 |
| `cases/migrated/libucl_compatibility_tests__libucl_basic_4_heredoc_strings` | §2, §6 |
| `cases/migrated/libucl_compatibility_tests__libucl_basic_4_package_manifest` | §1, §3, §4, §5, §6 |
| `cases/migrated/libucl_compatibility_tests__libucl_mixed_array_types` | §1, §4, §5, §6 |
| `cases/migrated/libucl_compatibility_tests__libucl_multiline_comments_nested` | §2, §4 |
| `cases/migrated/nginx_syntax_tests__bare_word_value_assignment` | §1 |
| `cases/migrated/nginx_syntax_tests__complex_nginx_config` | §1, §3, §4, §5, §7, §8 |
| `cases/migrated/nginx_syntax_tests__implicit_object_creation` | §1, §5, §6 |
| `cases/migrated/nginx_syntax_tests__implicit_object_with_explicit_separators` | §1, §5, §6 |
| `cases/migrated/nginx_syntax_tests__mixed_syntax_styles` | §1, §2, §3, §5, §6, §7 |
| `cases/migrated/nginx_syntax_tests__nested_implicit_objects` | §1, §3, §4, §5 |
| `cases/migrated/nginx_syntax_tests__nested_object_creation` | §1, §3, §5, §6 |
| `cases/migrated/nginx_syntax_tests__nginx_syntax_error_handling` | §1, §11 |
| `cases/migrated/nginx_syntax_tests__nginx_syntax_error_handling_2` | §1, §3, §11 |
| `cases/migrated/real_world_ucl_configs__real_world_configs__test_dovecot_ucl_config` | §1, §2, §3, §4, §5, §6, §7, §8 |
| `cases/migrated/real_world_ucl_configs__real_world_configs__test_freebsd_pkg_real_config` | §1, §2, §3, §4, §5, §6 |
| `cases/migrated/real_world_ucl_configs__real_world_configs__test_haproxy_ucl_config` | §1, §2, §3, §4, §5, §6, §8 |
| `cases/migrated/real_world_ucl_configs__real_world_configs__test_nginx_real_config` | §1, §3, §4, §5, §6, §7, §11 |
| `cases/migrated/real_world_ucl_configs__real_world_configs__test_rspamd_real_config` | §1, §2, §3, §4, §5, §6, §7, §8 |
| `cases/migrated/unicode_escape_tests__emoji_and_extended_unicode` | §1, §2, §3, §6, §11 |
| `cases/migrated/unicode_escape_tests__mixed_unicode_escape_formats` | §1, §6, §11 |
| `cases/migrated/unicode_escape_tests__unicode_boundary_values` | §1, §6, §11 |
| `cases/migrated/unicode_escape_tests__unicode_case_sensitivity` | §1, §6, §11 |
| `cases/migrated/unicode_escape_tests__unicode_escape_error_handling` | §1, §6, §11 |
| `cases/migrated/unicode_escape_tests__unicode_escape_error_handling_2` | §1, §6, §11 |
| `cases/migrated/unicode_escape_tests__unicode_escape_error_handling_3` | §1, §6, §11 |
| `cases/migrated/unicode_escape_tests__unicode_escape_error_handling_4` | §1, §6, §11 |
| `cases/migrated/unicode_escape_tests__unicode_escape_error_handling_5` | §1, §6, §11 |
| `cases/migrated/unicode_escape_tests__unicode_escape_error_handling_6` | §1, §6, §11 |
| `cases/migrated/unicode_escape_tests__unicode_escapes_in_arrays` | §1, §3, §6, §11 |
| `cases/migrated/unicode_escape_tests__unicode_escapes_in_keys` | §1, §3, §6, §11 |
| `cases/migrated/unicode_escape_tests__unicode_escapes_with_other_escapes` | §1, §6, §11 |
| `cases/migrated/unicode_escape_tests__unicode_in_multiline_strings` | §1, §6, §11 |
| `cases/migrated/unicode_escape_tests__unicode_normalization` | §1, §2, §6, §11 |
| `cases/migrated/unicode_escape_tests__variable_length_unicode_escapes` | §1, §6, §11 |
| `cases/review/01_url` | §4 |
| `cases/review/02_brackets` | §4 |
| `cases/review/03_numword` | §4, §5 |
| `cases/review/04_percent` | §4 |
| `cases/review/05_backslash` | §4 |
| `cases/review/06_ms` | §5 |
| `cases/review/07_hexdot` | §5 |
| `cases/review/08_hexbad` | §5 |
| `cases/review/09_twonames` | §3 |
| `cases/review/10_dupnested` | §3, §8 |
| `cases/review/11_slashslash` | §2, §3 |
| `cases/review/12_0b` | §5 |
| `cases/review/13_0o` | §5 |
| `cases/review/14_heredoc` | §6 |
| `cases/review/15_dollardollar` | §7 |
| `cases/review/16_dollarplain` | §7 |
| `cases/review/17_multikey` | §8 |
| `cases/review/18_emptykey` | §3, §11 |
| `cases/review/19_utf8key` | §3 |
| `cases/review/20_arraynested` | §1 |
| `cases/review/21_objinarray` | §1 |
| `cases/review/22_colonobj` | §1, §3 |
| `cases/review/23_semicolon_in_str` | §6 |
| `cases/review/24_nosep_obj` | §1 |
| `cases/review/25_neg_time` | §5 |
| `cases/review/26_bigint` | §5 |
| `cases/review/27_overflow` | §5, §11 |
| `cases/review/28_true_bare` | §4 |
| `cases/review/29_dot_key` | §3 |
| `cases/review/30_inline_comment` | §2 |
| `cases/spec/01-structure/array_atoms_with_spaces` | §1 |
| `cases/spec/01-structure/array_basic` | §1 |
| `cases/spec/01-structure/array_comment_group_spaced_comments_error` | §1 |
| `cases/spec/01-structure/array_comment_group_then_newline_error` | §1 |
| `cases/spec/01-structure/array_comment_group_then_space_close_error` | §1 |
| `cases/spec/01-structure/array_comment_inside` | §1 |
| `cases/spec/01-structure/array_containers_need_no_separator` | §1 |
| `cases/spec/01-structure/array_double_comma` | §1 |
| `cases/spec/01-structure/array_empty_forms` | §1 |
| `cases/spec/01-structure/array_ff_then_block_comment_element` | §1, §2 |
| `cases/spec/01-structure/array_ff_then_block_comment_newline_close_error` | §1, §2 |
| `cases/spec/01-structure/array_first_element_after_comment_group` | §1 |
| `cases/spec/01-structure/array_key_without_separator` | §1 |
| `cases/spec/01-structure/array_leading_comma_error` | §1 |
| `cases/spec/01-structure/array_nested` | §1 |
| `cases/spec/01-structure/array_newline_block_comment_newline_close` | §1, §2 |
| `cases/spec/01-structure/array_newline_separated` | §1 |
| `cases/spec/01-structure/array_of_objects_trailing_comma` | §1 |
| `cases/spec/01-structure/array_semicolon_separated` | §1 |
| `cases/spec/01-structure/array_space_separated_numbers` | §1 |
| `cases/spec/01-structure/array_strings_need_separator_error` | §1 |
| `cases/spec/01-structure/array_trailing_comma` | §1 |
| `cases/spec/01-structure/array_vt_then_block_comment_newline_close_error` | §1, §2 |
| `cases/spec/01-structure/array_vt_then_block_comment_starts_element` | §1, §2 |
| `cases/spec/01-structure/array_vt_then_close` | §1, §2 |
| `cases/spec/01-structure/array_vt_then_line_comment_close` | §1, §2 |
| `cases/spec/01-structure/array_vt_then_line_comment_starts_element` | §1, §2 |
| `cases/spec/01-structure/close_without_open_error` | §1 |
| `cases/spec/01-structure/comment_only_document` | §1 |
| `cases/spec/01-structure/empty_document` | §1 |
| `cases/spec/01-structure/empty_value_at_end_is_null` | §1 |
| `cases/spec/01-structure/empty_value_block_comment_at_end_error` | §1 |
| `cases/spec/01-structure/empty_value_block_comment_then_newline` | §1 |
| `cases/spec/01-structure/empty_value_block_comment_then_newline_at_end` | §1 |
| `cases/spec/01-structure/empty_value_line_comment_at_end_error` | §1 |
| `cases/spec/01-structure/empty_value_next_line_block_comment_then_newline_error` | §1 |
| `cases/spec/01-structure/empty_value_next_line_block_comment_then_space` | §1 |
| `cases/spec/01-structure/empty_value_next_line_comment_at_end` | §1 |
| `cases/spec/01-structure/empty_value_next_line_comment_then_spaces` | §1 |
| `cases/spec/01-structure/empty_value_skips_blank_lines` | §1 |
| `cases/spec/01-structure/empty_value_space_at_end_error` | §1 |
| `cases/spec/01-structure/empty_value_takes_next_line` | §1 |
| `cases/spec/01-structure/key_block_comment_newline_value_next_line` | §1 |
| `cases/spec/01-structure/key_equals_eof_error` | §1 |
| `cases/spec/01-structure/key_equals_semicolon_error` | §1 |
| `cases/spec/01-structure/key_form_feed_is_line_break` | §1 |
| `cases/spec/01-structure/key_line_comment_at_end_error` | §1 |
| `cases/spec/01-structure/key_line_comment_then_separator` | §1 |
| `cases/spec/01-structure/key_line_comment_value_next_line` | §1 |
| `cases/spec/01-structure/key_semicolon_error` | §1 |
| `cases/spec/01-structure/key_space_at_end_error` | §1 |
| `cases/spec/01-structure/key_space_newline_at_end_is_null` | §1 |
| `cases/spec/01-structure/key_space_newline_block_comment_newline_error` | §1 |
| `cases/spec/01-structure/key_space_newline_comment_then_spaces` | §1 |
| `cases/spec/01-structure/key_space_newline_object` | §1 |
| `cases/spec/01-structure/key_space_newline_separator_is_value` | §1 |
| `cases/spec/01-structure/key_space_newline_value_next_line` | §1 |
| `cases/spec/01-structure/key_vertical_tab_is_line_break` | §1 |
| `cases/spec/01-structure/key_without_value_error` | §1 |
| `cases/spec/01-structure/mismatched_close_array_error` | §1 |
| `cases/spec/01-structure/mismatched_close_object_error` | §1 |
| `cases/spec/01-structure/missing_delimiter_after_string_error` | §1 |
| `cases/spec/01-structure/missing_delimiter_string_adjacent_error` | §1 |
| `cases/spec/01-structure/nested_objects` | §1 |
| `cases/spec/01-structure/object_colon_brace` | §1 |
| `cases/spec/01-structure/object_empty_forms` | §1 |
| `cases/spec/01-structure/object_equals_brace` | §1 |
| `cases/spec/01-structure/object_leading_comma_error` | §1 |
| `cases/spec/01-structure/object_leading_semicolon_error` | §1 |
| `cases/spec/01-structure/object_on_next_line_after_equals` | §1 |
| `cases/spec/01-structure/object_trailing_separators` | §1 |
| `cases/spec/01-structure/object_value_needs_no_delimiter` | §1 |
| `cases/spec/01-structure/quoted_value_then_vertical_tab_error` | §1 |
| `cases/spec/01-structure/root_bracket_after_block_comment_and_newline_error` | §1 |
| `cases/spec/01-structure/root_bracket_after_comment_and_blank_line_error` | §1 |
| `cases/spec/01-structure/root_bracket_after_comment_and_space_error` | §1 |
| `cases/spec/01-structure/root_bracket_after_leading_whitespace` | §1 |
| `cases/spec/01-structure/root_bracket_after_line_comment_crlf` | §1 |
| `cases/spec/01-structure/root_bracket_after_whitespace_and_comment_error` | §1 |
| `cases/spec/01-structure/root_bracket_between_spaced_comments_error` | §1 |
| `cases/spec/01-structure/root_bracket_directly_after_comment_group` | §1 |
| `cases/spec/01-structure/root_leading_comma_error` | §1 |
| `cases/spec/01-structure/root_leading_semicolon_error` | §1 |
| `cases/spec/01-structure/root_leading_terminator_after_newline_error` | §1 |
| `cases/spec/01-structure/root_unbraced_after_comment_and_blank_line` | §1 |
| `cases/spec/01-structure/sep_colon` | §1 |
| `cases/spec/01-structure/sep_colon_equals_error` | §1 |
| `cases/spec/01-structure/sep_double_equals_error` | §1 |
| `cases/spec/01-structure/sep_equals` | §1 |
| `cases/spec/01-structure/sep_equals_colon_error` | §1 |
| `cases/spec/01-structure/sep_no_spaces` | §1 |
| `cases/spec/01-structure/sep_none` | §1 |
| `cases/spec/01-structure/sep_space_before_only` | §1 |
| `cases/spec/01-structure/separator_vertical_tab_is_line_break` | §1 |
| `cases/spec/01-structure/term_comma` | §1 |
| `cases/spec/01-structure/term_crlf` | §1 |
| `cases/spec/01-structure/term_newline_then_semicolon` | §1 |
| `cases/spec/01-structure/term_nul_byte` | §1 |
| `cases/spec/01-structure/term_repeated` | §1 |
| `cases/spec/01-structure/term_semicolon` | §1 |
| `cases/spec/01-structure/top_array` | §1 |
| `cases/spec/01-structure/top_array_trailing_ignored` | §1 |
| `cases/spec/01-structure/top_braced` | §1 |
| `cases/spec/01-structure/top_braced_json` | §1 |
| `cases/spec/01-structure/top_braced_second_object_ignored` | §1 |
| `cases/spec/01-structure/top_braced_trailing_ignored` | §1 |
| `cases/spec/01-structure/top_implicit` | §1 |
| `cases/spec/01-structure/unterminated_array_error` | §1 |
| `cases/spec/01-structure/unterminated_object_error` | §1 |
| `cases/spec/01-structure/unterminated_top_brace_error` | §1 |
| `cases/spec/01-structure/value_spans_rest_of_line` | §1 |
| `cases/spec/01-structure/value_without_key_error` | §1 |
| `cases/spec/01-structure/whitespace_only_document` | §1 |
| `cases/spec/02-comments/after_value_is_separator` | §2 |
| `cases/spec/02-comments/between_key_and_separator` | §2 |
| `cases/spec/02-comments/between_separator_and_value` | §2 |
| `cases/spec/02-comments/block_comment_after_value` | §2 |
| `cases/spec/02-comments/block_comment_escaped_quote_inside_quotes` | §2 |
| `cases/spec/02-comments/block_comment_escaped_quote_outside_quotes` | §2 |
| `cases/spec/02-comments/block_comment_mid_value_error` | §2 |
| `cases/spec/02-comments/block_comment_open_inside_quotes` | §2 |
| `cases/spec/02-comments/block_comment_quote_after_backslash_pair_error` | §2 |
| `cases/spec/02-comments/block_comment_single_quote_not_special` | §2 |
| `cases/spec/02-comments/block_comment_single_quotes_do_not_protect_error` | §2 |
| `cases/spec/02-comments/block_comment_unmatched_quote_error` | §2 |
| `cases/spec/02-comments/carriage_return_terminates` | §2 |
| `cases/spec/02-comments/form_feed_between_entries` | §2 |
| `cases/spec/02-comments/hash_at_end_of_input` | §2 |
| `cases/spec/02-comments/hash_ends_unquoted_value` | §2 |
| `cases/spec/02-comments/hash_inside_quotes` | §2 |
| `cases/spec/02-comments/hash_last_byte_after_comment_and_space_error` | §2 |
| `cases/spec/02-comments/hash_last_byte_after_entry` | §2 |
| `cases/spec/02-comments/hash_last_byte_after_formfeed_after_entry_error` | §2 |
| `cases/spec/02-comments/hash_last_byte_after_formfeed_comment_space_error` | §2 |
| `cases/spec/02-comments/hash_last_byte_after_formfeed_directly_after_comment` | §2 |
| `cases/spec/02-comments/hash_last_byte_after_formfeed_newline_error` | §2 |
| `cases/spec/02-comments/hash_last_byte_after_leading_newline_error` | §2 |
| `cases/spec/02-comments/hash_last_byte_after_leading_space_error` | §2 |
| `cases/spec/02-comments/hash_last_byte_after_macro_comment_then_space_error` | §2, §9 |
| `cases/spec/02-comments/hash_last_byte_after_macro_directly_after_comment` | §2, §9 |
| `cases/spec/02-comments/hash_last_byte_after_macro_error` | §2, §9 |
| `cases/spec/02-comments/hash_last_byte_after_tab_after_entry` | §2 |
| `cases/spec/02-comments/hash_last_byte_after_vt_after_separator_error` | §2 |
| `cases/spec/02-comments/hash_last_byte_at_start` | §2 |
| `cases/spec/02-comments/hash_last_byte_directly_after_comment` | §2 |
| `cases/spec/02-comments/hash_line` | §2 |
| `cases/spec/02-comments/hash_then_newline_after_leading_newline` | §2 |
| `cases/spec/02-comments/hash_then_space_after_leading_newline` | §2 |
| `cases/spec/02-comments/hash_to_end_of_line` | §2 |
| `cases/spec/02-comments/multiline` | §2 |
| `cases/spec/02-comments/nested` | §2 |
| `cases/spec/02-comments/nested_deep` | §2 |
| `cases/spec/02-comments/quoted_close_inside_comment` | §2 |
| `cases/spec/02-comments/slash_slash_is_a_key` | §2 |
| `cases/spec/02-comments/slash_star_not_at_value_start` | §2 |
| `cases/spec/02-comments/tab_and_space` | §2 |
| `cases/spec/02-comments/unterminated_error` | §2 |
| `cases/spec/02-comments/unterminated_nested_error` | §2 |
| `cases/spec/02-comments/utf8_bom_is_part_of_key` | §2 |
| `cases/spec/02-comments/vertical_tab_between_entries` | §2 |
| `cases/spec/02-comments/vertical_tab_is_not_a_separator` | §2 |
| `cases/spec/03-keys/bare_charset` | §3 |
| `cases/spec/03-keys/brace_directly_after_key_error` | §3 |
| `cases/spec/03-keys/bracket_directly_after_key_error` | §3 |
| `cases/spec/03-keys/case_sensitive` | §3 |
| `cases/spec/03-keys/dollar_in_bare_key_error` | §3 |
| `cases/spec/03-keys/dots_are_literal` | §3 |
| `cases/spec/03-keys/invalid_char_in_key_error` | §3 |
| `cases/spec/03-keys/newline_directly_after_key_error` | §3 |
| `cases/spec/03-keys/non_ascii` | §3 |
| `cases/spec/03-keys/quoted` | §3 |
| `cases/spec/03-keys/quoted_brace_directly_after` | §3 |
| `cases/spec/03-keys/quoted_empty_error` | §3 |
| `cases/spec/03-keys/quoted_escapes` | §3 |
| `cases/spec/03-keys/quoted_followed_by_quoted_value_error` | §3 |
| `cases/spec/03-keys/quoted_key_at_end_error` | §3 |
| `cases/spec/03-keys/quoted_key_comment_adjacent` | §3 |
| `cases/spec/03-keys/quoted_key_heredoc_adjacent` | §3 |
| `cases/spec/03-keys/quoted_key_names_adjacent` | §3 |
| `cases/spec/03-keys/quoted_key_space_at_end_error` | §3 |
| `cases/spec/03-keys/quoted_key_value_adjacent` | §3 |
| `cases/spec/03-keys/quoted_newline_before_separator` | §3 |
| `cases/spec/03-keys/quoted_no_variable_expansion` | §3 |
| `cases/spec/03-keys/quoted_value_after_bare_key_error` | §3 |
| `cases/spec/03-keys/section_any_keyword` | §3 |
| `cases/spec/03-keys/section_array_value` | §3 |
| `cases/spec/03-keys/section_bracket_in_block_comment_after_ff_at_end` | §3, §2 |
| `cases/spec/03-keys/section_bracket_in_block_comment_after_ff_last_byte_hash_error` | §3, §2 |
| `cases/spec/03-keys/section_bracket_in_block_comment_after_ff_name_next_line` | §3, §2 |
| `cases/spec/03-keys/section_bracket_in_comment_after_ff_at_end` | §3, §2 |
| `cases/spec/03-keys/section_bracket_in_comment_after_ff_last_byte_hash_error` | §3, §2 |
| `cases/spec/03-keys/section_bracket_in_comment_after_ff_last_byte_hash_saved` | §3, §2 |
| `cases/spec/03-keys/section_bracket_in_comment_after_ff_name_on_later_line` | §3, §2 |
| `cases/spec/03-keys/section_bracket_in_comment_after_ff_separator_key_error` | §3, §2 |
| `cases/spec/03-keys/section_bracket_in_comment_after_ff_then_blank_lines` | §3, §2 |
| `cases/spec/03-keys/section_bracket_in_comment_after_ff_two_names` | §3, §2 |
| `cases/spec/03-keys/section_bracket_in_comment_after_vt_ff_at_end` | §3, §2 |
| `cases/spec/03-keys/section_ff_before_bracket_error` | §3 |
| `cases/spec/03-keys/section_ff_inside_name_error` | §3 |
| `cases/spec/03-keys/section_lookahead_after_block_comment` | §3 |
| `cases/spec/03-keys/section_lookahead_after_line_comment` | §3 |
| `cases/spec/03-keys/section_lookahead_brace_in_value_error` | §3 |
| `cases/spec/03-keys/section_lookahead_brace_later_on_line_error` | §3 |
| `cases/spec/03-keys/section_lookahead_counts_comment_text_error` | §3 |
| `cases/spec/03-keys/section_lookahead_ff_then_line_comment` | §3 |
| `cases/spec/03-keys/section_lookahead_stops_at_newline` | §3 |
| `cases/spec/03-keys/section_lookahead_stops_at_semicolon` | §3 |
| `cases/spec/03-keys/section_mixed_names` | §3 |
| `cases/spec/03-keys/section_name_then_ff_before_bracket_error` | §3 |
| `cases/spec/03-keys/section_names_then_atom_is_value` | §3 |
| `cases/spec/03-keys/section_names_then_separator_is_value` | §3 |
| `cases/spec/03-keys/section_names_vt_ff_between_names` | §3 |
| `cases/spec/03-keys/section_newline_before_brace_error` | §3 |
| `cases/spec/03-keys/section_numeric_names` | §3 |
| `cases/spec/03-keys/section_one_name` | §3 |
| `cases/spec/03-keys/section_path_brace_inside_quotes` | §3 |
| `cases/spec/03-keys/section_path_left_open` | §3 |
| `cases/spec/03-keys/section_path_left_open_closed_by_array` | §3 |
| `cases/spec/03-keys/section_path_left_open_closed_by_object` | §3 |
| `cases/spec/03-keys/section_path_left_open_closed_by_section` | §3 |
| `cases/spec/03-keys/section_path_left_open_extra_close_error` | §3 |
| `cases/spec/03-keys/section_path_left_open_inner_container` | §3 |
| `cases/spec/03-keys/section_path_left_open_inside_braces_closed` | §3 |
| `cases/spec/03-keys/section_path_left_open_inside_braces_error` | §3 |
| `cases/spec/03-keys/section_path_left_open_nested_paths` | §3 |
| `cases/spec/03-keys/section_path_left_open_two_names` | §3 |
| `cases/spec/03-keys/section_path_separator_after_name_error` | §3 |
| `cases/spec/03-keys/section_path_separator_after_name_object_error` | §3 |
| `cases/spec/03-keys/section_path_separator_is_ignored` | §3 |
| `cases/spec/03-keys/section_quoted_first_key` | §3 |
| `cases/spec/03-keys/section_quoted_name_then_separator` | §3 |
| `cases/spec/03-keys/section_quoted_name_then_separator_error` | §3 |
| `cases/spec/03-keys/section_quoted_names` | §3 |
| `cases/spec/03-keys/section_separator_at_end_error` | §3 |
| `cases/spec/03-keys/section_separator_comment_takes_newline_error` | §3 |
| `cases/spec/03-keys/section_separator_newline_at_end` | §3 |
| `cases/spec/03-keys/section_separator_newline_bracket_error` | §3 |
| `cases/spec/03-keys/section_separator_newline_comment_at_end` | §3 |
| `cases/spec/03-keys/section_separator_newline_comment_then_name` | §3 |
| `cases/spec/03-keys/section_separator_newline_last_hash_error` | §3 |
| `cases/spec/03-keys/section_separator_newline_then_key_value` | §3 |
| `cases/spec/03-keys/section_separator_newline_then_name` | §3 |
| `cases/spec/03-keys/section_separator_space_at_end_error` | §3 |
| `cases/spec/03-keys/section_separator_space_newline_then_name` | §3 |
| `cases/spec/03-keys/section_separator_vt_at_end` | §3 |
| `cases/spec/03-keys/section_single_quoted_name_error` | §3 |
| `cases/spec/03-keys/section_two_names` | §3 |
| `cases/spec/03-keys/single_quoted_error` | §3 |
| `cases/spec/03-keys/starts_with_dash_error` | §3 |
| `cases/spec/03-keys/starts_with_digit` | §3 |
| `cases/spec/03-keys/starts_with_slash` | §3 |
| `cases/spec/03-keys/starts_with_underscore` | §3 |
| `cases/spec/04-atoms/backslash_at_end` | §4 |
| `cases/spec/04-atoms/backslash_escaped_terminators` | §4 |
| `cases/spec/04-atoms/backslash_escapes` | §4 |
| `cases/spec/04-atoms/backslash_invalid_unicode` | §4 |
| `cases/spec/04-atoms/backslash_invalid_unicode_prefixes` | §4 |
| `cases/spec/04-atoms/backslash_short_unicode_at_end` | §4 |
| `cases/spec/04-atoms/backslash_short_unicode_counts_bytes` | §4 |
| `cases/spec/04-atoms/bool_false_forms` | §4 |
| `cases/spec/04-atoms/bool_lookalikes_are_strings` | §4 |
| `cases/spec/04-atoms/bool_true_forms` | §4 |
| `cases/spec/04-atoms/braces_balanced` | §4 |
| `cases/spec/04-atoms/brackets_balanced` | §4 |
| `cases/spec/04-atoms/control_chars_allowed` | §4 |
| `cases/spec/04-atoms/keywords_with_trailing_space` | §4 |
| `cases/spec/04-atoms/nan_inf_lowercase_only` | §4 |
| `cases/spec/04-atoms/null_lowercase_only` | §4 |
| `cases/spec/04-atoms/path` | §4 |
| `cases/spec/04-atoms/punctuation` | §4 |
| `cases/spec/04-atoms/string_with_spaces` | §4 |
| `cases/spec/04-atoms/trailing_whitespace_stripped` | §4 |
| `cases/spec/04-atoms/unbalanced_close_brace_error` | §4 |
| `cases/spec/04-atoms/unbalanced_close_bracket_error` | §4 |
| `cases/spec/04-atoms/url` | §4 |
| `cases/spec/04-atoms/value_ends_at_terminators` | §4 |
| `cases/spec/04-atoms/value_with_dollar` | §4 |
| `cases/spec/05-numbers/big_integer_with_fraction_is_float` | §5 |
| `cases/spec/05-numbers/binary_multipliers` | §5 |
| `cases/spec/05-numbers/decimal_multipliers` | §5 |
| `cases/spec/05-numbers/dot_after_exponent_forms` | §5 |
| `cases/spec/05-numbers/dot_after_hex_digits_is_string` | §5 |
| `cases/spec/05-numbers/exponent_forms` | §5 |
| `cases/spec/05-numbers/exponent_sign_without_digits_big_integer` | §5 |
| `cases/spec/05-numbers/exponents` | §5 |
| `cases/spec/05-numbers/float_binary_multiplier_truncates` | §5 |
| `cases/spec/05-numbers/float_decimal_multiplier` | §5 |
| `cases/spec/05-numbers/float_max` | §5 |
| `cases/spec/05-numbers/float_overflow_error` | §5 |
| `cases/spec/05-numbers/float_underflow_error` | §5 |
| `cases/spec/05-numbers/floats` | §5 |
| `cases/spec/05-numbers/hex` | §5 |
| `cases/spec/05-numbers/hex_after_fraction_bad_suffix_strings` | §5 |
| `cases/spec/05-numbers/hex_after_fraction_first_number_unchecked` | §5 |
| `cases/spec/05-numbers/hex_after_fraction_negative_nothing_read_is_string` | §5 |
| `cases/spec/05-numbers/hex_after_fraction_no_time` | §5 |
| `cases/spec/05-numbers/hex_after_fraction_or_exponent` | §5 |
| `cases/spec/05-numbers/hex_after_fraction_range_error` | §5 |
| `cases/spec/05-numbers/hex_after_fraction_range_error_before_text` | §5 |
| `cases/spec/05-numbers/hex_after_fraction_suffixes_give_zero` | §5 |
| `cases/spec/05-numbers/hex_digits_before_x_ignored` | §5 |
| `cases/spec/05-numbers/hex_malformed_become_strings` | §5 |
| `cases/spec/05-numbers/hex_more_malformed` | §5 |
| `cases/spec/05-numbers/hex_overflow_error` | §5 |
| `cases/spec/05-numbers/hex_range_error_before_minus` | §5 |
| `cases/spec/05-numbers/hex_with_suffixes` | §5 |
| `cases/spec/05-numbers/int64_limits` | §5 |
| `cases/spec/05-numbers/int_overflow_error` | §5 |
| `cases/spec/05-numbers/int_underflow_error` | §5 |
| `cases/spec/05-numbers/integers` | §5 |
| `cases/spec/05-numbers/malformed_become_strings` | §5 |
| `cases/spec/05-numbers/malformed_before_range_is_string` | §5 |
| `cases/spec/05-numbers/min_normal` | §5 |
| `cases/spec/05-numbers/multiplier_overflow_wraps` | §5 |
| `cases/spec/05-numbers/nan_inf_time_lookalikes_are_strings` | §5 |
| `cases/spec/05-numbers/negative_multipliers` | §5 |
| `cases/spec/05-numbers/no_binary_or_octal` | §5 |
| `cases/spec/05-numbers/not_numbers` | §5 |
| `cases/spec/05-numbers/number_length_limit` | §5 |
| `cases/spec/05-numbers/number_then_block_comment` | §5 |
| `cases/spec/05-numbers/number_then_brace_is_string` | §5 |
| `cases/spec/05-numbers/number_then_hash` | §5 |
| `cases/spec/05-numbers/number_then_text_is_string` | §5 |
| `cases/spec/05-numbers/plain_number_trailing_whitespace` | §5 |
| `cases/spec/05-numbers/range_error_before_fraction` | §5 |
| `cases/spec/05-numbers/range_error_before_letters` | §5 |
| `cases/spec/05-numbers/range_error_before_suffix_text` | §5 |
| `cases/spec/05-numbers/range_error_before_trailing_space_text` | §5 |
| `cases/spec/05-numbers/range_error_dot_after_exponent` | §5 |
| `cases/spec/05-numbers/range_error_hex_before_trailing_x` | §5 |
| `cases/spec/05-numbers/range_error_underflow_before_suffix` | §5 |
| `cases/spec/05-numbers/subnormal_error` | §5 |
| `cases/spec/05-numbers/suffix_before_closing_brackets` | §5 |
| `cases/spec/05-numbers/suffix_before_separator` | §5 |
| `cases/spec/05-numbers/suffix_in_arrays` | §5 |
| `cases/spec/05-numbers/suffix_in_braces_before_space` | §5 |
| `cases/spec/05-numbers/suffix_then_comment_without_space` | §5 |
| `cases/spec/05-numbers/suffix_then_space_comment_is_string` | §5 |
| `cases/spec/05-numbers/suffix_then_whitespace_is_string` | §5 |
| `cases/spec/05-numbers/time_hour_day_week_year` | §5 |
| `cases/spec/05-numbers/time_infinite_in_json_form` | §5 |
| `cases/spec/05-numbers/time_kilo_giga_seconds` | §5 |
| `cases/spec/05-numbers/time_lookalikes_are_strings` | §5 |
| `cases/spec/05-numbers/time_milliseconds` | §5 |
| `cases/spec/05-numbers/time_minutes` | §5 |
| `cases/spec/05-numbers/time_seconds` | §5 |
| `cases/spec/05-numbers/time_subnormal_through_ms` | §5, §10 |
| `cases/spec/05-numbers/time_suffix_overflow_infinite` | §5, §10 |
| `cases/spec/05-numbers/unknown_multipliers_are_strings` | §5 |
| `cases/spec/05-numbers/very_long_number_is_string` | §5 |
| `cases/spec/06-strings/concatenation_error` | §6 |
| `cases/spec/06-strings/dq_backslash_at_end_error` | §6 |
| `cases/spec/06-strings/dq_basic` | §6 |
| `cases/spec/06-strings/dq_brace_unicode_error` | §6 |
| `cases/spec/06-strings/dq_control_char_error` | §6 |
| `cases/spec/06-strings/dq_del_allowed` | §6 |
| `cases/spec/06-strings/dq_escapes` | §6 |
| `cases/spec/06-strings/dq_not_interpreted` | §6 |
| `cases/spec/06-strings/dq_raw_newline_error` | §6 |
| `cases/spec/06-strings/dq_raw_tab_error` | §6 |
| `cases/spec/06-strings/dq_unicode_bmp` | §6 |
| `cases/spec/06-strings/dq_unicode_five_digits` | §6 |
| `cases/spec/06-strings/dq_unicode_invalid_error` | §6 |
| `cases/spec/06-strings/dq_unicode_nul` | §6 |
| `cases/spec/06-strings/dq_unicode_short_error` | §6 |
| `cases/spec/06-strings/dq_unit_separator_allowed` | §6 |
| `cases/spec/06-strings/dq_unknown_escape_drops_backslash` | §6 |
| `cases/spec/06-strings/dq_unterminated_error` | §6 |
| `cases/spec/06-strings/dq_utf8` | §6 |
| `cases/spec/06-strings/heredoc_basic` | §6 |
| `cases/spec/06-strings/heredoc_blank_line_content` | §6 |
| `cases/spec/06-strings/heredoc_crlf_content` | §6 |
| `cases/spec/06-strings/heredoc_crlf_opener_error` | §6 |
| `cases/spec/06-strings/heredoc_empty_error` | §6 |
| `cases/spec/06-strings/heredoc_empty_name_end_rule` | §6 |
| `cases/spec/06-strings/heredoc_empty_name_eof_error` | §6 |
| `cases/spec/06-strings/heredoc_empty_name_variables` | §6, §7 |
| `cases/spec/06-strings/heredoc_empty_terminator` | §6 |
| `cases/spec/06-strings/heredoc_first_line_is_content` | §6 |
| `cases/spec/06-strings/heredoc_in_array` | §6 |
| `cases/spec/06-strings/heredoc_indented_terminator_is_content` | §6 |
| `cases/spec/06-strings/heredoc_keeps_whitespace` | §6 |
| `cases/spec/06-strings/heredoc_longer_line_is_content` | §6 |
| `cases/spec/06-strings/heredoc_lowercase_error` | §6 |
| `cases/spec/06-strings/heredoc_no_escapes` | §6 |
| `cases/spec/06-strings/heredoc_opener_cut_by_end_error` | §6 |
| `cases/spec/06-strings/heredoc_opener_cut_by_end_in_arguments_error` | §6, §9 |
| `cases/spec/06-strings/heredoc_opener_cut_by_end_two_letters_error` | §6 |
| `cases/spec/06-strings/heredoc_opener_not_all_uppercase_is_string` | §6 |
| `cases/spec/06-strings/heredoc_opener_then_space_at_end_is_string` | §6 |
| `cases/spec/06-strings/heredoc_other_uppercase_names` | §6 |
| `cases/spec/06-strings/heredoc_repeated_letter_name` | §6 |
| `cases/spec/06-strings/heredoc_short_input_is_string` | §6 |
| `cases/spec/06-strings/heredoc_space_after_opener_error` | §6 |
| `cases/spec/06-strings/heredoc_terminator_at_eof` | §6 |
| `cases/spec/06-strings/heredoc_terminator_before_brace_error` | §6 |
| `cases/spec/06-strings/heredoc_terminator_then_newline_brace` | §6 |
| `cases/spec/06-strings/heredoc_terminator_then_separator` | §6 |
| `cases/spec/06-strings/heredoc_terminator_trailing_space_error` | §6 |
| `cases/spec/06-strings/heredoc_unterminated_error` | §6 |
| `cases/spec/06-strings/heredoc_variables` | §6 |
| `cases/spec/06-strings/sq_backslash_at_end_error` | §6 |
| `cases/spec/06-strings/sq_basic` | §6 |
| `cases/spec/06-strings/sq_escaped_quote` | §6 |
| `cases/spec/06-strings/sq_line_continuation` | §6 |
| `cases/spec/06-strings/sq_line_continuation_lone_cr` | §6 |
| `cases/spec/06-strings/sq_no_escapes` | §6 |
| `cases/spec/06-strings/sq_no_variables` | §6 |
| `cases/spec/06-strings/sq_not_interpreted` | §6 |
| `cases/spec/06-strings/sq_raw_newline` | §6 |
| `cases/spec/06-strings/sq_unterminated_error` | §6 |
| `cases/spec/06-strings/triple_quote_error` | §6 |
| `cases/spec/07-variables/backslash_dollar` | §7 |
| `cases/spec/07-variables/backslash_dollar_counts_dollar_in_unicode_escape` | §7 |
| `cases/spec/07-variables/backslash_dollar_unicode_escape_then_escaped_dollar` | §7 |
| `cases/spec/07-variables/backslash_dollar_unquoted_mixed` | §7 |
| `cases/spec/07-variables/braced` | §7 |
| `cases/spec/07-variables/dollar_escape_in_unquoted` | §7 |
| `cases/spec/07-variables/dollar_escape_only_when_expanding` | §7 |
| `cases/spec/07-variables/expanded_values_stay_strings` | §7 |
| `cases/spec/07-variables/extra_variable_longer_name` | §7 |
| `cases/spec/07-variables/extra_variable_order_short_first` | §7 |
| `cases/spec/07-variables/filename_for_string_input` | §7 |
| `cases/spec/07-variables/filename_reregistered_for_string_input` | §7 |
| `cases/spec/07-variables/filevars_disabled` | §7 |
| `cases/spec/07-variables/filevars_precede_registered` | §7 |
| `cases/spec/07-variables/handler_braced_whole_string` | §7 |
| `cases/spec/07-variables/handler_not_for_unbraced` | §7 |
| `cases/spec/07-variables/handler_refuses` | §7 |
| `cases/spec/07-variables/in_heredoc` | §7 |
| `cases/spec/07-variables/in_unquoted_atoms` | §7 |
| `cases/spec/07-variables/lone_dollar` | §7 |
| `cases/spec/07-variables/names_case_sensitive` | §7 |
| `cases/spec/07-variables/not_in_keys` | §7 |
| `cases/spec/07-variables/not_in_single_quotes` | §7 |
| `cases/spec/07-variables/numbers_are_not_expanded` | §7 |
| `cases/spec/07-variables/registered_wins_over_handler` | §7 |
| `cases/spec/07-variables/reregister_keeps_position` | §7 |
| `cases/spec/07-variables/unbraced` | §7 |
| `cases/spec/07-variables/unbraced_prefix_match` | §7 |
| `cases/spec/07-variables/unknown_preserved` | §7 |
| `cases/spec/07-variables/unterminated_brace_inner_reference` | §7 |
| `cases/spec/07-variables/upstream_mix` | §7 |
| `cases/spec/08-duplicates/chunk_priority` | §8 |
| `cases/spec/08-duplicates/explicit_array_not_flattened` | §8 |
| `cases/spec/08-duplicates/include_merge_scalar_quirk_default_mode` | §8 |
| `cases/spec/08-duplicates/inherit_kept_after_merge_object` | §8 |
| `cases/spec/08-duplicates/inherit_kept_after_merge_scalar` | §8 |
| `cases/spec/08-duplicates/inherit_merge_equal_adds` | §8 |
| `cases/spec/08-duplicates/inherit_merge_higher_replaces` | §8 |
| `cases/spec/08-duplicates/inherit_merge_lower_dropped` | §8 |
| `cases/spec/08-duplicates/inherit_merge_objects` | §8 |
| `cases/spec/08-duplicates/inherit_strategy_error` | §8 |
| `cases/spec/08-duplicates/inherit_strategy_rewrite` | §8 |
| `cases/spec/08-duplicates/keeps_first_position` | §8 |
| `cases/spec/08-duplicates/merge_empty_value_at_end_after_scalar` | §8, §1 |
| `cases/spec/08-duplicates/merge_empty_value_at_end_comments_join_object` | §8, §12 |
| `cases/spec/08-duplicates/merge_empty_value_at_end_higher_priority` | §8, §1 |
| `cases/spec/08-duplicates/merge_empty_value_at_end_keeps_array` | §8, §1 |
| `cases/spec/08-duplicates/merge_empty_value_at_end_keeps_object` | §8, §1 |
| `cases/spec/08-duplicates/merge_empty_value_at_end_of_included_file` | §8, §9 |
| `cases/spec/08-duplicates/merge_explicit_null_replaces_object` | §8 |
| `cases/spec/08-duplicates/merge_include_first_array_only` | §8 |
| `cases/spec/08-duplicates/merge_include_first_value_only` | §8 |
| `cases/spec/08-duplicates/merge_include_first_value_scalar` | §8 |
| `cases/spec/08-duplicates/merge_nested_priority_higher` | §8 |
| `cases/spec/08-duplicates/merge_nested_priority_lower` | §8 |
| `cases/spec/08-duplicates/merge_quirk_scalar_keeps_container_key` | §8 |
| `cases/spec/08-duplicates/merge_quirk_scalar_keeps_container_key_spelling` | §8 |
| `cases/spec/08-duplicates/mixed_types` | §8 |
| `cases/spec/08-duplicates/nia_chunk_priority` | §8 |
| `cases/spec/08-duplicates/nia_collection_has_priority_0` | §8 |
| `cases/spec/08-duplicates/nia_collection_replaced_higher_value` | §8 |
| `cases/spec/08-duplicates/nia_collection_replaced_lower_value` | §8 |
| `cases/spec/08-duplicates/nia_include_merge_array_then_repeat` | §8 |
| `cases/spec/08-duplicates/nia_include_merge_scalar` | §8 |
| `cases/spec/08-duplicates/nia_include_merge_scalar_then_higher` | §8 |
| `cases/spec/08-duplicates/nia_include_merge_scalar_then_object_error` | §8 |
| `cases/spec/08-duplicates/nia_include_merge_scalar_then_repeat_error` | §8 |
| `cases/spec/08-duplicates/nia_include_merge_scalar_then_rewrite` | §8 |
| `cases/spec/08-duplicates/nia_inherited_replaced` | §8 |
| `cases/spec/08-duplicates/nia_merge_array_extends_collection` | §8 |
| `cases/spec/08-duplicates/nia_merge_quirk_then_repeat_error` | §8 |
| `cases/spec/08-duplicates/nia_merge_scalar_replaces_collection` | §8 |
| `cases/spec/08-duplicates/nia_multivalue_entry_collects_first_value_only` | §8, §9 |
| `cases/spec/08-duplicates/nia_multivalue_entry_object_repeat` | §8, §9 |
| `cases/spec/08-duplicates/nia_priorities_compared_first` | §8 |
| `cases/spec/08-duplicates/no_implicit_arrays_objects` | §8 |
| `cases/spec/08-duplicates/no_implicit_arrays_scalars` | §8 |
| `cases/spec/08-duplicates/no_implicit_arrays_with_arrays` | §8 |
| `cases/spec/08-duplicates/objects_repeated_not_merged` | §8 |
| `cases/spec/08-duplicates/priority_15` | §8 |
| `cases/spec/08-duplicates/priority_16_is_0` | §8 |
| `cases/spec/08-duplicates/priority_applies_to_containers` | §8 |
| `cases/spec/08-duplicates/priority_equal_appends` | §8 |
| `cases/spec/08-duplicates/priority_higher_replaces` | §8 |
| `cases/spec/08-duplicates/priority_lower_ignored` | §8 |
| `cases/spec/08-duplicates/priority_modulo_16` | §8 |
| `cases/spec/08-duplicates/priority_negative` | §8 |
| `cases/spec/08-duplicates/repeated_inside_array_object` | §8 |
| `cases/spec/08-duplicates/repeated_inside_braces` | §8 |
| `cases/spec/08-duplicates/scalar_repeated` | §8 |
| `cases/spec/08-duplicates/sections_repeated_not_merged` | §8 |
| `cases/spec/08-duplicates/strategy_error` | §8 |
| `cases/spec/08-duplicates/strategy_error_no_duplicates` | §8 |
| `cases/spec/08-duplicates/strategy_merge_array_then_object_error` | §8 |
| `cases/spec/08-duplicates/strategy_merge_array_then_scalar` | §8 |
| `cases/spec/08-duplicates/strategy_merge_arrays` | §8 |
| `cases/spec/08-duplicates/strategy_merge_arrays_ignore_priority` | §8 |
| `cases/spec/08-duplicates/strategy_merge_first_scalar_of_several` | §8 |
| `cases/spec/08-duplicates/strategy_merge_object_then_array_error` | §8 |
| `cases/spec/08-duplicates/strategy_merge_object_then_scalar` | §8 |
| `cases/spec/08-duplicates/strategy_merge_objects` | §8 |
| `cases/spec/08-duplicates/strategy_merge_scalar_keeps_container_priority` | §8 |
| `cases/spec/08-duplicates/strategy_merge_scalar_then_object` | §8 |
| `cases/spec/08-duplicates/strategy_merge_scalars_append` | §8 |
| `cases/spec/08-duplicates/strategy_rewrite` | §8 |
| `cases/spec/09-macros/comments_carry_into_included_file` | §9, §12 |
| `cases/spec/09-macros/comments_end_of_included_file` | §9, §12 |
| `cases/spec/09-macros/comments_include_first_name_brace_after_entry` | §9, §12 |
| `cases/spec/09-macros/comments_include_first_name_brace_comment_in_file` | §9, §12 |
| `cases/spec/09-macros/comments_include_first_name_brace_in_braced_object` | §9, §12 |
| `cases/spec/09-macros/comments_include_first_name_brace_keeps_most_recent` | §9, §12 |
| `cases/spec/09-macros/comments_include_first_name_brace_under_key` | §9, §12 |
| `cases/spec/09-macros/comments_include_first_name_brace_under_prefix` | §9, §12 |
| `cases/spec/09-macros/comments_include_first_name_closed_by_object` | §9, §12 |
| `cases/spec/09-macros/comments_include_first_name_in_section_object` | §9, §12 |
| `cases/spec/09-macros/comments_include_key_array_object_most_recent` | §9, §12 |
| `cases/spec/09-macros/comments_include_key_object_most_recent` | §9, §12 |
| `cases/spec/09-macros/comments_include_key_takes_no_pending` | §9, §12 |
| `cases/spec/09-macros/comments_load_value_takes_none_after` | §9, §12 |
| `cases/spec/09-macros/comments_load_value_takes_none_before` | §9, §12 |
| `cases/spec/09-macros/comments_one_byte_file_attaches_pending` | §9, §12 |
| `cases/spec/09-macros/comments_zero_byte_file_keeps_pending` | §9, §12 |
| `cases/spec/09-macros/comments_zero_byte_file_with_key_keeps_pending` | §9, §12 |
| `cases/spec/09-macros/comments_zero_byte_glob_match_keeps_pending` | §9, §12 |
| `cases/spec/09-macros/include_array_root_error` | §9 |
| `cases/spec/09-macros/include_bare` | §9 |
| `cases/spec/09-macros/include_block_comment_then_open_brace_takes_nothing_over` | §1, §9 |
| `cases/spec/09-macros/include_braced_file_after_name` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_at_braced_root_error` | §9 |
| `cases/spec/09-macros/include_braced_file_closes_section_object` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_entry_first_then_path_error` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_entry_then_nested_braced_error` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_first_name_after_comment_error` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_first_name_and_brace_closed` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_first_name_closed` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_first_name_closed_by_object` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_first_name_closed_by_object_then_brace_error` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_first_name_closed_root_open_error` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_first_name_in_section_object_after_text` | §9, §3, §13 |
| `cases/spec/09-macros/include_braced_file_first_name_in_section_object_after_text_unclosed_error` | §9, §3, §13 |
| `cases/spec/09-macros/include_braced_file_first_name_in_section_object_error` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_first_name_same_line_error` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_first_name_separator_newline_error` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_first_name_then_macro_error` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_first_name_unclosed_error` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_first_quoted_name_error` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_inside_braces_error` | §9 |
| `cases/spec/09-macros/include_braced_file_nested_path_one_brace_error` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_priority_then_first_name` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_then_entries` | §9 |
| `cases/spec/09-macros/include_braced_file_twice` | §9 |
| `cases/spec/09-macros/include_braced_file_two_names_error` | §9, §3 |
| `cases/spec/09-macros/include_braced_file_with_key_inside_braces` | §9 |
| `cases/spec/09-macros/include_braces` | §9 |
| `cases/spec/09-macros/include_braces_variables` | §9 |
| `cases/spec/09-macros/include_closes_braced_root` | §9, §3 |
| `cases/spec/09-macros/include_closes_braced_root_in_nested_file_error` | §9, §3 |
| `cases/spec/09-macros/include_closes_braced_root_rest_of_file_ignored` | §9, §3 |
| `cases/spec/09-macros/include_closes_braced_root_rest_of_file_macro_ignored` | §9, §3 |
| `cases/spec/09-macros/include_closes_braced_root_then_block_comment_error` | §9, §3 |
| `cases/spec/09-macros/include_closes_braced_root_then_brace_error` | §9, §3 |
| `cases/spec/09-macros/include_closes_braced_root_then_comment_error` | §9, §3 |
| `cases/spec/09-macros/include_closes_braced_root_then_entry_error` | §9, §3 |
| `cases/spec/09-macros/include_closes_braced_root_then_semicolons` | §9, §3 |
| `cases/spec/09-macros/include_comment_blank_line_brace_error` | §9 |
| `cases/spec/09-macros/include_comment_then_open_brace_takes_nothing_over` | §1, §9 |
| `cases/spec/09-macros/include_comment_then_open_bracket_adds_nothing` | §1, §9 |
| `cases/spec/09-macros/include_comment_then_space_brace_error` | §1, §9 |
| `cases/spec/09-macros/include_curdir` | §9 |
| `cases/spec/09-macros/include_curdir_restored_after_include` | §9 |
| `cases/spec/09-macros/include_cycle_nesting_limit_error` | §9 |
| `cases/spec/09-macros/include_directory_error` | §9 |
| `cases/spec/09-macros/include_directory_try` | §9 |
| `cases/spec/09-macros/include_does_not_inherit_priority` | §9 |
| `cases/spec/09-macros/include_does_not_inherit_strategy` | §9 |
| `cases/spec/09-macros/include_duplicate_error` | §9 |
| `cases/spec/09-macros/include_duplicate_merge` | §9 |
| `cases/spec/09-macros/include_duplicate_param_ends_at_nul` | §9 |
| `cases/spec/09-macros/include_duplicate_rewrite` | §9 |
| `cases/spec/09-macros/include_duplicate_unknown_is_append` | §9 |
| `cases/spec/09-macros/include_duplicates_append` | §9 |
| `cases/spec/09-macros/include_empty_args` | §9 |
| `cases/spec/09-macros/include_empty_file` | §9 |
| `cases/spec/09-macros/include_empty_path_try` | §9 |
| `cases/spec/09-macros/include_empty_value_is_null` | §9 |
| `cases/spec/09-macros/include_end_check_stops_at_array_element_of_file` | §9, §3 |
| `cases/spec/09-macros/include_end_check_stops_at_braced_object_of_file` | §9, §3 |
| `cases/spec/09-macros/include_end_check_stops_at_object_with_taken_over_brace` | §9, §3 |
| `cases/spec/09-macros/include_file_closes_including_object` | §9 |
| `cases/spec/09-macros/include_file_closing_brace_at_top_level_error` | §9 |
| `cases/spec/09-macros/include_file_with_rejected_args_error` | §9 |
| `cases/spec/09-macros/include_glob` | §9 |
| `cases/spec/09-macros/include_glob_backslash_before_slash` | §9 |
| `cases/spec/09-macros/include_glob_backslash_inside_brackets` | §9 |
| `cases/spec/09-macros/include_glob_backslash_quotes_next_character` | §9 |
| `cases/spec/09-macros/include_glob_bang_negates` | §9 |
| `cases/spec/09-macros/include_glob_bracket_expression` | §9 |
| `cases/spec/09-macros/include_glob_bracket_needs_wildcard_error` | §9 |
| `cases/spec/09-macros/include_glob_bracket_negation_close_bracket_member` | §9 |
| `cases/spec/09-macros/include_glob_byte_order` | §9 |
| `cases/spec/09-macros/include_glob_caret_is_not_negation` | §9 |
| `cases/spec/09-macros/include_glob_close_bracket_first_is_member` | §9 |
| `cases/spec/09-macros/include_glob_directory_match_error` | §9 |
| `cases/spec/09-macros/include_glob_dot_components_and_double_slash` | §9 |
| `cases/spec/09-macros/include_glob_dot_star_matches_dot_entries_error` | §9 |
| `cases/spec/09-macros/include_glob_dot_star_try` | §9 |
| `cases/spec/09-macros/include_glob_in_directory_components` | §9 |
| `cases/spec/09-macros/include_glob_matches_including_file_error` | §9 |
| `cases/spec/09-macros/include_glob_no_brace_expansion` | §9 |
| `cases/spec/09-macros/include_glob_no_character_classes` | §9 |
| `cases/spec/09-macros/include_glob_no_match_stops_parsing` | §9 |
| `cases/spec/09-macros/include_glob_nul_first_wildcard_after_stops` | §9 |
| `cases/spec/09-macros/include_glob_prefix_key_from_first_file` | §9 |
| `cases/spec/09-macros/include_glob_question_mark` | §9 |
| `cases/spec/09-macros/include_glob_range` | §9 |
| `cases/spec/09-macros/include_glob_reversed_range_matches_nothing` | §9 |
| `cases/spec/09-macros/include_glob_search_path_cuts_at_nul_first_error` | §9 |
| `cases/spec/09-macros/include_glob_skips_hidden_files` | §9 |
| `cases/spec/09-macros/include_glob_stop_inside_match_ends_parse` | §9 |
| `cases/spec/09-macros/include_glob_trailing_slash_matches_directories_error` | §9 |
| `cases/spec/09-macros/include_glob_trailing_slash_no_directory_stops` | §9 |
| `cases/spec/09-macros/include_glob_trailing_slash_try` | §9 |
| `cases/spec/09-macros/include_glob_try_no_match` | §9 |
| `cases/spec/09-macros/include_glob_try_skips_directories` | §9 |
| `cases/spec/09-macros/include_glob_try_stop_inside_match_ends_parse` | §9 |
| `cases/spec/09-macros/include_glob_unclosed_bracket_is_literal` | §9 |
| `cases/spec/09-macros/include_glob_wildcard_after_nul_matches_part_before` | §9 |
| `cases/spec/09-macros/include_glob_wildcard_after_nul_no_match_stops` | §9 |
| `cases/spec/09-macros/include_glob_wildcard_after_nul_try_continues` | §9 |
| `cases/spec/09-macros/include_inside_object` | §9 |
| `cases/spec/09-macros/include_key_close_brace_at_top_level_error` | §9 |
| `cases/spec/09-macros/include_key_close_brace_under_taken_over_brace` | §9 |
| `cases/spec/09-macros/include_key_empty_string` | §9 |
| `cases/spec/09-macros/include_key_file_containers_close_at_end` | §9 |
| `cases/spec/09-macros/include_key_not_lowercased_but_matched` | §9 |
| `cases/spec/09-macros/include_key_open_brace_then_close_error` | §9 |
| `cases/spec/09-macros/include_key_param_ends_at_nul` | §9 |
| `cases/spec/09-macros/include_key_param_nul_first_empty_key` | §9 |
| `cases/spec/09-macros/include_key_share_braced_file` | §9, §3 |
| `cases/spec/09-macros/include_key_share_prefix` | §9 |
| `cases/spec/09-macros/include_key_share_section_object` | §9, §3 |
| `cases/spec/09-macros/include_key_share_target_array` | §9 |
| `cases/spec/09-macros/include_key_share_two_includes` | §9 |
| `cases/spec/09-macros/include_key_share_unused` | §9 |
| `cases/spec/09-macros/include_key_share_used_twice_error` | §9 |
| `cases/spec/09-macros/include_key_without_prefix` | §9 |
| `cases/spec/09-macros/include_left_open_after_separator_and_newline` | §9, §3 |
| `cases/spec/09-macros/include_left_open_closed_then_checked_error` | §9, §3 |
| `cases/spec/09-macros/include_left_open_end_check_stops` | §9, §3 |
| `cases/spec/09-macros/include_left_open_end_check_stops_in_included_file` | §9, §3 |
| `cases/spec/09-macros/include_left_open_end_check_stops_then_entries` | §9, §3 |
| `cases/spec/09-macros/include_left_open_from_both_units_close_together` | §9, §3 |
| `cases/spec/09-macros/include_left_open_section_persists` | §9 |
| `cases/spec/09-macros/include_lower_than_main_priority_dropped` | §9 |
| `cases/spec/09-macros/include_main_document_itself_error` | §9 |
| `cases/spec/09-macros/include_merge_ignores_priority` | §9 |
| `cases/spec/09-macros/include_merge_lower_priority_still_merges` | §9 |
| `cases/spec/09-macros/include_merge_scalar_higher_priority_replaces` | §9 |
| `cases/spec/09-macros/include_merge_scalar_lower_priority_dropped` | §9 |
| `cases/spec/09-macros/include_missing_error` | §9 |
| `cases/spec/09-macros/include_moves_filevars_last` | §9, §7 |
| `cases/spec/09-macros/include_moves_filevars_last_string_input` | §9, §7 |
| `cases/spec/09-macros/include_nested` | §9 |
| `cases/spec/09-macros/include_nesting_limit_error` | §9 |
| `cases/spec/09-macros/include_nesting_limit_ok` | §9 |
| `cases/spec/09-macros/include_newline_then_open_brace_adds_nothing` | §9 |
| `cases/spec/09-macros/include_only_open_brace_adds_nothing` | §9 |
| `cases/spec/09-macros/include_only_open_brace_takes_nothing_over` | §9 |
| `cases/spec/09-macros/include_only_open_bracket_adds_nothing` | §9 |
| `cases/spec/09-macros/include_open_brace_in_section_object_closed_by_includer` | §9, §3 |
| `cases/spec/09-macros/include_param_prefix_names` | §9 |
| `cases/spec/09-macros/include_path_empty_array_error` | §9 |
| `cases/spec/09-macros/include_path_first_dir` | §9 |
| `cases/spec/09-macros/include_path_first_miss_after_load_try_error` | §9 |
| `cases/spec/09-macros/include_path_first_miss_again_after_load_try_error` | §9 |
| `cases/spec/09-macros/include_path_first_miss_then_load_missing_error` | §9 |
| `cases/spec/09-macros/include_path_first_miss_then_load_try_empty_file_error` | §9 |
| `cases/spec/09-macros/include_path_first_miss_then_load_try_existing_file_error` | §9 |
| `cases/spec/09-macros/include_path_first_miss_then_optional_file_error` | §9 |
| `cases/spec/09-macros/include_path_first_miss_then_try_include_url_accepts_later` | §9 |
| `cases/spec/09-macros/include_path_first_miss_then_url_error` | §9 |
| `cases/spec/09-macros/include_path_first_miss_then_url_try_accepts_later` | §9 |
| `cases/spec/09-macros/include_path_first_miss_then_url_try_accepts_later_file_input` | §9 |
| `cases/spec/09-macros/include_path_first_miss_then_url_try_no_later_file` | §9 |
| `cases/spec/09-macros/include_path_first_miss_url_try_after_each_miss_accepts` | §9 |
| `cases/spec/09-macros/include_path_glob_all_dirs` | §9 |
| `cases/spec/09-macros/include_path_glob_last_dir_must_match_error` | §9 |
| `cases/spec/09-macros/include_path_later_list_replaces` | §9 |
| `cases/spec/09-macros/include_path_missing_in_first_dir_error` | §9 |
| `cases/spec/09-macros/include_path_no_string_entries_error` | §9 |
| `cases/spec/09-macros/include_path_param_entry_ends_at_nul` | §9 |
| `cases/spec/09-macros/include_path_persists` | §9 |
| `cases/spec/09-macros/include_path_string_ignored` | §9 |
| `cases/spec/09-macros/include_path_try_first_dir_only` | §9 |
| `cases/spec/09-macros/include_path_two_first_misses_then_url_try_accepts` | §9 |
| `cases/spec/09-macros/include_pattern_without_glob_error` | §9 |
| `cases/spec/09-macros/include_prefix_array_converted_collects_repeats` | §9 |
| `cases/spec/09-macros/include_prefix_array_existing_multivalue_first_array` | §9 |
| `cases/spec/09-macros/include_prefix_array_existing_multivalue_scalars` | §9 |
| `cases/spec/09-macros/include_prefix_array_existing_object` | §9 |
| `cases/spec/09-macros/include_prefix_array_existing_scalar` | §9 |
| `cases/spec/09-macros/include_prefix_array_new_does_not_collect` | §9 |
| `cases/spec/09-macros/include_prefix_array_priority` | §9 |
| `cases/spec/09-macros/include_prefix_array_target` | §9 |
| `cases/spec/09-macros/include_prefix_auto_key` | §9 |
| `cases/spec/09-macros/include_prefix_empty_file_creates_object` | §9 |
| `cases/spec/09-macros/include_prefix_existing_array_error` | §9 |
| `cases/spec/09-macros/include_prefix_existing_multivalue_first_scalar_error` | §9 |
| `cases/spec/09-macros/include_prefix_existing_multivalue_object` | §9 |
| `cases/spec/09-macros/include_prefix_existing_object` | §9 |
| `cases/spec/09-macros/include_prefix_existing_scalar_error` | §9 |
| `cases/spec/09-macros/include_prefix_keeps_other_extensions` | §9 |
| `cases/spec/09-macros/include_prefix_key` | §9 |
| `cases/spec/09-macros/include_prefix_priority_applies_to_nesting_object` | §9 |
| `cases/spec/09-macros/include_prefix_strips_ucl` | §9 |
| `cases/spec/09-macros/include_prefix_symlink_uses_target_name` | §9 |
| `cases/spec/09-macros/include_prefix_then_explicit_object_appends` | §9 |
| `cases/spec/09-macros/include_prefix_twice_same_object` | §9 |
| `cases/spec/09-macros/include_prefix_ucl_target` | §9 |
| `cases/spec/09-macros/include_priority` | §9 |
| `cases/spec/09-macros/include_priority_macro_inside` | §9 |
| `cases/spec/09-macros/include_priority_modulo_16` | §9 |
| `cases/spec/09-macros/include_priority_must_be_integer` | §9 |
| `cases/spec/09-macros/include_priority_negative` | §9 |
| `cases/spec/09-macros/include_priority_wins_over_later_entries` | §9 |
| `cases/spec/09-macros/include_priority_with_multiplier` | §9 |
| `cases/spec/09-macros/include_quoted` | §9 |
| `cases/spec/09-macros/include_relative_path_from_included_file` | §9 |
| `cases/spec/09-macros/include_self_error` | §9 |
| `cases/spec/09-macros/include_self_with_try_error` | §9 |
| `cases/spec/09-macros/include_sign_param_no_effect` | §9 |
| `cases/spec/09-macros/include_space_before_args` | §9 |
| `cases/spec/09-macros/include_strategy_only_for_included_entries` | §9 |
| `cases/spec/09-macros/include_syntax_error_in_file_error` | §9 |
| `cases/spec/09-macros/include_taken_over_section_closed_by_inner_array` | §9, §3 |
| `cases/spec/09-macros/include_taken_over_section_closed_by_inner_object` | §9, §3 |
| `cases/spec/09-macros/include_taken_over_section_closed_inside_braces` | §9, §3 |
| `cases/spec/09-macros/include_taken_over_section_closed_then_brace_error` | §9, §3 |
| `cases/spec/09-macros/include_taken_over_section_closed_with_inner_path` | §9, §3 |
| `cases/spec/09-macros/include_target_case_insensitive` | §9 |
| `cases/spec/09-macros/include_target_param_ends_at_nul` | §9 |
| `cases/spec/09-macros/include_target_unknown_means_object` | §9 |
| `cases/spec/09-macros/include_then_keys` | §9 |
| `cases/spec/09-macros/include_try_missing` | §9 |
| `cases/spec/09-macros/include_try_must_be_boolean_error` | §9 |
| `cases/spec/09-macros/include_try_string_ignored_error` | §9 |
| `cases/spec/09-macros/include_unclosed_brace_closed_by_includer` | §9 |
| `cases/spec/09-macros/include_unclosed_brace_error` | §9 |
| `cases/spec/09-macros/include_unclosed_brace_inside_braces` | §9 |
| `cases/spec/09-macros/include_unclosed_object_in_file_error` | §9 |
| `cases/spec/09-macros/include_unknown_param_ignored` | §9 |
| `cases/spec/09-macros/include_url_any_scheme_separator` | §9 |
| `cases/spec/09-macros/include_url_before_search_path` | §9 |
| `cases/spec/09-macros/include_url_glob_prefix_no_difference` | §9 |
| `cases/spec/09-macros/include_url_key_not_created` | §9 |
| `cases/spec/09-macros/include_url_like_path_is_a_path` | §9 |
| `cases/spec/09-macros/include_url_param_error` | §9 |
| `cases/spec/09-macros/include_url_path_param_replaces_list` | §9 |
| `cases/spec/09-macros/include_url_path_param_takes_effect` | §9 |
| `cases/spec/09-macros/include_url_try_skipped` | §9 |
| `cases/spec/09-macros/includes_like_include` | §9 |
| `cases/spec/09-macros/inherit_at_top_level` | §9 |
| `cases/spec/09-macros/inherit_bare_value_trailing_space_error` | §9 |
| `cases/spec/09-macros/inherit_basic` | §9 |
| `cases/spec/09-macros/inherit_braces_value` | §9 |
| `cases/spec/09-macros/inherit_container_first_value_only` | §9 |
| `cases/spec/09-macros/inherit_copies_have_no_comments` | §9, §12 |
| `cases/spec/09-macros/inherit_copies_key_with_nul` | §9 |
| `cases/spec/09-macros/inherit_copies_take_no_pending_comments` | §9, §12 |
| `cases/spec/09-macros/inherit_dotted_name_is_one_key` | §9 |
| `cases/spec/09-macros/inherit_dotted_name_not_a_path_error` | §9 |
| `cases/spec/09-macros/inherit_empty_name_error` | §9 |
| `cases/spec/09-macros/inherit_enclosing_object` | §9 |
| `cases/spec/09-macros/inherit_existing_keys_kept` | §9 |
| `cases/spec/09-macros/inherit_first_of_repeated` | §9 |
| `cases/spec/09-macros/inherit_first_value_rule_at_every_level` | §9 |
| `cases/spec/09-macros/inherit_from_included_file_uses_main_root` | §9 |
| `cases/spec/09-macros/inherit_inside_included_file_with_key` | §9 |
| `cases/spec/09-macros/inherit_is_shallow` | §9 |
| `cases/spec/09-macros/inherit_keeps_source_priority` | §9 |
| `cases/spec/09-macros/inherit_key_lowercase_name` | §9 |
| `cases/spec/09-macros/inherit_missing_error` | §9 |
| `cases/spec/09-macros/inherit_multivalue_replaced_by_explicit` | §9 |
| `cases/spec/09-macros/inherit_name_case_sensitive_error` | §9 |
| `cases/spec/09-macros/inherit_no_implicit_arrays_collects` | §9 |
| `cases/spec/09-macros/inherit_non_object_error` | §9 |
| `cases/spec/09-macros/inherit_own_key_earlier_value` | §9 |
| `cases/spec/09-macros/inherit_own_object` | §9 |
| `cases/spec/09-macros/inherit_replace_appends` | §9 |
| `cases/spec/09-macros/inherit_replace_copies_not_inherited` | §9 |
| `cases/spec/09-macros/inherit_replace_copy_keeps_collected_array` | §9, §8 |
| `cases/spec/09-macros/inherit_replace_copy_of_inherited_stays_inherited` | §9 |
| `cases/spec/09-macros/inherit_replace_first_value` | §9 |
| `cases/spec/09-macros/inherit_replace_ignores_priority` | §9, §8 |
| `cases/spec/09-macros/inherit_replace_ignores_strategy_error` | §9, §8 |
| `cases/spec/09-macros/inherit_replace_ignores_strategy_merge` | §9, §8 |
| `cases/spec/09-macros/inherit_replace_keyword_yes` | §9 |
| `cases/spec/09-macros/inherit_replace_must_be_boolean` | §9 |
| `cases/spec/09-macros/inherit_replace_no_implicit_arrays_multivalue` | §9, §8 |
| `cases/spec/09-macros/inherit_replace_no_prefix_match` | §9 |
| `cases/spec/09-macros/inherit_replace_own_container_first_value_only` | §9 |
| `cases/spec/09-macros/inherit_replace_own_object_duplicates` | §9 |
| `cases/spec/09-macros/inherit_replace_twice_duplicates` | §9 |
| `cases/spec/09-macros/inherit_replaced_whatever_priority` | §9 |
| `cases/spec/09-macros/inherit_root_array_error` | §9 |
| `cases/spec/09-macros/inherit_source_must_exist_already_error` | §9 |
| `cases/spec/09-macros/inherit_top_level_only_error` | §9 |
| `cases/spec/09-macros/inherit_twice_no_change` | §9 |
| `cases/spec/09-macros/inherit_variable_in_name` | §9 |
| `cases/spec/09-macros/load_directory_error` | §9 |
| `cases/spec/09-macros/load_directory_try` | §9 |
| `cases/spec/09-macros/load_empty_file_int_zero` | §9 |
| `cases/spec/09-macros/load_empty_file_string_inserts_nothing` | §9 |
| `cases/spec/09-macros/load_empty_key_error` | §9 |
| `cases/spec/09-macros/load_empty_path_error` | §9 |
| `cases/spec/09-macros/load_escape` | §9 |
| `cases/spec/09-macros/load_escape_all_bytes` | §9 |
| `cases/spec/09-macros/load_escape_must_be_boolean` | §9 |
| `cases/spec/09-macros/load_existing_key_before_empty_file` | §9 |
| `cases/spec/09-macros/load_existing_key_before_target` | §9 |
| `cases/spec/09-macros/load_existing_key_error` | §9 |
| `cases/spec/09-macros/load_ignores_priority_macro` | §9 |
| `cases/spec/09-macros/load_ignores_search_path` | §9 |
| `cases/spec/09-macros/load_inside_object` | §9 |
| `cases/spec/09-macros/load_int` | §9 |
| `cases/spec/09-macros/load_int_clamps` | §9 |
| `cases/spec/09-macros/load_int_leading_digits` | §9 |
| `cases/spec/09-macros/load_int_leading_whitespace_and_sign` | §9 |
| `cases/spec/09-macros/load_int_no_digits_is_zero` | §9 |
| `cases/spec/09-macros/load_int_stops_at_nul` | §9 |
| `cases/spec/09-macros/load_key_case_sensitive` | §9 |
| `cases/spec/09-macros/load_key_lowercase_existing_error` | §9 |
| `cases/spec/09-macros/load_key_lowercase_kept_spelling` | §9 |
| `cases/spec/09-macros/load_key_param_ends_at_nul` | §9 |
| `cases/spec/09-macros/load_key_param_nul_first_error` | §9 |
| `cases/spec/09-macros/load_missing_error` | §9 |
| `cases/spec/09-macros/load_missing_key_error_with_try` | §9 |
| `cases/spec/09-macros/load_multiline` | §9 |
| `cases/spec/09-macros/load_param_prefix_t_is_try` | §9 |
| `cases/spec/09-macros/load_param_prefix_tri_is_trim` | §9 |
| `cases/spec/09-macros/load_path_nul_first_missing_error` | §9 |
| `cases/spec/09-macros/load_priority` | §9 |
| `cases/spec/09-macros/load_priority_modulo_16` | §9 |
| `cases/spec/09-macros/load_string` | §9 |
| `cases/spec/09-macros/load_string_keeps_nul` | §9 |
| `cases/spec/09-macros/load_target_case_insensitive` | §9 |
| `cases/spec/09-macros/load_target_param_ends_at_nul` | §9 |
| `cases/spec/09-macros/load_then_same_key_appends` | §9 |
| `cases/spec/09-macros/load_trim` | §9 |
| `cases/spec/09-macros/load_trim_all_whitespace` | §9 |
| `cases/spec/09-macros/load_trim_then_escape` | §9 |
| `cases/spec/09-macros/load_trim_whitespace_only_empty_string` | §9 |
| `cases/spec/09-macros/load_try_missing` | §9 |
| `cases/spec/09-macros/load_try_missing_before_existing_key` | §9 |
| `cases/spec/09-macros/load_try_path_nul_first_skipped` | §9 |
| `cases/spec/09-macros/load_twice_same_key_error` | §9 |
| `cases/spec/09-macros/load_unknown_target_inserts_nothing` | §9 |
| `cases/spec/09-macros/load_without_key_error` | §9 |
| `cases/spec/09-macros/macro_after_entry_last_byte_hash_error` | §9 |
| `cases/spec/09-macros/macro_after_name_block_comment_reopens` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_closed_by_brace_no_reopen` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_comment_after_blank_lines_reopens` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_comment_to_end` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_comment_to_end_after_bracket_closed` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_comment_to_end_of_included_file` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_ignored_name_at_end_no_reopen` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_include_of_empty_file_comment_to_end` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_later_macro_next_line_reopens` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_later_macro_same_line_reopens` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_later_macro_then_newline_no_reopen` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_next_line_separator_key_error` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_nothing_after_no_reopen` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_priority_then_separator_key_error` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_reopen_end_check_stops` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_reopened_object_belongs_to_file_error` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_reopens_deeper_value` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_reopens_discarded_value` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_run_ignores_included_keys_error` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_run_survives_closing_brace_error` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_run_survives_closing_brace_name` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_run_survives_closing_bracket_error` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_run_survives_macro_error` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_semicolon_then_comment_reopens` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_separator_key_scalar_error` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_separator_key_then_brace_error` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_separator_key_then_bracket_key` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_separator_keys_chain` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_then_bracketed_value_closes` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_then_key_without_separator` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_then_left_open_path` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_then_separator_key_is_name_error` | §9, §3 |
| `cases/spec/09-macros/macro_after_name_whitespace_to_end_no_reopen` | §9, §3 |
| `cases/spec/09-macros/macro_after_separator_newline_name_starts_run_error` | §9, §3 |
| `cases/spec/09-macros/macro_args_after_comment_then_space_error` | §9 |
| `cases/spec/09-macros/macro_args_array_root_ignored` | §9 |
| `cases/spec/09-macros/macro_args_backslash_pair_quote_error` | §9 |
| `cases/spec/09-macros/macro_args_backslash_quote_opens_quoted_part` | §9 |
| `cases/spec/09-macros/macro_args_boolean_keywords` | §9 |
| `cases/spec/09-macros/macro_args_braced_root` | §9 |
| `cases/spec/09-macros/macro_args_braced_root_rest_ignored` | §9 |
| `cases/spec/09-macros/macro_args_comment_then_last_byte_hash_error` | §9 |
| `cases/spec/09-macros/macro_args_comments_not_saved` | §9, §12 |
| `cases/spec/09-macros/macro_args_directly_after_comment` | §9 |
| `cases/spec/09-macros/macro_args_escaped_quote_error` | §9 |
| `cases/spec/09-macros/macro_args_filename_is_undef` | §9, §7 |
| `cases/spec/09-macros/macro_args_filename_no_filevars` | §9, §7 |
| `cases/spec/09-macros/macro_args_include_inside` | §9 |
| `cases/spec/09-macros/macro_args_inherit_inside` | §9 |
| `cases/spec/09-macros/macro_args_key_lowercase` | §9 |
| `cases/spec/09-macros/macro_args_macros_inside` | §9 |
| `cases/spec/09-macros/macro_args_names_case_sensitive` | §9 |
| `cases/spec/09-macros/macro_args_nested_100_levels` | §9 |
| `cases/spec/09-macros/macro_args_nested_60_levels` | §9 |
| `cases/spec/09-macros/macro_args_nested_not_parameters` | §9 |
| `cases/spec/09-macros/macro_args_nested_rejected_dropped` | §9 |
| `cases/spec/09-macros/macro_args_nested_rejected_in_included_file` | §9 |
| `cases/spec/09-macros/macro_args_nested_rejected_no_value_error` | §9 |
| `cases/spec/09-macros/macro_args_nested_rejected_space_before_value_error` | §9 |
| `cases/spec/09-macros/macro_args_nested_rejected_value_after_paren` | §9 |
| `cases/spec/09-macros/macro_args_nested_stop_dropped` | §9 |
| `cases/spec/09-macros/macro_args_nested_unknown_macro_dropped` | §9 |
| `cases/spec/09-macros/macro_args_newline_then_end_error` | §9 |
| `cases/spec/09-macros/macro_args_newline_then_last_byte_hash_empty_value` | §9 |
| `cases/spec/09-macros/macro_args_no_implicit_arrays_repeated_name_error` | §9, §8 |
| `cases/spec/09-macros/macro_args_no_variables` | §9 |
| `cases/spec/09-macros/macro_args_paren_in_quotes` | §9 |
| `cases/spec/09-macros/macro_args_parse_error` | §9 |
| `cases/spec/09-macros/macro_args_registered_variables_unavailable_error` | §9 |
| `cases/spec/09-macros/macro_args_repeated_first_wins` | §9 |
| `cases/spec/09-macros/macro_args_single_quotes_do_not_protect_error` | §9 |
| `cases/spec/09-macros/macro_args_space_then_last_byte_hash_empty_value` | §9 |
| `cases/spec/09-macros/macro_args_stop_inside_error` | §9 |
| `cases/spec/09-macros/macro_args_then_comment_then_space_value_try` | §9 |
| `cases/spec/09-macros/macro_args_then_end_error` | §9 |
| `cases/spec/09-macros/macro_args_then_last_byte_hash_empty_value` | §9 |
| `cases/spec/09-macros/macro_args_then_last_byte_hash_error` | §9 |
| `cases/spec/09-macros/macro_args_two_prefixes_last_wins` | §9 |
| `cases/spec/09-macros/macro_args_unknown_macro_inside_error` | §9 |
| `cases/spec/09-macros/macro_bare_value` | §9 |
| `cases/spec/09-macros/macro_bare_value_trailing_space_kept_error` | §9 |
| `cases/spec/09-macros/macro_braces_value_leading_newline_skipped` | §9 |
| `cases/spec/09-macros/macro_braces_value_trailing_space_kept_error` | §9 |
| `cases/spec/09-macros/macro_braces_value_unterminated_error` | §9 |
| `cases/spec/09-macros/macro_comment_directly_before_value` | §9 |
| `cases/spec/09-macros/macro_comment_then_space_before_value_error` | §9 |
| `cases/spec/09-macros/macro_dot_at_end_ignored` | §9 |
| `cases/spec/09-macros/macro_in_object_inside_array` | §9 |
| `cases/spec/09-macros/macro_known_name_then_block_comment_at_end_ignored` | §9 |
| `cases/spec/09-macros/macro_known_name_then_block_comment_on_next_line_error` | §9 |
| `cases/spec/09-macros/macro_known_name_then_comment_and_last_hash_ignored` | §9 |
| `cases/spec/09-macros/macro_known_name_then_comment_at_end_ignored` | §9 |
| `cases/spec/09-macros/macro_known_name_then_comment_group_at_end_ignored` | §9 |
| `cases/spec/09-macros/macro_known_name_then_comment_then_blank_line_error` | §9 |
| `cases/spec/09-macros/macro_known_name_then_comment_then_space_error` | §9 |
| `cases/spec/09-macros/macro_known_name_then_last_byte_hash_error` | §9 |
| `cases/spec/09-macros/macro_known_name_then_whitespace_at_end_ignored` | §9 |
| `cases/spec/09-macros/macro_line_comment_then_indented_value_error` | §9 |
| `cases/spec/09-macros/macro_line_comment_then_value` | §9 |
| `cases/spec/09-macros/macro_name_at_end_after_quoted_key_name` | §9 |
| `cases/spec/09-macros/macro_name_at_end_after_section_path` | §9 |
| `cases/spec/09-macros/macro_name_at_end_ignored` | §9 |
| `cases/spec/09-macros/macro_name_at_end_inside_braces_error` | §9 |
| `cases/spec/09-macros/macro_name_case_sensitive_error` | §9 |
| `cases/spec/09-macros/macro_name_includes_semicolon_error` | §9 |
| `cases/spec/09-macros/macro_name_runs_to_whitespace_or_paren` | §9 |
| `cases/spec/09-macros/macro_name_then_last_byte_hash_empty_value` | §9 |
| `cases/spec/09-macros/macro_name_with_punctuation_at_end_ignored` | §9 |
| `cases/spec/09-macros/macro_not_recognised_as_value` | §9 |
| `cases/spec/09-macros/macro_not_recognised_in_arrays` | §9 |
| `cases/spec/09-macros/macro_paren_last_byte_is_value` | §9 |
| `cases/spec/09-macros/macro_paren_last_byte_priority_error` | §9 |
| `cases/spec/09-macros/macro_paren_then_space_unbalanced_error` | §9 |
| `cases/spec/09-macros/macro_quoted_value_bad_unicode_error` | §9 |
| `cases/spec/09-macros/macro_quoted_value_control_byte_error` | §9 |
| `cases/spec/09-macros/macro_quoted_value_not_unescaped` | §9 |
| `cases/spec/09-macros/macro_quoted_value_unterminated_error` | §9 |
| `cases/spec/09-macros/macro_space_after_dot_error` | §9 |
| `cases/spec/09-macros/macro_unbalanced_args_error` | §9 |
| `cases/spec/09-macros/macro_unknown_name_then_newline_error` | §9 |
| `cases/spec/09-macros/macro_value_nul_ends_include_path` | §9 |
| `cases/spec/09-macros/macro_value_nul_ends_load_path` | §9 |
| `cases/spec/09-macros/macro_value_nul_ends_priority` | §9 |
| `cases/spec/09-macros/macro_value_nul_first_priority_zero` | §9 |
| `cases/spec/09-macros/macro_value_nul_first_priority_zero_wins_over_args` | §9 |
| `cases/spec/09-macros/macro_value_nul_kept_by_inherit_error` | §9 |
| `cases/spec/09-macros/macro_value_then_comma_error` | §9 |
| `cases/spec/09-macros/macro_value_then_entry_without_space` | §9 |
| `cases/spec/09-macros/macro_value_then_last_byte_hash_error` | §9 |
| `cases/spec/09-macros/macro_value_then_newline_last_byte_hash_error` | §9 |
| `cases/spec/09-macros/macro_value_then_semicolons_and_comment` | §9 |
| `cases/spec/09-macros/macro_value_variables` | §9 |
| `cases/spec/09-macros/macro_word_after_section_name_error` | §9, §3 |
| `cases/spec/09-macros/macro_word_after_section_name_ignored` | §9, §3 |
| `cases/spec/09-macros/main_braced_root_first_name_has_no_brace_error` | §9 |
| `cases/spec/09-macros/no_filevars_include_defines_them` | §9, §12 |
| `cases/spec/09-macros/no_filevars_included_file_has_curdir` | §9, §12 |
| `cases/spec/09-macros/priority_args_must_be_integer_error` | §9 |
| `cases/spec/09-macros/priority_args_only` | §9 |
| `cases/spec/09-macros/priority_args_used_for_empty_value` | §9 |
| `cases/spec/09-macros/priority_braces_value` | §9 |
| `cases/spec/09-macros/priority_braces_value_trailing_space_error` | §9 |
| `cases/spec/09-macros/priority_comma_after_value_error` | §9 |
| `cases/spec/09-macros/priority_comment_then_space_value` | §9 |
| `cases/spec/09-macros/priority_continues_after_object` | §9 |
| `cases/spec/09-macros/priority_empty_args_error` | §9 |
| `cases/spec/09-macros/priority_empty_quoted_value_error` | §9 |
| `cases/spec/09-macros/priority_forms` | §9 |
| `cases/spec/09-macros/priority_hex_error` | §9 |
| `cases/spec/09-macros/priority_in_braces_trailing_space_error` | §9 |
| `cases/spec/09-macros/priority_invalid_error` | §9 |
| `cases/spec/09-macros/priority_leading_zeros_decimal` | §9 |
| `cases/spec/09-macros/priority_missing_value_error` | §9 |
| `cases/spec/09-macros/priority_quoted_leading_space_and_sign` | §9 |
| `cases/spec/09-macros/priority_trailing_space_before_semicolon_error` | §9 |
| `cases/spec/09-macros/priority_trailing_space_error` | §9 |
| `cases/spec/09-macros/priority_value_on_next_line_error` | §9 |
| `cases/spec/09-macros/priority_value_range` | §9 |
| `cases/spec/09-macros/priority_variable_in_value` | §9 |
| `cases/spec/09-macros/try_include_cycle_nesting_limit_error` | §9 |
| `cases/spec/09-macros/try_include_directory_stops_parsing` | §9 |
| `cases/spec/09-macros/try_include_empty_path_stops_parsing` | §9 |
| `cases/spec/09-macros/try_include_glob_no_match_continues` | §9 |
| `cases/spec/09-macros/try_include_glob_skips_directories` | §9 |
| `cases/spec/09-macros/try_include_glob_skips_including_file` | §9 |
| `cases/spec/09-macros/try_include_glob_try_false_directory_error` | §9 |
| `cases/spec/09-macros/try_include_glob_try_false_no_match_stops` | §9 |
| `cases/spec/09-macros/try_include_glob_try_false_only_self_error` | §9 |
| `cases/spec/09-macros/try_include_glob_try_false_skips_self` | §9 |
| `cases/spec/09-macros/try_include_main_document_itself_stops` | §9 |
| `cases/spec/09-macros/try_include_missing_stops_parsing` | §9 |
| `cases/spec/09-macros/try_include_path_missing_error` | §9 |
| `cases/spec/09-macros/try_include_path_searches_all_dirs` | §9 |
| `cases/spec/09-macros/try_include_present` | §9 |
| `cases/spec/09-macros/try_include_self_stops_parsing` | §9 |
| `cases/spec/09-macros/try_include_try_false_directory_error` | §9 |
| `cases/spec/09-macros/try_include_try_false_missing_stops` | §9 |
| `cases/spec/09-macros/try_include_try_false_self_stops` | §9 |
| `cases/spec/09-macros/try_include_try_true_directory_stops` | §9 |
| `cases/spec/09-macros/try_include_url_skipped` | §9 |
| `cases/spec/09-macros/try_include_url_try_false_error` | §9 |
| `cases/spec/09-macros/unknown_macro_error` | §9 |
| `cases/spec/09-macros/zero_byte_file_moves_filevars` | §9, §7 |
| `pending/09-macros/comments_include_first_name_in_section_object_after_text` | §9, §12, §13 |
| `pending/09-macros/comments_include_nested_braced_before_first_name` | §9, §12 |
| `pending/09-macros/comments_include_nested_text_before_first_name` | §9, §12, §13 |
| `pending/09-macros/include_braced_file_nested_braced_before_first_name` | §9, §3 |
| `pending/09-macros/include_braced_file_nested_braced_then_entry` | §9 |
| `pending/09-macros/include_braced_file_nested_close_brace_before_first_name` | §9, §3 |
| `pending/09-macros/include_braced_file_nested_path_before_first_name` | §9, §3 |
| `pending/09-macros/include_braced_file_nested_path_one_brace_closed_by_includer` | §9, §3 |
| `pending/09-macros/include_braced_file_nested_path_then_braces` | §9, §3 |
| `pending/09-macros/include_braced_file_nested_path_then_entry` | §9, §3 |
| `pending/09-macros/include_braced_file_nested_text_before_first_name` | §9, §3, §13 |
| `pending/09-macros/include_key_nested_close_brace_before_first_name` | §9, §3 |
| `pending/09-macros/include_path_first_miss_load_try_after_each_miss_accepts` | §9 |
| `pending/09-macros/include_path_first_miss_then_load_try_accepts_later` | §9 |
| `pending/09-macros/include_path_first_miss_then_load_try_accepts_later_file_input` | §9 |
| `pending/09-macros/include_path_first_miss_then_load_try_directory_accepts_later` | §9 |
| `pending/09-macros/include_path_first_miss_url_then_load_try_accepts` | §9 |
| `pending/09-macros/include_path_two_first_misses_then_load_try_accepts` | §9 |
| `cases/spec/10-output/arrays` | §10 |
| `cases/spec/10-output/comments_output_array_elements` | §10, §12 |
| `cases/spec/10-output/comments_output_before_and_after` | §10, §12 |
| `cases/spec/10-output/comments_output_block_and_cr` | §10, §12 |
| `cases/spec/10-output/comments_output_multi_value` | §10, §12 |
| `cases/spec/10-output/comments_output_nested_after` | §10, §12 |
| `cases/spec/10-output/comments_output_root_only` | §10, §12 |
| `cases/spec/10-output/comments_output_save_comments_flag` | §10, §12 |
| `cases/spec/10-output/comments_output_top_level_array` | §10, §12 |
| `cases/spec/10-output/empty_document` | §10 |
| `cases/spec/10-output/empty_key` | §10, §9 |
| `cases/spec/10-output/floats` | §10 |
| `cases/spec/10-output/floats_boundaries` | §10 |
| `cases/spec/10-output/floats_exact_decimal_expansion` | §10 |
| `cases/spec/10-output/floats_special` | §10 |
| `cases/spec/10-output/heredoc_first_line_eod` | §10 |
| `cases/spec/10-output/heredoc_kept` | §10 |
| `cases/spec/10-output/heredoc_with_eod_line` | §10 |
| `cases/spec/10-output/implicit_array_layouts` | §10 |
| `cases/spec/10-output/implicit_array_of_arrays` | §10 |
| `cases/spec/10-output/implicit_arrays` | §10 |
| `cases/spec/10-output/include_target_array_converted_key_bare` | §10 |
| `cases/spec/10-output/include_target_array_converted_key_spelling` | §10 |
| `cases/spec/10-output/inherit_copies_keep_output_facts` | §10, §9 |
| `cases/spec/10-output/integers_large` | §10 |
| `cases/spec/10-output/keys_quoting` | §10 |
| `cases/spec/10-output/load_multiline_output` | §10 |
| `cases/spec/10-output/long_string_boundary` | §10 |
| `cases/spec/10-output/long_string_with_newline` | §10 |
| `cases/spec/10-output/macro_created_keys_quoting` | §10, §9 |
| `cases/spec/10-output/multi_value_first_entry_at_root` | §10 |
| `cases/spec/10-output/multi_value_inline_first_kinds` | §10 |
| `cases/spec/10-output/multi_value_key_quoting_per_value` | §10 |
| `cases/spec/10-output/multi_value_key_spelling_per_value` | §10, §12 |
| `cases/spec/10-output/multi_value_merge_quirk_layout_empty` | §10 |
| `cases/spec/10-output/multi_value_merge_quirk_layout_inherited` | §10 |
| `cases/spec/10-output/multi_value_merge_quirk_layout_nan_inf` | §10 |
| `cases/spec/10-output/multi_value_merge_quirk_layout_nonempty` | §10 |
| `cases/spec/10-output/multi_value_merge_quirk_layout_own_kind` | §10 |
| `cases/spec/10-output/multi_value_mixed_values` | §10 |
| `cases/spec/10-output/nested_implicit_arrays` | §10 |
| `cases/spec/10-output/nested_layout` | §10 |
| `cases/spec/10-output/no_implicit_arrays_collection_key_bare` | §10 |
| `cases/spec/10-output/no_implicit_arrays_collection_key_first_spelling` | §10 |
| `cases/spec/10-output/objects` | §10 |
| `cases/spec/10-output/priorities_not_emitted` | §10 |
| `cases/spec/10-output/priority_not_emitted` | §10 |
| `cases/spec/10-output/readback_config_output` | §10 |
| `cases/spec/10-output/readback_fifteen_digit_min_normal_error` | §10, §5 |
| `cases/spec/10-output/readback_fifteen_digit_subnormal_error` | §10, §5 |
| `cases/spec/10-output/readback_float_above_max_error` | §10, §5 |
| `cases/spec/10-output/readback_float_forms` | §10, §5 |
| `cases/spec/10-output/readback_json_output` | §10 |
| `cases/spec/10-output/readback_minus_infinity_is_string` | §10, §4 |
| `cases/spec/10-output/readback_percent_f_length_limit` | §10, §5 |
| `cases/spec/10-output/readback_single_quoted_backslash_runs` | §6, §10 |
| `cases/spec/10-output/readback_time_forms` | §10, §5 |
| `cases/spec/10-output/readback_yaml_output` | §10 |
| `cases/spec/10-output/scalars` | §10 |
| `cases/spec/10-output/single_quoted_kept` | §10 |
| `cases/spec/10-output/single_quoted_raw_bytes` | §10 |
| `cases/spec/10-output/string_control_chars` | §10 |
| `cases/spec/10-output/string_escapes` | §10 |
| `cases/spec/10-output/string_utf8` | §10 |
| `cases/spec/10-output/strings_every_control_byte` | §10 |
| `cases/spec/10-output/times` | §10 |
| `cases/spec/10-output/top_level_array` | §10 |
| `cases/spec/10-output/top_level_array_empty` | §10 |
| `cases/spec/10-output/top_level_array_nested` | §10 |
| `cases/spec/11-errors/depth_limit_error` | §11 |
| `cases/spec/11-errors/garbage_after_value_error` | §11 |
| `cases/spec/11-errors/lone_bracket_error` | §11 |
| `cases/spec/11-errors/object_depth_limit_error` | §11 |
| `cases/spec/11-errors/unexpected_close_top_error` | §11 |
| `cases/spec/12-flags/comments_after_braced_root_ignored` | §12 |
| `cases/spec/12-flags/comments_after_joins_before_list_in_array` | §12 |
| `cases/spec/12-flags/comments_attach_to_next_value` | §12 |
| `cases/spec/12-flags/comments_before_joins_after_list_under_merge` | §12 |
| `cases/spec/12-flags/comments_before_macro_go_to_next_value` | §12, §9 |
| `cases/spec/12-flags/comments_block_comment_saved_with_next_byte` | §12 |
| `cases/spec/12-flags/comments_collected_array_keeps_comments` | §12 |
| `cases/spec/12-flags/comments_last_byte_hash_after_block_comment_saved` | §12 |
| `cases/spec/12-flags/comments_last_byte_hash_after_comment_saved` | §12 |
| `cases/spec/12-flags/comments_last_byte_hash_after_value_not_saved` | §12 |
| `cases/spec/12-flags/comments_last_byte_hash_alone_saved` | §12 |
| `cases/spec/12-flags/comments_last_byte_hash_for_value_on_next_line_saved` | §12 |
| `cases/spec/12-flags/comments_later_comments_join_first_list` | §12 |
| `cases/spec/12-flags/comments_left_open_sections_at_end` | §12 |
| `cases/spec/12-flags/comments_left_open_sections_closed_by_bracket` | §12 |
| `cases/spec/12-flags/comments_line_comment_keeps_cr` | §12 |
| `cases/spec/12-flags/comments_merge_after_comment_on_existing_object` | §12 |
| `cases/spec/12-flags/comments_merge_joins_existing_object_list` | §12 |
| `cases/spec/12-flags/comments_merge_scalar_keeps_container_comments` | §12 |
| `cases/spec/12-flags/comments_only_attach_to_root` | §12 |
| `cases/spec/12-flags/comments_repeated_key` | §12 |
| `cases/spec/12-flags/comments_rewrite_drops_replaced_value_comments` | §12 |
| `cases/spec/12-flags/comments_section_path_close` | §12 |
| `cases/spec/12-flags/comments_trailing_at_container_close` | §12 |
| `cases/spec/12-flags/comments_trailing_at_end` | §12 |
| `cases/spec/12-flags/comments_value_on_next_line_at_end` | §12 |
| `cases/spec/12-flags/comments_value_on_next_line_goes_to_next_value` | §12 |
| `cases/spec/12-flags/comments_value_on_next_line_joins_key_list` | §12 |
| `cases/spec/12-flags/comments_value_on_next_line_object` | §12 |
| `cases/spec/12-flags/disable_macro_name_at_end_error` | §12, §9 |
| `cases/spec/12-flags/disable_macro_no_variables` | §12 |
| `cases/spec/12-flags/disable_macro_priority_error` | §12 |
| `cases/spec/12-flags/disable_macro_rejects_macros` | §12 |
| `cases/spec/12-flags/key_lowercase` | §12 |
| `cases/spec/12-flags/key_lowercase_before_escapes` | §12, §3 |
| `cases/spec/12-flags/key_lowercase_collected_keeps_spelling` | §12 |
| `cases/spec/12-flags/key_lowercase_collection_replaced_takes_new_spelling` | §12 |
| `cases/spec/12-flags/key_lowercase_decoded_key_first_spelling_kept` | §12 |
| `cases/spec/12-flags/key_lowercase_escapes_checked_as_written_error` | §12 |
| `cases/spec/12-flags/key_lowercase_inherited_replaced_takes_new_spelling` | §12, §9 |
| `cases/spec/12-flags/key_lowercase_later_decoded_key_joins` | §12 |
| `cases/spec/12-flags/key_lowercase_lower_priority_keeps_spelling` | §12 |
| `cases/spec/12-flags/key_lowercase_merge_higher_priority_takes_new_spelling` | §12 |
| `cases/spec/12-flags/key_lowercase_merge_scalar_quirk_keeps_spelling` | §12 |
| `cases/spec/12-flags/key_lowercase_merged_keeps_spelling` | §12 |
| `cases/spec/12-flags/key_lowercase_merges_case` | §12 |
| `cases/spec/12-flags/key_lowercase_non_ascii` | §12 |
| `cases/spec/12-flags/key_lowercase_replacement_by_lowercase_spelling` | §12 |
| `cases/spec/12-flags/key_lowercase_replacement_takes_new_spelling` | §12 |
| `cases/spec/12-flags/key_lowercase_rewrite_takes_new_spelling` | §12 |
| `cases/spec/12-flags/key_lowercase_upper_u_escape_decoded_as_unquoted` | §12 |
| `cases/spec/12-flags/key_lowercase_upper_u_escape_without_flag` | §12 |
| `cases/spec/12-flags/no_filevars` | §12 |
| `cases/spec/12-flags/no_time` | §12 |
| `cases/spec/12-flags/save_comments_no_effect_on_values` | §12 |
| `cases/spec/12-flags/zerocopy_chunk_input` | §12, §13 |
| `cases/spec/12-flags/zerocopy_emit_repeated_key_in_text_error` | §12, §13, §8 |
| `cases/spec/12-flags/zerocopy_include_comment_only_file` | §12, §9 |
| `cases/spec/12-flags/zerocopy_include_empty_file` | §12, §9 |
| `cases/spec/12-flags/zerocopy_no_effect` | §12 |
| `cases/spec/12-flags/zerocopy_registered_macros_stable` | §12, §13 |
| `cases/spec/13-inputs/inputs_array_root_then_array_error` | §13 |
| `cases/spec/13-inputs/inputs_array_root_then_entries_error` | §13 |
| `cases/spec/13-inputs/inputs_braced_root_then_braced_root_error` | §13 |
| `cases/spec/13-inputs/inputs_braced_root_then_entries_error` | §13 |
| `cases/spec/13-inputs/inputs_closing_brace_in_later_input_error` | §13 |
| `cases/spec/13-inputs/inputs_comment_first_then_entries` | §13 |
| `cases/spec/13-inputs/inputs_comment_only_input_attaches_to_root` | §13 |
| `cases/spec/13-inputs/inputs_comments_stay_in_their_input` | §13 |
| `cases/spec/13-inputs/inputs_container_at_end` | §13 |
| `cases/spec/13-inputs/inputs_empty_first_then_array_error` | §13 |
| `cases/spec/13-inputs/inputs_empty_first_then_comment_error` | §13 |
| `cases/spec/13-inputs/inputs_empty_first_then_comment_only` | §13 |
| `cases/spec/13-inputs/inputs_empty_first_then_entries_error` | §13 |
| `cases/spec/13-inputs/inputs_empty_first_then_whitespace` | §13 |
| `cases/spec/13-inputs/inputs_empty_inputs_only` | §13 |
| `cases/spec/13-inputs/inputs_empty_later_input` | §13 |
| `cases/spec/13-inputs/inputs_entries_combine` | §13 |
| `cases/spec/13-inputs/inputs_entries_then_array_root_error` | §13 |
| `cases/spec/13-inputs/inputs_entries_then_braced_root_error` | §13 |
| `cases/spec/13-inputs/inputs_entry_split_across_inputs_error` | §13 |
| `cases/spec/13-inputs/inputs_error_in_first_input` | §13 |
| `cases/spec/13-inputs/inputs_error_in_later_input` | §13 |
| `cases/spec/13-inputs/inputs_heredoc_at_end` | §13 |
| `cases/spec/13-inputs/inputs_include_depth_shared_error` | §13 |
| `cases/spec/13-inputs/inputs_include_depth_shared_ok` | §13 |
| `cases/spec/13-inputs/inputs_include_resolves_from_working_directory` | §13 |
| `cases/spec/13-inputs/inputs_incomplete_entry_in_last_input_error` | §13 |
| `cases/spec/13-inputs/inputs_inherit_from_earlier_input` | §13 |
| `cases/spec/13-inputs/inputs_key_split_across_inputs_error` | §13 |
| `cases/spec/13-inputs/inputs_no_implicit_arrays_across` | §13 |
| `cases/spec/13-inputs/inputs_objects_append` | §13 |
| `cases/spec/13-inputs/inputs_objects_merge` | §13 |
| `cases/spec/13-inputs/inputs_priority_low_four_bits` | §13 |
| `cases/spec/13-inputs/inputs_priority_macro_in_later_input` | §13 |
| `cases/spec/13-inputs/inputs_priority_macro_stays_in_its_input` | §13 |
| `cases/spec/13-inputs/inputs_priority_per_input` | §13 |
| `cases/spec/13-inputs/inputs_quoted_value_at_end_error` | §13 |
| `cases/spec/13-inputs/inputs_registered_macro_in_later_input` | §13 |
| `cases/spec/13-inputs/inputs_section_object_left_open` | §13 |
| `cases/spec/13-inputs/inputs_section_object_left_open_bracket_error` | §13 |
| `cases/spec/13-inputs/inputs_seventeen_inputs_error` | §13 |
| `cases/spec/13-inputs/inputs_sixteen_inputs` | §13 |
| `cases/spec/13-inputs/inputs_stop_in_included_file_then_later_input` | §13 |
| `cases/spec/13-inputs/inputs_stop_in_later_input` | §13 |
| `cases/spec/13-inputs/inputs_stop_inside_object_next_input_fills_it` | §13 |
| `cases/spec/13-inputs/inputs_stop_inside_object_next_input_unclosed_error` | §13 |
| `cases/spec/13-inputs/inputs_stop_then_later_inputs` | §13 |
| `cases/spec/13-inputs/inputs_strategy_first_input_only` | §13 |
| `cases/spec/13-inputs/inputs_strategy_per_input_error` | §13 |
| `cases/spec/13-inputs/inputs_strategy_per_input_rewrite` | §13 |
| `cases/spec/13-inputs/inputs_unclosed_brace_error` | §13 |
| `cases/spec/13-inputs/inputs_value_at_end_then_comment` | §13 |
| `cases/spec/13-inputs/inputs_value_at_end_then_entry_error` | §13 |
| `cases/spec/13-inputs/inputs_value_at_end_then_line_break` | §13 |
| `cases/spec/13-inputs/inputs_value_at_end_then_semicolon` | §13 |
| `cases/spec/13-inputs/inputs_value_then_comma_at_end` | §13 |
| `cases/spec/13-inputs/inputs_value_then_comment_at_end` | §13 |
| `cases/spec/13-inputs/inputs_value_then_semicolon_at_end` | §13 |
| `cases/spec/13-inputs/inputs_value_then_space_at_end_error` | §13 |
| `cases/spec/13-inputs/inputs_variables_in_every_input` | §13 |
| `cases/spec/13-inputs/inputs_whitespace_first_then_entries` | §13 |
| `cases/spec/13-inputs/joins_array_root_then_comma_rest_ignored` | §13, §1 |
| `cases/spec/13-inputs/joins_array_root_then_line_break_rest_ignored` | §13, §1 |
| `cases/spec/13-inputs/joins_bare_value_trailing_space_error` | §13 |
| `cases/spec/13-inputs/joins_closed_root_then_brace_error` | §13, §1 |
| `cases/spec/13-inputs/joins_closed_root_then_comment_brace_error` | §13, §1, §12 |
| `cases/spec/13-inputs/joins_closed_root_then_comment_rest_ignored` | §13, §1, §12 |
| `cases/spec/13-inputs/joins_closed_root_then_line_break_brace_error` | §13, §1 |
| `cases/spec/13-inputs/joins_closed_root_then_line_break_rest_ignored` | §13, §1 |
| `cases/spec/13-inputs/joins_closed_root_then_semicolon_bracket_error` | §13, §1 |
| `cases/spec/13-inputs/joins_closed_root_then_semicolon_rest_ignored` | §13, §1 |
| `cases/spec/13-inputs/joins_closed_root_then_space_entry_error` | §13, §1 |
| `cases/spec/13-inputs/joins_closed_root_then_spaces_only` | §13, §1 |
| `cases/spec/13-inputs/joins_closed_root_then_vertical_tab_after_line_break` | §13, §1 |
| `cases/spec/13-inputs/joins_closed_root_then_vertical_tab_error` | §13, §1 |
| `cases/spec/13-inputs/joins_comment_input_attaches_to_earlier_value` | §13, §12 |
| `cases/spec/13-inputs/joins_comment_then_hash_last_byte` | §13, §2, §12 |
| `cases/spec/13-inputs/joins_comments_at_stop_attach_in_next_input` | §13, §9, §12 |
| `cases/spec/13-inputs/joins_comments_at_stop_then_empty_input` | §13, §9, §12 |
| `cases/spec/13-inputs/joins_comments_at_stop_then_whitespace_input` | §13, §9, §12 |
| `cases/spec/13-inputs/joins_empty_first_then_hash_attaches_to_root` | §13, §2 |
| `cases/spec/13-inputs/joins_empty_first_then_semicolon_error` | §13 |
| `cases/spec/13-inputs/joins_file_input_file_variables_after_registered` | §13, §7 |
| `cases/spec/13-inputs/joins_hash_last_byte_of_later_input_error` | §13, §2 |
| `cases/spec/13-inputs/joins_key_waiting_comment_after_key` | §13, §1, §12 |
| `cases/spec/13-inputs/joins_key_waiting_comment_in_value_input` | §13, §1, §12 |
| `cases/spec/13-inputs/joins_key_waiting_comments_before_key` | §13, §1, §12 |
| `cases/spec/13-inputs/joins_key_waiting_then_comment_input` | §13, §1, §12 |
| `cases/spec/13-inputs/joins_key_waiting_then_hash_input_error` | §13, §1, §2 |
| `cases/spec/13-inputs/joins_key_waiting_then_hash_line_value` | §13, §1, §2 |
| `cases/spec/13-inputs/joins_key_waiting_then_space_hash` | §13, §1, §2 |
| `cases/spec/13-inputs/joins_key_waiting_two_comment_groups` | §13, §1, §12 |
| `cases/spec/13-inputs/joins_line_break_hash_last_byte_error` | §13, §2 |
| `cases/spec/13-inputs/joins_line_break_input_needs_separator_error` | §13 |
| `cases/spec/13-inputs/joins_quoted_value_trailing_space` | §13 |
| `cases/spec/13-inputs/joins_same_file_from_two_text_inputs` | §13 |
| `cases/spec/13-inputs/joins_section_names_then_comment_input` | §13, §3, §12 |
| `cases/spec/13-inputs/joins_section_names_then_entry_error` | §13, §3 |
| `cases/spec/13-inputs/joins_section_names_then_semicolon_input` | §13, §3 |
| `cases/spec/13-inputs/joins_section_names_then_whitespace_input` | §13, §3 |
| `cases/spec/13-inputs/joins_single_quoted_value_trailing_tab` | §13 |
| `cases/spec/13-inputs/joins_space_hash_last_byte_error` | §13, §2 |
| `cases/spec/13-inputs/joins_stopped_include_again_is_self_inclusion_error` | §13, §9 |
| `cases/spec/13-inputs/joins_stopped_include_next_input_read` | §13, §9 |
| `cases/spec/13-inputs/joins_stopped_include_stays_open_fifteen_inputs_error` | §13, §9 |
| `cases/spec/13-inputs/joins_stopped_include_stays_open_fourteen_inputs` | §13, §9 |
| `cases/spec/13-inputs/joins_text_input_continues_file_input_self_include_error` | §13, §9 |
| `cases/spec/13-inputs/joins_time_value_trailing_space_error` | §13 |
| `cases/spec/13-inputs/joins_two_whitespace_inputs_cancel` | §13 |
| `cases/spec/13-inputs/joins_value_after_empty_and_comment_inputs` | §13, §1, §12 |
| `cases/spec/13-inputs/joins_value_at_end_empty_input_error` | §13 |
| `cases/spec/13-inputs/joins_value_at_end_then_closing_brace` | §13 |
| `cases/spec/13-inputs/joins_value_at_end_then_form_feed_error` | §13 |
| `cases/spec/13-inputs/joins_value_at_end_then_hash` | §13, §2 |
| `cases/spec/13-inputs/joins_value_at_end_then_nul` | §13 |
| `cases/spec/13-inputs/joins_value_at_end_then_vertical_tab_error` | §13 |
| `cases/spec/13-inputs/joins_value_at_end_then_whitespace_input` | §13 |
| `cases/spec/13-inputs/joins_value_at_end_two_whitespace_inputs_error` | §13 |
| `cases/spec/13-inputs/joins_value_in_next_input` | §13, §1 |
| `cases/spec/13-inputs/joins_value_in_next_input_comments_wait` | §13, §1, §12 |
| `cases/spec/13-inputs/joins_value_in_next_input_key_priority` | §13, §1 |
| `cases/spec/13-inputs/joins_value_in_next_input_key_strategy` | §13, §1 |
| `cases/spec/13-inputs/joins_value_in_next_input_whole_line` | §13, §1 |
| `cases/spec/13-inputs/joins_whitespace_first_input_not_counted` | §13 |
| `cases/spec/13-inputs/joins_whitespace_first_then_whitespace_error` | §13 |
| `cases/spec/13-inputs/joins_whitespace_input_needs_separator_error` | §13 |
| `cases/spec/13-inputs/joins_whitespace_input_then_semicolon` | §13 |
| `cases/spec/13-inputs/macro_registered_added_values_take_no_comments` | §13, §9, §12 |
| `cases/spec/13-inputs/macro_registered_after_section_name_error` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_args_backslash_quote_opens_quoted_part` | §9, §13 |
| `cases/spec/13-inputs/macro_registered_arguments` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_arguments_key_lowercase` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_array_arguments` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_at_end_ignored` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_bare_value` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_braced_value` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_context_in_included_file` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_context_is_root` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_ctx_copies_first_container_value` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_ctx_copy_keeps_root_priority` | §13 |
| `cases/spec/13-inputs/macro_registered_ctx_copy_priority_not_from_priority_macro` | §13 |
| `cases/spec/13-inputs/macro_registered_ctx_copy_priority_of_first_input` | §13 |
| `cases/spec/13-inputs/macro_registered_ctx_keeps_collection_key` | §8, §13 |
| `cases/spec/13-inputs/macro_registered_disable_macro_error` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_empty_values` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_entries_keep_section_closing` | §3, §13 |
| `cases/spec/13-inputs/macro_registered_entries_skip_duplicate_rules` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_failure_in_included_file` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_failure_inside_object` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_failure_stops` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_failure_then_later_input` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_in_array_is_string` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_in_included_file` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_in_object` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_name_case_sensitive_error` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_not_in_argument_document_error` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_overrides_builtin` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_paren_last_byte_is_value` | §9, §13 |
| `cases/spec/13-inputs/macro_registered_quoted_value` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_seen_copies_first_container_value` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_seen_keeps_scalar_values` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_text_array_error` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_text_braced_after_name_in_braced_root_error` | §3, §13 |
| `cases/spec/13-inputs/macro_registered_text_braced_after_name_keeps_section_open` | §3, §13 |
| `cases/spec/13-inputs/macro_registered_text_braced_in_object_error` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_text_braced_takes_root_brace` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_text_comment_then_open_brace` | §1, §13 |
| `cases/spec/13-inputs/macro_registered_text_comments` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_text_counts_as_unit_error` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_text_counts_as_unit_ok` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_text_empty` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_text_holds_macro` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_text_in_braced_object_no_effect` | §3, §13 |
| `cases/spec/13-inputs/macro_registered_text_in_place` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_text_in_place_includes_other_file` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_text_in_place_self_include_error` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_text_include_differs` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_text_incomplete_error` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_text_keeps_section_chain_open` | §3, §13 |
| `cases/spec/13-inputs/macro_registered_text_keeps_section_open` | §3, §13 |
| `cases/spec/13-inputs/macro_registered_text_later_section_closes` | §3, §13 |
| `cases/spec/13-inputs/macro_registered_text_only_open_brace` | §9, §13 |
| `cases/spec/13-inputs/macro_registered_text_only_open_bracket` | §9, §13 |
| `cases/spec/13-inputs/macro_registered_text_section_left_open_brace_error` | §3, §13 |
| `cases/spec/13-inputs/macro_registered_text_stop_does_not_stay_open` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_text_stop_stops_all` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_text_takes_input_priority` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_text_takes_input_strategy` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_text_then_braced_file_keeps_section_open` | §3, §13 |
| `cases/spec/13-inputs/macro_registered_text_unclosed_brace_error` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_unknown_name_error` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_value_nul_kept` | §9, §13 |
| `cases/spec/13-inputs/macro_registered_value_on_next_line` | §13, §9 |
| `cases/spec/13-inputs/macro_registered_value_variables` | §13, §9 |
| `libucl/basic/1` | §1, §4, §5, §6, §8, §10 |
| `libucl/basic/10` | §3, §8 |
| `libucl/basic/11` | §1, §10 |
| `libucl/basic/12` | §5, §10 |
| `libucl/basic/13` | §8, §9, §10 |
| `libucl/basic/14` | §2, §9, §10 |
| `libucl/basic/15` | §8, §9, §10 |
| `libucl/basic/16` | §8, §9, §10 |
| `libucl/basic/17` | §4, §10 |
| `libucl/basic/18` | §8, §9, §10 |
| `libucl/basic/19` | §8, §9, §10 |
| `libucl/basic/2` | §1, §3, §5, §6, §7, §8, §10 |
| `libucl/basic/22` | §3, §11 |
| `libucl/basic/23` | §8, §9, §10 |
| `libucl/basic/3` | §1, §2, §3, §4, §7, §8, §10 |
| `libucl/basic/4` | §1, §3, §5, §6, §10 |
| `libucl/basic/6` | §2, §10 |
| `libucl/basic/8` | §3, §4, §5, §8, §10 |
| `libucl/basic/9` | §7, §8, §9, §10 |
| `libucl/basic/comments` | §2, §8, §10 |
| `libucl/basic/escapes` | §6, §10 |
| `libucl/basic/heredoc_eod` | §6, §10 |
| `libucl/basic/issue312` | §8, §10 |
| `libucl/basic/issue319` | §6, §10 |
| `libucl/basic/load` | §9, §10 |
| `libucl/basic/squote` | §6, §10 |

# Spec review: candidate `spec-v1`

- Reviewed: `docs/spec/README.md` and `docs/spec/01-*.md` … `12-*.md` at branch `libucl-compat`,
  commit `c9e1dd5`. The spec content was last changed in `b56135b`.
- Reviewer role: spec reviewer (PROTOCOL.md, *Roles*). Date: 2026-09-22.
- Inputs: `docs/clean-room/PROTOCOL.md`; the spec; `tests/conformance/` (case inputs, `.flags`,
  golden files, `xfail.txt`); `cargo test --test conformance`; black-box runs of
  `target/libucl-oracle/ucl-dump` on scratch inputs. No libucl source was read, and neither was
  `tools/ucl-dump/ucl_dump.c`, anything under `target/libucl-oracle/libucl/`, `REVIEW.md`,
  `PLAN.md`, `PROGRESS.md` or any `quarantine/*` branch.

## Verdict

**Release after fixes.**

There are no bar-A blockers. The spec contains no libucl code and no internal libucl names. The
only libucl identifiers are the public `UCL_PARSER_*` flags in §12 and the pinned commit hash.
Sections follow format features, not source files. The bar-A findings are all wording: a few
places narrate a procedure ("scanning continues", "are read … consumed", "is examined", "the
parser holds a list") where the same fact can be stated as input and result.

Under bar B, the spec is largely accurate. Every citation resolves to a real case, and all 189
citations in §05, §07, §08 and §10 match their golden files, apart from the items below. But six
rules are wrong or missing in ways an implementer cannot recover from without guessing:

- one table entry contradicts a golden file (B-1);
- five examples have their escape text replaced by the decoded characters (B-2), in addition to
  the two places in B-1;
- the float-formatting condition is wrong at its boundaries (B-3);
- the 127-character number limit is off by one (B-4);
- `libucl/basic/18` depends on an `.inherit` behaviour no rule states (B-5);
- the merge strategy omits one error case (B-6).

### What must be fixed before tagging `spec-v1`

- All bar-A findings (A-1 … A-7). They are wording changes and must not add implementation
  detail.
- All bar-B majors (B-1 … B-6). Where a fix relies only on an oracle probe, the fix must include a
  committed case. The README makes golden files the authority, so a rule without a case cannot be
  checked.
- The bar-B minors (B-7 … B-20) do not block the tag. Fix them now if cheap; otherwise record each
  as a known gap (README *Uncertain behaviour*, or an entry in `QUESTIONS.md`) and fix it in
  `spec-v2`.

## The two sections the spec team flagged

**§5.5 (what may follow a number): no finding.** It is written as conditions on input bytes ("may
be followed by …", "must be followed immediately by …", "makes the whole value a string") with
results, and reads as a statement of behaviour, not of control flow. All 18 citations match their
golden files. Oracle probes of combinations it asserts but does not cite agree with it:

- `10 # c` → `int 10`
- `k { a = 1.5 }` → `float 1.5`
- `[1 ]` → `[int 1]`
- `1s<TAB>` → `"1s"`
- `1s<NUL>` → `time 1`
- `10<VT>` → string
- `0x10s ` → `"0x10s"`

**§10.3 (float formatting): minor bar-A finding (A-1) plus a major bar-B finding (B-3).**

- The conditions are stated as properties of the value, which is acceptable.
- The presentation is not: a numbered "1. as `%.1f` … 2. otherwise, as `%.15g` … 3. otherwise as
  `%f`" is an if/else-if/else chain over format strings.
- Referring to the ISO C `printf` conversions is acceptable, as an external public standard that
  defines the output bytes. The rule should be a table of mutually exclusive conditions, each
  mapped to an output shape.
- Separately, the second condition is wrong (B-3).

**§10.9 (how the `.res` files are produced): acceptable.** Its numbered steps describe the test
harness that produced the upstream `.res` files, not the library. PROTOCOL.md allows tests and test
tooling to be derived from libucl. The steps are accurate: I reproduced 23 of the 25 `.res` files
exactly with two oracle passes, and the other two differ for the reasons the section gives. One
detail is missing (B-19).

## Findings

Line numbers refer to commit `c9e1dd5`.

| ID | Bar | Severity | Location | What is wrong | Required change |
| --- | --- | --- | --- | --- | --- |
| A-1 | A | minor | `docs/spec/10-output.md:51-59` | The float rule is written as a branch sequence over format strings: "1. as `%.1f`, one decimal digit … 2. otherwise, as `%.15g` (C `printf` shortest form, 15 significant digits …) … 3. otherwise as `%f`". This follows the shape of a conditional chain rather than defining an output function. | Recast it as a table. Each row pairs a condition on v, stated as a property of the value (with the corrected condition from B-3), with the output shape. The conditions must be mutually exclusive. An output shape may be defined by reference to ISO C `printf` ("the bytes that the ISO C conversion `%.15g` produces for v"). Keep the examples. Don't add a description of how the value is tested. |
| A-2 | A | minor | `docs/spec/04-unquoted-values.md:95-98` | "When at least four characters follow `\u`, those four are consumed. Hex digits are read until the first non-hex character, and the code point is the value of the digits read before it, multiplied by 16." This narrates a decoder reading and consuming bytes. | State the result, for example: "The escape covers `\u` and the four characters after it. If all four are hex digits, the code point is their value. Otherwise it is 16 × the value of the hex digits before the first non-hex character among the four (0 if there are none)." |
| A-3 | A | minor | `docs/spec/07-variables.md:37-38` | "Scanning then continues right after the `${`, so a reference inside the braces is still expanded". This describes a scanner position. | For example: "The unresolved `${…}` text is kept as written, and references inside it are expanded as usual: `"${$ABI}"` → `"${unknown}"`." |
| A-4 | A | minor | `docs/spec/07-variables.md:7-8` | "The parser holds a list of variables, each a name and a value, kept in registration order." This describes a data structure. | State the observable ordering, for example: "Each registered variable has a name and a value. Variables are ordered by when their name was first registered; registering a name again changes its value, not its place in that order." |
| A-5 | A | minor | `docs/spec/03-keys.md:59-62` | "How names are told apart from a value: … The rest of the current line is examined, up to the first LF, CR, `,` or `;`." This describes a look-ahead procedure. | State it as a condition, for example: "When a key has no separator, the words after it are section names if and only if the rest of the line, up to the first LF, CR, `,` or `;`, contains `{` or `[` anywhere (including inside quotes or inside a word). Otherwise that text is the value." |
| A-6 | A | minor | `docs/spec/02-comments.md:32`; `docs/spec/09-macros.md:33`, `:42`, `:48`, `:51` | These phrasings describe processing steps: "text between double quotes is skipped as a unit"; "double-quoted text inside is skipped while matching"; "VALUE follows after skipping whitespace, line breaks and comments"; "with leading whitespace skipped"; "After the value, whitespace, line breaks and `;` are skipped." | State what the input may contain and what it means. For example: "a `*/` between double quotes inside a block comment does not end it"; "a `(` or `)` between double quotes does not count toward the balance"; "VALUE may be preceded by whitespace, line breaks and comments"; "leading whitespace inside the braces is not part of the value"; "VALUE may be followed by whitespace, line breaks and `;`, after which the next entry may start on the same line". |
| A-7 | A | minor | `docs/spec/04-unquoted-values.md:6`, `:50-53`; `docs/spec/07-variables.md:25` | Processing-order narration: "Its text is decided first (§4.1–4.3), then its meaning (§4.4–4.6)"; "They look at the bytes that follow the number, which is not quite the same as the extent of §4.1"; "the value is examined as below"; "a value is first classified (§4), then expanded". | State the observable consequences instead, for example: "Whether an unquoted value is a number depends on the bytes after the number (§5.5, §5.6), not only on its extent (§4.1)"; "Expansion never changes a value's type: `$T` with `T` = `true` → `"true"`." |
| B-1 | B | major | `docs/spec/10-output.md:44`, `:137` | Contradicts a golden file. The table says control bytes are written as `�`, a literal U+FFFD (bytes EF BF BD in the spec file). Every format's `cases/spec/10-output/string_control_chars.*.golden` file contains the six ASCII bytes `�` instead: backslash, `u`, `F`, `F`, `F`, `D`. An implementer following the table would emit the wrong bytes. | Write the output as the six-byte ASCII escape `�`, with uppercase hex like `\u000B` on line 43, in both places. |
| B-2 | B | major | `docs/spec/03-keys.md:27`; `docs/spec/04-unquoted-values.md:88`; `docs/spec/06-strings.md:25`, `:26`, `:31` | Example inputs show the decoded characters where the case inputs contain `\u` escape text, so input and result look identical or are wrong:<br>• `"A" = 2` (case `quoted_escapes`: `"A" = 2`);<br>• `aAb` (case `backslash_escapes`: `aAb`);<br>• `Aé€￿` (case `dq_unicode_bmp`: `"Aé€￿"`);<br>• `"ὤ0"` (case `dq_unicode_five_digits`: `"ὤ0"`);<br>• `😀` "becomes six bytes": as written this is one 4-byte character; the intended input is presumably `"😀"`. | Restore the escape text in these five places and the two in B-1. Then check every example against its case's input bytes. A non-ASCII scan does not catch ASCII decodings such as `A` → `A`. |
| B-3 | B | major | `docs/spec/10-output.md:53-59` | Rule 2 is wrong at the boundaries (oracle probes; no case covers them).<br>The oracle compares v with v rounded toward zero, not with the nearest integer, and the bound is strict:<br>• `2.99999999` → `3.000000` (the spec predicts `2.99999999`);<br>• `0.99999999` → `1.000000`;<br>• `-1.99999999` → `-2.000000`;<br>• `-2.00000001` → `-2.00000001`;<br>• `9e-8` → `9e-08`;<br>• `1e-7` → `0.000000`.<br>§10.3 also omits negative infinity: `-1e308k` → `-inf`. | Restate the condition as \|v − t\| < 10⁻⁷, where t is v rounded toward zero and t lies in the 32-bit range (the spec team should confirm that the range applies to t). List `-inf`. Add a §10 case with the values above. |
| B-4 | B | major | `docs/spec/05-numbers.md:49-50`; `docs/spec/10-output.md:153` | Off by one (oracle probe). A number written with 127 characters is already a string. `0.` followed by 123 zeros and `1` (126 characters) → `float 1e-124`; with one more zero (127 characters) → string. `very_long_number_is_string` uses 130 characters, so it does not pin the boundary. | "A number written with 127 or more characters is a string." Make the same change in §10.9. Add 126- and 127-character float cases. |
| B-5 | B | major | `docs/spec/09-macros.md:170-172`; case `libucl/basic/18` | A golden result no rule explains. In `libucl/basic/18`, `mything1 { … .inherit "mything1" … }` names the object that contains it and does not fail, yet §9.7 says a NAME missing from the root is an error. Oracle:<br>• `e { a = 1; .inherit "e" }` → `{ e: { a: int 1 } }`;<br>• `e { a = 1 }⏎e { .inherit "e"; b = 2 }` → `e: ⟨{a: int 1} \| {a: int 1, b: int 2}⟩` (the first value is used).<br>An implementation that treats the object as absent until its closing brace fails case 18. | State the observable rule: inside an object, `.inherit` of the key that holds that object is not an error; the first-value rule applies; when the source is the object itself, nothing is copied. Add a case. Describe the behaviour, not a mechanism. |
| B-6 | B | major | `docs/spec/08-duplicates.md:67-79`; `docs/spec/11-errors.md:26` | The merge strategy omits one combination (oracle): an existing object followed by a new array is an error. `a { x = 1 }⏎a = [2]` with `strategy:merge` fails ("cannot merge an object with an array"). §8.4 lists only array followed by object, and §11.1 only "merging an object into an array". | Add the rule and a case; update the §11.1 table. |
| B-7 | B | minor | `docs/spec/08-duplicates.md:75-79` | Priorities under `merge` are unstated in two places (oracle, `strategy:merge`):<br>• Arrays are concatenated whatever the priorities: `.priority 3`, `a = [1]`, `.priority 1`, `a = [2]` → `a: [int 1 @3, int 2 @1] @3`.<br>• A scalar that replaces a container keeps the container's priority: `.priority 3`, `a { x = 1 }`, `.priority 1`, `a = 2` → `a: int 2 @3`. With the priorities swapped → `int 2 @1`.<br>Also, the array half of "an object or array, with a scalar" has no case (oracle: `a = [1]⏎a = 2` → `int 2`). | State these rules and add cases. |
| B-8 | B | minor | `docs/spec/01-structure.md:103-104`, `:116-117` | §1.6 contradicts itself at end of input:<br>• "If only whitespace follows until the end of input, the value is `null`" predicts null for `a = ` (trailing space, no line break), but the oracle rejects it.<br>• `a = # c⏎` at end of input is an error, but `a = /* c */⏎` gives `null`. | Restate the rule: the value is `null` only if a line break follows the separator and nothing but spaces, tabs, block comments and blank lines follows until the end of input. A line comment in that stretch, or no line break at all, is an error. Add cases. |
| B-9 | B | minor | `docs/spec/01-structure.md:5-16` | A terminator before the first entry of the root is an error: `;a = 1`, `,a = 1` and `⏎;⏎a = 1` are all rejected by the oracle. §1.4 states this only for braced objects. | Add the rule and a case. |
| B-10 | B | minor | `docs/spec/07-variables.md:7-10`; `docs/spec/README.md:64-66` | Two gaps in variable order:<br>(a) "Registering an existing name again replaces its value and keeps its position" has no case. The oracle agrees: with `var:AB=x` then `var:ABI=re`, `"$ABI"` → `"re"`.<br>(b) The position of `FILENAME` and `CURDIR` relative to `var:` entries is not stated, although it affects prefix matching (§7.4). With `var:CUR=early`, `"$CURDIR"` still gives the directory, not `earlyDIR`. With `var:CURDIR=mine`, `"$CURDIR"` still gives the directory. | State the order the oracle uses, and what happens when a `var:` name equals a file variable. Add cases. |
| B-11 | B | minor | `docs/spec/05-numbers.md:33-36` | Citations do not show the claims:<br>• `-0x` and `0x-1` are attributed to `cases/review/08_hexbad`, which contains only `0xreadbeef`; no case contains either.<br>• `1x` is not in `hex_digits_before_x_ignored`.<br>The oracle agrees that all three are strings. The rule is also silent on a sign before digits-then-`x` (oracle: `-12x34` → `int -52`). | Add these inputs to cases, fix the citations, and state the signed form. |
| B-12 | B | minor | `docs/spec/04-unquoted-values.md:98-100` | The ×16 rule rests on one data point with a nonzero prefix (`\u4Z00`). Oracle: `\u12ZZ` → U+0120, `\u123Z` → U+1230, `\u1ZZZ` → U+0010. | Add cases for one-, two- and three-digit prefixes. |
| B-13 | B | minor | `docs/spec/10-output.md:96-101` | Boundaries not pinned: the cited strings are 84 and 85 bytes. Oracle: a string with a LF is written in JSON form at 80 bytes and as a heredoc at 81. A first line that is exactly `EOD` does not trigger the exception (the oracle writes `<<EOD⏎EOD⏎x⏎EOD`). This agrees with the text, but no case shows it. | Add cases. |
| B-14 | B | minor | `docs/spec/10-output.md:83-89`, `:107-114` | Two layouts are unstated:<br>• YAML output for an empty root (`empty_document.yaml.golden` is empty).<br>• An empty array or object as an array element, written `[]⏎` or `{}⏎` (`arrays.config.golden`). The "When empty" bullet covers only entries. | State both. Cite `empty_document` and `arrays`. |
| B-15 | B | minor | `docs/spec/09-macros.md:173-179`; `docs/spec/08-duplicates.md:53-54` | Three unstated or uncited `.inherit` details:<br>• Copies keep their source priority, although the goldens depend on it. In `libucl/basic/18`, `mything1.key` is `"val1" @3` while the current priority is 1. Oracle: `d { a = 1 }`, `e { .priority 2; .inherit "d" }` → `e.a` at priority 0.<br>• "Whatever the priorities" has no case. Oracle: an inherited `@3` value is replaced by an explicit `@1` value.<br>• "Not marked inherited" under `replace=true` is not shown by `inherit_replace_appends`. | State the priority rule and add cases. |
| B-16 | B | minor | `docs/spec/07-variables.md:21` | "Macro values (all three forms)": only the quoted form (`include_curdir`) and the bare form (`macro_value_variables`, `.priority $PRI`) are cited. No case covers `{…}`. | Add a case. |
| B-17 | B | minor | `docs/spec/03-keys.md:60-62` | "Even inside quotes" has no case. Oracle: `a "x{y"⏎` at end of input → `{ a: { "x{y": null } }`. A name followed by the end of input gives `null`, which no rule states. | Add cases and state the result. |
| B-18 | B | minor | `docs/spec/05-numbers.md:21`, `:106` | Two citation problems:<br>• Line 21 (nearest double) has no citation, although `floats` key `g` shows it.<br>• Line 106 cites `cases/additions/a02_bare_float_inside` (`foo 1.50 bar`). That value does not start with a digit, so it does not show what may follow a number; it belongs to §5.7. | Fix the citations. |
| B-19 | B | minor | `docs/spec/10-output.md:140-160` | Each `.res` file ends with one more LF than the config output. Two oracle passes plus one LF reproduce 23 of 25 `.res` files exactly. `14` matches when run from another directory, as the section says. `2` differs only because `ucl-dump` always registers `ABI`, which agrees with the section's "no `ABI` variable". | State the extra final LF. |
| B-20 | B | minor | `docs/spec/11-errors.md:32-36` | `object_depth_limit_error` nests 1025 objects, not the 1024 the text describes, so the object boundary is not pinned. Oracle: 1023 nested objects inside the root parse. | Say so, or adjust the case. |

## Citations checked

Method:

- **Existence.** A script extracted every backticked case name from `docs/spec/01-*.md` … `12-*.md`,
  resolving bare names against the section's own directory. All 636 citations resolve to an existing
  `<case>.golden.json`. The 48 unresolved tokens are keywords and parameter names (`true`, `glob`,
  `append`, …), not citations. The README coverage table lists all 716 cases and nothing else.
- **Content.** For each case below, I rendered the input and golden file in the spec's notation and
  compared them with the rule and any example that cites it. For §10, I also compared the
  `.config`, `.json`, `.json-compact` and `.yaml` goldens byte for byte with §10.2–§10.7.
- **Totals.** Checked against golden content: 189 citations in §05, §07, §08 and §10 (every
  citation there), plus 150 spot checks in the other sections, 339 in all. All are consistent
  except those marked. None of the 339 cases contradicts its rule; one golden contradicts a line of
  text (B-1). Citations that do not show their claim: `08_hexbad` for `-0x` and `0x-1`,
  `hex_digits_before_x_ignored` for `1x`, `a02_bare_float_inside`, and `numbers_are_not_expanded`
  (weak).
- **Test run.** `cargo test --test conformance` at `c9e1dd5`: 716 cases, 90 pass, 372 expected
  failures, 254 unexpected failures. All 254 are panics, which the LOG entry for C0 predicts at the
  branch head (`insert_with_strategy` is still `todo!()`). This says nothing about the spec.

### §05, every citation (73)

OK:

- `floats` (14, 19), `cases/additions/a38_trailing_dot_num` (14), `exponents` (16),
  `exponent_forms` (16, 24), `integers` (19, 20), `libucl/basic/1` (19, 31),
  `cases/additions/a39_leading_zero` (20).
- `not_numbers`, `malformed_become_strings` (24), `cases/additions/a16_plus_num`,
  `cases/additions/a37_leading_dot_value` (25), `cases/additions/a40_underscore_num` (26).
- `hex_malformed_become_strings`, `cases/review/07_hexdot` (33).
- `no_binary_or_octal` (37), `cases/review/12_0b`, `cases/review/13_0o` (38).
- `int64_limits` (43), `int_overflow_error`, `int_underflow_error`, `cases/review/26_bigint`,
  `cases/review/27_overflow` (44), `hex_overflow_error` (45), `float_overflow_error` (46),
  `float_underflow_error`, `subnormal_error` (47), `float_max`, `min_normal` (48).
- `binary_multipliers` (56, 72), `time_milliseconds`, `time_minutes` (56, 73),
  `cases/review/06_ms` (56), `cases/additions/a48_ms_uppercase`,
  `cases/additions/a49_min_upper` (57).
- `decimal_multipliers`, `negative_multipliers`, `float_decimal_multiplier` (72), `time_seconds`,
  `time_kilo_giga_seconds` (73), `time_hour_day_week_year`, `cases/review/25_neg_time`,
  `cases/additions/a22_time_h` (74), `cases/additions/a23_time_w` (75), `libucl/basic/2` (75, 107).
- `float_binary_multiplier_truncates`, `cases/additions/a14_float_kb`,
  `cases/additions/a24_int_float_exp_suffix` (84), `multiplier_overflow_wraps` (88).
- `unknown_multipliers_are_strings`, `time_lookalikes_are_strings`, `cases/additions/a50_b_suffix`
  (90), `hex_with_suffixes`, `cases/additions/a15_hex_suffix` (93).
- `plain_number_trailing_whitespace` (102), `number_then_hash` (103), `number_then_text_is_string`,
  `number_then_brace_is_string`, `cases/review/03_numword` (105),
  `cases/additions/a21_space_before_suffix` (106), `libucl/basic/12` (107).
- `suffix_before_separator` (111; its `30s⏎` is written CR LF in the case),
  `suffix_then_comment_without_space`, `suffix_before_closing_brackets` (111),
  `suffix_then_whitespace_is_string` (118), `suffix_then_space_comment_is_string` (120),
  `suffix_in_braces_before_space` (122), `suffix_in_arrays` (124), `number_then_block_comment`,
  `cases/spec/01-structure/array_comment_inside` (130).

Not OK:

- `very_long_number_is_string` (50): the golden file agrees, but the stated boundary is wrong
  (B-4).
- `cases/review/08_hexbad` (34): does not show `-0x` or `0x-1` (B-11).
- `hex_digits_before_x_ignored` (36): OK for `12x34`, `9x1f` and `x10`; `1x` is absent (B-11).
- `cases/additions/a02_bare_float_inside` (106): does not show the claim (B-18).

Oracle probes, beyond the citations:

- `123.456ms` → `time 0.123456` confirms "÷ 1000" rather than "× 0.001".
- `0x10w`, `0x10y`, `0x10ks`, `0x10gs` → `int 16`, and `0x10mb` → `int 16777216`. These agree
  with the "time suffixes accepted and ignored" quirk.

### §07, every citation (40)

OK:

- `names_case_sensitive` (12), `braced` (18, 34), `unbraced` (18, 47), `in_unquoted_atoms`,
  `cases/additions/a06_bare_var`, `libucl/basic/3` (19).
- `in_heredoc`, `cases/spec/06-strings/heredoc_variables` (20), `not_in_single_quotes`,
  `cases/spec/06-strings/sq_no_variables` (22), `not_in_keys`,
  `cases/spec/03-keys/quoted_no_variable_expansion` (23),
  `cases/spec/09-macros/macro_args_no_variables` (24), `expanded_values_stay_strings` (25).
- `cases/additions/a05_braced_var` (34), `unknown_preserved` (37, 53), `upstream_mix` (38, 62),
  `libucl/basic/2` (38, 62), `lone_dollar` (39, 55).
- `cases/additions/a04_unbraced_var` (47), `unbraced_prefix_match`, `cases/additions/v03_prefix`
  (48), `extra_variable_order_short_first` (51), `extra_variable_longer_name` (52),
  `handler_not_for_unbraced` (54), `cases/spec/04-atoms/value_with_dollar` (56).
- `dollar_escape_only_when_expanding` (62; all seven table rows match), `cases/review/15_dollardollar`,
  `cases/review/16_dollarplain`, `cases/additions/v01_vars` (63).
- `backslash_dollar` (78, 80), `handler_braced_whole_string` (87), `registered_wins_over_handler`
  (88), `handler_refuses` (89), `filename_for_string_input` (101), `filevars_disabled`,
  `cases/spec/12-flags/no_filevars` (102).

Not OK:

- `cases/spec/09-macros/macro_value_variables`, `cases/spec/09-macros/include_curdir` (21): OK, but
  they cover only two of the three forms (B-16).
- `numbers_are_not_expanded` (25): consistent but weak. It shows only that `10` stays an int;
  `expanded_values_stay_strings` carries the claim.

Oracle probes, beyond the citations:

- `a = $ABI$$x` → `"unknown$x"` and `a = $$x` → `"$$x"`: §7.5 also holds for unquoted values.
- `"${$ABI"` → `"${unknown"`.

### §08, every citation (50)

All consistent:

- `keeps_first_position` (10), `explicit_array_not_flattened` (13),
  `cases/additions/v02_explicit_then_scalar` (14), `mixed_types`, `libucl/basic/1`,
  `libucl/basic/2` (15), `repeated_inside_array_object` (19).
- `scalar_repeated`, `cases/review/17_multikey` (26), `objects_repeated_not_merged`,
  `libucl/basic/issue312`, `libucl/basic/comments` (28), `sections_repeated_not_merged`,
  `repeated_inside_braces`, `cases/review/10_dupnested` (31),
  `cases/additions/a27_nested_dup_in_braces`, `libucl/basic/8`, `libucl/basic/10` (32).
- `chunk_priority` (37), `priority_applies_to_containers` (39), `priority_modulo_16`,
  `priority_16_is_0`, `priority_negative`, `priority_15` (42).
- `priority_equal_appends` (49), `priority_higher_replaces`, `libucl/basic/13` (50),
  `priority_lower_ignored`, `libucl/basic/15`, `libucl/basic/16` (51).
- `cases/spec/09-macros/inherit_basic` (54).
- `libucl/basic/19` (59; all nine keys match append, merge and rewrite), `strategy_rewrite` (62),
  `strategy_error` (64), `strategy_error_no_duplicates` (65), `strategy_merge_objects` (71).
- `cases/spec/09-macros/include_merge_ignores_priority` (73),
  `cases/spec/09-macros/include_merge_lower_priority_still_merges` (74), `strategy_merge_arrays`
  (76), `strategy_merge_array_then_object_error` (77), `strategy_merge_object_then_scalar` (79).
- `strategy_merge_scalars_append`, `strategy_merge_scalar_then_object` (82),
  `cases/spec/09-macros/include_merge_scalar_lower_priority_dropped` (83),
  `cases/spec/09-macros/include_merge_scalar_higher_priority_replaces` (84).
- `no_implicit_arrays_scalars` (92), `no_implicit_arrays_with_arrays` (94),
  `no_implicit_arrays_objects` (95), `cases/spec/12-flags/key_lowercase_merges_case` (100).
- `libucl/basic/18` (54): consistent at equal priorities, but the golden also depends on B-5 and
  B-15.

### §10, every citation (26)

OK:

- `keys_quoting` (21; all 16 keys in all four formats), `priorities_not_emitted` (23).
- `scalars` (29, 71, 78, 114), `integers_large` (29), `times` (30), `string_escapes` (35),
  `string_utf8` (45).
- `floats` (59; all ten values fit the stated rules), `floats_special` (59).
- `empty_document` (68), `top_level_array` (68, 79, 114), `objects` (71, 78, 114), `arrays` (71, 78,
  114).
- `implicit_arrays` (92, 126, 129), `implicit_array_of_arrays` (92, 121), `nested_implicit_arrays`
  (92, 127), `implicit_array_layouts` (121, 126, 130; inline and normal layouts, the YAML stray
  comma and the first-array quirk all match).
- `heredoc_kept` (97, 99), `long_string_with_newline` (97), `load_multiline_output` (98),
  `heredoc_with_eod_line`, `libucl/basic/heredoc_eod` (101), `single_quoted_kept` (104).
- `libucl/basic/14` (157): as described.

Not OK:

- `string_control_chars` (36): contradicts the text at line 44 (B-1).
- `libucl/basic/*.res` (6, 79, 142): reproduced with an extra final LF (B-19).

Oracle probes, beyond the citations:

- `"a]b"`, `"a'b"`, `"a$b"`, `"-x"` and `"a@b"` are written bare, and `"x\/y"` and `"a\tb"` quoted.
  This agrees with the exhaustive byte list in §10.1.
- A non-empty object after the first element of an inline-layout array, and a non-root multi-value
  entry in YAML, lay out as §10.7 implies.

### Spot checks in other sections (150), all consistent unless marked

- **§01 (24):** `top_array_trailing_ignored`, `top_braced_trailing_ignored`,
  `top_braced_second_object_ignored`, `close_without_open_error`, `sep_double_equals_error`,
  `sep_none`, `term_nul_byte`, `term_repeated`, `value_spans_rest_of_line`,
  `missing_delimiter_after_string_error`, `object_value_needs_no_delimiter`,
  `object_leading_comma_error`, `array_leading_comma_error`, `array_double_comma`,
  `array_containers_need_no_separator`, `array_strings_need_separator_error`,
  `empty_value_takes_next_line`, `empty_value_skips_blank_lines`,
  `object_on_next_line_after_equals`, `empty_value_at_end_is_null`, `key_equals_eof_error`,
  `key_equals_semicolon_error`, `value_without_key_error`, `array_space_separated_numbers`.
- **§02 (10):** `vertical_tab_is_not_a_separator`, `utf8_bom_is_part_of_key`,
  `hash_ends_unquoted_value`, `quoted_close_inside_comment`, `block_comment_mid_value_error`,
  `slash_slash_is_a_key`, `after_value_is_separator`, `between_separator_and_value`, `nested`,
  `block_comment_after_value`.
- **§03 (16):** `bare_charset`, `brace_directly_after_key_error`,
  `quoted_newline_before_separator`, `section_lookahead_brace_in_value_error`,
  `section_names_then_separator_is_value`, `section_lookahead_stops_at_semicolon`,
  `section_numeric_names`, `quoted_brace_directly_after`, `starts_with_dash_error`,
  `section_quoted_names`, `section_mixed_names`, `quoted_followed_by_quoted_value_error`,
  `quoted_value_after_bare_key_error`, `section_array_value`,
  `section_lookahead_brace_later_on_line_error`, and `quoted_escapes` (golden OK; example text
  wrong, B-2).
- **§04 (13):** `url`, `brackets_balanced`, `braces_balanced`, `unbalanced_close_brace_error`,
  `backslash_escaped_terminators`, `backslash_at_end`, `backslash_invalid_unicode`,
  `bool_true_forms`, `nan_inf_lowercase_only`, `null_lowercase_only`,
  `keywords_with_trailing_space`, `control_chars_allowed`, and `backslash_escapes` (golden OK;
  example text wrong, B-2).
- **§06 (25):** `dq_unknown_escape_drops_backslash`, `dq_unit_separator_allowed`, `dq_del_allowed`,
  `dq_raw_tab_error`, `dq_backslash_at_end_error`, `sq_escaped_quote`, `sq_line_continuation`,
  `sq_no_escapes`, `heredoc_terminator_then_separator`, `heredoc_terminator_trailing_space_error`,
  `heredoc_terminator_then_newline_brace`, `heredoc_empty_error`, `heredoc_blank_line_content`,
  `heredoc_empty_terminator`, `heredoc_crlf_content`, `heredoc_short_input_is_string`,
  `heredoc_lowercase_error`, `heredoc_crlf_opener_error`, `heredoc_in_array`,
  `heredoc_longer_line_is_content`, `heredoc_terminator_before_brace_error`,
  `dq_brace_unicode_error`, `dq_unicode_nul`, and `dq_unicode_bmp`, `dq_unicode_five_digits`
  (golden OK; example text wrong, B-2).
- **§09 (45):** `macro_not_recognised_in_arrays`, `macro_not_recognised_as_value`,
  `macro_space_after_dot_error`, `include_space_before_args`, `include_param_prefix_names`,
  `include_priority_must_be_integer`, `priority_value_on_next_line_error`,
  `macro_quoted_value_not_unescaped`, `include_braces`, `include_bare`, `priority_args_only`,
  `include_then_keys`, `try_include_missing_stops_parsing`, `include_glob_no_match_stops_parsing`,
  `include_prefix_auto_key`, `include_prefix_array_existing_scalar`,
  `include_prefix_existing_object`, `include_self_error`, `include_nesting_limit_ok`,
  `include_nesting_limit_error`, `load_int_leading_digits`, `load_escape`,
  `load_unknown_target_inserts_nothing`, `priority_forms`, `inherit_at_top_level`,
  `inherit_existing_keys_kept`, `inherit_first_of_repeated`, `inherit_is_shallow`,
  `inherit_missing_error`, `inherit_non_object_error`, `inherit_top_level_only_error`,
  `include_priority_macro_inside`, `include_priority`, `include_prefix_array_target`,
  `include_key_without_prefix`, `include_glob`, `load_trim`, `load_multiline`,
  `includes_like_include`, `include_duplicate_rewrite`, `include_duplicate_error`,
  `include_nested`, `include_empty_value_is_null`, `load_existing_key_error`.
  `inherit_replace_appends` is OK for "appends", but the "not marked inherited" claim is not shown
  (B-15).
- **§11 (9):** `garbage_after_value_error`, `unexpected_close_top_error`, `lone_bracket_error`,
  `cases/errors/e02_invalid_escape_first`, `cases/errors/e03_lone_backslash`,
  `cases/errors/e04_nul_byte_first`, `libucl/basic/22`, `depth_limit_error` (1024 arrays; the
  oracle accepts 1023), and `object_depth_limit_error` (1025 levels, B-20).
- **§12 (8):** `key_lowercase`, `key_lowercase_non_ascii`, `zerocopy_no_effect`, `no_time`,
  `save_comments_no_effect_on_values`, `disable_macro_rejects_macros`,
  `disable_macro_priority_error`, `disable_macro_no_variables`.

### Gap sampling

- Every case under `cases/spec/` is cited in section text.
- A sample of 14 of the 184 `cases/migrated/` cases showed no golden behaviour that the rules fail
  to explain:
  - `…bare_word_validation_errors_5`, `…bare_word_validation_errors_4`, `…null_keyword_conversion`,
    `…test_number_format_compatibility`, `…duplicate_key_handling`;
  - `…error_handling_backward_compatibility_2`, `…heredoc_terminator_improvements_4`,
    `…heredoc_with_both_leading_and_trailing_whitespace`, `…implicit_arrays_with_comments`;
  - `…cpp_comments_preservation`, `…libucl_mixed_array_types`, `…nginx_syntax_error_handling`,
    `…unicode_boundary_values`, `…unicode_escapes_in_keys`.
- The gaps found are B-5, B-6, B-7, B-8, B-9, B-10, B-14, B-15 and B-17.

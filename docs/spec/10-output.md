# 10. Output formats

Cases: `tests/conformance/cases/spec/10-output/`. Every case in `tests/conformance/` that parses,
in any directory, has besides its typed dump the exact bytes libucl writes for the parsed value in
four formats: `<case>.config.golden`, `<case>.json.golden`, `<case>.json-compact.golden` and
`<case>.yaml.golden`. Cases that save comments also have `<case>.config-comments.golden` (§10.10).
**Those files are normative**; the rules below describe them, and the cases cited here are the
ones aimed at a rule. The upstream `libucl/basic/*.res` files are config output produced by a
two-pass procedure (§10.9).

## 10.1 What is written

Output is produced from a value tree. Besides the values themselves, three facts remembered from
parsing affect the config format:

1. **Single-quoted origin**: the string was written in single quotes (§6.2).
2. **Multi-line origin**: the string came from a heredoc (§6.3), or from `.load` with
   `multiline=true` (§9.6).
3. **Key needs quoting**: the key was written in double quotes **and** contained either a backslash
   escape or one of these bytes: space, TAB, LF, CR, FF, BS, NUL, `"`, `+`, `:`, `=`, `[`, `\`, `{`.
   A key written quoted without any of these is output bare (`"kq"` → `kq`, `"a.b"` → `a.b`,
   `"s/t"` → `s/t`). **Quirk:** so are keys containing `;`, `}`, `#` or `,`, which then cannot be
   read back (`keys_quoting`).

Values created by macros (§9):

- Copies made by `.inherit` keep all three facts of the values they copy
  (`inherit_copies_keep_output_facts`).
- A key created by `.load` needs quoting when it contains any of the bytes listed in fact 3,
  however it was written. **Quirk:** a key created by `.include` with `key` or `prefix` never does,
  so `.include(key="x y") …` is written `x y {…}` (`macro_created_keys_quoting`).

Keys of multi-value entries (§8): each value of the entry keeps the key as it was written for that
value, its spelling (which can differ under `key-lowercase`, §12.1) and fact 3. The config format
writes every value with its own key; JSON and YAML write the entry once, with the key of its
first value: `"x\u0041" = 1⏎xA = 2` gives the config lines `"xA" = 1;` and `xA = 2;`, and under
`key-lowercase`, `"\u0041" = 1⏎a = 2` gives `"A" = 1;` and `a = 2;` but the JSON key `"A"`
(`multi_value_key_quoting_per_value`, `multi_value_key_spelling_per_value`).

**The empty key.** Parsing rejects an empty key (§3), but `.include(key="")` creates one (§9.4).
JSON and compact JSON write it as the four bytes `null`, without quotes, and YAML likewise; the
config format writes it as nothing, so the line begins with the space before `{`:
`.include(key="") "…"` → config ` {⏎    x = 1;⏎…}⏎`, JSON `null: {…}` (`empty_key`).

Nothing else from parsing shows in the output: priorities are never written
(`priorities_not_emitted`), and values that `.inherit` copied (§9.7) or that `no-implicit-arrays`
collected into an array (§8.5) are written like any other value or array. A parse result contains
only the value kinds of §10.2. Output of a parse error is not defined.

## 10.2 Scalars (all formats)

| Value | Written as |
| --- | --- |
| int | decimal: `-5`, `9223372036854775807` (`scalars`, `integers_large`) |
| float, time | see §10.3; time is written like a float: `10s` → `10.0`, `1ms` → `0.001000` (`times`) |
| true / false | `true` / `false` |
| null | `null` |
| string | the JSON form below, unless §10.5 chooses another form |

**JSON form of a string**: wrapped in `"`, with these replacements (`string_escapes`,
`string_control_chars`):

| Byte | Written |
| --- | --- |
| `"` | `\"` |
| `\` | `\\` |
| LF, CR, TAB, BS, FF | `\n`, `\r`, `\t`, `\b`, `\f` |
| VT (0x0B) | the six ASCII characters `\u000B` |
| any other byte 0x00–0x1F, and DEL (0x7F) | the six ASCII characters `\uFFFD` (**quirk**: the original byte is lost) |
| everything else, including `/` and bytes ≥ 0x80 | unchanged, also where the bytes are not valid UTF-8, as after a surrogate escape (§6.1) (`string_utf8`) |

NUL is one of the "other" bytes: `"\u0000"` → `"\uFFFD"`. `strings_every_control_byte` covers
every byte 0x01–0x1F, DEL, NUL and some bytes ≥ 0x80. The same form is used for keys that need
quoting.

## 10.3 Floats and times

Let v be the value and t the value rounded toward zero. The output is the bytes that the ISO C
`printf` conversion in the second column produces for v, in the C locale:

| Condition on v | Conversion | Examples |
| --- | --- | --- |
| v is a whole number and −2³¹ ≤ v ≤ 2³¹−1 | `%.1f` | `2.0`, `-0.0`, `10.0`, `86400.0`, `2147483647.0`, `-2147483648.0` |
| v is not a whole number, −2³¹ ≤ t ≤ 2³¹−1, and \|v − t\| < 10⁻⁷ | `%.15g` | `1e-10`, `9e-08`, `-9e-08`, `2.00000001`, `-2.00000001`, `5.00000009` |
| any other v, including NaN, +∞ and −∞ | `%f` | `1.500000`, `0.100000`, `100.250000`, `3000000000.500000`, `2147483648.000000`, `-2147483649.000000`, `100000000000000000000.000000`, `nan`, `inf`, `-inf` |

The digits are those of the exact decimal value of the double, rounded to the precision the
conversion asks for, ties to even: `0.0078125` → `0.007812`, `0.0234375` → `0.023438`,
`1e300` → its 301 integer digits followed by `.000000` (`floats_exact_decimal_expansion`). In
detail:

- `%.1f` and `%f` write an optional `-`, the integer digits (at least one), `.`, and exactly 1 or 6
  decimals.
- `%.15g` rounds to 15 significant digits. Let X be the decimal exponent of the rounded value
  (`d.ddd…×10^X`). If −4 ≤ X < 15, the value is written in plain decimal form with 14 − X decimals;
  otherwise as one digit, `.`, 14 decimals, `e`, a sign and at least two exponent digits. Then
  trailing zeros after the `.` are removed, and the `.` too if nothing follows it: `9e-08`,
  `2.00000001`, `1e-10`.
- NaN is written `nan` and the infinities `inf` and `-inf`. Parsing produces NaN only from the
  keyword `nan`, never with a sign (§4), and `-inf` only when a multiplier or suffix overflows
  (§5.4; `floats_boundaries` key `l`).

Consequences of these conditions:

- Because t is rounded toward zero, a value just short of a whole number falls in the last row:
  `2.99999999` → `3.000000`, `0.99999999` → `1.000000`, `-1.99999999` → `-2.000000`. A value just
  past one, away from zero, falls in the middle row: `-2.00000001` → `-2.00000001`.
- The bound is strict: `1e-7` → `0.000000`, and `1.0000001` → `1.000000`.
- Whole numbers outside the 32-bit range fall in the last row: `2147483648.0` → `2147483648.000000`.

Cases: `floats`, `floats_special`, `floats_boundaries`, `times`.

## 10.4 JSON and compact JSON

- The root is always written with its brackets: `{…}` for an object, `[…]` for an array. An empty
  document gives `{}` (`empty_document`, `top_level_array`).
- Pretty JSON puts one member per line, indented four spaces per level, as `"key": value`, joined
  by `,⏎`. Arrays likewise, one element per line. Empty objects and arrays are `{}` and `[]`. There
  is no final line break (`objects`, `arrays`, `scalars`).
- Nested containers are written the same way, one level deeper: after `{` or `[`, a line break;
  each member or element on its own line, indented four more spaces; the members joined by `,⏎`;
  after the last one, a line break, the indentation of the container's own line, and `}` or `]`
  (`nested_layout`, `top_level_array_nested`).
- Keys are always in the JSON form, except the empty key (§10.1).
- Compact JSON is the same without any whitespace: `{"a":1,"b":[1,2]}`.
- Multi-value entries: §10.7.

## 10.5 Config (UCL) format

The config format writes UCL that libucl can read back (`objects`, `arrays`, `scalars`,
`top_level_array`, `libucl/basic/*.res`).

Layout:

- The root object's entries are written without braces. An empty root gives empty output. A root
  array is written `[`…`]` with no final line break.
- Indentation is four spaces per level (`nested_layout`, `top_level_array_nested`).
- A scalar entry: `key = value;⏎`.
- An object entry: `key {⏎ … }⏎`. An array entry: `key [⏎ … ]⏎`. When empty: `key {}⏎`, `key []⏎`.
- Inside arrays, scalar elements are written `value,⏎`, and object and array elements
  `{⏎…}⏎` and `[⏎…]⏎`, with no comma; empty ones are `{}⏎` and `[]⏎` (`arrays`).
- Keys are bare, unless they need quoting (§10.1); then they are in the JSON form.
- A **multi-value entry** is written as one entry line per value, in order, each with the key of
  its own value (§10.1): `k = 1;⏎k {⏎    a = 1;⏎}⏎k [⏎    2,⏎]⏎`
  (`implicit_arrays`, `implicit_array_of_arrays`, `nested_implicit_arrays`,
  `multi_value_mixed_values`).

Strings in the config format:

1. **Heredoc form** `<<EOD⏎content⏎EOD` is used when the string contains a LF **and** either is
   longer than 80 bytes or has a multi-line origin. With a LF and no multi-line origin, a string of
   exactly 80 bytes uses the JSON form and one of 81 bytes the heredoc form (`heredoc_kept`, `long_string_with_newline`,
   `long_string_boundary`, `load_multiline_output`). A multi-line-origin string without a LF uses
   the JSON form (`heredoc_kept`, key `b`). **Exception:** if a line after the first line is
   exactly `EOD` (a LF followed by `EOD` and then a LF or the end), the JSON form is used instead
   (`heredoc_with_eod_line`, `libucl/basic/heredoc_eod`). A first line that is `EOD` does not
   trigger the exception, and needs none, because the first line of a heredoc never ends it
   (§6.3): `"EOD⏎xxx…"` (84 bytes) is written `<<EOD⏎EOD⏎xxx…⏎EOD` (`heredoc_first_line_eod`).
2. Otherwise, **single-quoted form** `'…'`, with each `'` written `\'`, when the string has a
   single-quoted origin. Other bytes are written raw, line breaks included. If the content contains
   a backslash directly followed by `'`, the JSON form is used instead (`single_quoted_kept`,
   `single_quoted_raw_bytes`).
3. Otherwise, the **JSON form**.

## 10.6 YAML

- Root object entries are written `key: value`, one per line, joined by a single LF, without
  braces. An empty root object gives empty output (`empty_document`). A root array is written
  `[`…`]` in the JSON layout of §10.4.
- Nested objects and arrays use the JSON layout of §10.4, with `key: ` before members, and members
  joined by `,⏎` as in JSON.
- Keys are bare unless they need quoting (§10.1); the empty key is written `null`.
- Strings always use the JSON form.
- There is no final line break (`scalars`, `objects`, `arrays`, `top_level_array`).

## 10.7 Multi-value entries in JSON and YAML

A multi-value entry is written as an array under its key, with these **quirks**:

- If the first value is an explicit array, only that array is written; the other values are lost:
  `k = [1, 2]⏎k = 3` → `"k": [1, 2]` (`implicit_array_of_arrays`, `implicit_array_layouts` key `f`).
- The layout depends on the first value. If it is a non-empty string or a non-empty object, the
  array uses the normal layout of §10.4. If it is a number, time, boolean, null, empty string or
  empty object, the array uses an **inline** layout: the first element directly after `[` (with
  its indentation spaces), elements joined by `,⏎` plus indentation, and `]` directly after the
  last element: `"m": [        1,⏎        2]` (`implicit_arrays`, `implicit_array_layouts`,
  `nested_implicit_arrays`, `multi_value_inline_first_kinds`). An empty explicit array as first
  value is written alone, `"j": []`, by the rule above. The other values are written as array
  elements whatever their kinds; a container among them has the normal layout of §10.4 at its
  depth, also inside an inline array (`multi_value_inline_first_kinds` key `l`,
  `multi_value_mixed_values`).
- At the root of YAML, a multi-value entry that is the first entry has nothing before it
  (`multi_value_first_entry_at_root`).
- In YAML at the root level, a multi-value entry that is not the first entry is preceded by `,⏎`
  instead of `⏎`, which leaves a stray comma at the end of the previous line (`implicit_arrays`,
  `implicit_array_layouts`).

Compact JSON writes all multi-value entries as plain arrays, the first-array quirk included.

## 10.8 Reading output back

This section says what libucl makes of output text when it reads it again, for writers that must
round-trip exactly (such as serde serialization, where the project need not use the default
formats above). Reading is ordinary parsing: variables registered at that time expand (§7), and
the reader's flags apply.

**The output of §10.5–§10.7.** Reading the config output back gives the same value tree, apart
from these exceptions (found by reading back the config output of every conformance case):

- A time is written as a float and reads back as `float` (`times`).
- Bytes written `\uFFFD` (§10.2) read back as U+FFFD.
- Floats lose precision: the `%f` row keeps 6 decimals (`3.141592653589793` → `3.141593`), the
  `%.15g` row 15 significant digits. A `%f` output of 127 or more characters, without a leading
  `-`, reads back as a string (§5.3; `cases/spec/05-numbers/float_max`). The `%.15g` output of the
  smallest normal float, `2.2250738585072e-308`, lies below the normal range and is rejected
  (`readback_fifteen_digit_min_normal_error`).
- Keys written bare that cannot be read bare make the output unreadable: the quirk keys of §10.1,
  keys created by `.include` with `key` or `prefix`, and keys that do not begin with a byte a bare
  key may begin with (§3), such as a quoted key `$ABI` (`keys_quoting`,
  `macro_created_keys_quoting`, `cases/spec/03-keys/quoted_no_variable_expansion`). The empty key
  of §10.1 does not read back either: its line ` {` starts a braced root when it comes first
  (§1.1), so its object's entries become the root and the rest is ignored, and it is an error
  anywhere else.
- The JSON form of a string reads back as a double-quoted string, so a reference to a variable
  registered when reading expands, `FILENAME` and `CURDIR` included, and then `$$` collapses
  (§7.5; `cases/spec/07-variables/backslash_dollar`, `cases/spec/12-flags/no_filevars`). A string
  in the single-quoted form is not expanded (§7.2).
- The values of a multi-value entry written with differently spelled keys (§10.1) read back as
  separate keys, unless read with `key-lowercase`. Read with `no-implicit-arrays` or another
  strategy than `append`, repeated keys follow §8 as usual.
- Priorities, saved comments and the marks of inherited and collected values are not written.

JSON and YAML output read back like the config output, except that a multi-value entry becomes an
explicit array, with the loss of §10.7, and that single-quoted strings are written in the JSON form
and so do expand (`cases/spec/06-strings/sq_no_variables`). JSON quotes every key, so the key
exceptions above apply to YAML only. The inline layout, the stray YAML
comma and the unquoted `null` key read back without trouble: `null` becomes the key `"null"`
(`readback_json_output`, `readback_yaml_output`, `readback_config_output`).

**Forms that read back exactly.** Text written in these forms reads back as exactly the value
written:

| Value | Form |
| --- | --- |
| int | decimal, −9223372036854775808 to 9223372036854775807 (§5.3) |
| float | a decimal number with a `.` or an exponent (§5.1) and enough digits to identify the double; 17 significant digits always suffice: `1e16`, `1E16`, `0.10000000000000001`, `1.7976931348623157e308`, `2.2250738585072014e-308`, `-0.0`, `1.5e-10` (`readback_float_forms`). Without `.` or exponent the number reads as `int` (`10000000000000000`). NaN and +∞ are the keywords `nan` and `inf` (§4); −∞ has no keyword, since `-inf` is a string, but a multiplier that overflows gives it: `-1e308k` (§5.4; `readback_float_forms` keys `h` and `i`, `floats_boundaries` key `l`) |
| time | a decimal number, in the int or the float form, followed by `s`: that many seconds, exactly as the number reads as a float: `1.5s`, `1e-3s`, `0.30000000000000004s`, `-2.5s`, `1.5e3s` (`readback_time_forms`). Other suffixes multiply and can round (§5.4) |
| string | the JSON form with the escapes of §6.1, which can express every byte (`\u0000` for NUL), unless the string contains a reference to a variable registered when reading, or one a variable handler resolves (§7.7); the single-quoted form (§6.2), which never expands, for any string that does not end in a backslash and has no backslash directly before a `'` |
| key | double-quoted, with the escapes of §6.1, any bytes except the empty key; keys never expand (§7.2); `key-lowercase` lowercases them when reading (§12.1) |
| true, false, null | `true`, `false`, `null` |
| empty object, empty array | `{}`, `[]` |
| multi-value entry | one entry per value with the same key, read with the `append` strategy and without `no-implicit-arrays` |

Some values cannot be written so that libucl reads them back:

- **Floats beyond the range**: a literal larger than the largest double is an error
  (`readback_float_above_max_error`); only the infinities themselves can be written, as above.
- **Subnormal floats**: every literal below the normal range is an error
  (`cases/spec/05-numbers/subnormal_error`).
- A string that contains a reference to a registered variable **and** ends in a backslash or has
  a backslash before a `'`: the JSON form expands the reference, and the single-quoted form cannot
  hold it. There is no escape that prevents expansion in a double-quoted string, because `\$` is
  decoded before expansion (§7.6).
- The empty key (§10.1), which parsing rejects (§3).

## 10.9 How the upstream `.res` files are produced

Each `libucl/basic/N.res` is the result of two passes:

1. Parse `N.in` with `key-lowercase` set, `ABI` = `unknown`, and the file variables set from the
   input path.
2. Write the result in the config format.
3. Parse that output again, as a string document (§7.8), with `key-lowercase` set and no `ABI`
   variable.
4. Write the result in the config format again, followed by one extra LF. That output is the
   `.res` file.

A byte-exact comparison against `.res` must repeat both passes. Where the first pass writes
something that reads back differently, the `.res` shows the second reading. For example, a float
whose `%f` output (§10.3), without a leading `-`, has 127 or more characters reads back as a
string (§5.3), and a bare key containing `;` does not read back at all.

Every upstream `.res` file is reproduced exactly by the oracle build using this procedure, extra
final LF included, with one condition. `libucl/basic/14` includes `./1.in` with `try=true`, and its `.res` was produced from a
working directory where that relative path does not exist, so the include has no effect. The typed
golden file of case 14 was produced from the case's own directory, where the include succeeds. A
tier-2 comparison of case 14 must therefore run from another directory.

## 10.10 Config output with saved comments

libucl's config format can also write the comments saved under `save-comments` (§12.5), when the
application passes them to the writer; JSON and YAML never write comments. Every case whose
`.flags` include `save-comments` or `dump-comments` has this output as
`<case>.config-comments.golden`. It is the config output of §10.5 with these additions, for every
value written, the root and array elements included:

- Comments attached **before** a value (`"c"`, §12.5) are written at the start of the value's
  line, after its indentation: each comment's saved bytes, then LF and the indentation again. The
  value follows as usual: `# c1⏎a = 1` → `# c1⏎a = 1;⏎`; inside `b {…}` → `    # in b⏎    c [`
  (`comments_output_before_and_after`, `comments_output_array_elements`,
  `comments_output_top_level_array`).
- Comments attached **after** a value (`"ca"`) are written after the whole value, that is after its
  `;⏎` or `,⏎`, after the LF that follows its closing bracket, or, for the root, after the end of
  the output: each comment's saved bytes followed by LF. **Quirk:** the first of them starts at the
  beginning of the line, without indentation; each later one is preceded by the value's
  indentation: `b {⏎    c = 1;⏎# t1⏎    # t2⏎}⏎` (`comments_output_nested_after`,
  `comments_output_root_only`).
- Comments are written exactly as saved: the CR of a line comment stays, and a block comment keeps
  the byte after its `*/` (§12.5), so `/* x */` followed by a line break is written `/* x */⏎⏎`
  (`comments_output_block_and_cr`).
- Each value of a multi-value entry has its own comments (`comments_output_multi_value`).
- `save-comments` alone saves the same comments as `dump-comments`
  (`comments_output_save_comments_flag`).

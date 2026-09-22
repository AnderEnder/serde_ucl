# 10. Output formats

Cases: `tests/conformance/cases/spec/10-output/`. Each case there has, besides its typed dump, the
exact bytes libucl writes for the parsed value in four formats: `<case>.config.golden`,
`<case>.json.golden`, `<case>.json-compact.golden` and `<case>.yaml.golden`. **Those files are
normative**; the rules below describe them. The upstream `libucl/basic/*.res` files are config
output produced by a two-pass procedure (§10.9).

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

Priorities are never written (`priorities_not_emitted`). Output of a parse error is not defined.

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
| everything else, including `/` and bytes ≥ 0x80 | unchanged (`string_utf8`) |

The same form is used for keys that need quoting.

## 10.3 Floats and times

Let v be the value and t the value rounded toward zero. The output is the bytes that the ISO C
`printf` conversion in the second column produces for v, in the C locale:

| Condition on v | Conversion | Examples |
| --- | --- | --- |
| v is a whole number and −2³¹ ≤ v ≤ 2³¹−1 | `%.1f` | `2.0`, `-0.0`, `10.0`, `86400.0`, `2147483647.0`, `-2147483648.0` |
| v is not a whole number, −2³¹ ≤ t ≤ 2³¹−1, and \|v − t\| < 10⁻⁷ | `%.15g` | `1e-10`, `9e-08`, `-9e-08`, `2.00000001`, `-2.00000001`, `5.00000009` |
| any other v, including NaN, +∞ and −∞ | `%f` | `1.500000`, `0.100000`, `100.250000`, `3000000000.500000`, `2147483648.000000`, `-2147483649.000000`, `100000000000000000000.000000`, `nan`, `inf`, `-inf` |

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
- Keys are always in the JSON form.
- Compact JSON is the same without any whitespace: `{"a":1,"b":[1,2]}`.
- Multi-value entries: §10.7.

## 10.5 Config (UCL) format

The config format writes UCL that libucl can read back (`objects`, `arrays`, `scalars`,
`top_level_array`, `libucl/basic/*.res`).

Layout:

- The root object's entries are written without braces. An empty root gives empty output. A root
  array is written `[`…`]` with no final line break.
- Indentation is four spaces per level.
- A scalar entry: `key = value;⏎`.
- An object entry: `key {⏎ … }⏎`. An array entry: `key [⏎ … ]⏎`. When empty: `key {}⏎`, `key []⏎`.
- Inside arrays, scalar elements are written `value,⏎`, and object and array elements
  `{⏎…}⏎` and `[⏎…]⏎`, with no comma; empty ones are `{}⏎` and `[]⏎` (`arrays`).
- Keys are bare, unless they need quoting (§10.1); then they are in the JSON form.
- A **multi-value entry** is written as one entry line per value, all with the same key, in order
  (`implicit_arrays`, `implicit_array_of_arrays`, `nested_implicit_arrays`).

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
   a backslash directly followed by `'`, the JSON form is used instead (`single_quoted_kept`).
3. Otherwise, the **JSON form**.

## 10.6 YAML

- Root object entries are written `key: value`, one per line, without braces. An empty root
  object gives empty output (`empty_document`). A root array is written `[`…`]`.
- Nested objects and arrays use the JSON layout of §10.4, with `key: ` before members.
- Keys are bare unless they need quoting (§10.1).
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
  `nested_implicit_arrays`).
- In YAML at the root level, a multi-value entry that is not the first entry is preceded by `,⏎`
  instead of `⏎`, which leaves a stray comma at the end of the previous line (`implicit_arrays`,
  `implicit_array_layouts`).

Compact JSON writes all multi-value entries as plain arrays, the first-array quirk included.

## 10.8 Round trips

The config format generally reads back to the same value, with these exceptions: bytes that became
`\uFFFD` (§10.2), keys that were not quoted although they needed it (§10.1), floats written with six
decimals, and the multi-value quirks of §10.7 in JSON and YAML.

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

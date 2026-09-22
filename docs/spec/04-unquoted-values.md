# 4. Unquoted values

Cases: `tests/conformance/cases/spec/04-atoms/`.

A value that does not start with `"`, `'`, `{`, `[`, or a heredoc opener (§6.3) is an
**unquoted value**. Its extent is defined in §4.1–§4.3 and its meaning in §4.4–§4.6.

## 4.1 Extent

An unquoted value runs from its first byte up to, but not including, the first of:

- LF, CR or NUL, or `,` or `;`;
- `#` (§2.2);
- the start of a block comment `/*` (§2.4);
- a `}` or `]` that has no matching `{` or `[` earlier in the same value;
- the end of input.

Everything else belongs to the value, including spaces, `=`, `:`, `@`, `%`, `!`, `$`, `/` and
control characters other than those above:

- `hello world` → `"hello world"` (`string_with_spaces`, `cases/review/03_numword`)
- `http://pkg.freebsd.org/latest` → `"http://pkg.freebsd.org/latest"` (`url`, `cases/review/01_url`,
  `cases/additions/a33_url_in_array`)
- `/usr/local/etc` (`path`); `query %dn-%dv`, `a=b`, `a:b`, `x@y!z` (`punctuation`,
  `cases/review/04_percent`, `cases/additions/a45_eq_in_value`, `cases/additions/a46_colon_in_value`)
- a control byte such as 0x01 is kept (`control_chars_allowed`)
- `x,b = y;c = z` stops at each terminator (`value_ends_at_terminators`, `cases/additions/a28_value_semicolon_space`)

Inside an array, the same rules apply, so an element ends at `,`, `;`, a line break or the closing
`]` (§1.5).

## 4.2 Brackets inside a value

Bracket pairs inside an unquoted value are kept, counted separately for `{}` and `[]`:

- `some[]value`, `a[b]c` → kept whole; in an array, `[a[b]c, d]` → `["a[b]c", "d"]`
  (`brackets_balanced`, `cases/review/02_brackets`)
- `a{b}c` → `"a{b}c"`; `a{b` (never closed) → `"a{b"` (`braces_balanced`, `cases/additions/a47_brace_in_value`)
- An unmatched `}` or `]` ends the value and then closes the enclosing container. Where that does
  not fit, it is an error: `k { v = a}b }` and `k = a]b` are errors
  (`unbalanced_close_brace_error`, `unbalanced_close_bracket_error`).

## 4.3 Trailing whitespace

Spaces and tabs at the end of the value are removed; spaces inside are kept (`trailing_whitespace_stripped`).
A value that is empty after this is an error (§1.6).

## 4.4 Numbers

A value that starts with an ASCII digit or `-` may be a number (`int`, `float` or `time`); §5
defines which ones are. Whether it is a number depends on the bytes after the number (§5.5, §5.6),
not only on the extent of §4.1: for example `30/* c */` is the string `"30"`. §4.5 and §4.6 cover
the values that are not numbers.

## 4.5 Keywords

Keywords are compared against the whole value after trailing whitespace is removed
(`keywords_with_trailing_space`):

| Value | Result | Case sensitivity |
| --- | --- | --- |
| `true`, `yes`, `on` | `true` | ASCII case-insensitive: `TRUE`, `True`, `YES`, `ON` too |
| `false`, `no`, `off` | `false` | ASCII case-insensitive |
| `null` | `null` | lowercase only; `NULL` and `Null` are strings |
| `nan` | `float NaN` | lowercase only |
| `inf` | `float +∞` | lowercase only; `-inf`, `+inf`, `Inf`, `infinity` are strings |

Cases: `bool_true_forms`, `bool_false_forms`, `null_lowercase_only`, `nan_inf_lowercase_only`,
`bool_lookalikes_are_strings` (`true_`, `yess`, `on1` are strings), `cases/review/28_true_bare`,
`cases/additions/a12_bool_prefix`, `cases/additions/a13_case_bool`, `cases/additions/a17_neg_inf`,
`libucl/basic/1` (`yes`, `no`), `libucl/basic/8` (`flag on`).

Quoted strings are never keywords: `"true"`, `'yes'`, `"null"` stay strings (§6).

## 4.6 Strings

Any other unquoted value is a string. Its content is the value's text with backslash escapes
decoded (§4.7) and variables expanded (§7). A `$` written as `\$` is not expanded (§7.6).

## 4.7 Backslash escapes in unquoted values

A backslash makes the next byte part of the value, even a terminator or `#`:
`x\;y` → `"x;y"`, `x\#y` → `"x#y"`, `x\,y` → `"x,y"` (`backslash_escaped_terminators`).

The resulting text is decoded like a double-quoted string (§6.1): `x\ny` → `"x⏎y"`,
`x\ty\"z` → `"x⇥y\"z"`, `a\u0041b` → `"aAb"`. An unknown escape drops the backslash:
`x\q` → `"xq"` (`backslash_escapes`). A backslash as the very last byte of the value is kept:
`string that ends in slash\` at end of input → `{ string: "that ends in slash\\" }`
(`backslash_at_end`, `cases/review/05_backslash`, `libucl/basic/17`).

## 4.8 `\u` escapes in unquoted values

**Quirk.** Unlike double-quoted strings, an invalid `\u` escape in an unquoted value is not an
error. When at least four characters follow `\u`, the escape covers `\u` and those four
characters. If all four are hex digits, the code point is their value. Otherwise it is 16 × the
value of the hex digits before the first non-hex character among the four, or 0 if there are none:

- `x\uZZZZ` → `"x\u0000"`; `x\u00ZZ` → `"x\u0000"`; `x\u4Z00y` → `"x@y"` (U+0040)
  (`backslash_invalid_unicode`)
- `x\u1ZZZ` → `"x\u0010"`, `x\u12ZZ` → `"x\u0120"`, `x\u123Z` → `"x\u1230"`
  (`backslash_invalid_unicode_prefixes`)

When fewer than four characters of the value follow `\u`:

- With three, the code point is found the same way, with the end of the value counting as a
  fourth, non-hex character, and the three characters are part of the escape:
  `x\u123` → `"x\u1230"`, `x\u1Z2` → `"x\u0010"` (`backslash_short_unicode_at_end`).
- **Quirk.** With two or fewer, the `\` is dropped, the `u` is kept, the first character after
  it is dropped, and the rest of the value follows unchanged, its own escapes decoded: `x\u12` → `"xu2"`, `x\u41` → `"xu1"`,
  `x\u1` → `"xu"`, `x\uZ` → `"xu"`, `x\u` → `"xu"`, `a\u\n` → `"aun"`
  (`backslash_short_unicode_at_end`).

## 4.9 `$` in unquoted values

`$` is an ordinary byte unless it forms a variable reference (§7): `a = $` → `"$"`;
`cost$5` → `"cost$5"` (`value_with_dollar`, `cases/additions/a36_bare_dollar`).

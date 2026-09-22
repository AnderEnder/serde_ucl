# 3. Keys and named sections

Cases: `tests/conformance/cases/spec/03-keys/`.

## 3.1 Bare keys

- The first byte is an ASCII letter, an ASCII digit, `/`, `_`, or any byte from 0x80 to 0xFF
  (`starts_with_digit`, `starts_with_slash`, `starts_with_underscore`, `non_ascii`,
  `cases/additions/a10_numeric_key`, `cases/review/19_utf8key`).
- The following bytes are ASCII letters, digits, `_`, `-`, `.`, `/`, or bytes from 0x80 to 0xFF
  (`bare_charset`, `cases/additions/a11_dash_key`).
- A bare key ends at a space, a tab, `=` or `:`. Any other byte directly after it is an error;
  this includes `{`, `[` and a line break: `a{b = 1}`, `a[1, 2]`, `a⏎{ b = 1 }` are errors
  (`brace_directly_after_key_error`, `bracket_directly_after_key_error`,
  `newline_directly_after_key_error`, `invalid_char_in_key_error`, `dollar_in_bare_key_error`).
- A key cannot start with `-` (`starts_with_dash_error`). A key position starting with `.` is a
  macro (§9).
- Dots have no special meaning: `a.b = 1` is the key `"a.b"` (`dots_are_literal`,
  `cases/review/29_dot_key`).
- Keys are case-sensitive: `Key` and `key` are different keys (`case_sensitive`), unless
  `key-lowercase` is set (§12.1).
- Variables are never expanded in keys; `$` is not a valid bare-key byte (§7.5).

## 3.2 Quoted keys

- A key may be written as a double-quoted string with the escapes of §6.1: `"a b" = 1`,
  `"a\nb" = 1`, `"\u0041" = 2` → key `"A"` (`quoted`, `quoted_escapes`). Variables are not
  expanded in them (`quoted_no_variable_expansion`).
- The empty key is an error: `"" = 1` (`quoted_empty_error`, `cases/review/18_emptykey`).
- Single-quoted keys are an error: `'k' = 1` (`single_quoted_error`,
  `cases/additions/a42_key_quoted_single`).
- A quoted key may be followed directly by `{`, `[`, `=` or `:`: `"a"{b = 1}`, `"c"[1]`
  (`quoted_brace_directly_after`).
- **Quirk.** A quoted key may be followed by a line break. The value then starts on the next line,
  and a separator there becomes part of the value: `"a"⏎= 1` → `{ a: "= 1" }`
  (`quoted_newline_before_separator`).
- A quoted key followed by a quoted value with no separator is an error: `"a" "b" = 1`
  (`quoted_followed_by_quoted_value_error`).

## 3.3 Keys that need a value

See §1.6: a key alone on a line, or followed by `;`, is an error.

## 3.4 Named sections

A key followed by further names and then `{` or `[` creates nested objects, one per name:

- `section blah { param = "value" }` → `{ section: { blah: { param: "value" } } }` (`section_one_name`)
- `sub one two { key = 1 }` → `{ sub: { one: { two: { key: int 1 } } } }` (`section_two_names`,
  `cases/review/09_twonames`)
- Any key can have names; no keyword is special (`section_any_keyword`).
- Names may be bare keys (§3.1, including all-digit names) or double-quoted strings
  (`section_quoted_names`, `section_mixed_names`, `section_numeric_names`). A single-quoted name
  is an error (`section_single_quoted_name_error`).
- With `[`, the innermost name holds the array: `a b [1, 2]` → `{ a: { b: [int 1, int 2] } }`
  (`section_array_value`).
- `libucl/basic/8` and `libucl/basic/10` use this form throughout.

Which words are names. This applies to a key written without a separator. For any word, "the
rest of its line" is the text after it up to the first LF, CR, `,` or `;`.

- The key is a section name if and only if the rest of its line contains `{` or `[` anywhere,
  including inside quotes or inside a word. Otherwise the rest of the line is the key's value:
  - `a b c = 1` → `{ a: "b c = 1" }` (`section_names_then_separator_is_value`)
  - `a b c 1` → `{ a: "b c 1" }` (`section_names_then_atom_is_value`)
  - `k v⏎x { y = 1 }` → `{ k: "v", x: { y: int 1 } }` (`section_lookahead_stops_at_newline`)
  - `k v; x { y = 1 }` → `{ k: "v", x: { y: int 1 } }` (`section_lookahead_stops_at_semicolon`)
  - **Quirk.** `k a{b` and `k value with { brace` are errors, because the bracket on the line makes
    the words names (`section_lookahead_brace_in_value_error`,
    `section_lookahead_brace_later_on_line_error`).
- After a name, each following word is a name by the same test. The last name holds the object or
  array that follows it. A word whose rest of line has no `{` or `[` is an ordinary key, and the
  rest of the line is its value.
- **Quirk.** A bracket inside a quoted word makes the words before it names, but not the quoted
  word itself, which is then an ordinary key. `a "x{y"⏎` → `{ a: { "x{y": null } }`: a quoted key
  may be followed by a line break (§3.2), and the end of input then gives `null` (§1.6)
  (`section_path_brace_inside_quotes`). The objects created for such names have no closing
  bracket, so every later entry of the document goes into the innermost one:
  `x = 1⏎c "x{y" z⏎d = 1` → `{ x: int 1, c: { "x{y": "z", d: int 1 } }` (`section_path_left_open`).
  Inside braces, the enclosing `}` is then an error (`section_path_left_open_inside_braces_error`).
- **Quirk.** A `=` or `:` after a word that follows a name is ignored, and that word is a name too:
  `a b = c {d = 1}` → `{ a: { b: { c: { d: int 1 } } } }` (`section_path_separator_is_ignored`).
  Since such a word never holds the value itself, `a b = [1]` and `a b = {d = 1}` are errors
  (`section_path_separator_after_name_error`, `section_path_separator_after_name_object_error`).
- The bracket must be on the same line: `a b⏎{ c = 1 }` is an error, because `b` is the value of
  `a` and a `{` cannot start an entry (`section_newline_before_brace_error`).
- A bare key followed by a quoted string and a separator is an error: `a "b" = 1`
  (`quoted_value_after_bare_key_error`).

The nested objects are ordinary objects. Repeating a section does **not** merge it with the
earlier one: the outermost key receives another value (§8.2).

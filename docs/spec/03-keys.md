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
- Whitespace after a quoted key is optional. It may be followed directly by `{`, `[`, `=` or `:`
  (`"a"{b = 1}`, `"c"[1]`; `quoted_brace_directly_after`), by its value in any form
  (`"a"x` → `{ a: "x" }`, `"b"1` → `{ b: int 1 }`, `"c"'q'` → `{ c: "q" }`, `"d""r"` → `{ d: "r" }`,
  `"e"true` → `{ e: true }`, `quoted_key_value_adjacent`; `"a"<<EOD⏎x⏎EOD` → `{ a: "x" }`,
  `quoted_key_heredoc_adjacent`), by a comment (`"a"#c⏎v` → `{ a: "v" }`, `"b"/*c*/w` →
  `{ b: "w" }`, `quoted_key_comment_adjacent`), or by section names (`"a"b {c = 1}` →
  `{ a: { b: { c: int 1 } } }`, `quoted_key_names_adjacent`).
- A quoted key at the end of input, with or without spaces after it, is an error: `"a"`, `"a" `
  (`quoted_key_at_end_error`, `quoted_key_space_at_end_error`).
- **Quirk.** A quoted key may be followed by a line break. The value then starts on the next line,
  and a separator there becomes part of the value: `"a"⏎= 1` → `{ a: "= 1" }`
  (`quoted_newline_before_separator`).
- A quoted value after a quoted key must be followed by a terminator like any quoted value
  (§1.3): `"a" "b" = 1` is an error because ` = 1` follows the value `"b"`
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

Which words are names. This applies to a key written without a separator; the first key may be
bare or double-quoted: `"a" b {x = 1}` → `{ a: { b: { x: int 1 } } }` (`section_quoted_first_key`).
For any word, "the rest of its line" starts at the next word, after the spaces, tabs and comments
that follow the word, and runs to the first LF, CR, `,` or `;`. A line comment includes its line
break, so the next word may be on a later line. Comments inside the rest of the line are part of
it, so a bracket in them counts:

- `a # {⏎b = 1` → `{ a: "b = 1" }`: the comment is not part of the rest of the line, which starts
  at `b` and has no bracket (`section_lookahead_after_line_comment`).
- `a /* { */ b = 1` → `{ a: "b = 1" }`; `x /* { */ y {c = 1}` → `{ x: { y: { c: int 1 } } }`
  (`section_lookahead_after_block_comment`).
- `k v # {` → **error**: the rest of the line from `v` contains the `{` of the comment, so `k` is a
  name, and `v` is then a key with no value (`section_lookahead_counts_comment_text_error`).


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
  bracket, so they stay open, and later entries go into the innermost one:
  `x = 1⏎c "x{y" z⏎d = 1` → `{ x: int 1, c: { "x{y": "z", d: int 1 } }` (`section_path_left_open`).
  They close when one of these happens:
  - A container written with brackets that was opened in them closes: an object or array value,
    or the objects of another section path. Then every object left open this way closes with it,
    back to the nearest enclosing object written with a bracket, or to the root, and later entries
    go there. `x "y{" z⏎a { b = 1 }⏎d = 1` → `{ x: { "y{": "z", a: { b: int 1 } }, d: int 1 }`
    (`section_path_left_open_closed_by_object`); the same with `a = [1]` or `a b { x = 1 }` in
    place of `a { b = 1 }` (`section_path_left_open_closed_by_array`,
    `section_path_left_open_closed_by_section`), with two left-open names `p q "y{" z`
    (`section_path_left_open_two_names`), and with a second left-open path inside the first, which
    both close (`section_path_left_open_nested_paths`). Inside braces, they close back to the
    braced object: `k { c "x{y" z⏎a { b = 1 }⏎}⏎m = 1` →
    `{ k: { c: { "x{y": "z", a: { b: int 1 } } }, m: int 1 }`
    (`section_path_left_open_inside_braces_closed`). Containers nested deeper do not close them:
    `c "x{y" z⏎d { e { f = 1 } g = 2 }⏎h = 1` →
    `{ c: { "x{y": "z", d: { e: { f: int 1 }, g: int 2 } }, h: int 1 }`
    (`section_path_left_open_inner_container`).
  - The input ends. The end of an included file does not close them, and they affect what is
    checked at the end of a unit (§9.4). A macro directly after a name leaves the name's object
    open in the same way (§9.1).
  A `}` that arrives while they are open is an error (`section_path_left_open_inside_braces_error`,
  `section_path_left_open_extra_close_error`).
- **Quirk.** A `=` or `:` after a word that follows a name is ignored, and that word is a name too:
  `a b = c {d = 1}` → `{ a: { b: { c: { d: int 1 } } } }` (`section_path_separator_is_ignored`).
  The word may be quoted: `a "x{" = y {z = 1}` → `{ a: { "x{": { y: { z: int 1 } } } }`, while
  `a "x{" = y` is an error, because `y` is then a key with no value
  (`section_quoted_name_then_separator`, `section_quoted_name_then_separator_error`).
  Since such a word never holds the value itself, `a b = [1]` and `a b = {d = 1}` are errors
  (`section_path_separator_after_name_error`, `section_path_separator_after_name_object_error`).
  When such a separator is followed on its line only by spaces and then a line break (LF, CR, VT
  or FF), the next name may come after any whitespace and comments, on a later line, and if the
  input ends first the objects are kept, empty: `c "x{" =⏎d {}`, `c "x{" =⏎# c⏎ d {}` and
  `c "x{" = ⏎ d {}` → `{ c: { "x{": { d: {} } } }`; `c "x{" =⏎ d x` →
  `{ c: { "x{": { d: "x" } } }`; `c "x{" =⏎`, `c "x{" =<VT>` and `c "x{" =⏎# c⏎` →
  `{ c: { "x{": {} } }` (`section_separator_newline_then_name`,
  `section_separator_newline_comment_then_name`, `section_separator_space_newline_then_name`,
  `section_separator_newline_then_key_value`, `section_separator_newline_at_end`,
  `section_separator_vt_at_end`, `section_separator_newline_comment_at_end`). It is an error when
  the input ends directly after the separator or its spaces, or after a comment on its line,
  because that comment takes the line break: `c "x{" =`, `c "x{" =␠`, `c "x{" = # c⏎`
  (`section_separator_at_end_error`, `section_separator_space_at_end_error`,
  `section_separator_comment_takes_newline_error`). It is also an error when a bracket, or a `#`
  that is the last byte (§2.2), stands where the next name should start: `c "x{" =⏎{ }`,
  `c "x{" =⏎ #` (`section_separator_newline_bracket_error`,
  `section_separator_newline_last_hash_error`).
- VT and FF around names. The rest of a word's line starts after spaces, tabs and comments (above),
  so a VT or FF there is part of it and does not end it. When the rest of the line makes the word
  a name, VT and FF before the next name are skipped like spaces: `a <FF>b { c = 1 }`,
  `d e <FF><FF>f { g = 1 }`, `h i <FF> /* x */ j { k = 1 }`, `l m /* x */<FF>n { o = 1 }`,
  `p q = <FF>r { s = 1 }` and `t u <VT>v { w = 1 }` all give nested objects
  (`section_names_vt_ff_between_names`). They do not stand for the space before the bracket:
  `a <FF>{ b = 1 }` and `a b <FF>{ d = 1 }` are errors, because the bracket then stands where a
  name should start (`section_ff_before_bracket_error`, `section_name_then_ff_before_bracket_error`).
  If the rest of the line has no bracket, the word takes its value by §1.6:
  `a <FF># c⏎b { c = 1 }` → `{ a: "b { c = 1 }" }` (`section_lookahead_ff_then_line_comment`).
  Inside a bare name a VT or FF is invalid (§3.1): `a b<FF>c { d = 1 }` is an error
  (`section_ff_inside_name_error`).
- **Quirk.** Because a comment after a VT or FF is part of the rest of the line, a bracket inside
  it makes the word a name. What follows then behaves as after a separator and a line break
  (above): the next name may come after any whitespace and comments, on any later line, and if the
  input ends first the objects are kept, empty: `a <FF># {`, `"a" <FF>/* { */`,
  `a <VT><FF>/* { */<VT>` and `a <FF># {⏎⏎` → `{ a: {} }`; `a b <FF># {` →
  `{ a: { b: {} } }`; `a <FF># {⏎⏎b {}` and `a <FF>/* { */⏎b {}` → `{ a: { b: {} } }`
  (`section_bracket_in_comment_after_ff_at_end`, `section_bracket_in_block_comment_after_ff_at_end`,
  `section_bracket_in_comment_after_vt_ff_at_end`,
  `section_bracket_in_comment_after_ff_then_blank_lines`,
  `section_bracket_in_comment_after_ff_two_names`,
  `section_bracket_in_comment_after_ff_name_on_later_line`,
  `section_bracket_in_block_comment_after_ff_name_next_line`). A `#` that is the last byte after
  whitespace is an error there (§2.2): `a <FF># {⏎ #` and `a <FF>/* { */ #`, while
  `a <FF># {⏎#` → `{ a: {} }` (`section_bracket_in_comment_after_ff_last_byte_hash_error`,
  `section_bracket_in_block_comment_after_ff_last_byte_hash_error`,
  `section_bracket_in_comment_after_ff_last_byte_hash_saved`). The next key is a name even with a
  separator, as above, so `"a" <FF># {⏎b = 1` is an error
  (`section_bracket_in_comment_after_ff_separator_key_error`).
- The bracket must be on the same line: `a b⏎{ c = 1 }` is an error, because `b` is the value of
  `a` and a `{` cannot start an entry (`section_newline_before_brace_error`).
- A bare key followed by a quoted string and a separator is an error: `a "b" = 1`
  (`quoted_value_after_bare_key_error`).

The nested objects are ordinary objects. Repeating a section does **not** merge it with the
earlier one: the outermost key receives another value (§8.2).

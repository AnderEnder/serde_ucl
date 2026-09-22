# 1. Document structure and separators

Cases: `tests/conformance/cases/spec/01-structure/`.

## 1.1 The root value

The root value may be preceded by whitespace (space, tab, LF, CR, VT, FF) and comments (§2). Then:

- If the next byte is `[`, the root is an **array** (`top_array`).
- If it is `{`, the root is an **object written with braces** (`top_braced`, `top_braced_json`,
  `libucl/basic/1`, `libucl/basic/11`).
- Otherwise the root is an **object without braces**: entries run to the end of the input
  (`top_implicit`).

An empty document, or one with only whitespace and comments, gives an empty object `{}`
(`empty_document`, `whitespace_only_document`, `comment_only_document`).

**Quirk.** Once the closing `]` or `}` of a braced root has been read, the rest of the input is
ignored, whatever it contains:

- `[1, 2] anything here` → `[int 1, int 2]` (`top_array_trailing_ignored`)
- `{ a = 1 } b = 2` → `{ a: int 1 }` (`top_braced_trailing_ignored`)
- `{ a = 1 }⏎{ b = 2 }` → `{ a: int 1 }` (`top_braced_second_object_ignored`, `cases/additions/a25_top_multi_brace`)

A `}` or `]` that closes nothing is an error: `a = 1 }`, `}`, `]` (`close_without_open_error`,
`cases/spec/11-errors/unexpected_close_top_error`, `cases/spec/11-errors/lone_bracket_error`).

## 1.2 Entries and separators

An object holds entries. An entry is a key (§3), an optional separator, and a value.

- The separator is `=`, `:`, or nothing at all. Spaces and tabs around it are optional
  (`sep_equals`, `sep_colon`, `sep_none`, `sep_no_spaces`, `sep_space_before_only`,
  `cases/additions/a32_key_colon_nospace`).
- At most one separator is allowed: `a == b`, `a =: b`, `a := b` are errors
  (`sep_double_equals_error`, `sep_equals_colon_error`, `sep_colon_equals_error`).
- With no separator, the key must be followed by a space or tab (§3.1); what follows is either
  more section names (§3.4) or the value.

## 1.3 Terminating an entry

After a value come optional spaces and tabs, then any number of **terminators**: LF, CR, `,`,
`;` or a NUL byte, in any mix and repetition (`term_semicolon`, `term_comma`, `term_repeated`,
`term_newline_then_semicolon`, `term_crlf`, `term_nul_byte`). A comment after a value also counts
as a terminator (§2.4). A trailing terminator before the end of an object or of the input is
allowed.

A terminator before the first entry is an error, at the root as inside braces (§1.4):
`;a = 1`, `,a = 1` and `⏎;⏎a = 1` are rejected (`root_leading_semicolon_error`,
`root_leading_comma_error`, `root_leading_terminator_after_newline_error`).

What counts as "the value" depends on its kind:

- An **unquoted value** extends to the next terminator on the line, spaces included (§4):
  `a = 1 b = 2` → `{ a: "1 b = 2" }` (`value_spans_rest_of_line`).
- After a **quoted string** or **heredoc** (§6), the next thing must be whitespace, a comment, a
  terminator, or the closing bracket of the enclosing container. Anything else is an error:
  `a = "x" b = 2`, `a = "x"b = 2` (`missing_delimiter_after_string_error`,
  `missing_delimiter_string_adjacent_error`).
- After an **object or array** value, the next entry may follow on the same line without a
  terminator: `a = {x = 1} b = 2` → `{ a: {x: int 1}, b: int 2 }` (`object_value_needs_no_delimiter`,
  `cases/additions/a26_implicit_after_brace`).

## 1.4 Objects

An object value is `{ entries }`. The key may be followed directly by the brace, or by a
separator and the brace: `a { … }`, `a = { … }`, `a: { … }` (`object_equals_brace`,
`object_colon_brace`, `cases/review/24_nosep_obj`). Objects nest to any depth within the limit of
§11.2 (`nested_objects`, `cases/additions/a41_nested_obj_semicolons`).

- Empty objects: `{}`, `{ }`, `{⏎}` (`object_empty_forms`).
- A terminator after the last entry is allowed: `{ b = 1; }`, `{ d = 1, }` (`object_trailing_separators`,
  `cases/additions/a35_json_trailing_comma`).
- A terminator before the first entry is an error: `{ , b = 1 }`, `{ ; b = 1 }`
  (`object_leading_comma_error`, `object_leading_semicolon_error`).
- A missing `}` is an error, and so is a `]` closing an object (`unterminated_object_error`,
  `unterminated_top_brace_error`, `mismatched_close_object_error`, `cases/errors/e08_unterminated_object`).

## 1.5 Arrays

An array value is `[ values ]`, where values are separated by terminators (`,`, `;` or line
breaks).

- `array_basic`, `array_nested`, `array_key_without_separator` (`a [1, 2]`),
  `cases/review/20_arraynested`, `cases/review/21_objinarray`.
- Trailing and repeated separators are allowed: `[1, 2,]`, `[1,, 2]` (`array_trailing_comma`,
  `array_double_comma`, `array_of_objects_trailing_comma`). A separator before the first element
  is an error: `[, 1]` (`array_leading_comma_error`).
- Empty arrays: `[]`, `[ ]`, `[⏎]` (`array_empty_forms`).
- Line breaks and `;` separate elements too: `[⏎1⏎2⏎]`, `[1; 2]` (`array_newline_separated`,
  `array_semicolon_separated`).
- An unquoted element extends to the next `,`, `;`, `]` or line break, spaces included (§4):
  `[x y, z w]` → `["x y", "z w"]`; `[1 2 3]` → `["1 2 3"]` (`array_atoms_with_spaces`,
  `array_space_separated_numbers`, `cases/additions/a08_array_no_commas`).
- Quoted strings need a separator: `["x" "y"]` is an error (`array_strings_need_separator_error`).
- Object and array elements need none: `[[1] [2]]`, `[{x = 1} {y = 2}]` (`array_containers_need_no_separator`,
  `cases/additions/a09_array_objs_no_comma`).
- Comments may appear between elements (`array_comment_inside`; the `"2"` there is §5.6).
- A missing `]` is an error, and so is a `}` closing an array (`unterminated_array_error`,
  `mismatched_close_array_error`, `cases/errors/e07_unterminated_array`).
- Macros are not recognised inside arrays (§9.1).

## 1.6 Missing values

- A bare key followed by a line break, `;` or the end of input, with no separator, is an error: `a⏎`,
  `a;` (`key_without_value_error`, `key_semicolon_error`, `cases/additions/a19_key_only`). A quoted key
  may be followed by a line break (§3.2).
- A separator followed by a terminator is an error (empty value): `a = ;` (`key_equals_semicolon_error`,
  `cases/additions/a18_empty_value`).
- A value without a key is an error: `= 1` (`value_without_key_error`).

**Quirk: a separator with nothing after it on its line.** Spaces, tabs and comments after the
separator are not part of the value; a line comment includes its line break.

- If the input ends within them, the document is an error: `a =`, `a = `, `a = /* c */` and
  `a = # c⏎` at the end of input (`key_equals_eof_error`, `empty_value_space_at_end_error`,
  `empty_value_block_comment_at_end_error`, `empty_value_line_comment_at_end_error`).
- If they are followed by anything other than a line break, the value starts there:
  `b = # c⏎ 2` → `{ b: int 2 }` (`cases/spec/02-comments/between_separator_and_value`).
- If they are followed by a line break, the value is on a following line. Blank lines and
  whitespace before it are not part of the value, and neither is a group of comments directly
  after them, one right after another. The value starts immediately after that group, so
  whitespace after the group belongs to the value, and a line break directly after a block
  comment in the group leaves the value empty, which is an error:
  - `a =⏎b = 1` → `{ a: "b = 1" }` (`empty_value_takes_next_line`)
  - `a =⏎⏎⏎  v` → `{ a: "v" }` (`empty_value_skips_blank_lines`)
  - `a =⏎{ b = 1 }` → `{ a: { b: int 1 } }` (`object_on_next_line_after_equals`)
  - `a = /* c */⏎v` → `{ a: "v" }` (`empty_value_block_comment_then_newline`)
  - `a =⏎# c⏎  v` → `{ a: "  v" }` (`empty_value_next_line_comment_then_spaces`)
  - `a =⏎/* c */ v` → `{ a: " v" }` (`empty_value_next_line_block_comment_then_space`)
  - `a =⏎/* c */⏎b = 1` is an error (`empty_value_next_line_block_comment_then_newline_error`)
- If the input ends in that following text, the value is `null`: `a = ⏎`, `a = /* c */⏎` and
  `a =⏎# c⏎` → `{ a: null }` (`empty_value_at_end_is_null`,
  `empty_value_block_comment_then_newline_at_end`, `empty_value_next_line_comment_at_end`,
  `cases/spec/09-macros/include_empty_value_is_null`).

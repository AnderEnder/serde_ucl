# 2. Whitespace and comments

Cases: `tests/conformance/cases/spec/02-comments/`.

## 2.1 Whitespace

- Space and tab separate things on a line: `a⇥=⇥1`, `  b  =  2` (`tab_and_space`).
- Between entries, and before a value, LF, CR, VT (0x0B) and FF (0x0C) may also appear
  (`form_feed_between_entries`, `vertical_tab_between_entries`, `carriage_return_terminates`,
  `cases/spec/01-structure/term_crlf`).
- Only LF, CR, NUL, `,` and `;` end a value. A VT inside an unquoted value is part of it:
  `a = 1<VT>b = 2` → `{ a: "1<VT>b = 2" }` (`vertical_tab_is_not_a_separator`).
- A UTF-8 byte order mark is not special; at the start of a line it becomes part of the key
  (`utf8_bom_is_part_of_key`).

## 2.2 Line comments

`#` starts a comment that runs to the end of the line, or to the end of input
(`hash_to_end_of_line`, `hash_at_end_of_input`, `hash_line`, `cases/review/30_inline_comment`,
`libucl/basic/6`).

A `#` ends an unquoted value immediately, even with no space before it: `a = x#y` →
`{ a: "x" }`; `b = 1#2` → `{ b: int 1 }` (`hash_ends_unquoted_value`,
`cases/additions/a44_hash_in_value`). Inside quoted strings `#` is an ordinary character
(`hash_inside_quotes`). A `\#` escape inside an unquoted value keeps the `#` (§4.7).

## 2.3 Block comments

`/*` starts a comment that ends at the matching `*/` (`multiline`).

- Block comments nest: every `/*` inside needs its own `*/` (`nested`, `nested_deep`).
- A `*/` between double quotes inside a block comment does not end it: `/* "*/" still comment */`
  is one comment (`quoted_close_inside_comment`).
- A block comment that is not closed is an error (`unterminated_error`, `unterminated_nested_error`,
  `cases/errors/e10_unterminated_comment`).
- `libucl/basic/comments` combines these forms.

## 2.4 Where comments may appear

Wherever whitespace may appear between the parts of an entry, and between array elements:

- before a key; between a key and its separator: `a /* c */ = 1` (`between_key_and_separator`);
- between the separator and the value; a line comment there moves the value to the next line:
  `b = # c⏎ 2` → `{ b: int 2 }` (`between_separator_and_value`, §1.6);
- after a value, where a comment counts as a terminator: `a = "x" /* c */ b = 2` →
  `{ a: "x", b: int 2 }` (`after_value_is_separator`, `block_comment_after_value`);
- between array elements (`cases/spec/01-structure/array_comment_inside`).

An unquoted value stops where a block comment starts. The text after the comment then begins a
new entry, which is usually an error: `a = x/*c*/y` is an error (`block_comment_mid_value_error`,
`cases/additions/a43_comment_in_value`). For numbers followed by a comment, see §5.6.

## 2.5 Things that are not comments

- `//` is not a comment. `/` may start a key, so `// comment` is the entry `"//": "comment"`
  (`slash_slash_is_a_key`, `cases/review/11_slashslash`).
- A single `/` inside a value is an ordinary character: `a = 1/2` → `{ a: "1/2" }`
  (`slash_star_not_at_value_start`).

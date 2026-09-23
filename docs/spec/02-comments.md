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

**Quirk: a `#` that is the last byte of the input.** Where the first key of the root should
start, a `#` that is the very last byte of the input is an error when whitespace comes directly
before it: `⏎#`, ` #`, `# c⏎ #` are **errors** (`hash_last_byte_after_leading_newline_error`,
`hash_last_byte_after_leading_space_error`, `hash_last_byte_after_comment_and_space_error`). It is
an ordinary comment when it is the whole input, when it directly follows a comment, and after an
entry: `#` and `# c⏎#` → `{}`, `a = 1⏎ #` → `{ a: int 1 }` (`hash_last_byte_at_start`,
`hash_last_byte_directly_after_comment`, `hash_last_byte_after_entry`). Any byte after it, even a
space, makes it an ordinary comment: `⏎#⏎` and `⏎# ` → `{}`
(`hash_then_newline_after_leading_newline`, `hash_then_space_after_leading_newline`).

The same holds where the next entry should start after a macro, whatever came before the macro:
`.priority 1⏎ #` and `a = 1⏎.priority 1⏎ #` are errors, and so is `.priority 1#`, with no space
(`hash_last_byte_after_macro_error`, §9.2).

A `#` ends an unquoted value immediately, even with no space before it: `a = x#y` →
`{ a: "x" }`; `b = 1#2` → `{ b: int 1 }` (`hash_ends_unquoted_value`,
`cases/additions/a44_hash_in_value`). Inside quoted strings `#` is an ordinary character
(`hash_inside_quotes`). A `\#` escape inside an unquoted value keeps the `#` (§4.7).

## 2.3 Block comments

`/*` starts a comment that ends at the matching `*/` (`multiline`).

- Block comments nest: every `/*` inside needs its own `*/` (`nested`, `nested_deep`).
- Double quotes inside a block comment mark quoted parts. A `"` that does not directly follow a
  `\` begins or ends one. Inside a quoted part, `*/` and `/*` have no effect:
  `/* "*/" still comment */` is one comment (`quoted_close_inside_comment`),
  `/* "/*" */ a = 1` → `{ a: int 1 }` (`block_comment_open_inside_quotes`), and
  `/* "a\"*/" */ a = 1` → `{ a: int 1 }` (`block_comment_escaped_quote_inside_quotes`).
- **Quirk.** A `"` directly after a `\` never begins or ends a quoted part, even when that `\`
  itself follows another `\`: `/* \" */ a = 1` → `{ a: int 1 }`
  (`block_comment_escaped_quote_outside_quotes`), but `/* "a\\" */ a = 1` is an **error**: its
  last `"` does not end the quoted part, so the comment is never closed
  (`block_comment_quote_after_backslash_pair_error`).
- A quoted part that is still open when the input ends leaves the comment unterminated, which is
  an error: `/* a " b */ c = 1` (`block_comment_unmatched_quote_error`).
- Single quotes have no effect: `/* ' */ a = 1` → `{ a: int 1 }`, and `/* '*/' */ a = 1` is an
  **error**, because the comment ends at the first `*/` (`block_comment_single_quote_not_special`,
  `block_comment_single_quotes_do_not_protect_error`).
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

# 6. Strings

Cases: `tests/conformance/cases/spec/06-strings/`.

There are three quoted forms. None of them is ever read as a number, boolean or null:
`"10"`, `"1kb"`, `"true"`, `"null"`, `'10'` are strings (`dq_not_interpreted`, `sq_not_interpreted`).
A quoted value must be followed by whitespace, a comment, a terminator or a closing bracket (§1.3).

## 6.1 Double-quoted strings

`"…"` holds any bytes except those listed below; UTF-8 passes through unchanged (`dq_basic`,
`dq_utf8`, `cases/review/23_semicolon_in_str`).

Escapes (`dq_escapes`, `libucl/basic/escapes`, `cases/additions/a34_string_escape_slash`):

| Escape | Result |
| --- | --- |
| `\"` `\\` `\/` | `"` `\` `/` |
| `\b` `\f` `\n` `\r` `\t` | BS, FF, LF, CR, TAB |
| `\uXXXX` | the code point U+XXXX (exactly four hex digits, either case), encoded as UTF-8 |
| `\` + any other byte | that byte; the backslash is dropped: `"\q\$x"` → `"q$x"` (`dq_unknown_escape_drops_backslash`) |

Details of `\u`:

- `"\u0041\u00e9\u20ac\uFFFF"` → `"Aé€\uFFFF"` (`dq_unicode_bmp`); `\u0000` gives a NUL byte (`dq_unicode_nul`).
- Exactly four characters are used: `"\u1F640"` is U+1F64 followed by `0` (`dq_unicode_five_digits`).
- A non-hex character among the four is an error: `"\uZZZZ"`, `"\u12"`, `"\u{1F600}"`
  (`dq_unicode_invalid_error`, `dq_unicode_short_error`, `dq_brace_unicode_error`,
  `cases/errors/e09_short_unicode_escape`).
- **Quirk.** A surrogate code point (U+D800–U+DFFF) is encoded on its own as three bytes.
  `"\uD83D\uDE00"` becomes six bytes, not one four-byte character, and the result is not valid
  UTF-8. Under the project's UTF-8 rule (README, *Divergences*), such a string is an error.
  No case covers it, because the golden files hold only valid UTF-8 or hex.

Errors:

- A raw control byte from 0x00 to 0x1E inside the quotes, including TAB, LF and CR:
  `"x⏎y"`, `"x⇥y"`, `"x<0x01>y"` (`dq_raw_newline_error`, `dq_raw_tab_error`,
  `dq_control_char_error`, `cases/additions/a20_raw_newline_dq`).
  **Quirk:** 0x1F and DEL (0x7F) are allowed (`dq_unit_separator_allowed`, `dq_del_allowed`).
- A missing closing quote, or a backslash as the last byte of input (`dq_unterminated_error`,
  `dq_backslash_at_end_error`, `cases/errors/e01_unterminated_string_first`).
- `"""…"""` and `"a" + "b"` are not supported; both are errors (`triple_quote_error`,
  `concatenation_error`, `cases/additions/a01_plus_concat`, `cases/additions/a03_two_strings`).

Escapes are decoded first, then variables are expanded (§7), but only in a string that holds a
`$` as written, escaped or not (§7.6, **quirk**). A `\$` therefore does not stop expansion inside
double quotes, and a `$` that only a `\u0024` escape gives does not start it (§7.6).

## 6.2 Single-quoted strings

`'…'` is literal, with only two escapes (`sq_basic`, `libucl/basic/squote`):

- `\'` gives `'` (`sq_escaped_quote`).
- A backslash followed by LF, by CR LF, or by a CR that no LF follows, is removed together with
  the line break (line continuation): `'one\⏎two'` → `"onetwo"`, and likewise `'x\␍y'` →
  `"xy"`; after a lone CR a further CR stays, `'x\␍␍y'` → `"x␍y"`
  (`sq_line_continuation`, `cases/additions/a29_single_quote_cont`).
- Any other backslash stays, with the byte after it: `'x\ny'` → `"x\\ny"`, and `'x\\y'` keeps both
  backslashes (`sq_no_escapes`).
- Backslashes pair from the left: the byte after a backslash always belongs to it, even when it is
  another backslash, so it never starts an escape itself. `'a\\'` is `a` and two backslashes, and
  the `'` ends the string; in `'\\⏎y'` both backslashes and the LF stay; `'a\\\''` is `a`, two
  backslashes and `'` (`cases/spec/10-output/readback_single_quoted_backslash_runs`). A run of
  backslashes before a line break therefore removes the line break only when the run has odd
  length.
- Raw line breaks are allowed: `'x⏎y'` → `"x⏎y"` (`sq_raw_newline`).
- Variables are not expanded (`sq_no_variables`).
- A missing closing quote is an error, and so is a backslash as the last byte of input
  (`sq_unterminated_error`, `sq_backslash_at_end_error`).

Single-quoted strings keep their quoting style in config output (§10.5).

## 6.3 Heredoc strings

`<<NAME` followed directly by LF starts a multi-line string. NAME is zero or more uppercase ASCII
letters (`heredoc_basic`, `heredoc_other_uppercase_names`, `cases/review/14_heredoc`,
`libucl/basic/issue319`).

- The content is every line after the opening line, up to the **terminator line**. That is a
  line consisting of NAME, followed by LF, `;`, `,` or the end of input (`heredoc_terminator_at_eof`,
  `heredoc_terminator_then_separator`).
- The line break before the terminator line is not part of the content. Whitespace and other
  line breaks are kept (`heredoc_keeps_whitespace`).
- The first line after the opening line is always content, even if it is NAME:
  `<<EOD⏎EOD⏎x⏎EOD⏎` → `"EOD⏎x"` (`heredoc_first_line_is_content`).
- Lines that only look similar are content: ` EOD` (indented) and `EODX` (longer)
  (`heredoc_indented_terminator_is_content`, `heredoc_longer_line_is_content`, `libucl/basic/4`).
  The exception is a NAME of one repeated letter (quirk below).
- NAME followed by anything else is not a terminator, so the heredoc runs on and ends
  unterminated, which is an error. This covers trailing spaces (`EOD  `) and a `}` on the same
  line (`EOD}`); a `}` on the next line is fine (`heredoc_terminator_trailing_space_error`,
  `heredoc_terminator_before_brace_error`, `heredoc_terminator_then_newline_brace`).
- No escapes are processed: `a\nb "q"` stays as written (`heredoc_no_escapes`). Variables are
  expanded (`heredoc_variables`, §7).
- A missing terminator is an error (`heredoc_unterminated_error`).
- **Quirk.** So an empty heredoc cannot be written: `<<EOD⏎EOD⏎` has `EOD` as its first content
  line and no terminator line, which is an error (`heredoc_empty_error`,
  `cases/additions/a30_heredoc_empty`). One blank line gives
  the empty string: `<<EOD⏎⏎EOD⏎` → `""` (`heredoc_blank_line_content`).
- **Quirk.** A NAME of one letter repeated (`A`, `EE`, `XXX`) is also ended by a line of more
  of that letter, followed by LF, `;`, `,` or the end of input. If NAME has n letters and the line
  has m, the line break before the line and m − n − 1 of the letters stay in the content:
  `<<A⏎x⏎AA⏎` → `"x⏎"`, `<<A⏎x⏎AAA⏎` → `"x⏎A"`, `<<EE⏎x⏎EEE⏎` → `"x⏎"`, and `<<A⏎x⏎AA;` and
  `<<A⏎x⏎AA` at the end of input → `"x⏎"`. A line of the letters followed by anything else is
  content: `<<A⏎x⏎AAB⏎A⏎` → `"x⏎AAB"` (`heredoc_repeated_letter_name`).
- **Quirk.** An empty NAME is allowed, with an end rule of its own. The first content line, with
  its LF, is always content. After that LF the heredoc ends at the first LF, `;` or `,`, wherever
  it stands, at the start of a line or inside one. The string is the text from the start of the
  content up to that byte, without its last byte; the LF, `;` or `,` itself is not part of the
  heredoc and is read next, after the value (`heredoc_empty_name_end_rule`):
  - `<<⏎x⏎⏎` → `"x"`: an empty line ends it, and the byte dropped is the line break
    (`heredoc_empty_terminator`); `<<⏎⏎⏎` → `""`; `<<⏎x⏎;` → `"x"`;
  - `<<⏎a⏎b⏎` → `"a⏎"` and `<<⏎ab⏎cd⏎` → `"ab⏎c"`: the second line ends at its line break, and
    its last byte is dropped; `<<⏎x⏎y;⏎` → `"x⏎"`, and the `;` then ends the value;
  - `<<⏎;x⏎⏎` → `";x"` and `<<⏎x;⏎⏎` → `"x;"`: bytes of the first line never end it;
  - with no LF, `;` or `,` after the first LF, the heredoc is unterminated, which is an error:
    `<<⏎content⏎`, `<<⏎x⏎y` and `<<⏎⏎` at the end of input (`heredoc_empty_name_eof_error`);
  - only a `$` in the first content line turns on variable expansion, which then covers the whole
    string; a `$` after the first LF does not: `<<⏎a⏎${ABI}x⏎` → `"a⏎${ABI}"`, but
    `<<⏎$ABI⏎${ABI}x⏎` → `"unknown⏎unknown"` (`heredoc_empty_name_variables`, §7).
- CR has no special meaning in the content: `x⏎` written with CR LF gives `"x\r"`
  (`heredoc_crlf_content`). CR LF directly after the opener means it is not a heredoc (below).
- Heredocs may be array elements (`heredoc_in_array`).

When `<<` is **not** followed by uppercase letters and LF (a lowercase name, a space, or CR), the
text is an ordinary unquoted value. What follows then usually does not parse:
`<<eod⏎x⏎eod`, `<<EOD ⏎…`, `<<EOD\r⏎…` are errors (`heredoc_lowercase_error`,
`heredoc_space_after_opener_error`, `heredoc_crlf_opener_error`, `cases/additions/a31_heredoc_crlf`).
**Quirk:** fewer than four bytes from `<<` to the end of input also gives an unquoted value:
`a = <<E` at end of input → `"<<E"` (`heredoc_short_input_is_string`). **Quirk:** with four or
more, `<<` followed by uppercase letters only, up to the end of the unit (the input, an included
file, an argument document or text parsed in place), is an error: `k = <<EO` and `k <<AA` at the
end of input, and `.priority(k = <<EOD) 1`, whose argument document ends there
(`heredoc_opener_cut_by_end_error`, `heredoc_opener_cut_by_end_two_letters_error`,
`heredoc_opener_cut_by_end_in_arguments_error`). Any other byte before the end makes it an ordinary
unquoted value again: `k = <<EO␠` at the end of input → `"<<EO"`; `<<AB1`, `<<Ab`, `[<<EOD]` and
`<<EO;` give `"<<AB1"`, `"<<Ab"`, `["<<EOD"]` and `"<<EO"` (`heredoc_opener_then_space_at_end_is_string`,
`heredoc_opener_not_all_uppercase_is_string`).

A string from a heredoc keeps that origin for config output (§10.5).
`libucl/basic/heredoc_eod` is a double-quoted string that contains `EOD` lines; it shows the
config output falling back to the double-quoted form.

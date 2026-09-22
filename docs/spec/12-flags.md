# 12. Parser flags

Cases: `tests/conformance/cases/spec/12-flags/`. Flags are libucl's public parser flags. The
conformance oracle turns them on through `.flags` files (README).

| Public name | `.flags` name | Section |
| --- | --- | --- |
| `UCL_PARSER_KEY_LOWERCASE` | `key-lowercase` | 12.1 |
| `UCL_PARSER_ZEROCOPY` | `zerocopy` | 12.2 |
| `UCL_PARSER_NO_TIME` | `no-time` | 12.3 |
| `UCL_PARSER_NO_IMPLICIT_ARRAYS` | `no-implicit-arrays` | 12.4 |
| `UCL_PARSER_SAVE_COMMENTS` | `save-comments` | 12.5 |
| `UCL_PARSER_DISABLE_MACRO` | `disable-macro` | 12.6 |
| `UCL_PARSER_NO_FILEVARS` | `no-filevars` | 12.7 |

## 12.1 `key-lowercase`

Every key is lowercased before it is stored or compared. This covers bare and quoted keys and
section names. **Quirk.** A quoted key is lowercased as written, before its escapes are decoded, so
letters inside escape sequences change and letters produced by escapes do not: `"\u0041" = 1` →
key `"A"`; `"\N" = 1` → key LF, because `\N` becomes `\n`; `"\U0042" = 1` → key `"B"`, because
`\U` becomes `\u`; `"\\N" = 1` → key `"\\n"` (a backslash and `n`); `"A\tB" = 1` → key
`"a\tb"` (`key_lowercase_before_escapes`). Only ASCII letters change; `É` stays `É`. Values are unaffected
(`key_lowercase`, `key_lowercase_non_ascii`). Keys that differ only in ASCII case are the same
key, so they form multi-value entries (`key_lowercase_merges_case`, §8.6).

## 12.2 `zerocopy`

No observable effect on the value tree (`zerocopy_no_effect`).

## 12.3 `no-time`

The time suffixes `s`, `min`, `h`, `d`, `w` and `y` are not recognised, so such values become
strings: `1s` → `"1s"`, `1min` → `"1min"`. **Quirk:** `ms`, `ks` and `gs` still give time values.
The multipliers `k`, `kb` and so on are unaffected (`no_time`).

## 12.4 `no-implicit-arrays`

Repeated keys collect their values into an explicit array instead of a multi-value entry (§8.5).
§8.5 also covers how priorities, `merge` and inherited values interact with the collected array,
including two quirks.

## 12.5 `save-comments`

No effect on the value tree (`save_comments_no_effect_on_values`). libucl also saves the comments
and attaches each to a value. The cases below record the attachment through the oracle's
`dump-comments` setting (conformance README): a value's `"c"` lists the comments attached before
it, and `"ca"` those attached after it.

What is saved:

- A line comment, from `#` up to but not including its LF. A CR before the LF stays: `# c␍⏎` →
  `"# c\r"` (`comments_line_comment_keeps_cr`).
- **Quirk.** A block comment, from `/*` through `*/` and the one byte after it, whatever that byte
  is: `/* c */a = 1` → `"/* c */a"`, `/* d */⏎` → `"/* d */\n"`
  (`comments_block_comment_saved_with_next_byte`). A nested block comment is saved as one comment.
  **Uncertain (undefined in libucl):** at the end of input that byte lies outside the document.
- Comments are kept in input order.

Where they attach:

- Comments not yet attached go, as *before* comments, to the next value that is created: the value
  of the next key, the next value of a repeated key, or the next array element. It does not matter
  where they stand: before the key, between the key and its value, or after the previous value on
  its line. `# c1⏎# c2⏎a = 1 # c3⏎b = /* c4 */ 2⏎c = [ 1, # c5⏎ 2 ]` → `a` gets `c1` and `c2`,
  `b` gets `c3` and `c4`, and the element `2` gets `c5` (`comments_attach_to_next_value`);
  `a = 1⏎a = 2 # c⏎a = 3` → the third value of `a` gets `c` (`comments_repeated_key`).
- When `}` or `]` closes a container, comments not yet attached go, as *after* comments, to the
  value created most recently: in `a { b = 1 # c1⏎}` to the value of `b`, in `d {⏎# c2⏎}` to the
  object `d`, in `e { f { g = 1 } # c3⏎}` to the value of `g`
  (`comments_trailing_at_container_close`).
- At the end of input, likewise as *after* comments to the value created most recently, or to the
  root when there is none: `a = 1⏎# c` (`comments_trailing_at_end`), `# c`
  (`comments_only_attach_to_root`).
- When the objects of a section path close together with its bracket, the outermost of them counts
  as the value created most recently: `a b { c = 1 } # z` → `z` after the object `a`
  (`comments_section_path_close`).
- Comments after the closing bracket of a braced root are ignored like the rest of the input
  (§1.1; `comments_after_braced_root_ignored`).

libucl's config output can include saved comments when the application passes them in. The output
formats of §10 are specified without them.

## 12.6 `disable-macro`

- **Quirk.** Any macro is a syntax error; the `.` at the start of a key is not accepted
  (`disable_macro_rejects_macros`, `disable_macro_priority_error`). libucl's documentation says
  "treat macros as comments", but that is not what the oracle does.
- This includes a macro at the end of input, which is ignored without the flag (§9.2):
  `a = 1⏎.foo` is an error (`disable_macro_name_at_end_error`).
- Variable expansion is switched off everywhere: `"$ABI"` and `$ABI` stay as written
  (`disable_macro_no_variables`).

## 12.7 `no-filevars`

`FILENAME` and `CURDIR` are not defined (§7.8), so references to them stay as written
(`no_filevars`, `cases/spec/07-variables/filevars_disabled`). Other variables work as usual.

## 12.8 Other parser settings used by the oracle

These are parser inputs rather than flags, and have `.flags` entries too: `var:NAME=VALUE`
(§7.1), `variable-handler` (§7.7), `priority:N` and `strategy:NAME` for the main document
(§8.3, §8.4), and `string-input` (§7.8).

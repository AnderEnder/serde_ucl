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

- **Quirk.** Because lowercasing comes first, `\U` in a quoted key becomes `\u`. The escapes are
  checked as written, where `\U` is an unknown escape and allowed (§6.1), and the lowercased text
  is then decoded with the rules of §4.8 for `\u` in unquoted values, so an invalid or short `\u`
  made this way is not an error: `"\U12"` → `u2`, `"\U123"` → U+1230, `"\U"` → `u`,
  `"\U\""` → `u"`, `"\UZZZZ"` → a NUL byte (`key_lowercase_upper_u_escape_decoded_as_unquoted`).
  A `\u` written as such is checked as without the flag: `"\uZZZZ" = 1` is an error
  (`key_lowercase_escapes_checked_as_written_error`). Without the flag, `"\UZZZZ"` is the key
  `UZZZZ` (`key_lowercase_upper_u_escape_without_flag`).
- Escapes can produce uppercase letters, so the keys of one entry may differ in case. Keys are
  compared ignoring ASCII case. The entry keeps the spelling of its first key while values are
  added to it, collected (§8.5), dropped for a lower priority, or merged into (§8.4):
  `"\u0041" = 1⏎a = 2` → `A: ⟨int 1 | int 2⟩`; `"\u0041\u0042" = 3⏎ab = 4⏎"aB" = 5` → `AB`
  with three values; `A = 1⏎"\u0041" = 2` → `a: ⟨int 1 | int 2⟩`;
  `.priority 3⏎a = 1⏎.priority 1⏎"\u0041" = 2` → `a: int 1 @3`
  (`key_lowercase_decoded_key_first_spelling_kept`, `key_lowercase_later_decoded_key_joins`,
  `key_lowercase_lower_priority_keeps_spelling`, `key_lowercase_collected_keeps_spelling`,
  `key_lowercase_merged_keeps_spelling`).
- When a new value replaces all of the entry's values, the entry takes the new key's spelling and
  keeps its place among the keys. That happens for a higher priority (§8.3), under `rewrite`, when
  an inherited value is replaced (§9.7), and when a collected array is replaced (§8.5):
  `x = 1⏎a { b = 1 }⏎.priority 3⏎"\u0041" { c = 1 }⏎y = 1` → keys `x`, `A`, `y`;
  `"\u0041" = 1⏎.priority 3⏎a = 2` → `a`; with `rewrite`, `a = 1⏎"\u0041" = 2` → `A: int 2`;
  `d { a = 1 }⏎e { .inherit "d"; "\u0041" = 2 }` → `e.A`
  (`key_lowercase_replacement_takes_new_spelling`,
  `key_lowercase_replacement_by_lowercase_spelling`, `key_lowercase_rewrite_takes_new_spelling`,
  `key_lowercase_inherited_replaced_takes_new_spelling`,
  `key_lowercase_collection_replaced_takes_new_spelling`,
  `key_lowercase_merge_higher_priority_takes_new_spelling`). Under `merge`, the scalar that takes a
  container's place (§8.4) does not count as such a replacement: `a { x = 1 }⏎"\u0041" = 2` →
  `a: int 2` (`key_lowercase_merge_scalar_quirk_keeps_spelling`).
- Keys created by macros keep their spelling and compare the same way (§9.4, §9.6, §9.7).

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
- **Quirk.** A `#` that is the last byte of the input is saved, as the comment `"#"`, only where it
  is read as a comment: as the whole input, directly after another comment (after the LF of a line
  comment, or directly after `*/`), and where a value on a following line may start (§1.6):
  `#`, `a = 1⏎# c⏎#`, `a = 1⏎/* c */#` and `a =⏎ #` save it (`comments_last_byte_hash_alone_saved`,
  `comments_last_byte_hash_after_comment_saved`, `comments_last_byte_hash_after_block_comment_saved`,
  `comments_last_byte_hash_for_value_on_next_line_saved`). After a value it only ends the value:
  `a = 1 #`, `a = 1#`, `a = 1⏎#`, `a = [1]#` and `a = 1;#` save nothing
  (`comments_last_byte_hash_after_value_not_saved`). Elsewhere it is an error (§2.2).
- Comments are kept in input order.

Where they attach:

- Each value has **one** list of comments. The first comments attached to a value decide whether
  the list comes before it (`"c"`) or after it (`"ca"`). Comments attached to the same value later
  are appended to that list, whatever their own placement: `# c⏎a = 1 # d` → `a` has
  `"c": ["# c", "# d"]`; `a = [ # c⏎1 ] # d` → the element `1` has `"c": ["# c", "# d"]`
  (`comments_later_comments_join_first_list`, `comments_after_joins_before_list_in_array`).
- Comments not yet attached go, as *before* comments, to the next value that is created: the value
  of the next key, the next value of a repeated key, or the next array element. An entry's value is
  created when its key has been read together with what follows the key on its line up to the
  value: the separator, spaces and comments. An array element is created where it starts. So it
  does not matter where the comments stand: before the key, between the key and its value, or after
  the previous value on its line. `# c1⏎# c2⏎a = 1 # c3⏎b = /* c4 */ 2⏎c = [ 1, # c5⏎ 2 ]` → `a`
  gets `c1` and `c2`, `b` gets `c3` and `c4`, and the element `2` gets `c5`
  (`comments_attach_to_next_value`); `a = 1⏎a = 2 # c⏎a = 3` → the third value of `a` gets `c`
  (`comments_repeated_key`). Macros create no value, so comments before them stay pending
  (`comments_before_macro_go_to_next_value`). This includes the copies that `.inherit` makes and
  the value that `.load` creates: they take no comments, and the value created most recently stays
  what it was (`cases/spec/09-macros/inherit_copies_have_no_comments`,
  `cases/spec/09-macros/inherit_copies_take_no_pending_comments`,
  `cases/spec/09-macros/comments_load_value_takes_none_after`,
  `cases/spec/09-macros/comments_load_value_takes_none_before`). The object, or the array and its
  object, that `key` or `prefix` creates for an include (§9.4) takes no pending comments either,
  which go to the first value of the file instead, but it is the value created most recently until
  the file creates one: `a = 1⏎.include(key="k") "files/v4/empty.txt"⏎# t` → `k: {}` with
  `"ca": ["# t"]`; with `target="array"`, the object inside `k` gets it
  (`cases/spec/09-macros/comments_include_key_takes_no_pending`,
  `cases/spec/09-macros/comments_include_key_object_most_recent`,
  `cases/spec/09-macros/comments_include_key_array_object_most_recent`). Comments inside macro
  argument documents (§9.2) are never saved (`cases/spec/09-macros/macro_args_comments_not_saved`).
- A value on a following line (§1.6) was created with its key, before the comments in front of it
  were read. Those comments therefore go to the next value created, or to the value created most
  recently when a container closes or the input ends (below): `c =⏎#z⏎b⏎d = 1` → `d` gets `#z`;
  `a =⏎# c⏎{ b = 1 }` → `b` gets `# c`; `a =⏎# c⏎` → `a: null` with `"ca": ["# c"]`;
  `# p⏎a =⏎# c⏎v` → `a: "v"` with `"c": ["# p", "# c"]`
  (`comments_value_on_next_line_goes_to_next_value`, `comments_value_on_next_line_object`,
  `comments_value_on_next_line_at_end`, `comments_value_on_next_line_joins_key_list`).
- When `}` or `]` closes a container, comments not yet attached go, as *after* comments, to the
  value created most recently: in `a { b = 1 # c1⏎}` to the value of `b`, in `d {⏎# c2⏎}` to the
  object `d`, in `e { f { g = 1 } # c3⏎}` to the value of `g`
  (`comments_trailing_at_container_close`).
- At the end of input, likewise as *after* comments to the value created most recently, or to the
  root when there is none: `a = 1⏎# c` (`comments_trailing_at_end`), `# c`
  (`comments_only_attach_to_root`). The end of an included file counts as an end of input here, and
  comments pending before an include may go to a value of the included file (§9.4;
  `cases/spec/09-macros/comments_carry_into_included_file`,
  `cases/spec/09-macros/comments_end_of_included_file`).
- When the objects of a section path close together with its bracket, the outermost of them counts
  as the value created most recently: `a b { c = 1 } # z` → `z` after the object `a`
  (`comments_section_path_close`). The same holds when a bracket closes objects that a section
  path left open (§3.4): `x "y{" z⏎a { b = 1 }⏎# c` → `x` gets `"ca": ["# c"]`. At the end of input
  they close without making a difference: `x "y{" z⏎# c` → the value `z` gets it
  (`comments_left_open_sections_closed_by_bracket`, `comments_left_open_sections_at_end`).
- Comments after the closing bracket of a braced root are ignored like the rest of the input
  (§1.1; `comments_after_braced_root_ignored`).

Comments and the rules of §8:

- Comments stay with their value when §8 moves it into a collected array: under
  `no-implicit-arrays`, `# c⏎a = 1⏎a = 2⏎# e` → `a: [1 with "c": ["# c"], 2 with "ca": ["# e"]]`
  (`comments_collected_array_keeps_comments`).
- Under `merge`, a repeated key whose value merges into the existing container attaches its
  comments to that container, where they join its list: `# c⏎a { x = 1 }⏎# d⏎a { y = 2 }` →
  `a` has `"c": ["# c", "# d"]`; `a { x = 1 }⏎a { }⏎# e` → `a` has `"ca": ["# e"]`;
  `r { a { } # e⏎}⏎r { # d⏎a { y = 2 } }` → `r.a` has `"ca": ["# e", "# d"]`. The scalar that
  takes a container's place (§8.4) keeps the container's comments: `# c⏎k { m = 1 }⏎# d⏎k = 1` →
  `k: int 1` with `"c": ["# c", "# d"]` (`comments_merge_joins_existing_object_list`,
  `comments_merge_after_comment_on_existing_object`, `comments_before_joins_after_list_under_merge`,
  `comments_merge_scalar_keeps_container_comments`).
- The comments of a value that §8 discards are lost with it: under `rewrite`,
  `# c⏎a = 1⏎# d⏎a = 2` → `a: int 2` with `"c": ["# d"]` only
  (`comments_rewrite_drops_replaced_value_comments`). **Uncertain (undefined in libucl):** the
  comments of a value that was replaced, under `rewrite` or by a higher priority, can reappear on a
  value created later, depending on memory reuse (under `rewrite`, `# c⏎k = 2⏎k = 3⏎q = 4` gave
  `q` the comment `# c`). No case pins this.

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

- **Quirk.** Including a file (§9.4) defines them for that file anyway, and they then stay defined
  in the including unit after the include, with the included file's values
  (`cases/spec/09-macros/no_filevars_include_defines_them`,
  `cases/spec/09-macros/no_filevars_included_file_has_curdir`). In macro arguments they are not
  defined (§9.2).
- The flag only stops the definitions that a parser makes for a document given as a string. libucl's
  function for parsing a file defines them from the file's path whatever the flag says. The
  conformance oracle parses each case as a string and defines them from the case's path itself,
  unless the case has `no-filevars` (README, *How the conformance oracle runs every case*); this
  section and the cases describe that behaviour.

## 12.8 Other parser settings used by the oracle

These are parser inputs rather than flags, and have `.flags` entries too: `var:NAME=VALUE`
(§7.1), `variable-handler` (§7.7), `priority:N` and `strategy:NAME` for the main document
(§8.3, §8.4), and `string-input` (§7.8).

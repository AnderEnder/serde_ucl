# 9. Macros

Cases: `tests/conformance/cases/spec/09-macros/`. The files these cases include or load are in
`cases/spec/09-macros/files/`.

Project note: in the project, `.includes` is unsupported and `.load` sits behind a default-off
feature (README, *Divergences*). This section describes libucl.

## 9.1 Where macros are recognised

A macro is recognised only where a key could start, and only when that byte is `.`. That means at
the top level of an object, the root included, or inside `{…}`
(`include_inside_object`, `inherit_at_top_level`). An object that is an array element counts:
`[ { .include "files/a.inc" }, { .priority 3⏎b = 1 }, { c = 1 } ]` works
(`macro_in_object_inside_array`). Anywhere else the text is an ordinary value:

- inside arrays: `arr = [ .include "files/a.inc" ]` → `{ arr: [".include \"files/a.inc\""] }`
  (`macro_not_recognised_in_arrays`)
- as a value: `k = .include "files/a.inc"` → `{ k: ".include \"files/a.inc\"" }`
  (`macro_not_recognised_as_value`)

A key position includes the one after a section name (§3.4): `a .b {c = 1}` is an error, because
`.b` is an unknown macro inside the new object `a` (`macro_word_after_section_name_error`);
`"a".51"x{y"z` → `{ a: {} }`, because `.51"x{y"z` is a macro whose name runs to the end of input
(§9.2; `macro_word_after_section_name_ignored`).

Included files (§9.4) are parsed with the same flags, so macros work in them as in the main
document.

With `disable-macro` set, a macro is a syntax error (§12.6).

## 9.2 Syntax

`.NAME` `(ARGUMENTS)`? VALUE

### NAME

- NAME runs from after the `.` to the first whitespace byte (space, TAB, LF, CR, VT, FF) or `(`.
  Every other byte belongs to it, so `.priority;a = 1` has the name `priority;a` and is an error
  (`macro_name_includes_semicolon_error`).
- It must be non-empty (`. include` is an error), matches case-sensitively (`.Include` is an
  error), and must be one of `include`, `try_include`, `includes`, `priority`, `load`, `inherit`,
  or a macro the application registered. An unknown name is an error (`unknown_macro_error`,
  `macro_space_after_dot_error`, `macro_name_case_sensitive_error`,
  `cases/errors/e05_unknown_macro`, `cases/errors/e06_unknown_macro_with_args`).

### Between the parts

After NAME, whitespace, line breaks included, is skipped, and then **at most one group of
comments**: comments that follow each other directly, a line comment including its LF (as in
§1.1). ARGUMENTS, if present, must start directly after that, with `(`. After ARGUMENTS, the same
skipping happens once more. VALUE starts exactly where the skipping stops.

**Quirk.** So whitespace that follows a comment group is not skipped. It becomes the start of an
unquoted VALUE (below), and a quoted or braced value after it is then just part of that text:

- `.include /* c */"files/a.inc"` and `.include # c⏎"files/a.inc"` work
  (`macro_comment_directly_before_value`, `macro_line_comment_then_value`);
- `.include /* c */ "files/a.inc"` and `.include # c⏎ "files/a.inc"` try to include the file
  ` "files/a.inc"` and fail (`macro_comment_then_space_before_value_error`,
  `macro_line_comment_then_indented_value_error`);
- `.include /* c */(try=true) "files/a.inc"` works, while `.include /* c */ (try=true) "files/a.inc"`
  has no ARGUMENTS and the VALUE ` (try=true) "files/a.inc"` (`macro_args_directly_after_comment`,
  `macro_args_after_comment_then_space_error`);
- `.include(try=true) /* c */ "files/a.inc"⏎k = 1` → `{ k: int 1 }`: the path ` "files/a.inc"`
  does not exist, and `try` skips it (`macro_args_then_comment_then_space_value_try`).
- `.priority /* c */ 3` works, because `.priority` accepts leading whitespace in its value (§9.5;
  `priority_comment_then_space_value`).

Whitespace alone, line breaks included, is fine: `.include (try=true) …`
(`include_space_before_args`), `.priority⏎3`.

### Quirk: a macro at the end of input

- If the input ends inside NAME, the macro is ignored, whatever NAME is, and there is no error:
  `a = 1⏎.foo` → `{ a: int 1 }`, `.` → `{}`, `.foo;` → `{}`, `.include"files/a.inc"` → `{}`
  (the name is `include"files/a.inc"`) (`macro_name_at_end_ignored`, `macro_dot_at_end_ignored`,
  `macro_name_with_punctuation_at_end_ignored`, `macro_name_runs_to_whitespace_or_paren`).
- A known NAME is also ignored when the input ends where the skipping after NAME stops, that is,
  after whitespace and at most one comment group: `.priority⏎`, `.include # c⏎`,
  `.include /* d */`, `.include⏎# c⏎# d⏎` and `.include # c⏎#` do nothing
  (`macro_known_name_then_whitespace_at_end_ignored`,
  `macro_known_name_then_comment_at_end_ignored`,
  `macro_known_name_then_block_comment_at_end_ignored`,
  `macro_known_name_then_comment_group_at_end_ignored`,
  `macro_known_name_then_comment_and_last_hash_ignored`).
- Anything left after that is VALUE, however little: `.include /* d */␠` includes the file ` `,
  and `.include # c⏎⏎` and `.include⏎/* d */⏎` include the empty path; all three are errors,
  because the file is missing (`macro_known_name_then_comment_then_space_error`,
  `macro_known_name_then_comment_then_blank_line_error`,
  `macro_known_name_then_block_comment_on_next_line_error`).
- A `#` that is the last byte of the input, directly after that whitespace, does not start a
  comment (compare §2.2). It ends an empty VALUE: `.include⏎#`, `.include⇥#` and
  `a = 1⏎.include #` are errors (`macro_known_name_then_last_byte_hash_error`,
  `macro_after_entry_last_byte_hash_error`).
- An unknown NAME followed by whitespace or `(` is an error even at the end: `.foo⏎`
  (`macro_unknown_name_then_newline_error`).
- After ARGUMENTS, the end of input (after the skipping) is an error:
  `.priority(priority=2)` (`macro_args_then_end_error`).
- Objects that a section path left without a closing bracket (§3.4) do not make an ignored macro
  an error: `a b .foo"{"` → `{ a: { b: {} } }`, `"a" .foo"{"` → `{ a: {} }`. An object opened
  with `{` still has to be closed: `a { .foo` is an error
  (`macro_name_at_end_after_section_path`, `macro_name_at_end_after_quoted_key_name`,
  `macro_name_at_end_inside_braces_error`).
- With `disable-macro`, every macro is an error, at the end of input too (§12.6).

### ARGUMENTS

- The text runs from `(` to the matching `)`. Parentheses inside double-quoted parts do not count
  toward the balance: `.priority(p="(") 1⏎k = 1` → `{ k: int 1 @1 }` (`macro_args_paren_in_quotes`).
  As in block comments (§2.3), a `"` directly after a `\` never begins or ends a quoted part, even
  when that `\` follows another `\`: `.priority(p="a\") 1⏎k = 1`, `.priority(p="a\\") 1` and
  `.priority(p="\\") 1` never balance. Single quotes do not protect:
  `.priority(p='(') 1⏎k = 1` does not balance either (`macro_args_escaped_quote_error`,
  `macro_args_backslash_pair_quote_error`, `macro_args_single_quotes_do_not_protect_error`).
  Unbalanced parentheses take the rest of the input, which is then an error
  (`macro_unbalanced_args_error`).
- The text between the parentheses is parsed as a separate UCL document with the same parser
  flags, priority 0 and the `append` strategy, as if it were given as a string to a new parser.
  **Quirk.** So the application's variables and handler are not available there: `$ABI` stays as
  written (`macro_args_no_variables`). Only the file variables of a string document exist (§7.8):
  `FILENAME` is `undef` and `CURDIR` is the working directory, whichever file the macro is in, and
  with `no-filevars` neither exists: `.include(key="$FILENAME") …` nests under the key `undef`, or
  under `$FILENAME` with the flag (`macro_args_filename_is_undef`, `macro_args_filename_no_filevars`).
  A syntax error in the arguments is an error: `.include(x) "files/a.inc"`
  (`macro_args_parse_error`). `()` is allowed (`include_empty_args`).
- The parameters are the entries of that document's root object. Entries are separated as in any
  document: `,`, `;` or a line break. A braced root works: `.include({priority=2}) …` → priority 2
  (`macro_args_braced_root`). A root array gives no parameters (`macro_args_array_root_ignored`),
  and neither do entries nested deeper: `.include(a {priority=2}) …` has no `priority`
  (`macro_args_nested_not_parameters`).
- Because the flags are the same, `key-lowercase` lowercases parameter names:
  `.include(PRIORITY=2) …` sets the priority with the flag and is ignored without it
  (`macro_args_key_lowercase`, `macro_args_names_case_sensitive`).

**Matching parameters (quirk).** Each macro has a table of parameters below; each parameter has a
type.

- A written name matches a parameter when it is a prefix of the parameter's name and the value
  has the parameter's type. Among parameters of that type, the first in the table's order that the
  name is a prefix of is chosen: for `.include`, `t=true` means `try` and `p=true` means `prefix`
  (`include_param_prefix_names`).
- The types: *bool* needs one of the keywords `true`, `false`, `yes`, `no`, `on`, `off` (§4);
  *string* needs a string, quoted or unquoted; *int* needs an integer, multipliers allowed
  (`1k` is 1000); *array* needs an explicit array. A value of any other type is ignored as if the
  parameter were absent: `try=1`, `try="true"`, `priority="2"`, `priority=2.0`
  (`include_try_must_be_boolean_error`, `include_try_string_ignored_error`,
  `include_priority_must_be_integer`, `macro_args_boolean_keywords`).
- Unknown names are ignored (`include_unknown_param_ignored`, `libucl/basic/13`).
- A name written twice forms a multi-value entry (§8), and only its first value counts:
  `priority=1, priority=2` → 1 (`macro_args_repeated_first_wins`). Two different names that
  match the same parameter both apply, in order, so the last one wins: `prio=2, pr=3` → 3
  (`macro_args_two_prefixes_last_wins`).

### VALUE

VALUE takes one of three forms (`include_quoted`, `include_bare`, `include_braces`):

- `"…"`: read like a double-quoted string (§6.1), so its errors apply: a raw control byte, a bad
  `\u` escape and a missing closing quote are errors (`macro_quoted_value_control_byte_error`,
  `macro_quoted_value_bad_unicode_error`, `macro_quoted_value_unterminated_error`). But escapes
  are **not** decoded: the value is the bytes between the quotes as written
  (`macro_quoted_value_not_unescaped`).
- `{…}`: leading whitespace, line breaks included, is skipped; the value is then the raw bytes up
  to the first `}`, trailing whitespace included: `.include { files/a.inc }` names
  `files/a.inc␠` and fails, `.include {⏎files/a.inc}` works (`include_braces`,
  `include_braces_variables`, `macro_braces_value_trailing_space_kept_error`,
  `macro_braces_value_leading_newline_skipped`). A missing `}` is an error
  (`macro_braces_value_unterminated_error`).
- anything else: the bytes up to the next LF, CR, NUL, `,`, `;`, `#`, `]` or `}`. **Trailing
  spaces and tabs are part of the value**, and `/*` does not end it: `.include files/a.inc ;`
  names `files/a.inc␠` (`macro_bare_value`, `macro_bare_value_trailing_space_kept_error`; for
  `.priority`, see §9.5). The value may be empty, for example directly before `;`
  (`priority_args_only`).

Variables are expanded in the value, in all three forms, by the rules of §7
(`macro_value_variables`, `include_curdir`). Because escapes are not decoded, a backslash has no
effect on expansion: `.include "\$ABI.inc"` names the file `\unknown.inc`.

After VALUE, whitespace, line breaks and `;` are skipped, and the next entry may start right there,
on the same line: `.include "files/a.inc"k = 1`, `.include "files/a.inc";; # c⏎k = 1`
(`include_then_keys`, `macro_value_then_entry_without_space`,
`macro_value_then_semicolons_and_comment`). A `,` there is an error
(`macro_value_then_comma_error`, `priority_comma_after_value_error`). A `#` that is the last
byte of the input is an error there, with or without whitespace before it: `.priority 1#`,
`a = 1⏎.priority 1⏎ #` (`macro_value_then_last_byte_hash_error`,
`macro_value_then_newline_last_byte_hash_error`, `cases/spec/02-comments/hash_last_byte_after_macro_error`).

A value on a following line is taken as the value even when it was meant as an entry:
`.priority⏎a = 1` is an error, because `a = 1` is not a priority (`priority_missing_value_error`).
So is one after an argument list: `.priority(priority=4)⏎a = 1` (**quirk**,
`priority_value_on_next_line_error`). A missing value at the end of input is covered by the quirk
above.

## 9.3 File paths

The path is used as written. A relative path resolves against the **process's current working
directory**, not the directory of the including file, and that holds inside included files too:
a file included as `files/v4/sub/rel.inc` that says `.include "files/a.inc"` includes
`files/a.inc` of the working directory (`include_relative_path_from_included_file`). Use
`${CURDIR}` for paths relative to the including file (`include_curdir`, `include_nested`,
`libucl/basic/9`, `libucl/basic/13`).

The conformance oracle runs each case from its own directory, so relative paths in the cases
resolve against the case's directory. An implementation that resolves paths some other way must
still give these results when parsing a case.

Before a file is included, its path is resolved to an absolute path with symbolic links and `.`
and `..` components resolved. That resolved path is the one compared in the self-inclusion check
(§9.4), set in `FILENAME` and `CURDIR`, and used for the automatic key of `prefix`.

## 9.4 `.include`, `.try_include`, `.includes`

### The included unit

The file is parsed as a new input unit, and its entries go into the object where the macro
appears (`include_quoted`, `include_inside_object`, `libucl/basic/23`).

- The unit has its own priority and duplicate strategy: the `priority` and `duplicate` parameters,
  0 and `append` by default. It does **not** take over the including unit's priority, whether set
  by `.priority` or given for the main document, or its strategy (`include_does_not_inherit_priority`,
  `include_does_not_inherit_strategy`, `include_lower_than_main_priority_dropped`). Its entries
  follow the rules of §8 against the keys already there, and later entries of the including unit
  follow the including unit's rules against them:
  `.include(priority=5) "files/a.inc"⏎x = 2` keeps only the included `x`, and
  `.include(duplicate="rewrite") "files/a.inc"⏎x = 2` gives `x: ⟨int 1 | int 2⟩`
  (`include_duplicates_append`, `include_priority_wins_over_later_entries`,
  `include_strategy_only_for_included_entries`).
- A `.priority` inside the included file affects only that file (`include_priority_macro_inside`).
- An empty file, or one with only whitespace and comments, adds nothing (`include_empty_file`).
- An error inside the included file fails the whole document (`include_syntax_error_in_file_error`).
- A value left empty at the end of an included file is `null` (`include_empty_value_is_null`, §1.6).
- While the file is parsed, `FILENAME` and `CURDIR` refer to it; afterwards they are restored
  (`include_nested`, `include_curdir_restored_after_include`). **Quirk.** This happens even with
  `no-filevars` (§12.7). There, nothing is restored, because nothing was defined before: after the
  include, the including unit sees the included file's `FILENAME` and `CURDIR`
  (`no_filevars_include_defines_them`, `no_filevars_included_file_has_curdir`).
- At most 16 input units may be open at once, the main document included. So 15 nested includes
  are fine and 16 are an error (`include_nesting_limit_ok`, `include_nesting_limit_error`). A cycle
  of files including each other ends at this limit, with an error
  (`include_cycle_nesting_limit_error`).
- Saved comments (§12.5) are collected across units: comments pending before the macro may attach
  to the first value of the included file, and the end of the included file attaches pending
  comments as the end of input does (`comments_carry_into_included_file`,
  `comments_end_of_included_file`).

### Where the entries go

An included file follows §1.1 for its start. A `[` there, where a bracketed root would start, is
an **error** (`include_array_root_error`). A `{` after a comment group and a blank line is an
error too, as for the main document (`include_comment_blank_line_brace_error`).

**Quirk: braces around an included file.** A `{` where a bracketed root would start does not
create an object. The entries still go into the object where the macro stands
(`libucl/basic/23`). Instead, the file's `{` takes the place of that object's own opening brace:

- The file's matching `}` does not close the object. Entries after it in the file go into the
  object too; unlike a main document (§1.1), the rest of the file is not ignored:
  `{ a = 1 }⏎b = 2` → `a` and `b` (`include_braced_file_then_entries`,
  `include_braced_file_twice`).
- The object has lost its own brace, so a `}` that later closes it is an **error**. A braced file
  can therefore be included at the top level of a document without outer braces, or under a key
  (below), whose object has no brace of its own, but not directly inside `{…}`:
  `x { .include "files/v4/braced.inc" }` and `{⏎.include "files/v4/braced.inc"⏎}` are errors,
  `x { .include(key="k") "files/v4/braced.inc"⏎y = 1 }` works
  (`include_braced_file_inside_braces_error`, `include_braced_file_at_braced_root_error`,
  `include_braced_file_with_key_inside_braces`).
- If the file does not close its brace, the object keeps it. At the top level the document is then
  unterminated, an error, unless the including unit closes it later. A `}` that closes a brace
  taken over from an included file removes the brace but again leaves the object open:
  `.include "files/v4/open_brace.inc"⏎q = 1⏎}⏎r = 2` → `a`, `q` and `r` at the top level;
  `x { .include "files/v4/open_brace.inc"⏎}⏎q = 1` → `{ x: { a: int 1, q: int 1 } }`
  (`include_unclosed_brace_error`, `include_unclosed_brace_closed_by_includer`,
  `include_unclosed_brace_inside_braces`). Here `open_brace.inc` is `{ a = 1`.

The same brace bookkeeping explains two more results:

- A `}` in an included file may close an object that the including unit opened with `{`:
  `x { .include "files/v4/close_brace.inc"⏎q = 1` with `close_brace.inc` = `a = 1 }` →
  `{ x: { a: int 1 }, q: int 1 }`. At the top level of a document without braces, that `}` is an
  error (`include_file_closes_including_object`, `include_file_closing_brace_at_top_level_error`).
- At the end of each included file, every object opened with `{` in that file must be closed
  (`include_unclosed_object_in_file_error`). Objects opened elsewhere are not checked there.

**Quirk.** Objects that a section path left open (§3.4) at the end of an included file stay open,
and later entries of the including unit go into them: with `left_open.inc` = `x "y{" z`,
`.include "files/v4/left_open.inc"⏎k = 1` → `{ x: { "y{": "z", k: int 1 } }`
(`include_left_open_section_persists`).

### Parameters

All optional, matched as in §9.2, listed in matching order within each type:

| Parameter | Type | Default | Meaning |
| --- | --- | --- | --- |
| `try` | bool | false (`.include`) | a missing file is skipped (below) |
| `sign` | bool | false (`.include`), true (`.includes`) | signature checking (below) |
| `glob` | bool | false | expand patterns in the path |
| `url` | bool | false | allow URLs (below) |
| `prefix` | bool | false | nest the contents under a key |
| `key` | string | none | the key for nesting; setting it implies nesting, even with `prefix=false`; the empty string is a valid key (`include_key_without_prefix`, `include_key_empty_string`) |
| `target` | string | `object` | `array` in any letter case means array; **any other string** means object (`include_target_case_insensitive`, `include_target_unknown_means_object`) |
| `duplicate` | string | `append` | the included unit's strategy, exactly one of `append`, `merge`, `rewrite`, `error` (§8.4); any other string, `Rewrite` included, leaves `append` (`include_duplicate_unknown_is_append`) |
| `path` | array | none | search directories (below) |
| `priority` | int | 0 | the included unit's priority (§8.3), taken modulo 16 like every priority: `17` → 1, `-1` → 15, `1k` → 8 (`include_priority_modulo_16`, `include_priority_negative`, `include_priority_with_multiplier`) |

Cases for strategies and priorities: `include_priority`, `include_duplicate_rewrite`,
`include_duplicate_error`, `include_duplicate_merge`, `include_merge_ignores_priority`,
`include_merge_lower_priority_still_merges`, `include_merge_scalar_higher_priority_replaces`,
`include_merge_scalar_lower_priority_dropped`, `libucl/basic/13`, `libucl/basic/15`,
`libucl/basic/16`, `libucl/basic/19`.

### Missing and unusable files

"Stops silently" below means (**quirk**): parsing ends at the macro without an error, in every open
unit. Nothing after the macro is read, neither in the file that holds it nor in the files that
included that one, and the entries parsed so far are the result. libucl's parse function reports
failure in this situation but records no error message and keeps the tree built so far; the golden
files record that tree, and §11.1 counts it as a result, not an error.

| The file | `.include` | `.include(try=true)` | `.try_include` |
| --- | --- | --- | --- |
| does not exist, including the empty path | error (`include_missing_error`) | skipped (`include_try_missing`, `include_empty_path_try`, `libucl/basic/14`) | stops silently (`try_include_missing_stops_parsing`, `try_include_empty_path_stops_parsing`) |
| is a directory or another non-regular file | error (`include_directory_error`) | skipped (`include_directory_try`) | stops silently (`try_include_directory_stops_parsing`) |
| is the file that holds the macro (resolved path, §9.3) | error (`include_self_error`, `include_main_document_itself_error`) | error (`include_self_with_try_error`) | stops silently (`try_include_self_stops_parsing`, `try_include_main_document_itself_stops`) |
| exists and is readable | included (`try_include_present`) | included | included |

"Skipped" means nothing is included and parsing goes on after the macro. Only the file that holds
the macro counts as itself; a cycle through other files ends at the nesting limit (above).
`libucl/basic/9` ends with a `.try_include` of a missing file, so nothing is lost there.

### Globs

With `glob=true`, a path that contains `*` or `?` is a pattern. A path without either is used as
written, even if it contains `[`: `files/v4/g/[ab].inc` is simply missing
(`include_glob_bracket_needs_wildcard_error`, `include_pattern_without_glob_error` for a pattern
without `glob=true`).

- Patterns follow the POSIX shell rules: `*`, `?` and bracket expressions such as `[ab]`, with no
  brace expansion (`{a,b}` is literal) and no `~` expansion. A `*` or `?` does not match a leading
  `.` in a file name, so hidden files are left out (`include_glob`, `include_glob_question_mark`,
  `include_glob_bracket_expression`, `include_glob_no_brace_expansion`,
  `include_glob_skips_hidden_files`).
- The matching files are included one by one, sorted by byte value: `10.inc`, `9.inc`, `B.inc`,
  `_u.inc`, `a.inc` (`include_glob_byte_order`).
- A match that cannot be included behaves as in the table above, except that `.try_include` skips
  it and goes on with the next match instead of stopping: a matched directory is an error for
  `.include`, and skipped with `try=true` or `.try_include` (`include_glob_directory_match_error`,
  `include_glob_try_skips_directories`, `try_include_glob_skips_directories`). A match that is the
  including file is an error even with `try=true`, and skipped by `.try_include`
  (`include_glob_matches_including_file_error`, `try_include_glob_skips_including_file`).
- If nothing matches, `.include` **stops silently** (**quirk**, `include_glob_no_match_stops_parsing`).
  With `try=true`, and for `.try_include`, nothing is included and parsing goes on
  (`include_glob_try_no_match`, `try_include_glob_no_match_continues`); `.try_include` with an
  explicit `try=false` stops silently (`try_include_glob_try_false_no_match_stops`).

### Nesting under a key

Given `key="K"`, or `prefix=true` without a key, the contents go under key K of the current
object instead of directly into it (`include_prefix_key`, `include_key_without_prefix`).

- With `prefix=true` and no key, K is the base name of the resolved path (§9.3), without a final
  `.conf` or `.ucl`: `a.inc` gives `a.inc`, `c.conf` gives `c`, `include_prefix_ucl_target.ucl`
  gives `include_prefix_ucl_target`, `d.ucl.txt` stays `d.ucl.txt`, and a symbolic link named
  `link.inc` to `c.conf` gives `c` (`include_prefix_auto_key`, `include_prefix_strips_ucl`,
  `include_prefix_keeps_other_extensions`, `include_prefix_symlink_uses_target_name`, `libucl/basic/9`).
- **Quirk.** With `glob=true`, K is taken from the first matched file and used for all of them:
  `.include(glob=true, prefix=true) "files/v4/g/[ab]*"` → `{ "a.inc": { ga: int 1, gb: int 1 } }`
  (`include_glob_prefix_key_from_first_file`).
- K is not lowercased under `key-lowercase`, but it is compared with existing keys ignoring ASCII
  case, like every key under that flag (§12.1; `include_key_not_lowercased_but_matched`).
- An object or array created for K has the include's `priority`
  (`include_prefix_priority_applies_to_nesting_object`, `include_prefix_array_priority`).
- An empty file still creates K, as an empty object (`include_prefix_empty_file_creates_object`).

What happens depends on `target` and on K's value. If K holds several values (§8), only the
first decides and receives the contents:

| `target` | K absent | first value is an object | first value is an array | first value is anything else |
| --- | --- | --- | --- | --- |
| `object` | new object K | contents go into it; other values kept | error (`include_prefix_existing_array_error`) | error (`include_prefix_existing_scalar_error`, `include_prefix_existing_multivalue_first_scalar_error`) |
| `array` | new array K = [object of contents] | **quirk:** K becomes [first value, object of contents] | a new object of contents is appended; other values kept | **quirk:** K becomes [first value, object of contents] (`include_prefix_array_existing_scalar`) |

**Quirk.** When K is replaced by a new array, K's other values are lost:
`k = 1⏎k = 2⏎.include(key="k", target="array") …` → `k: [int 1, { … }]`
(`include_prefix_array_existing_multivalue_scalars`).

Cases: `include_prefix_existing_object`, `include_prefix_existing_multivalue_object`,
`include_prefix_array_target`, `include_prefix_array_existing_object`,
`include_prefix_array_existing_multivalue_first_array`, `include_prefix_twice_same_object`,
`libucl/basic/9` (which includes an existing object and an existing array).

K is then an ordinary entry: a later `k { … }` is another value of K under the rules of §8
(`include_prefix_then_explicit_object_appends`). **Quirk.** With `no-implicit-arrays` (§8.5), an
array that replaced K's old value collects later repeats of K, as a collection array does; an
array created because K was absent does not: `k = 1⏎.include(key="k", target="array") …⏎k = 3` →
`k: [int 1, { … }, int 3]`, but without the first line `k: [[{ … }], int 3]`
(`include_prefix_array_converted_collects_repeats`, `include_prefix_array_new_does_not_collect`).

### Signatures, URLs and search paths

These parameters are outside the project's decided scope (README, *Known gaps*). libucl behaves as
follows:

- `sign=true`, the default for `.includes`, asks for signature checking when libucl is built with
  it: the file `PATH.sig` must verify. The oracle build has none, so `sign` has no effect and
  `.includes` behaves exactly like `.include` (`includes_like_include`,
  `include_sign_param_no_effect`). The project rejects `.includes`.
- `url=true` with `://` in the path fetches a URL when libucl is built with URL support. The
  oracle build has none, so this is an error (`include_url_param_error`). Without `url=true`, a
  path with `://` is an ordinary path (`include_url_like_path_is_a_path`).
- `path=[…]` sets a list of search directories. Entries that are not strings are skipped; a
  `path` that is not an array is ignored (`include_path_string_ignored`). **Quirks:**
  - The list stays in effect for every later include of the whole parse, with or without `path`
    (`include_path_persists`).
  - While a list is in effect, each path is tried as `DIR/PATH`, absolute paths included, for
    the directories in order. For `.include`, the first directory decides: if the file is missing
    there, that is an error at once (`include_path_first_dir`,
    `include_path_missing_in_first_dir_error`), and with `try=true` it is skipped without looking
    further (`include_path_try_first_dir_only`). `.try_include` does search: the first directory
    that has the file is used (`try_include_path_searches_all_dirs`), and a file found in none of
    them is an error, not a silent stop (`try_include_path_missing_error`). An empty list makes
    every include an error (`include_path_empty_array_error`).
  - With `glob=true`, the pattern is expanded in every directory, and all matches are included
    (`include_path_glob_all_dirs`). Without `try=true`, `.include` then fails when the **last**
    directory has no match, whatever the others had (`include_path_glob_last_dir_must_match_error`).

## 9.5 `.priority`

`.priority N` sets the priority of every value that follows in the current input unit (§8.3). Values
before it keep theirs. It applies to the end of the unit, also outside the object where the macro
stands: `a { .priority 3⏎b = 1 }⏎c = 1` gives both `b` and `c` priority 3
(`priority_continues_after_object`). Included files do not take it over (§9.4).

- N is read as a decimal integer: optional leading whitespace, an optional `+` or `-`, then decimal
  digits, and **nothing after them**. Leading zeros are decimal (`010` → 10). Quoted or in braces
  it works the same: `.priority 3`, `.priority "4"`, `.priority " 3"`, `.priority "+4"`,
  `.priority {3}` (`priority_forms`, `priority_quoted_leading_space_and_sign`,
  `priority_braces_value`, `priority_leading_zeros_decimal`).
- Anything else is an error: letters, a hex form, a sign alone (`.priority x`, `.priority "0x10"`;
  `priority_invalid_error`, `priority_hex_error`), and **trailing whitespace**. Because an
  unquoted VALUE keeps its trailing spaces and tabs (§9.2), `.priority 3 # c`, `.priority 3 ;` and
  `a { .priority 3 }` are errors, and so is `.priority { 3 }` (`priority_trailing_space_error`,
  `priority_trailing_space_before_semicolon_error`, `priority_in_braces_trailing_space_error`,
  `priority_braces_value_trailing_space_error`). End the value directly with a line break or
  `;`, and put a comment on its own line.
- The number is first limited to the 64-bit signed range, then taken modulo 16 (§8.3):
  `17` → 1, `-1` → 15, `99999999999999999999` → 15 (limited to 2⁶³−1),
  `-9223372036854775809` → 0 (limited to −2⁶³) (`priority_value_range`).
- Variables are expanded first: with `P` = `7`, `.priority $P` → 7 and `.priority "1$P"` → 17,
  that is 1 (`priority_variable_in_value`).
- If the value is empty, the int parameter `priority` is used instead: `.priority(priority=4);`,
  `.priority(priority=4) ""` (`priority_args_only`, `priority_args_used_for_empty_value`). When
  both are present, the value wins: `.priority(priority=6) 5` → 5 (`priority_forms`). A parameter
  of another type is ignored (§9.2). With neither, it is an error: `.priority ""`,
  `.priority();`, `.priority(priority=2.5);` (`priority_empty_quoted_value_error`,
  `priority_empty_args_error`, `priority_args_must_be_integer_error`).
- A value on the next line is still the value: `.priority⏎a = 1` is an error
  (`priority_missing_value_error`); with nothing left in the input, `.priority` does nothing (§9.2).
- Cases: `libucl/basic/18`, `cases/spec/08-duplicates/priority_*`.

## 9.6 `.load`

`.load(key="K", …) "PATH"` reads a file's bytes into a single value under key K of the current
object (`load_string`, `load_inside_object`).

| Parameter | Type | Default | Meaning |
| --- | --- | --- | --- |
| `try` | bool | false | a missing or unusable file is not an error, and nothing is inserted (`load_try_missing`, `load_directory_try`); otherwise it is an error (`load_missing_error`, `load_directory_error`) |
| `multiline` | bool | false | no effect on the value; the string counts as a heredoc for config output (§10.5) |
| `escape` | bool | false | store the content escaped (below) |
| `trim` | bool | false | strip leading and trailing whitespace (below) |
| `key` | string | **required** | the key to create; missing or empty is an error (`load_without_key_error`, `load_empty_key_error`) |
| `target` | string | `string` | `string` or `int`, in any letter case (`load_target_case_insensitive`); any other value: nothing is inserted and no error is raised (`load_unknown_target_inserts_nothing`) |
| `priority` | int | 0 | the value's priority, modulo 16 (`load_priority`, `load_priority_modulo_16`) |

Parameters match as in §9.2, in the table's order within each type, so `t=true` and `tr=true` mean
`try`, and `tri=true` means `trim` (`load_param_prefix_t_is_try`, `load_param_prefix_tri_is_trim`).
`escape=1` is not a bool and is ignored (`load_escape_must_be_boolean`).

- The path is used as written (§9.3). An empty path is an error, even with `try=true`
  (`load_empty_path_error`).
- The value's priority is the `priority` parameter only; `.priority` does not affect it
  (`load_ignores_priority_macro`).
- If K already exists in the current object, it is an error (`load_existing_key_error`,
  `load_twice_same_key_error`). Keys compare as usual: case-sensitively, or ignoring ASCII case
  under `key-lowercase` (`load_key_case_sensitive`, `load_key_lowercase_existing_error`). K is
  not lowercased (`load_key_lowercase_kept_spelling`). A later entry with key K is another value
  under §8: `.load(key="k") …⏎k = 1` → `k: ⟨"42⏎" | int 1⟩` (`load_then_same_key_appends`).

`target="string"`:

- Without `trim`, the content is kept as it is, final line break and NUL bytes included
  (`load_string`, `load_multiline`, `load_string_keeps_nul`).
- **Quirk.** An empty file inserts nothing (`load_empty_file_string_inserts_nothing`).
- `trim=true` removes leading and trailing space, TAB, LF, CR, VT and FF (`load_trim`,
  `load_trim_all_whitespace`).
- `escape=true` then replaces bytes as follows, and leaves every other byte, space included,
  unchanged (`load_escape`, `load_escape_all_bytes`, `load_trim_then_escape`):

  | Byte | Stored as |
  | --- | --- |
  | `"` | `\"` |
  | `\` | `\\` |
  | LF, CR, TAB, BS, FF | `\n`, `\r`, `\t`, `\b`, `\f` |
  | NUL | the six characters `\u0000` |
  | VT | the six characters `\u000B` |

`target="int"`: the content is read as a decimal integer: optional leading
whitespace (space, TAB, LF, VT, FF, CR), an optional `+` or `-`, then as many decimal digits as
follow. The rest of the content is ignored, and reading stops at a NUL byte. With no digits the
value is `int 0`, an empty file included; a number outside the 64-bit range is limited to the
nearest end (`load_int`, `load_int_leading_digits`, `load_int_leading_whitespace_and_sign`,
`load_int_no_digits_is_zero`, `load_empty_file_int_zero`, `load_int_clamps`,
`load_int_stops_at_nul`).

Cases: `libucl/basic/load`.

## 9.7 `.inherit`

`.inherit "NAME"` copies entries from another object into the current one.

- NAME is a key of the **root** object that exists at that point, compared like any key (ignoring
  ASCII case under `key-lowercase`). Inside an included file it still names a key of the main
  document's root (`inherit_from_included_file_uses_main_root`, `inherit_inside_included_file_with_key`,
  `inherit_key_lowercase_name`, `inherit_name_case_sensitive_error`). NAME is one key, never a path:
  `.inherit "d.x"` names the key `d.x` (`inherit_dotted_name_is_one_key`,
  `inherit_dotted_name_not_a_path_error`). Variables are expanded in it (§9.2;
  `inherit_variable_in_name`). Its trailing spaces count, so `e { .inherit d }` names `d␠` and
  fails; `{d}` works (`inherit_bare_value_trailing_space_error`, `inherit_braces_value`).
- If NAME holds several values, the first is used (`inherit_first_of_repeated`). If NAME is
  missing, empty, defined only later, not an object, or not at the root, it is an error
  (`inherit_missing_error`, `inherit_empty_name_error`, `inherit_source_must_exist_already_error`,
  `inherit_non_object_error`, `inherit_top_level_only_error`). With a root array, every
  `.inherit` is an error (`inherit_root_array_error`).
- A root key can be named from inside its own object: `.inherit` of the key that holds the current
  object is not an error. The first-value rule applies: in `e { a = 1 }⏎e { .inherit "e"; b = 2 }`
  the source is the first `e`, so the second gets `a` (`inherit_own_key_earlier_value`). When the
  source is the current object itself, nothing is copied: `e { a = 1; .inherit "e" }` →
  `{ e: { a: int 1 } }` (`inherit_own_object`, `libucl/basic/18` with `.inherit "mything1"`).
- **Quirk.** A root object can also be named from an object nested inside it. Its entry for that
  nested object is then copied too, with the entries it has received so far:
  `o { x = 1; e { .inherit "o" } }` → `{ o: { x: int 1, e: { x: int 1, e: { x: int 1 } } } }`
  (`inherit_enclosing_object`).
- Each entry of NAME whose key the current object does not yet have is copied, with all its values
  (`inherit_basic`, `inherit_existing_keys_kept`). A second `.inherit` of the same object therefore
  copies nothing (`inherit_twice_no_change`). Copies are **inherited**: a later explicit value
  for the same key replaces them, all values at once, whatever the priorities (§8.3;
  `inherit_basic` → `b: int 3`, not two values; `inherit_replaced_whatever_priority`,
  `inherit_multivalue_replaced_by_explicit`). Repeats after that follow §8 as usual, including
  `no-implicit-arrays` (`inherit_no_implicit_arrays_collects`).
- Copies keep the priority they had in NAME, not the current priority: with `d` at priority 2 and
  `e { .inherit "d" }` at priority 1, `e.a` has priority 2 (`inherit_keeps_source_priority`,
  `libucl/basic/18`).
- Copies keep the facts that config output uses (§10.1), for example a single-quoted origin
  (`cases/spec/10-output/inherit_copies_keep_output_facts`).
- The copy is shallow at the entry level. A later explicit `a { … }` replaces an inherited `a`
  entirely (`inherit_is_shallow`).
- **Quirk.** With `replace=true`, every entry is copied even when the key exists. The copy is then
  added as another value, not a replacement (`inherit_replace_appends`). It is not marked
  inherited, so a later explicit value is added to it rather than replacing it:
  `e { .inherit(replace=true) "d"; a = 2 }` → `e.a: ⟨int 1 | int 2⟩`
  (`inherit_replace_copies_not_inherited`). Each such `.inherit` adds another copy, and one of the
  current object copies its entries onto themselves: `e { a = 1; .inherit(replace=true) "e" }` →
  `e.a: ⟨int 1 | int 1⟩` (`inherit_replace_twice_duplicates`, `inherit_replace_own_object_duplicates`).
- **Quirk.** `replace` is not matched like other parameters: only the exact name `replace` counts
  (lowercased first under `key-lowercase`), a repeated `replace` uses its first value, and the
  value must be a bool (`inherit_replace_no_prefix_match`, `inherit_replace_first_value`,
  `inherit_replace_must_be_boolean`, `inherit_replace_keyword_yes`). Other parameters are ignored.
- At the root, `.inherit` copies into the root object itself (`inherit_at_top_level`).
- Cases: `libucl/basic/18`.

# 9. Macros

Cases: `tests/conformance/cases/spec/09-macros/`. The files these cases include or load are in
`cases/spec/09-macros/files/`.

Project note: in the project, `.includes` and `sign=true` are unsupported, `.load` sits behind a
default-off feature, and macro argument documents nest at most 64 deep (README, *Divergences*).
This section describes libucl.

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

**A macro directly after a name.** A known macro there runs inside the name's object. That object
has no bracket of its own, so it is left open like the objects of a section path that ends in an
ordinary key (§3.4), and closes the same way: `"s".include "files/a.inc"k { z = 1 }⏎m = 1` →
`{ s: { x: int 1, y: "inc", k: { z: int 1 } }, m: int 1 }`; `"s".priority {3}k { z = 1 } m = [1]`
→ `{ s: { k: { z: int 1 @3 } @3 }, m: [int 1] @3 }` (`macro_after_name_then_key_without_separator`,
`macro_after_name_then_bracketed_value_closes`).

- **Quirk.** The macro does not end the run of names. The next key read after it in the same
  unit counts as a word that follows a name (§3.4): with a `=` or `:` after it, it is a name too,
  even on a later line. So `"s".include "files/a.inc"k = [1]`, `"s".priority {3}k = [1]` and
  `"s".priority {3}⏎"k" = [1]` are errors, because the `[` stands where a name should start, and
  `"s".priority {3}k = l = n { z = 1 }` →
  `{ s: { k: { l: { n: { z: int 1 @3 } @3 } @3 } @3 } }`
  (`macro_after_name_then_separator_key_is_name_error`,
  `macro_after_name_priority_then_separator_key_error`,
  `macro_after_name_next_line_separator_key_error`, `macro_after_name_separator_keys_chain`). A key
  without a separator is tested as usual (§3.4) and ends the run:
  `"s".include "files/a.inc"x "y{" z⏎k = [1]` →
  `{ s: { x: ⟨int 1 | { "y{": "z", k: [int 1] }⟩, y: "inc" } }`
  (`macro_after_name_then_left_open_path`); `"s".priority {3}⏎k =⏎l { z = 1 }` →
  `{ s: { k: { l: { z: int 1 @3 } @3 } @3 } }`, `k` being a name and `l` a key with an object
  value (`macro_after_name_separator_key_then_bracket_key`). Nothing else ends the run: not other
  macros, not a `}` or `]` that closes a container, and not the keys of a file that a macro
  includes, which are read in a unit of their own. So `"s".priority {3}⏎.priority 4⏎k = [1]`,
  `x { "s".include {files/v5/o_empty.inc} }⏎k = [1]`,
  `a = [ { "s".include {files/v5/o_empty.inc} } ]⏎k = [1]` and
  `"s".priority {3}⏎.include "files/a.inc"⏎k = [1]` are errors, and
  `x { "s".include {files/v5/o_empty.inc} }⏎k = l {}` → `{ x: { s: { o: {} } }, k: { l: {} } }`
  (`macro_after_name_run_survives_macro_error`, `macro_after_name_run_survives_closing_brace_error`,
  `macro_after_name_run_survives_closing_bracket_error`,
  `macro_after_name_run_ignores_included_keys_error`,
  `macro_after_name_run_survives_closing_brace_name`). A name after which `=` or `:` and a line
  break follow (§3.4) starts a run in the same way: `c "x{" =⏎.priority 3⏎k = [1]` is an error
  (`macro_after_separator_newline_name_starts_run_error`). What follows a key that became a name
  is read as the next key: `"s".priority {3}⏎k = ⏎{ z = 1 }` is an error, because `{` cannot
  start a key, and so is `"s".priority {3}⏎a = 1⏎`, because the key `1` has nothing after it
  (`macro_after_name_separator_key_then_brace_error`,
  `macro_after_name_separator_key_scalar_error`).
- **Quirk.** When a macro of the run is followed, up to the end of its unit (the input, or the
  included file that holds it), by whitespace and comments only, **at least one comment** among
  them, the value created most recently (§12.5) is opened once more as a left-open object, if it is
  an object. The `;` and whitespace directly after a macro belong to it (§9.2). Whitespace alone,
  or nothing, does not do this, and neither do comments followed by anything else, for example a
  `}`, or by a macro name that runs to the end of input (§9.2):
  `macro_after_name_whitespace_to_end_no_reopen`, `macro_after_name_nothing_after_no_reopen`,
  `macro_after_name_closed_by_brace_no_reopen`, `macro_after_name_ignored_name_at_end_no_reopen`
  and `macro_after_name_later_macro_then_newline_no_reopen` leave the including unit's later
  entries at the top level, while `macro_after_name_comment_after_blank_lines_reopens`,
  `macro_after_name_semicolon_then_comment_reopens` and `macro_after_name_block_comment_reopens`
  put them into `s`. The macro that counts is the last one of the run, wherever it stands:
  `"s".include {files/v5/o_empty.inc} .priority {1} # [` and
  `"s".include {files/v5/o_empty.inc} # c⏎.priority {1} # d` reopen
  (`macro_after_name_later_macro_same_line_reopens`,
  `macro_after_name_later_macro_next_line_reopens`). For `.priority`, `.inherit` and an include
  that added nothing, the value created most recently is the name's object: `"s".priority {3}#[` and
  `"s".include "files/v4/empty.txt" # [` → `{ s: {} }` (`macro_after_name_comment_to_end`,
  `macro_after_name_include_of_empty_file_comment_to_end`). In an included file, the including
  unit's later entries then go into it: a file holding `"s".priority {3}#[`, included by
  `.include "…"⏎k = 1`, gives `{ s: { k: int 1 } }`
  (`macro_after_name_comment_to_end_of_included_file`). When a bracket in an included file closed
  `s` (§3.4), `s` is still the value created most recently: a file holding
  `"s".include "files/v5/o_empty.inc" # [`, where `o_empty.inc` is `o {}`, included the same way,
  gives `{ s: { o: {}, k: int 1 } }` (`macro_after_name_comment_to_end_after_bracket_closed`).
  The reopened object counts as opened by the unit that reopens it (§9.4, *The check at the end
  of a unit*): included by `a {⏎.include "…"⏎k = 1`, that same file gives
  `{ a: { s: { o: {}, k: int 1 } } }`, the check at the end of the main document stopping at `s`
  (`macro_after_name_reopen_end_check_stops`), while a file `a {⏎"s".include(key="k") {m.inc} # c`,
  where `m.inc` is `m {}`, reopens `m` as its own, so the check at the end of that file reaches
  `a`, whose brace is open: error (`macro_after_name_reopened_object_belongs_to_file_error`). The
  value may lie deeper than the object where the macro stands: a file
  `"s".include(key="k") "files/v6/m.inc" # [` sends the including unit's later entries into
  `s.k.m` (`macro_after_name_reopens_deeper_value`). A value that §8 discarded is reopened all the
  same, and the entries that go into it are lost with it: `.priority 5⏎s = 1⏎.include "…"⏎k = 1`
  with a file `.priority 1⏎"s".include {files/v5/o_empty.inc} # c` → `{ s: int 1 @5 }`
  (`macro_after_name_reopens_discarded_value`).
  **Uncertain (undefined in libucl):** if the value created most recently is not an object, for
  example the last value of a file the macro included, libucl crashes, reports an error or turns
  that value into an object, depending on the value. The implementation may choose.

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
  or a macro the application registered (§13.2). An unknown name is an error (`unknown_macro_error`,
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
  `.priority(priority=2)`, `a = 1⏎.try_include()⏎` (`macro_args_then_end_error`,
  `macro_args_newline_then_end_error`). A `#` that is the last byte of the input there, directly
  after `)` or after the skipping, does not start a comment either. It ends an empty VALUE, and the
  macro runs: `a = 1⏎.try_include()#`, `a = 1⏎.try_include() #` and `a = 1⏎.try_include()⏎#` stop
  silently with `{ a: int 1 }`, because the path is empty (§9.4)
  (`macro_args_then_last_byte_hash_empty_value`,
  `macro_args_space_then_last_byte_hash_empty_value`,
  `macro_args_newline_then_last_byte_hash_empty_value`). The same `#` then stands after VALUE,
  where a last-byte `#` is an error (*VALUE*, below), so the document fails unless the macro
  stopped the parse: `.priority(priority=4)#` and `.priority(priority=4) /* c */#` are errors
  (`macro_args_then_last_byte_hash_error`, `macro_args_comment_then_last_byte_hash_error`). The
  same holds after NAME: `a = 1⏎.try_include⏎#` stops silently
  (`macro_name_then_last_byte_hash_empty_value`).
- Objects that a section path left without a closing bracket (§3.4) do not make an ignored macro
  an error: `a b .foo"{"` → `{ a: { b: {} } }`, `"a" .foo"{"` → `{ a: {} }`. An object opened
  with `{` still has to be closed: `a { .foo` is an error
  (`macro_name_at_end_after_section_path`, `macro_name_at_end_after_quoted_key_name`,
  `macro_name_at_end_inside_braces_error`).
- With `disable-macro`, every macro is an error, at the end of input too (§12.6).

### ARGUMENTS

- The text runs from `(` to the matching `)`. Parentheses inside double-quoted parts do not count
  toward the balance: `.priority(p="(") 1⏎k = 1` → `{ k: int 1 @1 }` (`macro_args_paren_in_quotes`).
  **Quirk.** Outside a quoted part, every `"` begins one, also directly after a `\`, unlike in block
  comments (§2.3): `.priority(p=1 \"a) b") 1⏎a = 1` → `a: int 1 @1`, because the `)` after `a` is
  inside the quoted part that `\"` began, and the text ends at the last `)`; a registered macro
  given `.seen(k=\"x)") v` receives `k` = `"x)"` with its quotes (§4.7 decodes `\"`)
  (`macro_args_backslash_quote_opens_quoted_part`,
  `cases/spec/13-inputs/macro_registered_args_backslash_quote_opens_quoted_part`). Inside a quoted
  part, a `"` directly after a `\` does not end it, even when that `\` follows another `\`:
  `.priority(p="a\") 1⏎k = 1`, `.priority(p="a\\") 1` and `.priority(p="\\") 1` never balance. Single quotes do not protect:
  `.priority(p='(') 1⏎k = 1` does not balance either (`macro_args_escaped_quote_error`,
  `macro_args_backslash_pair_quote_error`, `macro_args_single_quotes_do_not_protect_error`).
  Unbalanced parentheses take the rest of the input, which is then an error
  (`macro_unbalanced_args_error`). **Quirk.** A `(` that is the last byte of its unit (the input,
  an included file, an argument document or text parsed in place) does not begin ARGUMENTS: it is
  the VALUE, and the macro runs without ARGUMENTS. `a = 1⏎.try_include(` stops silently with
  `{ a: int 1 }`, since the file `(` is missing (§9.4); `.priority(` is an error, since `(` is not a
  priority; a registered macro receives the VALUE `(` (§13.2) (`macro_paren_last_byte_is_value`,
  `macro_paren_last_byte_priority_error`,
  `cases/spec/13-inputs/macro_registered_paren_last_byte_is_value`). With any byte after the `(`,
  the rule above holds: `a = 1⏎.try_include(␠` is an error
  (`macro_paren_then_space_unbalanced_error`).
- The text between the parentheses is parsed as a separate UCL document with the same parser
  flags, priority 0 and the `append` strategy, as if it were given as a string to a new parser.
  **Quirk.** So the application's variables and handler are not available there: `$ABI` stays as
  written (`macro_args_no_variables`). Only the file variables of a string document exist (§7.8):
  `FILENAME` is `undef` and `CURDIR` is the working directory, whichever file the macro is in, and
  with `no-filevars` neither exists: `.include(key="$FILENAME") …` nests under the key `undef`, or
  under `$FILENAME` with the flag (`macro_args_filename_is_undef`, `macro_args_filename_no_filevars`).
  A syntax error in the arguments is an error: `.include(x) "files/a.inc"`
  (`macro_args_parse_error`). `()` is allowed (`include_empty_args`). So is any other rejection of
  the argument document, below: an error of a macro inside it, or a silent stop there. This holds
  for a macro in the document, in a file it includes, in text parsed in place (§13.2) and in a
  later input (§13.1): `.include "files/v12/args_bad.inc"`, where the file holds
  `.priority(x) 1⏎b = 1`, is an error (`include_file_with_rejected_args_error`).
  **Quirk.** Such a rejection does not stop the parse. The macro runs without ARGUMENTS, with its
  VALUE beginning directly after the `)`, as in the next quirk, and parsing goes on; the document
  is then an error unless a later macro skips its file, as after a first-directory miss of
  `.include` (§9.4, *Quirk: a later skipped URL include or `.load`*). That macro may be in the
  same input or a later one (§13.1), and in a file included or text parsed in place after the
  rejection (§9.4, *Quirk: the miss and the skip in different units*). The skipping macros are a
  skipped `.include(try=true, url=true)` or `.try_include(url=true)` with `://` in its path, and
  a `.load(try=true)` that reads nothing.
  `.priority(x) 3⏎a = 1⏎.load(try=true, key="t") "missing.txt"` and the same with
  `.include(try=true, url=true) ://` as the last line give `a: int 1 @3`; so do
  an unknown macro in the argument document, `.priority(.foo 1) 3`, a silent stop there,
  `.priority(.try_include "missing") 3`, a rejection in text parsed in place,
  `.emit ".priority(x) 3"` (§13.2), and a rejection in an included file, `files/v12/args_bad.inc`,
  which gives `b: int 1 @1`; `.include(x)"files/a.inc"` includes
  the file; a registered macro receives no ARGUMENTS and the VALUE as it stands after the `)`
  (`macro_args_rejected_then_load_try_accepts`, `macro_args_rejected_then_url_try_accepts`,
  `macro_args_rejected_unknown_macro_then_load_try_accepts`,
  `macro_args_stopped_then_load_try_accepts`,
  `pending/13-inputs/macro_registered_text_args_rejected_then_load_try_accepts`,
  `macro_args_rejected_in_included_file_then_load_try_accepts`,
  `macro_args_rejected_include_runs_without_args`,
  `pending/13-inputs/macro_registered_args_rejected_then_load_try_accepts`). One skip covers
  every such rejection and every first-directory miss before it, and none after it
  (`macro_args_rejected_and_first_miss_then_one_skip_accepts`,
  `macro_args_rejected_after_skip_error`);
  a `.load(try=true)` that reads its file does not count
  (`macro_args_rejected_then_load_existing_file_error`). A macro that fails or stops silently when
  it runs without ARGUMENTS ends the parse there, with the error: `.include(x) "files/a.inc"` names
  the missing file ` "files/a.inc"`, and `.try_include(x)"missing.inc"` stops at its missing file,
  so a skip after either is never read (`macro_args_rejected_include_space_value_error`,
  `macro_args_rejected_try_include_stops_error`).
- **Quirk: a rejected argument document inside an argument document.** When the macro stands
  inside an argument document, or in a file that an argument document includes, the rejection of
  its own ARGUMENTS is not an error. The macro runs without ARGUMENTS, and its VALUE begins at the
  byte directly after the closing `)`: nothing is skipped there, neither whitespace nor comments,
  and *VALUE* (below) then reads it as usual.
  - `.priority(.priority(x) 1; priority=3);⏎a 1` → `a: int 1 @3`: the inner VALUE is ` 1`, which
    §9.5 reads as 1, and the outer macro finds `priority=3` (`macro_args_nested_rejected_dropped`).
    The same holds for any rejection there: an unknown macro, `.priority(.foo 1) 1`, and a silent
    stop, `.priority(.try_include "missing") 1`, in place of `.priority(x) 1`
    (`macro_args_nested_unknown_macro_dropped`, `macro_args_nested_stop_dropped`); and a file
    included from the argument document, `.priority(.include "files/v12/args_bad.inc";
    priority=3);` → `@3` (`macro_args_nested_rejected_in_included_file`).
  - Because nothing is skipped, the VALUE's form depends on the byte after `)`:
    `.priority(.include(x)"files/a.inc"; priority=3);` includes the file into the argument
    document, while `.priority(.include(x) "files/a.inc"; priority=3);` is an error, since the
    VALUE is then the bare ` "files/a.inc"`, a missing file; `.priority(.priority(x); priority=3);`
    is an error too, since the VALUE is empty and there is no `priority` parameter
    (`macro_args_nested_rejected_value_after_paren`,
    `macro_args_nested_rejected_space_before_value_error`,
    `macro_args_nested_rejected_no_value_error`). Such an error rejects the enclosing argument
    document, and at the document level that is an error as above.
- Everything in this specification applies inside that document, macros included:
  `.priority(.priority 3⏎priority = 2);⏎a = 1` → `a: int 1 @2`;
  `.priority(d { priority = 3 }; .inherit "d");⏎a = 1` → `a: int 1 @3`; an unknown macro there
  is an error (`macro_args_macros_inside`, `macro_args_inherit_inside`,
  `macro_args_unknown_macro_inside_error`). A braced root ends the document and the rest of it is
  ignored (§1.1): `.priority({priority=2} x=1);` → priority 2
  (`macro_args_braced_root_rest_ignored`). Under `no-implicit-arrays`, a repeated name collects its
  values into an array (§8.5), which then has the wrong type for an int parameter:
  `.priority(priority=1, priority=2);` is an error with the flag
  (`macro_args_no_implicit_arrays_repeated_name_error`). Registered variables are not available
  (above): `.inherit "$V"` there names the key `$V` (`macro_args_registered_variables_unavailable_error`).
  Files can be included, with relative paths resolved as in §9.3:
  `.priority(.include "files/v5/pri3.inc");`, where the file holds `priority = 3`, sets priority 3
  (`macro_args_include_inside`). A silent stop (§9.4) inside the document makes the macro an error:
  `.priority(.try_include "missing"; priority = 3);` (`macro_args_stop_inside_error`). Comments in
  the document are never saved (§12.5; `macro_args_comments_not_saved`).
- **Nesting depth.** Since argument documents may hold macros with arguments, they can nest. libucl
  sets no limit of its own: 60 and 100 levels parse (`macro_args_nested_60_levels`,
  `macro_args_nested_100_levels`), and on the oracle machine 20,000 levels parse while 100,000
  crash it. **Uncertain (undefined in libucl)** beyond the depth at which libucl crashes, which
  depends on the platform. **Project divergence:** the project allows at most 64 documents nested in
  one another, the document that holds the outermost macro included; deeper nesting is an error
  (README, *Divergences*).
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

**Quirk: a NUL byte in VALUE.** Only the braced form can hold one (a bare value ends at NUL, and a
quoted one rejects it as a raw control byte). What a macro does with it depends on the macro:

- A path ends at the first NUL: `.include {files/a.inc<NUL>zzz}` includes `files/a.inc`, and so
  for `.try_include`, `.includes` and `.load` (§9.3; `macro_value_nul_ends_include_path`,
  `macro_value_nul_ends_load_path`). The part before the NUL may be empty. The include macros
  then include the empty path, as for an empty VALUE (§9.4). For `.load`, only a VALUE of zero
  bytes is the empty path of §9.6; a VALUE that starts with NUL names the empty path as a file,
  which is missing: `.load(key="k") {<NUL>zz}` is an error, and with `try=true` nothing is
  inserted and parsing goes on (`load_path_nul_first_missing_error`,
  `load_try_path_nul_first_skipped`).
- With `glob=true`, whether the path is a pattern is decided on the whole VALUE, the NUL and the
  bytes after it included, while the pattern is the part before the NUL (§9.4, *Globs*).
- A priority (§9.5) is read from the part before the first NUL: `.priority {3<NUL>x}` sets 3. An
  empty part there counts as 0 without an error, and the VALUE is not empty, so it wins over the
  parameter: `.priority {<NUL>}` and `.priority(priority=4) {<NUL>}` set priority 0
  (`macro_value_nul_ends_priority`, `macro_value_nul_first_priority_zero`,
  `macro_value_nul_first_priority_zero_wins_over_args`).
- `.inherit` (§9.7) uses every byte, so `.inherit {d<NUL>zz}` names the key `d<NUL>zz` and fails
  when there is none (`macro_value_nul_kept_by_inherit_error`), and a registered macro receives
  every byte (§13.2; `cases/spec/13-inputs/macro_registered_value_nul_kept`).

**Quirk: a NUL byte in a string parameter.** A string in ARGUMENTS can hold a NUL byte through a
`\u0000` escape or a short `\u` escape (§4.8). The string parameters of the include macros (`key`,
`target`, `duplicate` and the entries of `path`, §9.4) and of `.load` (`key` and `target`, §9.6)
end at their first NUL:

- `.include(key="s\u0000t") "files/a.inc"` nests the entries under `s`, with `prefix=true` too,
  and `key="\u0000t"` nests them under the empty key (`include_key_param_ends_at_nul`,
  `include_key_param_nul_first_empty_key`).
- `.load(key="s\u0000t")` inserts under `s`, and `.load(key="\u0000t")` is an error, as for an
  empty key (`load_key_param_ends_at_nul`, `load_key_param_nul_first_error`).
- `duplicate="rewrite\u0000zz"` is `rewrite`, `target="array\u0000q"` is `array`, and `.load`'s
  `target="int\u0000z"` is `int` (`include_duplicate_param_ends_at_nul`,
  `include_target_param_ends_at_nul`, `load_target_param_ends_at_nul`).
- `path=["files\u0000zz"]` searches the directory `files` (`include_path_param_entry_ends_at_nul`).

After VALUE, whitespace, line breaks and `;` are skipped, and the next entry may start right there,
on the same line: `.include "files/a.inc"k = 1`, `.include "files/a.inc";; # c⏎k = 1`
(`include_then_keys`, `macro_value_then_entry_without_space`,
`macro_value_then_semicolons_and_comment`). A `,` there is an error
(`macro_value_then_comma_error`, `priority_comma_after_value_error`). A `#` that is the last
byte of the input is an error there, with or without whitespace before it: `.priority 1#`,
`a = 1⏎.priority 1⏎ #` (`macro_value_then_last_byte_hash_error`,
`macro_value_then_newline_last_byte_hash_error`, `cases/spec/02-comments/hash_last_byte_after_macro_error`).
Only a `#` directly after a comment is a comment there, as where the first key of the root would
start (§2.2): `.priority 3;#⏎#` → `{}`, but `.priority 3;#⏎ #` is an error
(`cases/spec/02-comments/hash_last_byte_after_macro_directly_after_comment`,
`cases/spec/02-comments/hash_last_byte_after_macro_comment_then_space_error`).

A value on a following line is taken as the value even when it was meant as an entry:
`.priority⏎a = 1` is an error, because `a = 1` is not a priority (`priority_missing_value_error`).
So is one after an argument list: `.priority(priority=4)⏎a = 1` (**quirk**,
`priority_value_on_next_line_error`). A missing value at the end of input is covered by the quirk
above.

## 9.3 File paths

The path is used as written, up to its first NUL byte if it has one (§9.2, *VALUE*). A relative path resolves against the **process's current working
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
- **Quirk.** An include also moves `FILENAME` and `CURDIR` to the end of the lookup order of §7.1,
  `FILENAME` first, for the included file and for the rest of the whole parse, whether or not they
  were defined before. This matters for unbraced references (§7.4): with `FILE` = `f` and
  `CUR` = `c` registered, `$FILENAME` and `$CURDIR` in the included file, and in the including unit
  after the include, give `fNAME` and `cDIR`, where before the include `$FILENAME` gave the path.
  For a string document, `c = "$FILENAME"⏎.include "…"⏎d = "$FILENAME"` gives `c: "undef"`,
  `d: "fNAME"` (`include_moves_filevars_last`, `include_moves_filevars_last_string_input`).
- At most 16 input units may be open at once, the main document included. So 15 nested includes
  are fine and 16 are an error (`include_nesting_limit_ok`, `include_nesting_limit_error`). A cycle
  of files including each other ends at this limit, with an error, also for `.try_include`
  (`include_cycle_nesting_limit_error`, `try_include_cycle_nesting_limit_error`). Earlier inputs
  of the same parser and text a registered macro has parsed in place count too (§13).
- Saved comments (§12.5) are collected across units: comments pending before the macro may attach
  to the first value of the included file, and the end of the included file attaches pending
  comments as the end of input does (`comments_carry_into_included_file`,
  `comments_end_of_included_file`). **Quirk.** A file of zero bytes is not read at all, so its
  end attaches nothing: comments pending before the macro stay pending and go to the next value,
  also with `key` and for a glob match: `a = 1⏎# c⏎.include "files/v4/empty.txt"⏎b = 1` → `b` has
  `"c": ["# c"]`, while with a file of one LF, `a` gets `"ca": ["# c"]`
  (`comments_zero_byte_file_keeps_pending`, `comments_zero_byte_file_with_key_keeps_pending`,
  `comments_zero_byte_glob_match_keeps_pending`, `comments_one_byte_file_attaches_pending`). Such
  a file still sets and moves `FILENAME` and `CURDIR` as above (`zero_byte_file_moves_filevars`).

### Where the entries go

An included file follows §1.1 for its start. A `[` there, where a bracketed root would start, is
an **error** (`include_array_root_error`). **Uncertain (undefined in libucl):** libucl does not
stop at that `[`; it reads on, so that a silent stop in the file can come first, and a file such
as `[ { a = 1 } ]` crashes it. The project reports the error at the `[`. A `{` after a comment group and a blank line is an
error too, as for the main document (`include_comment_blank_line_brace_error`).

**Quirk: a file that ends right after its leading bracket.** When that `{` or `[` is the last byte
of the file, and stands where §1.1 lets a bracketed root start (after whitespace alone, or
directly after a comment group at the start of the file), nothing is taken over and nothing is
checked:
the file adds nothing, and the object where the macro stands keeps its own brace. With the file
`{`, `a = 1⏎.include "…"⏎b = 2` → `{ a: int 1, b: int 2 }`, and
`x { .include "…"⏎b = 2 }⏎c = 3` → `{ x: { b: int 2 }, c: int 3 }`; the same with the file `⏎{`
and with the file `[` (`include_only_open_brace_adds_nothing`,
`include_only_open_brace_takes_nothing_over`, `include_newline_then_open_brace_adds_nothing`,
`include_only_open_bracket_adds_nothing`), and with the files `# c⏎{`, `/* c */{` and `# c⏎[`
(`include_comment_then_open_brace_takes_nothing_over`,
`include_block_comment_then_open_brace_takes_nothing_over`,
`include_comment_then_open_bracket_adds_nothing`). A `{` that does not stand where a root can start
is an error, as for the main document: `⏎# c⏎{` and `# c⏎␠␠{` (`include_comment_blank_line_brace_error`,
`include_comment_then_space_brace_error`). With any byte after the `{`, even a space, the brace is
taken over as below: with the file `{␠`, the second example puts `c` into `x`. Text parsed in
place (§13.2) follows the same rule.

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
- **Quirk.** That holds for the root and for objects written with braces. When the object whose
  brace was taken over is a section object without a bracket of its own (left open by a section
  path, §3.4, or the object of a name followed by the macro, §9.1), the `}` that removes the brace
  closes it, together with the section objects around it, as a closing bracket closes left-open
  objects in §3.4: `x "y{" z⏎.include "files/v4/braced.inc"⏎q = 1` →
  `{ x: { "y{": "z", a: int 1 }, q: int 1 }`;
  `x "y{" z⏎.include "files/v4/open_brace.inc"⏎q = 1⏎}⏎r = 2` →
  `{ x: { "y{": "z", a: int 1, q: int 1 }, r: int 2 }`;
  `"s".include "files/v4/braced.inc"x "y{" z⏎k = [1]` →
  `{ s: { a: int 1 }, x: { "y{": "z", k: [int 1] } }` (`include_braced_file_closes_section_object`,
  `include_open_brace_in_section_object_closed_by_includer`, `include_braced_file_after_name`).
  Such a section object also closes, taken-over brace and all, when a container written with
  brackets that was opened in it closes, as left-open objects do (§3.4):
  `x "y{" z⏎.include "files/v4/open_brace.inc"⏎m { }⏎n = 1` →
  `{ x: { "y{": "z", a: int 1, m: {} }, n: int 1 }`, likewise with `m = [1]`, with a second
  left-open path `p "q{" r` before `m { }`, and inside `a { … }`
  (`include_taken_over_section_closed_by_inner_object`,
  `include_taken_over_section_closed_by_inner_array`,
  `include_taken_over_section_closed_with_inner_path`,
  `include_taken_over_section_closed_inside_braces`). A later `}` then has nothing to close:
  `x "y{" z⏎.include "files/v4/open_brace.inc"⏎m { }⏎}` is an error
  (`include_taken_over_section_closed_then_brace_error`).
- **Quirk.** When the file's leading `{` has taken over the brace of the root, of an object written
  with braces, or of the object that `key` or `prefix` creates for the file (below), and the first
  key the file then reads, after comments and macros, is the first name of a section path (§3.4),
  that name's object gets a brace of its own: a `}` closes it, and if it is still open at the end
  of the file, the check below reports it. The brace taken over stays with the object where the
  macro stands. So a file `{⏎x "y{" z` is an error, also written `{ x "y{" z`, `{⏎# c⏎x "y{" z`,
  `{ "q" "y{" z`, `{⏎c "x{" =⏎` or `{⏎"s".priority {3}⏎`
  (`include_braced_file_first_name_unclosed_error`,
  `include_braced_file_first_name_same_line_error`,
  `include_braced_file_first_name_after_comment_error`, `include_braced_file_first_quoted_name_error`,
  `include_braced_file_first_name_separator_newline_error`,
  `include_braced_file_first_name_then_macro_error`). A file `{ x "y{" z⏎}` closes `x`, and the
  including unit must still close the brace it lost: `.include "…"⏎q = 1⏎}` →
  `{ x: { "y{": "z" }, q: int 1 }`, and without the `}` it is an error; a file
  `{ x "y{" z⏎}⏎}` closes both (`include_braced_file_first_name_closed`,
  `include_braced_file_first_name_closed_root_open_error`,
  `include_braced_file_first_name_and_brace_closed`). Like the objects that a section path leaves
  open (§3.4), that name's object also closes when a container written with brackets that was
  opened in it closes, and a `}` after that closes the brace taken over: with a file
  `{ x "y{" z⏎a { b = 1 }⏎}`, `.include "…"⏎q = 1` →
  `{ x: { "y{": "z", a: { b: int 1 } }, q: int 1 }`, and a further `}` in the including unit is
  an error (`include_braced_file_first_name_closed_by_object`,
  `include_braced_file_first_name_closed_by_object_then_brace_error`). When the brace taken over
  is that of a section object without a bracket of its own (above), the first name gets a brace
  of its own too, and its `}`, as that of a container written with brackets that was opened in
  the section object, also closes the section object, taken-over brace and all (above). So with
  the file `files/v6/q42_closed_twice.inc`, `{ x "y{" z⏎}⏎}`, `s "t{" u⏎.include "…"⏎q = 1` is an
  error at the file's second `}` (`include_braced_file_first_name_in_section_object_error`). When
  text parsed in place has made that section object stop closing on its own (§13.2, *Quirk: text
  in place and a section object left open*), the name's `}` closes only the name's object, and a
  second `}` removes the brace taken over, leaving the section object open: with a file
  `{ .emit ""⏎x "y{" z⏎}⏎}`, `s "t{" u⏎.include "…"⏎q = 1` →
  `{ s: { "t{": "u", x: { "y{": "z" }, q: int 1 } }`, and with only the first `}` in the file it
  is an error (`include_braced_file_first_name_in_section_object_after_text`,
  `include_braced_file_first_name_in_section_object_after_text_unclosed_error`). Which value a
  comment after these closings attaches to is in §12.5. A `.priority` before the name makes no
  difference (`include_braced_file_priority_then_first_name`). Only that first name gets a brace:
  `{ a b "y{" z⏎}` is an error, because the `}` finds `b` without one, and so is a file whose
  first key is an ordinary entry, `{ a = 1⏎x "y{" z⏎}⏎}` (`include_braced_file_two_names_error`,
  `include_braced_file_entry_first_then_path_error`). A main document's leading `{` does not do
  this, because it opens the root (§1.1): `{ x "y{" z⏎}⏎}` is an error
  (`main_braced_root_first_name_has_no_brace_error`).
- **Quirk: macros before the first key.** The keys that a file included by such a file, or text
  parsed in place in it (§13.2), reads are not the file's own: its first key is the first one it
  reads itself. Until then, or until a `}` of its own (below), the file takes a brace over again
  after each macro, that of the object its entries then go into, so a `}` of the nested file or
  text that removed the brace taken over (above) does not end the takeover for the file. This
  happens only when the file holds more after the macro than whitespace and `;` (below). A brace
  taken over again is added to any brace the file still holds, not put in its place, and the
  file's `}`s close the innermost first. An object that still holds a brace taken over is not
  given a second one:
  - With a file `{ .include "files/v4/braced.inc"⏎a2 = 2⏎}`, `.include "…"⏎q = 1` →
    `{ a: int 1, a2: int 2, q: int 1 }` (`include_braced_file_nested_braced_then_entry`).
  - The first name still gets its brace, and the file's last `}` closes the brace taken over: with
    a file `{ .include "files/v4/braced.inc"⏎x "y{" z⏎}⏎}`, `.include "…"⏎q = 1` →
    `{ a: int 1, x: { "y{": "z" }, q: int 1 }`; the same with `files/v4/close_brace.inc`
    (`a = 1 }`) as the nested file, also under `key="k"`, where the entries and `x` go into `k`,
    and with text `{ a = 1 }` parsed in place
    (`include_braced_file_nested_braced_before_first_name`,
    `include_braced_file_nested_close_brace_before_first_name`,
    `include_key_nested_close_brace_before_first_name`,
    `include_braced_file_nested_text_before_first_name`).
  - A nested file that leaves a section path open puts the first name inside it, and the name's
    `}` closes both, as above: with a file `{ .include "files/v4/left_open.inc"⏎p "q{" r⏎}⏎}`,
    `.include "…"⏎q = 1` → `{ x: { "y{": "z", p: { "q{": "r" } }, q: int 1 }`
    (`include_braced_file_nested_path_before_first_name`).
  - A nested file that leaves a section path open and removes no brace leaves the file holding
    two: the section object's, taken over again, and the one it took over first, which stays with
    the object where the macro stands. The file's first `}` closes the section object and its
    second the other brace: with a file `{ .include "files/v4/left_open.inc"⏎w = 1⏎}⏎}`,
    `.include "…"⏎q = 1` → `{ x: { "y{": "z", w: int 1 }, q: int 1 }`, and the same without
    `w = 1`. With only the first `}` in the file, the root keeps the other brace, so
    `.include "…"⏎q = 1` is an error and `.include "…"⏎q = 1⏎}` is accepted, with `w` in `x` and
    `q` at the top level (`include_braced_file_nested_path_then_entry`,
    `include_braced_file_nested_path_then_braces`,
    `include_braced_file_nested_path_one_brace_error`,
    `include_braced_file_nested_path_one_brace_closed_by_includer`).
  - Once the file has read a key of its own, a nested file's `}` removes the brace for good: a
    file `{ a0 = 0⏎.include "files/v4/braced.inc"⏎}` is an error, because its `}` has nothing
    left to close (`include_braced_file_entry_then_nested_braced_error`).
  - Nothing is taken over again after a macro that only whitespace and `;` follow up to the end of
    the file. With a file `{ .include "files/v4/braced.inc"⏎;⏎`, `.include "…"⏎q = 1` →
    `{ a: int 1, q: int 1 }`, and `…⏎q = 1⏎}` is an error at its `}`. The same holds without the
    `;`, without the line break, with blank lines and spaces, and with a second macro
    `.include "files/v4/close_brace.inc"` as the last line, after which nothing follows either
    (`include_braced_file_nested_macro_at_end`,
    `include_braced_file_nested_macro_at_end_then_brace_error`).
  - Anything else after the macro, a comment included, takes the brace over again. With a file
    `{ .include "files/v4/braced.inc"⏎# c⏎`, `.include "…"⏎q = 1` is an error, because the brace
    is still held at the end, and `…⏎q = 1⏎}` → `{ a: int 1, q: int 1 }`; the same with
    `/* c */` or `.priority 1` on the next line or `; # c` after the macro
    (`include_braced_file_nested_macro_then_comment_unclosed_error`,
    `include_braced_file_nested_macro_then_comment_closed_by_includer`).
  - For a nested file that leaves a section path open, that decides whether its section object
    holds a brace. With a file `{ .include "files/v4/left_open.inc"⏎`,
    `.include "…"⏎q = 1⏎}⏎r = 2` is an error at the `}`, which finds `x` without a brace. With a
    file `{ .include "files/v4/left_open.inc"⏎# c⏎`, `x` holds the brace, so
    `.include "…"⏎q = 1⏎}⏎r = 2⏎}` → `{ x: { "y{": "z", q: int 1 }, r: int 2 }`
    (`include_braced_file_nested_left_open_at_end_brace_error`,
    `include_braced_file_nested_left_open_then_comment`).
  - Text parsed in place follows the same rule: `.emit "{ .include files/v4/braced.inc;"⏎q = 1`
    → `{ a: int 1, q: int 1 }`, while `.emit "{ .include files/v4/braced.inc; /* c */"⏎q = 1` is
    an error and needs a later `}` (`include_braced_text_nested_macro_at_end`,
    `include_braced_text_nested_macro_then_comment_unclosed_error`).
  - A nested file that leaves the brace it took over unclosed leaves it with the object, which
    then gets no second one: with a file `{ .include "files/v4/open_brace.inc"⏎}`, the file's `}`
    removes that brace, so `.include "…"⏎q = 1` → `{ a: int 1, q: int 1 }`, and `…⏎q = 1⏎}` is an
    error (`include_braced_file_nested_open_brace_kept`,
    `include_braced_file_nested_open_brace_kept_then_brace_error`).
  - A `}` of the file's own, read before any key of its own, ends the takeover as a key does:
    later macros take no brace over, and the first name after it gets no brace of its own (above).
    Each of these files makes the document an error, whether the including unit is
    `.include "…"⏎q = 1` or `.include "…"⏎q = 1⏎}`:
    `{ }⏎.include "files/v4/braced.inc"⏎a2 = 2⏎}` and `{ }⏎.priority 1⏎}`, at their last `}`;
    `{ .include "files/v4/braced.inc"⏎}⏎.include "files/v4/braced.inc"⏎x "y{" z⏎}⏎}`, at the `}`
    after `x "y{" z`; and `{ .include "files/v4/left_open.inc"⏎}⏎p "q{" r⏎}⏎}`, at the `}` after
    `r`, although the brace the file took over first is still held
    (`include_braced_file_own_brace_ends_takeover_error`,
    `include_braced_file_own_brace_then_macro_brace_error`,
    `include_braced_file_own_brace_after_nested_then_name_error`,
    `include_braced_file_own_brace_then_name_no_brace_error`). With a file
    `{ }⏎.priority 1⏎k = 1`, `.include "…"⏎q = 1` → `{ k: int 1 @1, q: int 1 }`, and `…⏎q = 1⏎}`
    is an error (`include_braced_file_own_brace_then_priority`).

The same brace bookkeeping explains why a `}` in an included file may close an object that the
including unit opened with `{`: `x { .include "files/v4/close_brace.inc"⏎q = 1` with
`close_brace.inc` = `a = 1 }` → `{ x: { a: int 1 }, q: int 1 }`. At the top level of a document
without braces, that `}` is an error (`include_file_closes_including_object`,
`include_file_closing_brace_at_top_level_error`).

- **Quirk.** When that `}` closes the braced root of the main document (§1.1), the rest of the file
  is ignored, as after a braced root, and after the macro the including unit may hold only
  whitespace and `;`: `{⏎.include "files/v4/close_brace.inc"⏎` → `{ a: int 1 }`, also when the file
  goes on (`a = 1 }⏎b = 2`, `a = 1 }⏎.foo`) or `;;` follows the macro
  (`include_closes_braced_root`, `include_closes_braced_root_then_semicolons`,
  `include_closes_braced_root_rest_of_file_ignored`,
  `include_closes_braced_root_rest_of_file_macro_ignored`). Anything else after it is an error,
  a key, a `}`, and even a comment: `…⏎k = 1`, `…⏎}`, `…⏎# c`, `…⏎/* c */`
  (`include_closes_braced_root_then_entry_error`, `include_closes_braced_root_then_brace_error`,
  `include_closes_braced_root_then_comment_error`,
  `include_closes_braced_root_then_block_comment_error`), and so is anything after the macro in a
  file between the two: `mid.inc` = `.include "files/v4/close_brace.inc"⏎z = 1`
  (`include_closes_braced_root_in_nested_file_error`).
- **Uncertain (undefined in libucl):** when that `}` closes an object that is an element of an
  array the including unit opened, libucl goes on reading the including unit as if it were still
  in the object while its entries would go to the array; a key there crashes it, and `]`, `,` or
  the end of input are errors. The project's choice: the including unit goes on inside the array,
  so `a = [ { .include "files/v4/close_brace.inc"⏎]` → `{ a: [{ a: int 1 }] }`, and a following
  `b = 1` line is the element `"b = 1"`.

**Quirk.** Objects that a section path left open (§3.4) at the end of an included file stay open,
and later entries of the including unit go into them: with `left_open.inc` = `x "y{" z`,
`.include "files/v4/left_open.inc"⏎k = 1` → `{ x: { "y{": "z", k: int 1 } }`
(`include_left_open_section_persists`). Left-open objects of different units close together, in
whatever order the units opened them: `"s".include "files/v4/left_open.inc"x {a = 1}⏎k = [1]` →
`{ s: { x: { "y{": "z", x: { a: int 1 } } }, k: [int 1] }`
(`include_left_open_from_both_units_close_together`).

**The check at the end of a unit.** At the end of each unit, the main document or an included
file, the containers still open are checked from the innermost outward, and the check stops at the
first container that another unit opened, whatever kind of container that is. Every container it
reaches that holds a bracket, its own or a taken-over one (above), was not closed, and that is an
error: an included file must close the objects it opened with `{`
(`include_unclosed_object_in_file_error`). The container where the check stops may be an object
whose brace a file took over, an object with a brace of its own, or an array element: with a file
`x { .include {files/v4/braced.inc}`, `a {⏎.include "…"⏎k = 1` →
`{ a: { x: { a: int 1, k: int 1 } } }`; with a file `b {⏎.include "files/v4/left_open.inc"⏎`,
`a {⏎.include "…"⏎m {}` parses; with a file `a = [ {⏎.include "…"` whose included file is
`x "y{" = ⏎`, a later `m { n = 1 }` in the main document goes into `x."y{"`
(`include_end_check_stops_at_object_with_taken_over_brace`,
`include_end_check_stops_at_braced_object_of_file`,
`include_end_check_stops_at_array_element_of_file`). **Quirk.** Containers
below one that another unit opened are therefore not checked:

- `a {⏎.include "files/v4/left_open.inc"` → `{ a: { x: { "y{": "z" } } }` without an error,
  although the brace of `a` is never closed, because the innermost open container `x` was opened by
  the included file. Later entries go into `x` (`include_left_open_end_check_stops`,
  `include_left_open_end_check_stops_then_entries`). The same happens at the end of an included
  file: a file holding `a {⏎.include "files/v4/left_open.inc"⏎`, included by `.include "…"⏎k = 1`,
  gives `{ a: { x: { "y{": "z", k: int 1 } } }`
  (`include_left_open_end_check_stops_in_included_file`); and with objects left open after a
  separator and a line break (§3.4), a file holding `c "x{" =⏎` inside `a {`
  (`include_left_open_after_separator_and_newline`).
- Once a bracketed container closes those objects (§3.4), the check reaches the containers below
  again: `a {⏎.include "files/v4/left_open.inc"⏎m { n = 1 }` is an error
  (`include_left_open_closed_then_checked_error`).
- **Uncertain (undefined in libucl):** whether a container opened by a unit that has already ended
  counts as opened by a later unit depends on memory reuse in libucl. In
  `.include "files/v4/left_open.inc"⏎.include "files/v4/open_brace.inc"`, where the second file
  takes over the brace of the object `x` that the first left open, the oracle checked `x` at the end
  of the second file and reported its brace unclosed. No case pins this.

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

| The file | `.include` | `.include(try=true)` | `.try_include` | `.try_include(try=false)` |
| --- | --- | --- | --- | --- |
| does not exist, including the empty path | error (`include_missing_error`) | skipped (`include_try_missing`, `include_empty_path_try`, `libucl/basic/14`) | stops silently (`try_include_missing_stops_parsing`, `try_include_empty_path_stops_parsing`) | stops silently (`try_include_try_false_missing_stops`) |
| is a directory or another non-regular file | error (`include_directory_error`) | skipped (`include_directory_try`) | stops silently (`try_include_directory_stops_parsing`) | error (`try_include_try_false_directory_error`) |
| exists as a regular file but cannot be opened for reading | error | error | error | error |
| is the file that holds the macro (resolved path, §9.3) | error (`include_self_error`, `include_main_document_itself_error`) | error (`include_self_with_try_error`) | stops silently (`try_include_self_stops_parsing`, `try_include_main_document_itself_stops`) | stops silently (`try_include_try_false_self_stops`) |
| exists and is readable | included (`try_include_present`) | included | included | included |

"Skipped" means nothing is included and parsing goes on after the macro. `.try_include(try=true)`
behaves as `.try_include` (`try_include_try_true_directory_stops`). Only the file that holds the
macro counts as itself; a cycle through other files ends at the nesting limit (above), for
`.try_include` too. `libucl/basic/9` ends with a `.try_include` of a missing file, so nothing is
lost there.

**Filesystem-dependent access failure.** A glob match that is a regular file but cannot be
opened for reading is also an error, even with `try=true` or `.try_include`. On the oracle host,
`/.file` is such a file and is among the matches of `"/.*"`, so
`.include(g=true,t=true) "/.*"` is an error. The same pattern may have a different result on a
host with different files or permissions. No portable golden case pins read denial: the case
files cannot guarantee it across checkouts and test identities. The neighboring rules for `.*`
and `t=true` are pinned by `include_glob_dot_star_try` and `include_param_prefix_names`.

A silent stop inside an included file ends the parse in every open unit, also when the file was a
match of a glob pattern of `.include`, with or without `try=true`
(`include_glob_stop_inside_match_ends_parse`, `include_glob_try_stop_inside_match_ends_parse`).
**Uncertain (undefined in libucl):** when an included file fails, by an error or a silent stop,
and libucl goes on with the same macro regardless, the oracle crashes. `.try_include` with a glob
pattern goes on after a failing match, with the next match or, after the last, with the rest of
the input: `.try_include(glob=true) "q/*.inc"` where a match holds `.try_include "missing"` or a
syntax error. A search path (below) goes on with the next directory:
`.include(path=["q", "q2"]) "x.inc"` where `q/x.inc` fails; a failure in the last directory is an
error. **Project choice:** the failure counts as it does anywhere else: an error fails the
document, and a silent stop ends the parse.

### Globs

With `glob=true`, a path that contains `*` or `?` is a pattern. A path without either is used as
written, even if it contains `[`: `files/v4/g/[ab].inc` is simply missing
(`include_glob_bracket_needs_wildcard_error`, `include_pattern_without_glob_error` for a pattern
without `glob=true`).

**Quirk: a NUL byte in a pattern.** For a VALUE with a NUL byte (§9.2), the `*` or `?` is looked
for in the whole VALUE, after the NUL too, and the pattern is the part before the NUL. So
`a = 1⏎.include(glob=true) {files/nomatch<NUL>*}⏎b = 2` is the pattern `files/nomatch`, which
matches nothing and stops silently (below): `{ a: int 1 }`. `{<NUL>*}` is the empty pattern and
stops too, `{files/a.inc<NUL>*}` includes `files/a.inc`, and with `try=true` a pattern that matches
nothing is skipped (`include_glob_wildcard_after_nul_no_match_stops`,
`include_glob_nul_first_wildcard_after_stops`, `include_glob_wildcard_after_nul_matches_part_before`,
`include_glob_wildcard_after_nul_try_continues`). With a search path in effect (*Signatures, URLs
and search paths*), the path is cut at the NUL before the wildcard is looked for:
`.include(glob=true, path=["."]) {files/a.in<NUL>?}` is the missing path `./files/a.in`, an error
(`include_glob_search_path_cuts_at_nul_first_error`).

- Patterns use `*`, `?` and bracket expressions such as `[ab]`, with no brace expansion (`{a,b}` is
  literal) and no `~` expansion (`include_glob`, `include_glob_question_mark`,
  `include_glob_bracket_expression`, `include_glob_no_brace_expansion`). In detail:
  - Wildcards work in directory components too: `files/v4/?/a.inc`, `files/v4/*/pa.inc`.
    Components `.` and `..` and a doubled `/` are allowed (`include_glob_in_directory_components`,
    `include_glob_dot_components_and_double_slash`).
  - A `.` at the start of a name is matched only by a `.` in the pattern, so `*` and `?` leave
    hidden files out (`include_glob_skips_hidden_files`). A pattern `.*` also matches the entries
    `.` and `..`, which are directories: `.include(glob=true) "files/v4/g/.*"` is an error, and with
    `try=true` only the hidden file is included (`include_glob_dot_star_matches_dot_entries_error`,
    `include_glob_dot_star_try`).
  - A bracket expression matches one character of its set. Ranges such as `a-b` work, and a
    reversed range matches nothing (`include_glob_range`,
    `include_glob_reversed_range_matches_nothing`). Only `!` directly after `[` negates the set;
    `^` is an ordinary member, so `[^a]*` matches `a.inc` (`include_glob_bang_negates`,
    `include_glob_caret_is_not_negation`). A `]` directly after `[` or `[!` is a member
    (`include_glob_close_bracket_first_is_member`,
    `include_glob_bracket_negation_close_bracket_member`). There are no character classes:
    `[[:alpha:]]*` is the set of `[`, `:`, `a`, `l`, `p` and `h`, then a literal `]`, and matches
    `a]x.inc` (`include_glob_no_character_classes`). A `[` without a closing `]` is an ordinary
    character: `[a*` matches `[ab.inc` (`include_glob_unclosed_bracket_is_literal`).
  - A backslash makes the next character ordinary, inside brackets too: `\[a*` matches `[ab.inc`,
    and `[\a]*` is the set of `a` (`include_glob_backslash_quotes_next_character`,
    `include_glob_backslash_inside_brackets`). The cases write these patterns in braces.
  - A pattern that ends in `/` matches directories only. `files/v4/g/*/` matches the directory
    `sub`, so `.include` fails and `try=true` skips it; where no directory matches, `.include`
    stops silently, as for no match below (`include_glob_trailing_slash_matches_directories_error`,
    `include_glob_trailing_slash_try`, `include_glob_trailing_slash_no_directory_stops`).
    **Uncertain (depends on the operating system):** on the oracle platform, such a pattern also
    matches a symbolic link to a regular file, which is then included, and a plain path that ends
    in `/` after the name of a file or of a link to one names that file (`.include
    "files/c.conf/"`). On Linux neither holds: the link is not matched, and the plain path is an
    error. No case pins this; the project may follow either.
  - libucl leaves matching and sorting to the C library, and other C libraries differ, for example
    in `[^…]` and character classes: with glibc, `[^a]*` does not match `a.inc` and
    `[[:alpha:]]*` matches any name that starts with a letter. The rules here are those of the
    oracle's C library, the cases follow them, and they are what an implementation follows on
    every platform. The golden files of the cases that give other results with another C library
    are also recorded per platform, for the drift check only (`tests/conformance/README.md`).
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
- `.try_include(try=false)` with matches: a matched directory is an error
  (`try_include_glob_try_false_directory_error`). A match that is the including file is still
  skipped, but when no match was included at all, that is an error: a file `main.inc` that holds
  `.try_include(glob=true, try=false) "…/*.inc"` matching itself and `other.inc` includes
  `other.inc`, and the same macro in a directory where it matches only itself is an error
  (`try_include_glob_try_false_skips_self`, `try_include_glob_try_false_only_self_error`).

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
  (`include_glob_prefix_key_from_first_file`). This depends on the C library as well: with glibc,
  each matched file gets its own key.
- K is not lowercased under `key-lowercase`, but it is compared with existing keys ignoring ASCII
  case, like every key under that flag (§12.1; `include_key_not_lowercased_but_matched`).
- An object or array created for K has the include's `priority`
  (`include_prefix_priority_applies_to_nesting_object`, `include_prefix_array_priority`).
- An empty file still creates K, as an empty object (`include_prefix_empty_file_creates_object`).
- The containers the file leaves open close when it ends, together with K's object, and later
  entries of the including unit go where the macro stands:
  `.include(key="k") "files/v4/left_open.inc"⏎q = 1` → `{ k: { x: { "y{": "z" } }, q: int 1 }`
  (`include_key_file_containers_close_at_end`). A `{` at the start of the file takes over the brace
  of K's object (above) and goes away with it:
  `.include(key="k") "files/v4/open_brace.inc"⏎q = 1⏎}` is an error, because the `}` then has
  nothing to close (`include_key_open_brace_then_close_error`).
- A `}` in the file that would close K's object is an error when the object where the macro stands
  holds no bracket: `.include(key="k") "files/v4/close_brace.inc"⏎q = 1`
  (`include_key_close_brace_at_top_level_error`). When that object holds a brace taken over from an
  earlier included file, the `}` uses up K's share of that brace and closes nothing:
  `.include "files/v4/open_brace.inc"⏎.include(key="k") "files/v4/close_brace.inc"⏎q = 1⏎}` →
  `{ a: int 1, k: { a: int 1 }, q: int 1 }`, the last `}` removing the taken-over brace of the root
  (`include_key_close_brace_under_taken_over_brace`). K's object gets such a share whenever the
  object where the macro stands holds a taken-over brace: with `target="array"` the share goes to
  the new object inside the array, and it works the same with `prefix=true` and under a section
  object whose brace a file took over (`include_key_share_target_array`,
  `include_key_share_prefix`, `include_key_share_section_object`). Each include gets its own
  share (`include_key_share_two_includes`); a share left unused does no harm, and a braced file
  under K works as usual (`include_key_share_unused`, `include_key_share_braced_file`); a second
  `}` in the file finds nothing to close, an error (`include_key_share_used_twice_error`).
  **Uncertain (undefined in libucl):** when the
  object holds only its own bracket, as in `x { .include(key="k") "files/v4/close_brace.inc"⏎q = 1`,
  libucl crashes. The implementation may choose.

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

What the project does with these parameters is decided in README, *Divergences decided by the
project*. libucl behaves as follows:

- `sign=true`, the default for `.includes`, asks for signature checking when libucl is built with
  it: the file `PATH.sig` must verify. The oracle build has none, so `sign` has no effect and
  `.includes` behaves exactly like `.include` (`includes_like_include`,
  `include_sign_param_no_effect`). The project rejects `.includes`, and `sign=true` on any include
  macro, with its "unsupported" error; `sign=false` is accepted.
- `url=true` with `://` in the path fetches a URL when libucl is built with URL support. The
  oracle build has none, and the project never fetches URLs and behaves as that build. Such an
  include is recognised before the search path and before globs and `key` or `prefix` are looked
  at, and it includes nothing:
  - for `.include` it is an error (`include_url_param_error`);
  - with `try=true`, and for `.try_include`, it is skipped and parsing goes on; for `.try_include`
    this is not a silent stop (`include_url_try_skipped`, `try_include_url_skipped`);
  - `.try_include(try=false)` makes it an error again (`try_include_url_try_false_error`);
  - with `key` or `prefix`, no key is created (`include_url_key_not_created`), and a search path
    in effect is not used (`include_url_before_search_path`);
  - its own `path` parameter still takes effect, and replaces the search list for later includes
    (`include_url_path_param_takes_effect`, `include_url_path_param_replaces_list`);
  - any `://` in the path makes it a URL, whatever comes before it: `files://a.inc`, `://`
    (`include_url_any_scheme_separator`); `glob` and `prefix` make no difference
    (`include_url_glob_prefix_no_difference`).

  Without `url=true`, a path with `://` is an ordinary path (`include_url_like_path_is_a_path`).
- `path=[…]` sets a list of search directories. Entries that are not strings are skipped; a
  `path` that is not an array is ignored (`include_path_string_ignored`). A later `path` replaces
  the list: `path=["files/v4/p1"]` and then `path=["files/v4/p2"]` searches `p2` alone
  (`include_path_later_list_replaces`). A list with no strings is empty: `path=[1]` makes every
  include an error (`include_path_no_string_entries_error`). `.load` does not use the list (§9.6).
  **Quirks:**
  - The list stays in effect for every later include of the whole parse, with or without `path`
    (`include_path_persists`).
  - While a list is in effect, each path is tried as `DIR/PATH`, absolute paths included, for
    the directories in order. For `.include`, the first directory normally decides: if the file
    is missing there, the document is an error (`include_path_first_dir`,
    `include_path_missing_in_first_dir_error`), and with `try=true` it is skipped without looking
    further (`include_path_try_first_dir_only`). `.try_include` does search: the first directory
    that has the file is used (`try_include_path_searches_all_dirs`), and a file found in none of
    them is an error, not a silent stop (`try_include_path_missing_error`). An empty list makes
    every include an error (`include_path_empty_array_error`).
  - **Quirk: a later skipped URL include or `.load`.** If the first directory of a `.include`
    without `try=true` or `glob=true` lacks the file but a later directory has it, and a
    subsequent macro skips its file, the document is accepted with the later directory's file
    included. Entries between the two macros and after the second are read too. The same skips
    discard a rejected argument document of an earlier macro (§9.2, *ARGUMENTS*), and a skip
    covers errors of both kinds before it, also from a later input (§13.1) or another unit
    (below). The macros that count are a `.include(try=true, url=true)` or
    `.try_include(url=true)` with `://` in its path that is skipped
    (`include_path_first_miss_then_url_try_accepts_later`,
    `include_path_first_miss_then_try_include_url_accepts_later`), and a `.load` with `try=true`
    whose file is missing or unusable, so that it reads nothing (§9.6): after
    `.include(path=["", "files/v4/p1"]) "pa.inc"`, the lines `x = 1`,
    `.load(try=true, key="t") "missing.txt"` and `after = 2` give
    `{ pa: int 1, x: int 1, after: int 2 }`, and so does the directory `"files/v4"` as the `.load`
    path (`include_path_first_miss_then_load_try_accepts_later`,
    `include_path_first_miss_then_load_try_directory_accepts_later`). This holds whether the
    document is given as a file or as text
    (`include_path_first_miss_then_url_try_accepts_later_file_input`,
    `include_path_first_miss_then_load_try_accepts_later_file_input`). Without a matching later
    directory, or when the later URL include is not skippable, the document is an error
    (`include_path_first_miss_then_url_try_no_later_file`,
    `include_path_first_miss_then_url_error`). So it is when the `.load` reads its file, an empty
    one included, although that inserts nothing (§9.6), or, without `try=true`, fails on a
    missing one (`include_path_first_miss_then_load_try_existing_file_error`,
    `include_path_first_miss_then_load_try_empty_file_error`,
    `include_path_first_miss_then_load_missing_error`), and when the skip comes before the
    `.include` (`include_path_first_miss_after_load_try_error`). A skip covers every such miss
    before it, and none after it: a later miss needs a later skip of its own. The list stays in
    effect, so a later `.include "pa.inc"` misses in `""` again; without a further skip the
    document is an error (`include_path_first_miss_again_after_load_try_error`), and with one it
    is accepted, both files included: `pa: ⟨int 1 | int 1⟩`, whether a skip follows each miss or
    one skip follows two misses, and whether the skips are URL includes, `.load`s or one of each
    (`include_path_first_miss_load_try_after_each_miss_accepts`,
    `include_path_two_first_misses_then_load_try_accepts`,
    `include_path_first_miss_url_try_after_each_miss_accepts`,
    `include_path_two_first_misses_then_url_try_accepts`,
    `include_path_first_miss_url_then_load_try_accepts`). A later optional ordinary file include
    does not change the first-directory error
    (`include_path_first_miss_then_optional_file_error`). The ordinary first-directory rule above
    applies otherwise. These cases do not establish an exception for other later macros or other
    kinds of file failure.
  - **Quirk: the miss and the skip in different units.** The error and the skip need not be in
    the same unit. What counts is the order in which the parse reads them, through included
    files, text parsed in place (§13.2) and later inputs (§13.1) alike. This holds for a
    first-directory miss and for a rejected argument document (§9.2). Write MISS for
    `.include(path=["", "files/v4/p1"]) "pa.inc"` and SKIP for
    `.load(try=true, key="t") "missing.txt"`. The following give `{ pa: int 1, q: int 1 }`:
    - a file `MISS⏎SKIP` included by `.include "…"⏎q = 1`;
    - a file `MISS` included by `.include "…"⏎SKIP⏎q = 1`;
    - text parsed in place after the miss, `MISS⏎.emit {SKIP}⏎q = 1`;
    - a miss in text parsed in place, `.emit {MISS⏎SKIP}⏎q = 1` and `.emit {MISS}⏎SKIP⏎q = 1`;
    - a file holding SKIP, or the skipped URL include `.include(try=true, url=true) ://`,
      included after the miss: `MISS⏎.include(path=["files/v23"]) "load_skip.inc"⏎q = 1`.

    (`pending/09-macros/include_path_first_miss_and_load_try_in_included_file`,
    `pending/09-macros/include_path_first_miss_in_included_file_then_load_try`,
    `pending/09-macros/include_path_first_miss_then_load_try_in_text`,
    `pending/09-macros/include_path_first_miss_and_load_try_in_text`,
    `pending/09-macros/include_path_first_miss_in_text_then_load_try`,
    `pending/09-macros/include_path_first_miss_then_load_try_in_included_file`,
    `pending/09-macros/include_path_first_miss_then_url_try_in_included_file`).
    Likewise `.priority(x) 3⏎a = 1⏎.include "files/v23/load_skip.inc"` → `a: int 1 @3`
    (`pending/09-macros/macro_args_rejected_then_load_try_in_included_file`).

    Without a later skip, a miss in an included file or in text parsed in place is an error
    (`include_path_first_miss_in_included_file_error`, `include_path_first_miss_in_text_error`).
    A skip in a macro argument document does not count, because that document is parsed on its
    own (§9.2): `MISS⏎.priority(SKIP) 1⏎q = 1` is an error
    (`include_path_first_miss_then_skip_in_args_error`).

    The list that the miss set stays in effect (above), so a file included after it is searched
    in those directories too. `MISS⏎.include "files/v23/load_skip.inc"⏎q = 1` is an error: no
    directory of the list has that path, so the include fails and its SKIP is never read
    (`include_path_first_miss_then_skip_file_outside_list_error`).

    A skip still covers only the misses read before it. With the file `MISS⏎SKIP`,
    `.include "…"⏎.include "pa.inc"⏎q = 1` is an error, because the second include misses in `""`
    again. With a further SKIP before `q = 1`, it gives `pa: ⟨int 1 | int 1⟩`
    (`include_path_skip_in_included_file_then_miss_error`,
    `pending/09-macros/include_path_skip_in_included_file_then_miss_then_skip`).
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
- **Quirk.** The text ends at its first NUL byte, which only a braced VALUE can hold (§9.2): what
  follows the NUL is not read, and an empty text before it counts as 0 rather than as an error,
  so `.priority {<NUL>}` sets 0 (`macro_value_nul_ends_priority`,
  `macro_value_nul_first_priority_zero`).
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

- The path is used as written (§9.3); a search path (§9.4) is never used
  (`load_ignores_search_path`). An empty VALUE is an error, even with `try=true`
  (`load_empty_path_error`). A VALUE that starts with a NUL byte is not empty: it names the empty
  path as a file, which is missing (§9.2, *Quirk: a NUL byte in VALUE*).
- The checks come in this order, and the first that fails decides: `key` missing or empty, an
  error even with `try=true` (`load_without_key_error`, `load_missing_key_error_with_try`); an
  empty VALUE; a missing or unusable file, an error or with `try=true` nothing inserted; K already
  present, an error (below). So `t = 1⏎.load(key="t", try=true) "missing"` → `{ t: int 1 }`, while
  `t = 1⏎.load(key="t", target="float") "files/num.txt"` and
  `t = 1⏎.load(key="t") "files/v4/empty.txt"` are errors (`load_try_missing_before_existing_key`,
  `load_existing_key_before_target`, `load_existing_key_before_empty_file`).
- A `.load` with `try=true` that inserts nothing because its file is missing or unusable also
  lets an earlier first-directory miss of a `.include` pass (§9.4, *Quirk: a later skipped URL
  include or `.load`*).
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
  `load_trim_all_whitespace`). A file of whitespace only then gives `""`; only an empty file
  inserts nothing (`load_trim_whitespace_only_empty_string`).
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
- Each entry of NAME whose key the current object does not yet have is copied (`inherit_basic`,
  `inherit_existing_keys_kept`). **Quirk.** If the entry's first value is an object or an array,
  only that value is copied; otherwise all its values are: with
  `d { a = [1]; a = 5; b { x = 1 }; b = 5; c = 5; c = [1]; c = 6 }`, `e { .inherit "d" }` gets
  `a: [int 1]`, `b: { x: int 1 }` and `c: ⟨int 5 | [int 1] | int 6⟩`
  (`inherit_container_first_value_only`). The rule holds at every level of the copy: inside a
  copied object, and in the objects of a copied array, an entry whose first value is an object or
  an array keeps that value only, and an entry whose first value is anything else keeps all its
  values. With `d { o { b { x = 1 }; b = 5; a = [1]; a = 2; s = 1; s = 2 }; r = [ { b { x = 1 };
  b = 5 } ] }`, `e { .inherit "d" }` gets `o: { b: { x: int 1 }, a: [int 1], s: ⟨int 1 | int 2⟩ }`
  and `r: [ { b: { x: int 1 } } ]` (`inherit_first_value_rule_at_every_level`). A second
  `.inherit` of the same object therefore
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
- Keys are copied byte for byte, NUL bytes included: `d { "k\u0000z" = 1 }⏎e { .inherit "d" }`
  gives `e` the key `k<NUL>z` (`inherit_copies_key_with_nul`). **Uncertain (undefined in
  libucl):** a copied string that holds a NUL byte keeps its length, but its bytes after the
  first NUL depend on memory contents. On the oracle machine they come out as NUL bytes: `d { a = "\u0000x" }⏎e { .inherit "d" }` gives `e.a` = `"\u0000\u0000"`, and
  `"x\u0000yz"` gives `"x\u0000\u0000\u0000"`. The project copies the bytes as they are.
- The copy is shallow at the entry level. A later explicit `a { … }` replaces an inherited `a`
  entirely (`inherit_is_shallow`).
- **Quirk.** With `replace=true`, every entry is copied even when the key exists. The copied values
  are then added to the entry as further values, not as a replacement (`inherit_replace_appends`),
  and §8 is not applied: priorities, the strategy and `no-implicit-arrays` play no part.
  `d { a = 1 }⏎.priority 2⏎e { a = 0; .inherit(replace=true) "d" }` → `e.a: ⟨int 0 @2 | int 1⟩`;
  the same inputs give `⟨int 0 | int 1⟩` without an error under `error`, a multi-value entry
  rather than an array under `no-implicit-arrays`, and under `merge`, with objects,
  `⟨{ y: int 1 } | { x: int 1 }⟩` (`inherit_replace_ignores_priority`,
  `inherit_replace_ignores_strategy_error`, `inherit_replace_no_implicit_arrays_multivalue`,
  `inherit_replace_ignores_strategy_merge`). Such copies are not marked inherited, so a later
  explicit value is added to them rather than replacing them: `e { .inherit(replace=true) "d"; a = 2 }`
  → `e.a: ⟨int 1 | int 2⟩` (`inherit_replace_copies_not_inherited`). But a copy keeps the marks of
  the value it copies: a copy of an inherited value is inherited, and under `no-implicit-arrays` a
  copy of a collected array (§8.5) collects later repeats:
  `d { a = 1 }⏎e { .inherit "d" }⏎f { .inherit(replace=true) "e"; a = 5 }` → `f.a: int 5`;
  with the flag, `d { a = 1; a = 2 }⏎e { .inherit(replace=true) "d"; a = 3 }` →
  `e.a: [int 1, int 2, int 3]` (`inherit_replace_copy_of_inherited_stays_inherited`,
  `inherit_replace_copy_keeps_collected_array`). Each such `.inherit` adds another copy, and one of
  the current object copies its entries onto themselves: `e { a = 1; .inherit(replace=true) "e" }`
  → `e.a: ⟨int 1 | int 1⟩`, and the first-value rule above applies:
  `e { a = [1]; a = 5; .inherit(replace=true) "e" }` → `e.a: ⟨[int 1] | int 5 | [int 1]⟩`
  (`inherit_replace_twice_duplicates`, `inherit_replace_own_object_duplicates`,
  `inherit_replace_own_container_first_value_only`).
- Copies carry no saved comments, and for §12.5 they are not values that were created: comments
  pending before `.inherit` stay pending (`inherit_copies_have_no_comments`,
  `inherit_copies_take_no_pending_comments`).
- **Quirk.** `replace` is not matched like other parameters: only the exact name `replace` counts
  (lowercased first under `key-lowercase`), a repeated `replace` uses its first value, and the
  value must be a bool (`inherit_replace_no_prefix_match`, `inherit_replace_first_value`,
  `inherit_replace_must_be_boolean`, `inherit_replace_keyword_yes`). Other parameters are ignored.
- At the root, `.inherit` copies into the root object itself (`inherit_at_top_level`).
- Cases: `libucl/basic/18`.

# 13. Several inputs and registered macros

The sections before this one describe one document read by a parser. libucl's parser also lets an
application give it several inputs in turn, building one result (§13.1), and register macros of its
own (§13.2). This section describes both as libucl behaves.

The cases are in `tests/conformance/cases/spec/13-inputs/`; a bare case name here means that
directory. A case's `.ucl` file is its first input; a `.inputs` file
lists the further inputs, and the flags `registered-macros` and `registered-priority-override`
register the oracle's test macros (§13.2, *The test macros*). `tests/conformance/README.md`
describes the format.

## 13.1 Several inputs

An input is a document given as text or a file given by its path. Each input comes with a priority
(§8.3) and a duplicate strategy (§8.4); a file input is read as §9.3 and §12.7 describe for a
document given as a file.

### Entries combine

Each input goes on where the one before it ended: its top-level entries go into the same root,
and a repeated key follows §8 across inputs, each value with the priority and strategy of the
input it comes from. The strategy of an input decides what happens when that input repeats a key;
values that earlier inputs added are not treated again.

- `a = 1; b = 2` then `a = 3; c = 4` → `{ a: [1, 3], b: 2, c: 4 }` (`inputs_entries_combine`); with
  the second input at priority 5, `a` is 3 alone, at priority 5 (`inputs_priority_per_input`);
  with the second input under `rewrite`, `a` is 3 (`inputs_strategy_per_input_rewrite`); under
  `error` the result is an error (`inputs_strategy_per_input_error`). A first input under
  `rewrite` followed by one under `append` keeps both values (`inputs_strategy_first_input_only`).
- Objects merge under `merge` and form a multi-value entry under `append`
  (`inputs_objects_merge`, `inputs_objects_append`); under `no-implicit-arrays` a repeated key
  collects its values across inputs as in one input (§12.4; `inputs_no_implicit_arrays_across`).
- An input's priority is taken modulo 16 like every priority (§8.3): 17 → 1, 16 → 0
  (`inputs_priority_low_four_bits`). `.priority` in an input applies to the rest of that input
  only (§9.5); the next input starts with its own priority (`inputs_priority_macro_stays_in_its_input`,
  `inputs_priority_macro_in_later_input`).
- Variables registered with the parser, and its variable handler (§7.7), apply to every input
  (`inputs_variables_in_every_input`), and so do registered macros (§13.2;
  `inputs_registered_macro_in_later_input`). `.inherit` finds objects that earlier inputs added
  (§9.7; `inputs_inherit_from_earlier_input`).
- Saved comments (§12.5) attach as in one document, with the inputs read as one text, except at
  the end of an input:
  - Comments still pending at the end of an input attach there, as at the end of a document, to
    the value created most recently, in that input or an earlier one. Only when no value has been
    created at all do they attach after the root. Comments at the start of an input attach to its
    first value (`inputs_comments_stay_in_their_input`,
    `inputs_comment_only_input_attaches_to_root`, `joins_comment_input_attaches_to_earlier_value`,
    `joins_empty_first_then_hash_attaches_to_root`).
  - Comments pending at a silent stop (below) are not attached at the stop: they attach to the next
    value, which the next input creates, or at the end of the next input that has content. A
    zero-byte input does not count (`joins_comments_at_stop_attach_in_next_input`,
    `joins_comments_at_stop_then_whitespace_input`, `joins_comments_at_stop_then_empty_input`).
  - Comments pending while a key waits for its value in a later input (below) wait with it. The
    group before the key attaches before the value, and the group held back after the key
    (§12.5) attaches after it, as in one document. Comments in the input that holds the value
    are treated as in one document (`joins_value_in_next_input_comments_wait`,
    `joins_key_waiting_comments_before_key`, `joins_key_waiting_comment_after_key`,
    `joins_key_waiting_two_comment_groups`, `joins_key_waiting_comment_in_value_input`).

### The root

Only the first input sets up the root (§1.1). Later inputs add entries to an object root; they
cannot start a root of their own.

- **Quirk.** After a braced root has closed, and after an array root, a later input is read as the
  rest of a document after its root (§1.1).
  - After optional spaces and TABs, it must begin with a line break, `;`, `,` or a comment, or
    end. Anything else is an error: an entry, a bracket, VT, FF, or a space before an entry
    (`inputs_braced_root_then_entries_error`, `inputs_braced_root_then_braced_root_error`,
    `inputs_array_root_then_array_error`, `inputs_array_root_then_entries_error`,
    `joins_closed_root_then_space_entry_error`, `joins_closed_root_then_vertical_tab_error`,
    `joins_closed_root_then_brace_error`).
  - After such a beginning, and any further mix of these, the rest of the input is ignored,
    whatever it holds (`joins_closed_root_then_line_break_rest_ignored`,
    `joins_closed_root_then_semicolon_rest_ignored`, `joins_closed_root_then_comment_rest_ignored`,
    `joins_closed_root_then_vertical_tab_after_line_break`, `joins_closed_root_then_spaces_only`,
    `joins_array_root_then_line_break_rest_ignored`, `joins_array_root_then_comma_rest_ignored`).
  - The exception is a `}` or `]` that follows them, which is an error
    (`joins_closed_root_then_line_break_brace_error`,
    `joins_closed_root_then_semicolon_bracket_error`, `joins_closed_root_then_comment_brace_error`).
- After an object root without braces, a later input that starts with `{` or `[` is an error: the
  bracket stands where a key must (`inputs_entries_then_braced_root_error`,
  `inputs_entries_then_array_root_error`).
- A first input of whitespace or comments only sets up the object root as a document would, and
  later inputs add to it (`inputs_whitespace_first_then_entries`,
  `inputs_comment_first_then_entries`).
- A zero-byte input after the first changes nothing (`inputs_empty_later_input`), and zero-byte
  inputs alone give `{}` (`inputs_empty_inputs_only`).
- **Quirk.** A zero-byte **first** input gives an empty object root that later inputs cannot add
  to: a later input with content is an error (`inputs_empty_first_then_entries_error`,
  `inputs_empty_first_then_array_error`). A later input is accepted only when nothing is left of
  it after a group of comments at its very start (§1.1) and then whitespace: `# c⏎` and whitespace
  alone are fine, whitespace followed by a comment is an error, and so is `;`
  (`inputs_empty_first_then_comment_only`, `inputs_empty_first_then_whitespace`,
  `inputs_empty_first_then_comment_error`, `joins_empty_first_then_semicolon_error`). **Uncertain (crashes libucl):** a later input that
  starts with `{` after a zero-byte first input.

### Where one input ends and the next begins

**Quirk.** The end of an input is not a separator (§1.3). The rules below say where a later input
needs one; the `joins_*` cases show each of them.

- **A value at the end of its input.** A value that ends at the very end of its input must be
  separated from the next input's first entry. So must an unquoted value followed there only by
  spaces or TABs, since those spaces belong to the value's scan.
  - The next input must begin with a line break, `;`, `,`, a comment or a NUL byte. It may also
    begin with the `}` or `]` that closes the value's container. Otherwise its first entry is an
    error, and so is a VT or FF there: `x = 1`, `x = 1 `, `x = true `, `x = 1s ` and `x = "s"`
    followed by `k = 1;` are errors, while `x = 1` followed by `⏎k = 1;`, `;k = 1;` or `# c⏎k = 1;`
    is fine (`inputs_value_at_end_then_entry_error`, `inputs_value_then_space_at_end_error`,
    `inputs_quoted_value_at_end_error`, `inputs_value_at_end_then_line_break`,
    `inputs_value_at_end_then_semicolon`, `inputs_value_at_end_then_comment`,
    `joins_bare_value_trailing_space_error`, `joins_time_value_trailing_space_error`,
    `joins_value_at_end_then_nul`, `joins_value_at_end_then_closing_brace`,
    `joins_value_at_end_then_form_feed_error`, `joins_value_at_end_then_vertical_tab_error`).
  - A quoted value ends at its closing quote, so spaces or TABs after it at the end of its input
    are a separator: `x = "s" ` and `x = 'a'⇥` followed by `k = 1` are fine
    (`joins_quoted_value_trailing_space`, `joins_single_quoted_value_trailing_tab`).
  - A later input of spaces, TABs or line breaks alone also separates. A second such input
    undoes that, and a third separates again; a zero-byte input changes nothing
    (`joins_value_at_end_then_whitespace_input`, `joins_value_at_end_two_whitespace_inputs_error`,
    `joins_value_at_end_empty_input_error`).
  - A `#` that is the last byte of the next input is a comment (`joins_value_at_end_then_hash`).
- **An input of whitespace alone where an entry could start.** A later input of whitespace alone
  (space, TAB, LF, CR, VT or FF), read where an entry could start, makes the next input with
  entries need a separator first, as after a value. A second such input undoes that, and so on
  alternately.
  - Examples: `a = 1;⏎` followed by ` ` or `⏎`, and then `k = 1`, is an error; with two inputs of
    spaces in between it is fine; `;k = 1` or a comment first is fine.
  - The first input is not counted: `  ` then `k = 1` is fine, while `  `, `  ` and `k = 1` is an
    error.
  - The same holds after section names that end their input with a separator and a line break
    (§3.4): `c "x{" =⏎` followed by `d {}` is an error, while a comment, an input of spaces or `;`
    in between makes it `{ c: { "x{": { d: {} } } }`.
  - Cases: `joins_whitespace_input_needs_separator_error`, `joins_two_whitespace_inputs_cancel`,
    `joins_line_break_input_needs_separator_error`, `joins_whitespace_input_then_semicolon`,
    `joins_whitespace_first_input_not_counted`, `joins_whitespace_first_then_whitespace_error`,
    `joins_section_names_then_entry_error`, `joins_section_names_then_comment_input`,
    `joins_section_names_then_whitespace_input`, `joins_section_names_then_semicolon_input`.
- **A `#` as the last byte of a later input.** Where an entry could start, such a `#` is an error
  when it comes after whitespace, as §2.2 describes, and the start of a later input counts as
  after whitespace. `a = 1;⏎` followed by `#`, ` #` or `⏎#` is an error; followed by `# c⏎#` it is
  fine (`joins_hash_last_byte_of_later_input_error`, `joins_space_hash_last_byte_error`,
  `joins_line_break_hash_last_byte_error`, `joins_comment_then_hash_last_byte`).
- A value followed in its own input by `;`, `,`, a line break or a comment needs nothing more, and
  neither does an object or array value or a heredoc (`inputs_value_then_semicolon_at_end`,
  `inputs_value_then_comma_at_end`, `inputs_value_then_comment_at_end`, `inputs_container_at_end`,
  `inputs_heredoc_at_end`).
- An entry cannot be split across inputs: an input that ends after a key, or after its `=`, is an
  error, whatever follows, and so is a last input that does (`inputs_key_split_across_inputs_error`,
  `inputs_entry_split_across_inputs_error`, `inputs_incomplete_entry_in_last_input_error`).
- **A value on a following line** (§1.6) may come from a later input: an input that ends with a key,
  its separator and a line break leaves the key waiting, and the next input with content gives its
  value.
  - `x =⏎` followed by `v` gives `{ x: "v" }`, followed by `k = 1⏎` gives `{ x: "k = 1" }`, and
    zero-byte inputs and comment-only inputs in between are skipped. A key still waiting after
    the last input is `null`, as at the end of a document.
  - The value takes the priority and duplicate strategy of the key's input, `.priority` included:
    `.priority 3⏎x =⏎` followed by `{ a = 1 }` at priority 5 gives `x` at 3 and `a` at 5.
    `x = 1;⏎x =⏎` followed by `v` in an input under `error` gives `{ x: [1, "v"] }`.
  - While a key waits, a later input that is a single `#` is an error, as a `#` directly after
    whitespace is where a value is expected. ` #`, `# c` and `#⏎v` are not errors.
  - Cases: `joins_value_in_next_input`, `joins_value_in_next_input_whole_line`,
    `joins_value_after_empty_and_comment_inputs`, `joins_value_in_next_input_key_priority`,
    `joins_value_in_next_input_key_strategy`, `joins_key_waiting_then_hash_input_error`,
    `joins_key_waiting_then_space_hash`, `joins_key_waiting_then_comment_input`,
    `joins_key_waiting_then_hash_line_value`.

### Brackets

- The check at the end of a unit (§9.4) runs at the end of every input, for the containers that
  input opened: an input that leaves a brace open is an error, even if a later input would close
  it (`inputs_unclosed_brace_error`). A `}` in a later input that closes nothing is an error
  (`inputs_closing_brace_in_later_input_error`).
- Section objects (§3.4) left open at the end of an input stay open: the next input's entries go
  into them. `"s".priority {3}⏎` followed by `k = 1;` → `{ s: { k: 1 } }`; a `}` in that later
  input is an error, as in one document (`inputs_section_object_left_open`,
  `inputs_section_object_left_open_bracket_error`).

### Errors and silent stops

- An error in any input makes the result an error, whatever the other inputs hold
  (`inputs_error_in_first_input`, `inputs_error_in_later_input`). An error ends its input, as in
  one document, and a skip in a later input does not discard it:
  `a = 1⏎.include "nonexistent.inc"⏎c = 3` followed by an input
  `b = 2⏎.load(try=true, key="t") "missing.txt"` is an error
  (`inputs_missing_include_then_skip_in_later_input_error`).
- **Quirk.** The two errors that do not end the parse, a rejected argument document (§9.2,
  *ARGUMENTS*) and the first-directory miss of a `.include` (§9.4, *Quirk: a later skipped URL
  include or `.load`*), are discarded by a skip in a later input as by one later in their own,
  in an included file or text parsed in place as in the input itself (§9.4, *Quirk: the miss and
  the skip in different units*):
  `.priority(x) 3⏎a = 1` followed by that input gives `{ a: int 1 @3, b: int 2 }`, also with an
  input in between and with the input given as a file; the first-directory miss of
  `.include(path=["", "…/files/v4/p1"]) "pa.inc"` followed by an input with a skipped URL include
  or that `.load` gives `pa: int 1` and `b: int 2`. Without such a skip in a later input the result
  is an error, and a skip in an earlier input covers nothing after it
  (`inputs_args_rejected_then_skip_in_later_input`, `inputs_args_rejected_skip_two_inputs_later`,
  `inputs_args_rejected_skip_in_later_file_input`, `inputs_first_miss_then_url_skip_in_later_input`,
  `inputs_first_miss_then_load_skip_in_later_input`,
  `inputs_args_rejected_later_input_without_skip_error`,
  `inputs_skip_in_earlier_input_covers_nothing_error`).
- A silent stop (§9.4, *Missing and unusable files*; a failing registered macro, §13.2) ends the
  input that holds the macro, with every file it was including, and nothing else: the next input
  is read, and goes on where the stopped one left off (`joins_stopped_include_next_input_read`) (`inputs_stop_then_later_inputs`,
  `inputs_stop_in_later_input`, `inputs_stop_in_included_file_then_later_input`). Objects the
  stopped input left open stay open: the next input's entries go into them, and it may close
  them; the check at the end of that later input stops before them, but still covers the
  containers it opened itself (`inputs_stop_inside_object_next_input_fills_it`,
  `inputs_stop_inside_object_next_input_unclosed_error`, `macro_registered_failure_then_later_input`).

### File variables and paths

- An input given as text leaves `FILENAME` and `CURDIR` as they are. An input given as a file sets
  them from its path, also under `no-filevars` (§12.7), and they keep those values for later
  inputs given as text. No case for the values themselves: the golden file would contain the
  checkout path.
- Where they stand in the lookup order (§7.1): an input given as a file changes the values of
  `FILENAME` and `CURDIR` where they already stand. When they are not defined, as under
  `no-filevars`, it defines them after the variables registered so far. So a registered `FILE`
  still matches `$FILENAME` first (`joins_file_input_file_variables_after_registered`, with
  `FILE` = `f`: `"fNAME"` in both inputs).
- **Quirk.** An input given as text goes on in the file of the input before it, for the
  self-inclusion check of §9.4. After an input given as the file `s.inc`, a text input that
  includes `s.inc` includes it from itself, an error
  (`joins_text_input_continues_file_input_self_include_error`; compare
  `joins_same_file_from_two_text_inputs`). Text a registered macro has parsed in place is in the
  file that holds the macro (`macro_registered_text_in_place_self_include_error`, compare
  `macro_registered_text_in_place_includes_other_file`).
- Relative include paths resolve against the working directory in every input (§9.3), not against
  the directory of an input given as a file (`inputs_include_resolves_from_working_directory`).

### How many inputs

**Quirk.** Every input counts as an open unit (§9.4, *The included unit*) for the rest of the parse.
So a parser takes at most 16 inputs, and the 17th is an error (`inputs_sixteen_inputs`,
`inputs_seventeen_inputs_error`). In the Nth input, includes may nest at most 16 − N deep: in the
15th input one level of include works, in the 16th none does (`inputs_include_depth_shared_ok`,
`inputs_include_depth_shared_error`).

**Quirk.** An included file in which the parse stopped silently (§9.4, a `.try_include` of a
missing file or a failing registered macro in it) stays open for the rest of the parse. It counts
as an open unit, so with one such file only 14 further inputs are accepted, not 15, and its
`FILENAME` and `CURDIR` stay in effect. A later include of the same file includes it from itself,
an error (`joins_stopped_include_stays_open_fourteen_inputs`,
`joins_stopped_include_stays_open_fifteen_inputs_error`,
`joins_stopped_include_again_is_self_inclusion_error`). Text that a registered macro parsed in
place does not stay open after a stop in it (`macro_registered_text_stop_does_not_stay_open`).

## 13.2 Registered macros

An application may register a macro under a name. Its handler runs where the macro stands, and
receives the macro's VALUE and ARGUMENTS; a context macro also receives the root.

### Where they are recognised

- A registered macro is recognised wherever a built-in macro is, by the same rules (§9.1, §9.2),
  in every input and in included files: `o { .seen "v" }` runs in `o`, a macro in an array is a
  string, the run of names after a section name continues after it, a name at the end of input
  is ignored, and a value on the next line is its value (`macro_registered_in_object`,
  `macro_registered_in_included_file`, `macro_registered_in_array_is_string`,
  `macro_registered_after_section_name_error`, `macro_registered_at_end_ignored`,
  `macro_registered_value_on_next_line`).
- Argument documents (§9.2) know only the built-in macros: a registered one there is an error
  (`macro_registered_not_in_argument_document_error`).
- Names match exactly and case-sensitively, `key-lowercase` notwithstanding. A name that is
  neither built in nor registered is still an error (`macro_registered_name_case_sensitive_error`,
  `macro_registered_unknown_name_error`).
- Under `disable-macro` a registered macro is an error like every macro (§12.6;
  `macro_registered_disable_macro_error`).
- A name the application registers replaces a built-in macro of that name:
  `.priority 3` then runs the application's handler and sets no priority
  (`macro_registered_overrides_builtin`).

### What the handler receives

- **The VALUE as text** (§9.2, *VALUE*): for `"…"` the bytes between the quotes, escapes not decoded;
  for `{…}` the bytes from the first that is not whitespace up to the `}`, trailing whitespace
  kept; otherwise the bytes up to the end of the value, trailing spaces kept. Variables are
  expanded (§7), with `$$` as §7.5 says. An empty value is empty text
  (`macro_registered_quoted_value`, `macro_registered_braced_value`, `macro_registered_bare_value`,
  `macro_registered_value_variables`, `macro_registered_empty_values`).
- **The ARGUMENTS as the argument document's root**, parsed as §9.2 describes. That is an object,
  or an array when the argument document is one (`.seen([1, 2]) "v"`;
  `macro_registered_array_arguments`). For an object: repeated names
  give multi-value entries, built-in macros work in it, and `key-lowercase` lowercases the names
  but not the values. `()` gives an empty object; without parentheses there is no object at all.
  No parameter table applies: the handler sees every name as written
  (`macro_registered_arguments`, `macro_registered_arguments_key_lowercase`,
  `macro_registered_empty_values`).
- **A context macro** also receives the root as built so far, with the containers still open and
  the entries they have so far, also when the macro stands in an included file
  (`macro_registered_context_is_root`, `macro_registered_context_in_included_file`). `.inherit`
  (§9.7) is the built-in context macro.
- **The root's priority.** The root carries the priority of the input that created it, the first
  (§13.1); `.priority` does not change it. §8.7 says this priority has no effect on the result,
  but it goes with a copy of the root: the copy that the test macro `.ctx` adds (below) has that
  priority, as its own entries keep theirs. With the first input at priority 3, `a = 1⏎.ctx c` →
  `ctx: { a: int 1 @3 } @3`, and `.priority 7⏎a = 1⏎.ctx c` → `a: int 1 @7`,
  `ctx: { a: int 1 @7 } @3`; with the first input at priority 0 and `.ctx c` in a later input at
  priority 5, the copy has priority 0 (`macro_registered_ctx_copy_keeps_root_priority`,
  `macro_registered_ctx_copy_priority_not_from_priority_macro`,
  `macro_registered_ctx_copy_priority_of_first_input`). The object `.seen` adds is new and has
  priority 0, and so is the copy of ARGUMENTS in it, whose root comes from an argument document
  (priority 0, §9.2).

### What the handler can do

- **Add entries** to the innermost open object. They appear where the macro stands, in order with
  the entries around it. §8 does not apply to them: they take priority 0 and join an existing key
  as a further value, whatever the input's priority and strategy and `no-implicit-arrays`
  (`macro_registered_in_object`, `macro_registered_empty_values`,
  `macro_registered_entries_skip_duplicate_rules`). Values a handler adds do not count as created
  values for saved comments (§12.5), as with built-in macros: comments before the macro attach to
  the next value the input creates (`macro_registered_added_values_take_no_comments`).
- **Have text parsed in place of the macro.** The text is read as the content of an included file
  is (§9.4, *Where the entries go* and *The check at the end of a unit*): a leading `{` takes over
  the brace of the object the entries go into, the text must close the brackets it opens, an
  entry must be complete within it, and it counts as one more open unit for the nesting limit
  (`macro_registered_text_in_place`, `macro_registered_text_braced_takes_root_brace`,
  `macro_registered_text_braced_in_object_error`, `macro_registered_text_unclosed_brace_error`,
  `macro_registered_text_array_error`, `macro_registered_text_incomplete_error`,
  `macro_registered_text_empty`, `macro_registered_text_holds_macro`,
  `macro_registered_text_counts_as_unit_ok`, `macro_registered_text_counts_as_unit_error`).
  Text that is nothing but a `{` or `[`, where §1.1 lets a bracketed root start (whitespace, or a
  comment group at the start of the text, directly before it), adds nothing and takes no brace
  over, as for an included file (§9.4, **quirk**): `a = 1⏎.emit "{"⏎b = 2` →
  `{ a: int 1, b: int 2 }`, and `o { .emit "{" }` leaves `o` closed by its own `}`; the same with
  the text `/* c */{` (`macro_registered_text_only_open_brace`,
  `macro_registered_text_only_open_bracket`, `macro_registered_text_comment_then_open_brace`). Text that
  starts with `[` and holds more is an error for `[1]` and `[]` (`macro_registered_text_array_error`).
  **Uncertain (undefined in libucl)** for other such text: libucl writes the value after the `[`
  over the most recent value, or over the root when there is none (`.emit "[1"` alone gives the
  result `int 1`), and reads the rest in the enclosing object. The project may report an error at
  the `[`, as it does for an included file (§9.4).
  Unlike an included file, it takes the priority and the duplicate strategy in effect in the input
  that holds the macro, `.priority` included, and it sets no file variables
  (`macro_registered_text_takes_input_priority`, `macro_registered_text_takes_input_strategy`;
  compare `macro_registered_text_include_differs`). Comments in it are saved as in the input
  (`macro_registered_text_comments`), and a silent stop in it stops the input
  (`macro_registered_text_stop_stops_all`). A first-directory miss, a rejected argument document
  and a skip in it count as in the input that holds the macro, in the order the parse reads them
  (§9.4, *Quirk: the miss and the skip in different units*). Under `zerocopy`, text from an
  expanded macro VALUE has an undefined result for its keys and strings and for what later
  depends on them (§12.2).

  **Quirk: text in place and a section object left open.** When text is parsed in place while
  the innermost open object is a section object (§3.4), that object stops closing on its own for
  the rest of the parse, even when the text is empty. A bracketed container that closes in it no
  longer closes it (§3.4, *Quirk*), and neither does a `}` that removes a brace taken over from it,
  by the text itself or by a later included file (§9.4). The section objects around it stay open
  with it, because they would close only after it. A section object created later, inside it,
  still closes as usual.
  - `t "{"1⏎.emit ""⏎o {}⏎c 3` → `{ t: { "{": int 1, o: {}, c: int 3 } }`, where without the
    macro `c` would be at the top level (`macro_registered_text_keeps_section_open`); the same
    with the text `b = 2`, with `o = [1]`, and for two names `a b "{" 1`, whose objects both stay
    open (`macro_registered_text_keeps_section_chain_open`).
  - A `}` meant for a braced object around it then finds the section object open, an error as in
    §3.4: `r { t "{"1⏎.emit ""⏎o {}⏎c 3⏎}` (`macro_registered_text_section_left_open_brace_error`).
  - `t "{"1⏎.emit ""⏎u "{" 2⏎o {}⏎c 3` → `u` closes with `o` and `c` goes into `t`
    (`macro_registered_text_later_section_closes`).
  - After a macro directly after a name (§9.1), a leading `{` in the text takes over the brace of
    the section object, and its `}` leaves it open: `"s".emit "{ a = 1 }"x "y{" z⏎k = [1]` →
    `{ s: { a: int 1, x: { "y{": "z", k: [int 1] } } }`, where an included file with the same
    content puts `x` at the top level; and `{n .emit "{}"}` is an error, because the root's `}`
    finds `n` open (`macro_registered_text_braced_after_name_keeps_section_open`,
    `macro_registered_text_braced_after_name_in_braced_root_error`).
  - A later included file whose `{` takes over the brace does not close it either:
    `t "{"1⏎.emit ""⏎.include "braced.inc"⏎c 3` puts `c` into `t`
    (`macro_registered_text_then_braced_file_keeps_section_open`). When the first key of such a
    file is the first name of a section path, that name's `}` closes only the name's object
    (§9.4, *Quirk*, the first name of a braced file).
  - An innermost object written with braces, or the root, is not affected, and neither is a
    handler that only adds entries (`macro_registered_text_in_braced_object_no_effect`,
    `macro_registered_entries_keep_section_closing`).
- **Fail.** A handler succeeds or fails; libucl gives it no way to report an error message. A
  failure is a silent stop (§9.4): parsing ends at the macro in every open unit of the input, the
  entries parsed so far are the result, and later inputs are still read (§13.1)
  (`macro_registered_failure_stops`, `macro_registered_failure_inside_object`,
  `macro_registered_failure_in_included_file`, `macro_registered_failure_then_later_input`). An
  error in text it had parsed in place is an error, as above.

### The test macros

The oracle registers these, each with access to the parser, under the flag `registered-macros`:

| Macro | Kind | What it does |
| --- | --- | --- |
| `.emit` | plain | has its VALUE text parsed in place, and fails if that fails or stops |
| `.seen` | plain | adds to the innermost open object the key `seen` with the object `{ data: <VALUE text as a string>, args: <a copy of the ARGUMENTS, or null> }`; fails if the innermost open container is not an object |
| `.fail` | plain | fails and does nothing else |
| `.ctx` | context | adds to the innermost open object the key `ctx` with a copy of the root it received; fails like `.seen` |

The copies that `.seen` and `.ctx` make are made the way `.inherit` copies (§9.7, #24): an entry
whose first value is an object or an array is copied as that value only, while an entry of
several scalar values keeps them all. `.seen(o = {x = 1}, o = {y = 2}) "v"` gives
`args: { o: { x: 1 } }`, and `.seen(n = 1, n = 2) "v"` keeps both values of `n`
(`macro_registered_seen_copies_first_container_value`,
`macro_registered_ctx_copies_first_container_value`, `macro_registered_seen_keeps_scalar_values`).
This is a property of the test macros, not of what a handler receives: a handler receives the
ARGUMENTS and the root complete.

**Uncertain (undefined in libucl).** Two details of these copies depend on memory contents in
libucl:

- Under `no-implicit-arrays`, the array that collects a repeated name in ARGUMENTS (§8.5) keeps
  the length of its key in `.seen`'s copy but not its bytes; the oracle machine gives NUL bytes,
  `.seen(n = 1; n = 2) "v"` → `args: { "\u0000": [int 1, int 2] }`. Copies of the root keep such
  keys, as `.inherit` does (`macro_registered_ctx_keeps_collection_key`).
- A copied string that holds a NUL byte loses the bytes after it (§9.7).

The conformance runner's test macros may copy keys and strings exactly.

Under `registered-priority-override` the oracle also registers the handler of `.seen` under the
name `priority`, after the built-in macros. The conformance runner needs the same four macros,
with this behaviour, to run these cases.

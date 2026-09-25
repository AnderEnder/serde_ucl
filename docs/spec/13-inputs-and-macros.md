# 13. Several inputs and registered macros

The sections before this one describe one document read by a parser. libucl's parser also lets an
application give it several inputs in turn, building one result (§13.1), and register macros of its
own (§13.2). This section describes both as libucl behaves.

The cases are in `tests/conformance/pending/13-inputs/`, where the runners do not read them yet; a
bare case name here means that directory. A case's `.ucl` file is its first input; a `.inputs` file
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
- Saved comments (§12.5) do not cross from one input to the next. Comments still pending at the
  end of an input attach there, as at the end of a document, to the value created most recently,
  or as comments after the root when the input created no value; comments at the start of an
  input attach to its first value (`inputs_comments_stay_in_their_input`,
  `inputs_comment_only_input_attaches_to_root`).

### The root

Only the first input sets up the root (§1.1). Later inputs add entries to an object root; they
cannot start a root of their own.

- After a braced root has closed, and after an array root, a later input with content is an error
  (`inputs_braced_root_then_entries_error`, `inputs_braced_root_then_braced_root_error`,
  `inputs_array_root_then_array_error`, `inputs_array_root_then_entries_error`).
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
  alone are fine, whitespace followed by a comment is an error
  (`inputs_empty_first_then_comment_only`, `inputs_empty_first_then_whitespace`,
  `inputs_empty_first_then_comment_error`). **Uncertain (crashes libucl):** a later input that
  starts with `{` after a zero-byte first input.

### Where one input ends and the next begins

**Quirk.** The end of an input is not a separator (§1.3).

- A value that ends at the very end of its input, or is followed there only by spaces or tabs,
  must be separated from the next input's first entry: that input must begin with a line break,
  `;`, `,` or a comment, or its first entry is an error. This holds for numbers, quoted strings and
  other scalars: `x = 1`, `x = 1 ` and `x = "s"` followed by `k = 1;` are errors, while `x = 1`
  followed by `⏎k = 1;`, `;k = 1;` or `# c⏎k = 1;` is fine (`inputs_value_at_end_then_entry_error`,
  `inputs_value_then_space_at_end_error`, `inputs_quoted_value_at_end_error`,
  `inputs_value_at_end_then_line_break`, `inputs_value_at_end_then_semicolon`,
  `inputs_value_at_end_then_comment`).
- A value followed in its own input by `;`, `,`, a line break or a comment needs nothing more, and
  neither does an object or array value or a heredoc (`inputs_value_then_semicolon_at_end`,
  `inputs_value_then_comma_at_end`, `inputs_value_then_comment_at_end`, `inputs_container_at_end`,
  `inputs_heredoc_at_end`).
- An entry cannot be split across inputs: an input that ends after a key, or after its `=`, is an
  error, whatever follows, and so is a last input that does (`inputs_key_split_across_inputs_error`,
  `inputs_entry_split_across_inputs_error`, `inputs_incomplete_entry_in_last_input_error`).

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
  (`inputs_error_in_first_input`, `inputs_error_in_later_input`).
- A silent stop (§9.4, *Missing and unusable files*; a failing registered macro, §13.2) ends the
  input that holds the macro, with every file it was including, and nothing else: the next input
  is read, and goes on where the stopped one left off (`inputs_stop_then_later_inputs`,
  `inputs_stop_in_later_input`, `inputs_stop_in_included_file_then_later_input`). Objects the
  stopped input left open stay open: the next input's entries go into them, and it may close
  them; the check at the end of that later input stops before them, but still covers the
  containers it opened itself (`inputs_stop_inside_object_next_input_fills_it`,
  `inputs_stop_inside_object_next_input_unclosed_error`, `macro_registered_failure_then_later_input`).

### File variables and paths

- An input given as text leaves `FILENAME` and `CURDIR` as they are. An input given as a file sets
  them from its path, also under `no-filevars` (§12.7), and they keep those values for later
  inputs given as text. No case: the golden file would contain the checkout path.
- Relative include paths resolve against the working directory in every input (§9.3), not against
  the directory of an input given as a file (`inputs_include_resolves_from_working_directory`).

### How many inputs

**Quirk.** Every input counts as an open unit (§9.4, *The included unit*) for the rest of the parse.
So a parser takes at most 16 inputs, and the 17th is an error (`inputs_sixteen_inputs`,
`inputs_seventeen_inputs_error`). In the Nth input, includes may nest at most 16 − N deep: in the
15th input one level of include works, in the 16th none does (`inputs_include_depth_shared_ok`,
`inputs_include_depth_shared_error`).

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
- **The ARGUMENTS as an object**, the argument document parsed as §9.2 describes: repeated names
  give multi-value entries, built-in macros work in it, and `key-lowercase` lowercases the names
  but not the values. `()` gives an empty object; without parentheses there is no object at all.
  No parameter table applies: the handler sees every name as written
  (`macro_registered_arguments`, `macro_registered_arguments_key_lowercase`,
  `macro_registered_empty_values`).
- **A context macro** also receives the root as built so far, with the containers still open and
  the entries they have so far, also when the macro stands in an included file
  (`macro_registered_context_is_root`, `macro_registered_context_in_included_file`). `.inherit`
  (§9.7) is the built-in context macro.

### What the handler can do

- **Add entries** to the innermost open object. They appear where the macro stands, in order with
  the entries around it. §8 does not apply to them: they take priority 0 and join an existing key
  as a further value, whatever the input's priority and strategy and `no-implicit-arrays`
  (`macro_registered_in_object`, `macro_registered_empty_values`,
  `macro_registered_entries_skip_duplicate_rules`).
- **Have text parsed in place of the macro.** The text is read as the content of an included file
  is (§9.4, *Where the entries go* and *The check at the end of a unit*): a leading `{` takes over
  the brace of the object the entries go into, the text must close the brackets it opens, an
  entry must be complete within it, and it counts as one more open unit for the nesting limit
  (`macro_registered_text_in_place`, `macro_registered_text_braced_takes_root_brace`,
  `macro_registered_text_braced_in_object_error`, `macro_registered_text_unclosed_brace_error`,
  `macro_registered_text_array_error`, `macro_registered_text_incomplete_error`,
  `macro_registered_text_empty`, `macro_registered_text_holds_macro`,
  `macro_registered_text_counts_as_unit_ok`, `macro_registered_text_counts_as_unit_error`).
  Unlike an included file, it takes the priority and the duplicate strategy in effect in the input
  that holds the macro, `.priority` included, and it sets no file variables
  (`macro_registered_text_takes_input_priority`, `macro_registered_text_takes_input_strategy`;
  compare `macro_registered_text_include_differs`). Comments in it are saved as in the input
  (`macro_registered_text_comments`), and a silent stop in it stops the input
  (`macro_registered_text_stop_stops_all`).
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
| `.seen` | plain | adds to the innermost open object the key `seen` with the object `{ data: <VALUE text as a string>, args: <a copy of the ARGUMENTS object, or null> }`; fails if the innermost open container is not an object |
| `.fail` | plain | fails and does nothing else |
| `.ctx` | context | adds to the innermost open object the key `ctx` with a copy of the root it received; fails like `.seen` |

Under `registered-priority-override` the oracle also registers the handler of `.seen` under the
name `priority`, after the built-in macros. The conformance runner needs the same four macros,
with this behaviour, to run these cases.

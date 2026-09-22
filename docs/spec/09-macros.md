# 9. Macros

Cases: `tests/conformance/cases/spec/09-macros/`. The files these cases include or load are in
`cases/spec/09-macros/files/`.

Project note: in the project, `.includes` is unsupported and `.load` sits behind a default-off
feature (README, *Divergences*). This section describes libucl.

## 9.1 Where macros are recognised

A macro is recognised only where a key could start, and only when that byte is `.`. That means at
the top level of an object, the root included, or inside `{…}`
(`include_inside_object`, `inherit_at_top_level`). Anywhere else the text is an ordinary value:

- inside arrays: `arr = [ .include "files/a.inc" ]` → `{ arr: [".include \"files/a.inc\""] }`
  (`macro_not_recognised_in_arrays`)
- as a value: `k = .include "files/a.inc"` → `{ k: ".include \"files/a.inc\"" }`
  (`macro_not_recognised_as_value`)

A key position includes the one after a section name (§3.4): `a .b {c = 1}` is an error, because
`.b` is an unknown macro inside the new object `a` (`macro_word_after_section_name_error`);
`"a".51"x{y"z` → `{ a: {} }`, because `.51"x{y"z` is a macro whose name runs to the end of input
(§9.2; `macro_word_after_section_name_ignored`).

With `disable-macro` set, a macro is a syntax error (§12.6).

## 9.2 Syntax

`.NAME` `(ARGUMENTS)`? VALUE

- **NAME** runs from after the `.` to the first whitespace or `(`. It must be non-empty
  (`. include` is an error), matches case-sensitively (`.Include` is an error), and must be one of
  `include`, `try_include`, `includes`, `priority`, `load`, `inherit`, or a macro the application
  registered. An unknown name is an error (`unknown_macro_error`, `macro_space_after_dot_error`,
  `macro_name_case_sensitive_error`, `cases/errors/e05_unknown_macro`,
  `cases/errors/e06_unknown_macro_with_args`).
- **Quirk: a macro at the end of input.** If the input ends inside NAME, the macro is ignored,
  whatever NAME is, and there is no error: `a = 1⏎.foo` → `{ a: int 1 }`, `.` → `{}`, and
  `.foo;` → `{}`, the `;` being part of the name (`macro_name_at_end_ignored`,
  `macro_dot_at_end_ignored`, `macro_name_with_punctuation_at_end_ignored`). The same holds for a
  known NAME followed only by whitespace and comments up to the end of input: `.priority⏎` and
  `.include # c⏎` do nothing (`macro_known_name_then_whitespace_at_end_ignored`,
  `macro_known_name_then_comment_at_end_ignored`). An unknown NAME followed by whitespace or `(`
  is an error even then: `.foo⏎` (`macro_unknown_name_then_newline_error`). After an argument
  list, a missing value is an error: `.priority(priority=2)` at the end of input
  (`macro_args_then_end_error`). With `disable-macro`, every macro is an error, at the end of input
  too (§12.6).
- **ARGUMENTS**, optional, may be preceded by whitespace: `.include (try=true) …` works
  (`include_space_before_args`). The text between `(` and the matching `)` is a UCL document read
  with the same flags; its root object holds the parameters. Nested parentheses must balance, and a
  `(` or `)` between double quotes does not count toward the balance. Parameters may be separated by `,` or `;`,
  and `()` is allowed (`include_empty_args`). Unbalanced parentheses are an error
  (`macro_unbalanced_args_error`). Variables are not expanded in arguments
  (`macro_args_no_variables`).
- Parameter names match by **prefix** (**quirk**): a written name matches a parameter when it is a
  prefix of that parameter's name, and the value's type chooses between candidates. For example
  `t=true` means `try` (`include_param_prefix_names`). Unknown parameters are ignored
  (`include_unknown_param_ignored`, `libucl/basic/13`). A parameter whose value has the wrong type
  is ignored: `priority="2"` has no effect (`include_priority_must_be_integer`).
- **VALUE** may be preceded by whitespace, line breaks and comments. So it may sit on a later
  line, which matters when the argument list is followed by a line break (**quirk**:
  `.priority(priority=4)⏎a = 1` takes `a = 1` as the value and fails, `priority_value_on_next_line_error`).
  It takes one of three forms (`include_quoted`, `include_bare`, `include_braces`):
  - `"…"`: the raw bytes between the quotes, with **no** escape processing
    (`macro_quoted_value_not_unescaped`);
  - `{…}`: the raw bytes up to the first `}`; leading whitespace inside the braces is not part of
    the value (`include_braces`, `include_braces_variables`);
  - anything else: bytes up to the next LF, CR, NUL, `,`, `;`, `#`, `]` or `}`. This may be empty,
    for example directly before `;` (`priority_args_only`).
- VALUE may be followed by whitespace, line breaks and `;`, after which the next entry may start on
  the same line (`include_then_keys`).
- Variables are expanded in the value (§7.2; `macro_value_variables`, `include_curdir`).
- A value on a following line is taken as the value even when it was meant as an entry:
  `.priority⏎a = 1` is an error, because `a = 1` is not a priority (`priority_missing_value_error`).
  A missing value at the end of input is covered by the quirk under NAME above.

## 9.3 File paths

The path is used as written. A relative path resolves against the **process's current working
directory**, not the directory of the including file. The conformance oracle runs each case from
its own directory. Use `${CURDIR}` for paths relative to the including file (`include_curdir`,
`include_nested`, `libucl/basic/9`, `libucl/basic/13`).

## 9.4 `.include`, `.try_include`, `.includes`

The file is parsed as a new input unit, and its entries go into the object where the macro
appears (`include_quoted`, `include_inside_object`, `libucl/basic/23`).

- The included file may have its own outer braces; they are transparent (`libucl/basic/23`).
- Its entries follow the duplicate rules of §8, against the keys already there and those that come
  later (`include_duplicates_append`).
- An error inside the included file fails the whole document.
- A value left empty at the end of an included file is `null` (`include_empty_value_is_null`, §1.6).
- While the file is parsed, `FILENAME` and `CURDIR` refer to it; afterwards they are restored
  (`include_nested`).
- A file that includes itself, compared by resolved path, is an error (`include_self_error`).
- At most 16 input units may be open at once, the main document included. So 15 nested includes
  are fine and 16 are an error (`include_nesting_limit_ok`, `include_nesting_limit_error`).

Parameters, all optional, matched as in §9.2:

| Parameter | Type | Default | Meaning |
| --- | --- | --- | --- |
| `try` | bool | false (`.include`) | a missing file is not an error, and nothing is included |
| `glob` | bool | false | expand `*` and `?` in the path |
| `prefix` | bool | false | nest the contents under a key (below) |
| `key` | string | none | the key for nesting; setting it implies nesting |
| `target` | string | `object` | `object` or `array`, for nesting |
| `priority` | int | 0 | the priority of the included values (§8.3) |
| `duplicate` | string | `append` | the included unit's strategy: `append`, `merge`, `rewrite`, `error` (§8.4) |

Other parameters (`sign`, `url`, `path`) concern signature checking, URLs and search paths. They
are outside the project's scope and have no cases.

**Missing files:**

- `.include` of a missing file is an error (`include_missing_error`).
- With `try=true` it is not an error; nothing is included and the rest of the document is parsed
  (`include_try_missing`, `libucl/basic/14`).
- **Quirk.** `.try_include` of a missing file stops parsing silently at that point, without an
  error. Entries after it are lost: `.try_include "files/missing.inc"⏎k = 1` → `{}`
  (`try_include_missing_stops_parsing`). A present file is included normally (`try_include_present`).
  `libucl/basic/9` ends with such a `.try_include`, so nothing is lost there.

**Globs** (`glob=true`): `*` and `?` in the path are expanded, and the matching files are included
one by one in sorted order (`include_glob`). If nothing matches, then (**quirk**) parsing stops
silently as with `.try_include`, unless `try=true` is also given (`include_glob_no_match_stops_parsing`,
`include_glob_try_no_match`). Without `glob=true`, `*` is an ordinary path character, so the
file is simply missing (`include_pattern_without_glob_error`).

**Nesting under a key.** Given `key="K"`, or `prefix=true` without a key, the contents go under
key K of the current object instead of directly into it. With `prefix=true` and no key, K is the
file's base name without a trailing `.conf` or `.ucl`: `a.inc` gives `a.inc`, `c.conf` gives `c`
(`include_prefix_auto_key`, `include_prefix_key`, `include_key_without_prefix`, `libucl/basic/9`).

| `target` | K absent | K holds an object | K holds an array | K holds anything else |
| --- | --- | --- | --- | --- |
| `object` | new object K | contents go into it | error (`include_prefix_existing_array_error`) | error (`include_prefix_existing_scalar_error`) |
| `array` | new array K = [object of contents] | **quirk:** K becomes [old value, object of contents] | a new object of contents is appended | **quirk:** K becomes [old value, object of contents] (`include_prefix_array_existing_scalar`) |

Cases: `include_prefix_existing_object`, `include_prefix_array_target`, `libucl/basic/9` (which
includes an existing object and an existing array).

**Priority and strategy:** `include_priority`, `include_priority_macro_inside`,
`include_duplicate_rewrite`, `include_duplicate_error`, `include_duplicate_merge`,
`include_merge_*`, `libucl/basic/13`, `libucl/basic/15`, `libucl/basic/16`, `libucl/basic/19`.
A `.priority` inside the included file affects only that file (`include_priority_macro_inside`).

**`.includes`** is `.include` with signature checking: when libucl is built with signature support,
the file `PATH.sig` must verify. The oracle build has none, so `.includes` behaves exactly like
`.include` (`includes_like_include`). The project rejects `.includes`.

## 9.5 `.priority`

`.priority N` sets the priority of every value that follows in the current input unit (§8.3). Values
before it keep theirs.

- N is a decimal integer, optionally in double quotes: `.priority 3`, `.priority "4"` (`priority_forms`).
- If the value is empty, the `priority` argument is used instead: `.priority(priority=4);`
  (`priority_args_only`). When both are present, the value wins: `.priority(priority=6) 5` → 5
  (`priority_forms`).
- A value that is not an integer, including trailing characters, is an error (`priority_invalid_error`).
  A value on the next line is still the value: `.priority⏎a = 1` is an error
  (`priority_missing_value_error`); with nothing left in the input, `.priority` does nothing
  (§9.2).
- Numbers are kept modulo 16 (§8.3).
- Cases: `libucl/basic/18`, `cases/spec/08-duplicates/priority_*`.

## 9.6 `.load`

`.load(key="K", …) "PATH"` reads a file's bytes into a single value under key K of the current
object.

| Parameter | Type | Default | Meaning |
| --- | --- | --- | --- |
| `key` | string | **required** | the key to create; missing is an error (`load_without_key_error`) |
| `target` | string | `string` | `string`: the bytes as a string; `int`: the leading decimal integer of the content; any other value: nothing is inserted and no error is raised (`load_unknown_target_inserts_nothing`) |
| `try` | bool | false | a missing file is not an error, and nothing is inserted (`load_try_missing`); otherwise it is an error (`load_missing_error`) |
| `trim` | bool | false | strip leading and trailing whitespace (`load_trim`) |
| `escape` | bool | false | store the content JSON-escaped: `\` becomes `\\`, `"` becomes `\"`, a line break becomes `\n` (`load_escape`) |
| `multiline` | bool | false | no effect on the value; the string counts as a heredoc for config output (§10.5) |
| `priority` | int | 0 | the value's priority (`load_priority`) |

- Without trimming, the content is kept as is, final line break included (`load_string`, `load_multiline`).
- `target="int"` gives an int from the leading decimal digits of the content: `42⏎` and
  `42abc⏎` both give `int 42` (`load_int`, `load_int_leading_digits`).
- If K already exists in the current object, it is an error (`load_existing_key_error`).
- Cases: `libucl/basic/load`.

## 9.7 `.inherit`

`.inherit "NAME"` copies entries from another object into the current one.

- NAME is a key of the **root** object. If NAME holds several values, the first is used
  (`inherit_first_of_repeated`). If NAME is missing, not an object, or not at the root, it is an
  error (`inherit_missing_error`, `inherit_non_object_error`, `inherit_top_level_only_error`).
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
  (`inherit_basic`, `inherit_existing_keys_kept`). Copies are **inherited**: a later explicit value
  for the same key replaces them, whatever the priorities (§8.3; `inherit_basic` → `b: int 3`,
  not two values; `inherit_replaced_whatever_priority`).
- Copies keep the priority they had in NAME, not the current priority: with `d` at priority 2 and
  `e { .inherit "d" }` at priority 1, `e.a` has priority 2 (`inherit_keeps_source_priority`,
  `libucl/basic/18`).
- The copy is shallow at the entry level. A later explicit `a { … }` replaces an inherited `a`
  entirely (`inherit_is_shallow`).
- **Quirk.** With `replace=true`, every entry is copied even when the key exists. The copy is then
  added as another value, not a replacement (`inherit_replace_appends`). It is not marked
  inherited, so a later explicit value is added to it rather than replacing it:
  `e { .inherit(replace=true) "d"; a = 2 }` → `e.a: ⟨int 1 | int 2⟩` (`inherit_replace_copies_not_inherited`).
- At the root, `.inherit` copies into the root object itself (`inherit_at_top_level`).
- Cases: `libucl/basic/18`.

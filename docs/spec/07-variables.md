# 7. Variable expansion

Cases: `tests/conformance/cases/spec/07-variables/`.

## 7.1 Registered variables

Each registered variable has a name and a value. Variables are ordered by when their name was
first registered; registering a name again changes its value, not its place in that order
(`reregister_keeps_position`). The order matters for unbraced references (§7.4). **Quirk.** Including
a file moves `FILENAME` and `CURDIR` to the end of the order (§9.4).

The order in the conformance oracle is: the file variables `FILENAME` and `CURDIR` (unless
`no-filevars`, §12.7), then `ABI` = `unknown`, then the `var:` entries of the case's `.flags` file
in file order. So with `FILE` registered, `"$FILENAME"` still gives the value of `FILENAME`
(`filevars_precede_registered`). When the document comes from a file, `FILENAME` and `CURDIR` hold
the file's path and directory whatever a `var:` entry for them says. For a document given as a
string, a `var:` entry for them does change their value (`filename_reregistered_for_string_input`).
The file case has no committed case, because its golden file would contain the checkout path.

Names are case-sensitive: `$abi` and `${abi}` do not match `ABI` (`names_case_sensitive`).

## 7.2 Where expansion happens

| Place | Expanded? | Cases |
| --- | --- | --- |
| double-quoted string values | yes | `braced`, `unbraced` |
| unquoted string values | yes | `in_unquoted_atoms`, `cases/additions/a06_bare_var`, `libucl/basic/3` (`${ABI}` in a URL) |
| heredoc strings | yes; with an empty NAME only when the first content line has a `$` (§6.3) | `in_heredoc`, `cases/spec/06-strings/heredoc_variables`, `cases/spec/06-strings/heredoc_empty_name_variables` |
| macro values (all three forms) | yes | `cases/spec/09-macros/include_curdir` (quoted), `cases/spec/09-macros/include_braces_variables` (braces), `cases/spec/09-macros/macro_value_variables` (bare) |
| single-quoted strings | no | `not_in_single_quotes`, `cases/spec/06-strings/sq_no_variables` |
| keys, bare or quoted | no | `not_in_keys`, `cases/spec/03-keys/quoted_no_variable_expansion` |
| macro argument lists `(…)` | only `FILENAME` = `undef` and `CURDIR` = the working directory, as for a string document, and neither with `no-filevars` (§9.2) | `cases/spec/09-macros/macro_args_no_variables`, `cases/spec/09-macros/macro_args_filename_is_undef` |
| numbers and keywords | no: expansion never changes a value's type, so it never produces a number, boolean or null (`$T` with `T` = `true` → `"true"`; `1$ABI` → `"1unknown"`) | `expanded_values_stay_strings`, `numbers_are_not_expanded` |

With `disable-macro` set, nothing is expanded anywhere (§12.6).

## 7.3 Braced references: `${NAME}`

NAME is everything between `${` and the first `}`.

- If NAME exactly equals a registered name, the reference is replaced by the value:
  `"x${ABI}y"` → `"xunknowny"` (`braced`, `cases/additions/a05_braced_var`).
- Otherwise, if a handler is installed, it is asked (§7.7).
- Otherwise the unresolved `${…}` text is kept as written: `"${unknown}"` → `"${unknown}"`;
  `"${}"` → `"${}"` (`unknown_preserved`). References inside it are expanded as usual:
  `"${$ABI}"` → `"${unknown}"` (`upstream_mix`, `libucl/basic/2`).
- A `${` with no `}` after it is kept as written, and references after it are expanded as usual:
  `"${ABI"` → `"${ABI"` (`lone_dollar`); `"${$ABI"` → `"${unknown"` (`unterminated_brace_inner_reference`).

## 7.4 Unbraced references: `$NAME`

After a `$` that is not followed by `{` or `$`, the **first registered variable, in registration
order, whose name is a prefix of the following text** is substituted. The rest of the text stays
as it is:

- `"$ABI"` → `"unknown"`, `"$ABI/x"` → `"unknown/x"` (`unbraced`, `cases/additions/a04_unbraced_var`)
- `"$ABItest"` → `"unknowntest"`, `"$ABIX"` → `"unknownX"` (`unbraced_prefix_match`, `cases/additions/v03_prefix`)
- Registration order decides between names that are prefixes of each other. With `AB` registered
  after `ABI`, `$ABI` still matches `ABI` first: `"$AB $ABI $ABIX"` → `"short unknown unknownX"`
  (`extra_variable_order_short_first`). With `ABIX` registered after `ABI`, `$ABIX` still
  matches `ABI`: `"$AB $ABI $ABIX"` → `"$AB unknown unknownX"` (`extra_variable_longer_name`).
- If no registered name matches, the `$` is kept: `"$unknown"` → `"$unknown"` (`unknown_preserved`).
- The handler (§7.7) is never asked for unbraced references (`handler_not_for_unbraced`).
- A `$` at the end of the text is kept: `"$"`, `"a$"` (`lone_dollar`); so is an unquoted `$`
  (`cases/spec/04-atoms/value_with_dollar`).

## 7.5 `$$`

**Quirk.** `$$` stands for a literal `$`, but only in a string where at least one reference was
actually replaced. In a string with no successful replacement, the whole text is kept exactly as
written, `$$` included (`dollar_escape_only_when_expanding`, `upstream_mix`, `libucl/basic/2`,
`cases/review/15_dollardollar`, `cases/review/16_dollarplain`, `cases/additions/v01_vars`):

| Input | Result | Why |
| --- | --- | --- |
| `"$$test"` | `"$$test"` | nothing replaced, text kept as written |
| `"$$"` | `"$$"` | same |
| `"$$ABI"` | `"$$ABI"` | same: the `$$` protects `ABI`, so nothing is replaced |
| `"$ABI$$ABI"` | `"unknown$ABI"` | one replacement, so `$$` → `$`, and that `ABI` is protected |
| `"$ABI$$"` | `"unknown$"` | |
| `"$ABI$$$"` | `"unknown$$"` | `$$` → `$`, then a lone `$` is kept |
| `"$ABI$$ABI$$$ABI$$$$"` | `"unknown$ABI$unknown$$"` | |

The same holds for unquoted values: `$ABI$$x` → `"unknown$x"`, `$$x` → `"$$x"`
(`dollar_escape_in_unquoted`).

## 7.6 Backslash before `$`

- In **double-quoted** strings, escapes are decoded before expansion, so `"\$ABI"` → `"unknown"`
  (`backslash_dollar`).
- **Quirk.** A double-quoted string is expanded only if it holds a `$` as written, also one
  written as `\$`. A `$` that a `\u0024` escape decodes to does not count on its own:
  `"\u0024ABI"` → `"$ABI"`, `"\u0024{ABI}"` → `"${ABI}"`, `"x\u0024ABI"` → `"x$ABI"`, and under
  `variable-handler` (§7.7) `"\u0024{H_X}"` → `"${H_X}"`. Only the string itself counts, not a
  `$` elsewhere in the document: `a = "\u0024ABI" # $` and `b = "\u0024ABI"; c = "$"` keep
  `"$ABI"` (`pending/07-variables/quoted_unicode_dollar_not_expanded`,
  `pending/07-variables/quoted_unicode_dollar_handler_not_expanded`,
  `pending/07-variables/quoted_unicode_dollar_dollar_outside_string_not_counted`). When the
  string holds a `$` as written, the decoded text is expanded as a whole, the `$` of a `\u0024`
  included: `"$ABI\u0024ABI"` → `"unknownunknown"`, `"\u0024ABI$"` → `"unknown$"`,
  `"\u0024ABI\$"` → `"unknown$"`, `"\$A\u0024ABI"` → `"$Aunknown"`
  (`quoted_unicode_dollar_with_written_dollar_expanded`). Unquoted values follow the next
  quirk; a heredoc decodes no escapes (§6.3).
- **Quirk.** In **unquoted** values, escapes are decoded first too (§4.7), so `\$` becomes `$`.
  Expansion then happens only if the value as written has at least one `$` that is not written as
  `\$`; otherwise the decoded text is kept as it is (`backslash_dollar`,
  `backslash_dollar_unquoted_mixed`):
  - every `$` written as `\$`, so nothing is expanded: `\$ABI` → `"$ABI"`, `x\$ABI` → `"x$ABI"`,
    `\$A\$ABI` → `"$A$ABI"`, `\\\$ABI` → `"\\$ABI"`;
  - some other `$`, so the decoded text is expanded as a whole: `$ABI\$ABI` → `"unknownunknown"`,
    `\$ABI$ABI` → `"unknownunknown"`, `\$ABIc$` → `"unknownc$"`, `\$ABI $` → `"unknown $"`,
    `${\$ABI` → `"${unknown"`;
  - the `$$` rule of §7.5 applies to the decoded text: `x$\$ABI` → `"x$$ABI"`;
  - a `$` after an escaped backslash is not escaped: `\\$ABI` → `"\\unknown"`;
  - every `$` as written counts, also one that a `\u` escape consumes or drops (§4.8):
    `\$ABI\u$000` → `"unknown\u0000"`, `\$ABI\u$` → `"unknownu"`
    (`backslash_dollar_counts_dollar_in_unicode_escape`);
  - whether a `$` is written as `\$` is decided on the text as written, where a `\` and the byte
    after it always go together: in `\$ABI\u\$` every `$` has its own `\`, so nothing is
    expanded, and decoding then gives `"$ABIu$"` (§4.8 drops the `\` after a short `\u`); in
    `\$ABI\u$` the second `$` has none, so `"unknownu"`
    (`backslash_dollar_unicode_escape_then_escaped_dollar`).

## 7.7 Variable handler

An application may install a handler, a callback that resolves names that are not registered.

- It is asked only for braced references whose name is not registered: `"${H_X}"` →
  `"[handled]"`; `${H_X}` unquoted → `"[handled]"` (`handler_braced_whole_string`).
- Registered variables take precedence (`registered_wins_over_handler`).
- If the handler refuses, the reference is left as it is (`handler_refuses`).

**Uncertain (undefined in libucl).** When a handler-resolved reference shares its string with
other text, before or after it, libucl's result depends on memory contents: text after the
reference is dropped, text before it can cut the result short or leave the reference unexpanded,
and bytes that were never written can appear in the value. This holds for a macro's VALUE too
(§9.2), and then what the macro does with it is undefined as well, the document's acceptance
included: the path an `.include` reads, or the text a registered macro has parsed in place
(§13.2). With the handler of the oracle, `.include(glob=true) "${H_}*/"` and
`.emit "a ${H_}"` were errors, while the same documents with `[handled]` written in place of the
reference are accepted. No case pins this, because the golden file would not be reproducible.
The project substitutes the handler's value in place, as for registered variables (README,
*Divergences decided by the project*): the string, and what a macro does with it, are those of
the text with the substitution made.

## 7.8 File variables

With file variables on (the default), `FILENAME` is the absolute path of the file being parsed and
`CURDIR` its directory. Inside an included file they refer to that file and are restored
afterwards (§9.4). For a document given as a string rather than a file, `FILENAME` is `undef` and `CURDIR` is the
process's working directory (`filename_for_string_input`). With `no-filevars`, neither is
defined, so `"$FILENAME"` stays as written (`filevars_disabled`, `cases/spec/12-flags/no_filevars`).

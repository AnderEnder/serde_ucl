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
section names. Only ASCII letters change; `É` stays `É`. Values are unaffected
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

No effect on the value tree (`save_comments_no_effect_on_values`). libucl then also collects the
comments and associates each with a nearby value, for its config output.

**Uncertain.** The rules for which value a comment attaches to, and how comments appear in output,
are not covered by any case. An implementation may collect comments with their positions and
leave attachment unspecified.

## 12.6 `disable-macro`

- **Quirk.** Any macro is a syntax error; the `.` at the start of a key is not accepted
  (`disable_macro_rejects_macros`, `disable_macro_priority_error`). libucl's documentation says
  "treat macros as comments", but that is not what the oracle does.
- Variable expansion is switched off everywhere: `"$ABI"` and `$ABI` stay as written
  (`disable_macro_no_variables`).

## 12.7 `no-filevars`

`FILENAME` and `CURDIR` are not defined (§7.8), so references to them stay as written
(`no_filevars`, `cases/spec/07-variables/filevars_disabled`). Other variables work as usual.

## 12.8 Other parser settings used by the oracle

These are parser inputs rather than flags, and have `.flags` entries too: `var:NAME=VALUE`
(§7.1), `variable-handler` (§7.7), `priority:N` and `strategy:NAME` for the main document
(§8.3, §8.4), and `string-input` (§7.8).

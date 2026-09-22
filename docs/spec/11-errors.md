# 11. Errors and limits

Cases: `tests/conformance/cases/spec/11-errors/`, and every case whose golden file is
`{"error":true}`.

## 11.1 Errors reject the whole document

A parse error rejects the whole document; no partial result is returned. (The silent stops of §9.4
are not errors: there, the part parsed so far is the result.) The conformance suite compares only
whether a document fails, not the message or position. The implementation should still report a
line and column.

**The wording of error messages is not part of this specification.** Implementations write their
own messages. What is specified is whether a document is accepted or rejected, and, only where a
section says so explicitly, the kind of error and its position. No section currently fixes an
error kind or an exact position, so an error's kind and position are the implementation's choice.

Error conditions, by section:

| Condition | Section | Examples |
| --- | --- | --- |
| unterminated object, array, string, heredoc or block comment | 1.4, 1.5, 6, 2.3 | `cases/errors/e01_unterminated_string_first`, `cases/errors/e07_unterminated_array`, `cases/errors/e08_unterminated_object`, `cases/errors/e10_unterminated_comment` |
| a closing bracket that does not match | 1.1, 1.4, 1.5 | `unexpected_close_top_error`, `lone_bracket_error` |
| a missing delimiter after a quoted value; a stray byte after a value | 1.3 | `garbage_after_value_error` (`a = "x" @`) |
| a double separator; a key without a value; an empty value; a value without a key | 1.2, 1.6 | `cases/errors/e02_invalid_escape_first` (the quoted key `"\q"` with no value) |
| an invalid byte in or directly after a bare key; an empty or single-quoted key | 3.1, 3.2 | `cases/errors/e03_lone_backslash`, `cases/errors/e04_nul_byte_first` |
| a single-quoted section name; a line break before a section's `{` | 3.4 | |
| a raw control byte in a double-quoted string; a bad `\u` escape there | 6.1 | `cases/errors/e09_short_unicode_escape` |
| an int or float out of range | 5.3 | |
| an unknown or malformed macro; bad macro arguments; macro-specific errors | 9 | `cases/errors/e05_unknown_macro`, `cases/errors/e06_unknown_macro_with_args` |
| a repeated key under the `error` strategy; under `merge`, an array followed by an object or an object followed by an array | 8.4 | `cases/spec/08-duplicates/strategy_merge_array_then_object_error`, `cases/spec/08-duplicates/strategy_merge_object_then_array_error` |
| any macro under `disable-macro` | 12.6 | |
| nesting deeper than the limits below | 11.2 | |

## 11.2 Limits

- **Nesting depth.** At most 1024 containers (objects and arrays) may be open at once, counting
  the root. 1024 nested arrays inside the root are an error (`depth_limit_error`), and so are
  1024 nested objects (`object_depth_limit_error`). One level less, 1023 arrays or 1023 objects
  inside the root, parses. That boundary was checked against the oracle but has no committed
  case, because its golden file would exceed the nesting that the conformance runner's JSON
  reader accepts.
- **Include depth.** At most 16 input units may be open at once, the main document included (§9.4).
- There is no limit on the length of keys or strings, or on the number of values. libucl offers
  such limits only as opt-in settings.

## 11.3 Encoding

libucl works on bytes. Keys and strings may contain any byte sequence, including invalid UTF-8
(`libucl/basic/22`: the key is the single byte 0xFF). **Project divergence:** keys and strings
must be valid UTF-8, and invalid sequences are an error (README, *Divergences*). This includes
strings made invalid by surrogate `\u` escapes (§6.1).

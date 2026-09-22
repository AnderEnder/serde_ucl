# 5. Numbers, multipliers and time suffixes

Cases: `tests/conformance/cases/spec/05-numbers/`.

Numbers are unquoted values (§4) that start with an ASCII digit or `-`. A quoted `"10"` is always
a string (§6). A value that does not satisfy these rules becomes a string (§4.6), except for the
range errors of §5.8.

## 5.1 Decimal numbers

The form is: an optional `-`, one or more digits, an optional fraction, an optional exponent, and
an optional suffix (§5.4).

- Fraction: `.` followed by zero or more digits. `1.` is `float 1` (`floats`, `cases/additions/a38_trailing_dot_num`).
- Exponent: `e` or `E`, then a digit or a sign, then digits. `1.e3` is `float 1000`
  (`exponents`, `exponent_forms`).
- The value is a **float** if it has a fraction or an exponent, otherwise an **int**:
  `0`, `-0` → `int 0`; `-0.0` → `float -0`; `100.000` → `float 100`; `1e3` → `float 1000`
  (`integers`, `floats`, `libucl/basic/1`).
- Leading zeros are decimal, not octal: `007` → `int 7` (`integers`, `cases/additions/a39_leading_zero`).
- Floats are read to the nearest double: `3.14159265358979323846` → `float 3.1415926535897931`
  (`floats`).

Not numbers, so strings: `.5`, `-.5`, `+1`, `--1`, `-`, `-a`, `1_000`, `1e`, `1e+`, `1ee3`,
`1..2`, `1.2.3`, `1e3.5`, `1.5e` (`not_numbers`, `malformed_become_strings`, `exponent_forms`,
`cases/additions/a16_plus_num`, `cases/additions/a37_leading_dot_value`,
`cases/additions/a40_underscore_num`).

## 5.2 Hexadecimal numbers

- `0x` or `0X` followed by one or more hex digits (either case) is an **int**: `0x10` → `16`,
  `0X1f` → `31`, `-0x10` → `-16` (`hex`, `libucl/basic/1` with `-0xdeadbeef`).
- Hex numbers have no fraction or exponent: `0x1.5` and `0xdeadbeef.1` are strings
  (`hex_malformed_become_strings`, `cases/review/07_hexdot`). So are `0x`, `0xg`, `0xreadbeef`,
  `-0x` and `0x-1` (`hex_malformed_become_strings`, `cases/review/08_hexbad`, `hex_more_malformed`).
- **Quirk.** The `x` may follow any run of decimal digits, and those digits are ignored:
  `12x34` → `int 52` (0x34); `9x1f` → `int 31`. A leading `-` negates the result: `-12x34` →
  `int -52`. `x10` and `1x` are strings (`hex_digits_before_x_ignored`, `hex_more_malformed`).
- **Quirk.** After a number with a `.` or an exponent, the hex digits after an `x` are taken as a
  decimal number: `1.5x10` → `int 0`, `1e5x10` → `int 0`, `1.x5` → `int 0`,
  `1.5x1e5` → `int 0`. The value is `int 0`, except with a binary multiplier (§5.4), which gives
  that decimal number, truncated, times the multiplier: `1.5x10kb` → `int 10240`. When the hex
  digits do not form a decimal number followed by what §5.5 allows, the value is a string:
  `1.5x1f`, `1e5x`, `1.5x10.5` (`hex_after_fraction_or_exponent`).
- There is no binary or octal syntax: `0b1010` and `0o755` are strings (`no_binary_or_octal`,
  `cases/review/12_0b`, `cases/review/13_0o`).

## 5.3 Range

- An int must fit in 64-bit signed range: `9223372036854775807` and `-9223372036854775808` are
  accepted; one beyond either end is an **error**, not a string (`int64_limits`,
  `int_overflow_error`, `int_underflow_error`, `cases/review/26_bigint`, `cases/review/27_overflow`).
  The same holds for hex: `0x7FFFFFFFFFFFFFFF` is fine, `0xFFFFFFFFFFFFFFFF` is an error (`hex_overflow_error`).
- A float that overflows is an error: `1e309` (`float_overflow_error`). So is one below the
  smallest normal double: `1e-400`, `1e-310` (`float_underflow_error`, `subnormal_error`).
  `1e308` and `2.2250738585072014e-308` are fine (`float_max`, `min_normal`).
- **Quirk.** Long numbers are strings, whatever their digits. A decimal number is a string when
  its digits, `.` and exponent together take 127 or more characters; with 126 it is still a
  number. A leading `-` and a suffix do not count. For a hex number, the digits after the `x`
  count, with the same limit (`number_length_limit`, `very_long_number_is_string`).

## 5.4 Suffixes

A suffix is letters written directly after the number, with no space. Letters are
ASCII case-insensitive, so `1KB`, `1kB`, `1Kb`, `1MS` and `1MIN` all work
(`binary_multipliers`, `time_milliseconds`, `time_minutes`, `cases/review/06_ms`,
`cases/additions/a48_ms_uppercase`, `cases/additions/a49_min_upper`).

| Suffix | Meaning | Result type |
| --- | --- | --- |
| `k`, `m`, `g` | × 10³, × 10⁶, × 10⁹ | same as the number: int stays int (`1k` → `int 1000`), float stays float (`1.5k` → `float 1500`) |
| `kb`, `mb`, `gb` | × 2¹⁰, × 2²⁰, × 2³⁰ | always int |
| `ms` | ÷ 1000 | time |
| `ks`, `gs` | × 10³, × 10⁹ seconds (**quirk**) | time |
| `s` | seconds | time |
| `min` | × 60 | time |
| `h` | × 3600 | time |
| `d` | × 86400 | time |
| `w` | × 604800 | time |
| `y` | × 31536000 (365 days) | time |

Cases: `decimal_multipliers`, `binary_multipliers`, `negative_multipliers`, `float_decimal_multiplier`,
`time_seconds`, `time_milliseconds`, `time_kilo_giga_seconds`, `time_minutes`,
`time_hour_day_week_year`, `cases/review/25_neg_time`, `cases/additions/a22_time_h`,
`cases/additions/a23_time_w`, `libucl/basic/2`.

Notes:

- `m` alone means mega, not minutes: `1m` → `int 1000000`; minutes need `min`.
- A time value is a float count of seconds, of type `time`. `1s` → `time 1`, `1.5min` → `time 90`,
  `1ms` → `time 0.001`, `-1.5h` → `time -5400`.
- **Quirk.** With `kb`, `mb` and `gb`, a float is first truncated toward zero:
  `1.5kb` → `int 1024`, `2.9mb` → `int 2097152`, `1e3kb` → `int 1024000`
  (`float_binary_multiplier_truncates`, `cases/additions/a14_float_kb`, `cases/additions/a24_int_float_exp_suffix`).
- **Quirk.** Multiplying an int can overflow, and the result wraps around in 64-bit two's
  complement without error: `9223372036854775807k` → `int -1000`,
  `9223372036854775807kb` → `int -1024`. A float that overflows through a multiplier becomes
  `float +∞`: `1e308k` (`multiplier_overflow_wraps`).
- No other suffix exists; `1t`, `1tb`, `1b`, `1B`, `1mins`, `1sec`, `1hr` are strings
  (`unknown_multipliers_are_strings`, `time_lookalikes_are_strings`, `cases/additions/a50_b_suffix`).
- **Quirk.** Hex numbers take the multipliers (`0x10k` → `int 16000`, `0x10kb` → `int 16384`), but
  time suffixes are accepted and ignored: `0x10s`, `0x10ms`, `0x10min`, `0x10h` → `int 16`
  (`hex_with_suffixes`, `cases/additions/a15_hex_suffix`).
- With `no-time` set, `s`, `min`, `h`, `d`, `w` and `y` are not recognised (§12.3).

## 5.5 What may follow a number

This is where numbers differ most from the extent rule of §4.1, and where most quirks live.

**Without a suffix**, the number may be followed by the end of input, a terminator (LF, CR, NUL,
`,`, `;`), `#`, `}` or `]`. It may also be followed by spaces or tabs and then one of these:
`10  ` → `int 10`, `10 ;` → `int 10`, `1.5 ` → `float 1.5` (`plain_number_trailing_whitespace`,
`number_then_hash`). Anything else makes the whole unquoted value a string:
`1 2`, `1-2`, `1+2`, `1/2`, `1:2`, `1=2`, `10 s`, `5 kb`, `12 value`, `1{` are strings
(`number_then_text_is_string`, `number_then_brace_is_string`, `cases/review/03_numword`,
`cases/additions/a21_space_before_suffix`,
`libucl/basic/12`, `libucl/basic/2` with `111some`).

**With a suffix**, the suffix must be followed *immediately* by the end of input, a terminator,
`#`, `}` or `]`: `30s;`, `30s,`, `30s⏎`, `30s# c`, `1kb}` inside an object, `[1kb]`, `5min#x`
(`suffix_before_separator`, `suffix_then_comment_without_space`, `suffix_before_closing_brackets`).

**Quirk.** A space or tab after a suffix makes the whole value a **string**, even when only
whitespace or a comment follows on the line. Trailing whitespace is then removed from the string
(§4.3):

- `1k ` → `"1k"`, `1kb ` → `"1kb"`, `1s ` → `"1s"`, `30min ` → `"30min"`
  (`suffix_then_whitespace_is_string`)
- `30s # comment` → `"30s"`, `30kb # comment` → `"30kb"`, `30s /* comment */` → `"30s"`
  (`suffix_then_space_comment_is_string`)
- `k { a = 1s }` → `{ k: { a: "1s" } }`, but `j { a = 1s}` → `{ j: { a: time 1 } }`
  (`suffix_in_braces_before_space`)
- `[30s, 1kb ]` → `[time 30, "1kb"]`; `[30s , 1kb]` → `["30s", int 1024]`; `[1s ]` → `["1s"]`
  (`suffix_in_arrays`)

## 5.6 Numbers before a block comment

**Quirk.** A number followed by `/*`, directly or after spaces, is not a number. The value becomes
the string that ends before the comment: `30/* c */` → `"30"`, and `[1 /* c */ 2]` →
`["1", int 2]` (`number_then_block_comment`, `cases/spec/01-structure/array_comment_inside`).
A number followed by `#` is fine (§5.5).

## 5.7 Numbers inside strings and keys

Numbers are recognised only as whole unquoted values. A key such as `123` is the string key
`"123"` (§3.1), `"1kb"` in quotes is a string, and so is a value with a number inside it:
`foo 1.50 bar` → `"foo 1.50 bar"` (`cases/additions/a02_bare_float_inside`).

## 5.8 Errors

These inputs make the whole document fail: an int or hex value outside 64-bit signed range, and a
float that overflows or falls below the smallest normal double (§5.3). Every other malformed
number is a string.

**Range errors and the text after a number.** The *number text* is an optional `-` followed either
by decimal digits with at most one `.` and at most one exponent (`e` or `E`, an optional sign,
digits), or by the hex form of §5.2. When the number text is within the length limit of §5.3 and
its value is out of range, the document fails whatever follows the number text: a suffix, spaces,
or other text. The rules of §5.5 that would otherwise make the value a string do not apply:
`99999999999999999999 x`, `1e999.5`, `1e999b`, `99999999999999999999k5`, `1e-400mins` and
`0x8000000000000000x` are **errors** (`range_error_before_trailing_space_text`,
`range_error_before_fraction`, `range_error_before_letters`, `range_error_before_suffix_text`,
`range_error_underflow_before_suffix`, `range_error_hex_before_trailing_x`). The value is a
string, with no range error, when the number text is malformed: when it goes on with a second `.`,
a second exponent, an `e` or `E` not followed by a digit or sign, or an `x` or `X` after decimal
digits not followed by a hex digit: `1e999x`, `1e999e`, `99999999999999999999X`,
`99999999999999999999e` and `1e999..` are strings (`malformed_before_range_is_string`). A large
integer with a fraction or an exponent is a float, not an error: `99999999999999999999.5` →
`float 1e+20`, `99999999999999999999E5` → `float 1.0000000000000001e+25`
(`big_integer_with_fraction_is_float`).

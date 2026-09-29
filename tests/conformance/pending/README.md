# Pending cases

Three cases for `spec-v15` question #80 (§9.4) are waiting for implementation. The conformance
runners do not read this directory; `scripts/regen-golden.sh`
regenerates its oracle goldens. Once the crate passes them, move the cases and their golden files
to `tests/conformance/cases/spec/09-macros/`. The `files/v4/p1/pa.inc` fixture is identical to the
one already there and can be dropped on the move.

- `09-macros/include_path_first_miss_then_url_try_accepts_later`: string input; a later
  `.include(try=true, url=true)` is skipped.
- `09-macros/include_path_first_miss_then_url_try_accepts_later_file_input`: the same input
  given as a file.
- `09-macros/include_path_first_miss_then_try_include_url_accepts_later`: string input; an
  intervening entry precedes the later `.try_include(url=true)`.

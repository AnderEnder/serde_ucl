# Pending cases

Six cases for question #83 (§12.5, draft for `spec-v18`) are waiting for implementation. The
conformance runners do not read this directory; `scripts/regen-golden.sh` regenerates its oracle
goldens. Once the crate passes them, move the cases and their golden files to
`tests/conformance/cases/spec/09-macros/`. The fixtures under `files/` are identical to the ones
already there and can be dropped on the move.

- `09-macros/comments_include_first_name_brace_keeps_most_recent`: a comment after the include
  goes to the value `z`, not to the object `x`.
- `09-macros/comments_include_first_name_brace_in_braced_object`: the same with the include
  inside `o { … }`.
- `09-macros/comments_include_first_name_brace_under_key`: the same with `key="k"`.
- `09-macros/comments_include_first_name_brace_under_prefix`: the same with `prefix=true`.
- `09-macros/comments_include_first_name_brace_comment_in_file`: the same for a comment in the
  file between its two `}`.
- `09-macros/comments_include_first_name_brace_after_entry`: with an entry after the section path,
  its value gets the comment.

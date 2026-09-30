# Pending cases

The conformance runners do not read this directory; `scripts/regen-golden.sh` regenerates its
oracle goldens. Once the crate passes a case, move it and its golden files to
`tests/conformance/cases/spec/09-macros/`.

Fifteen cases of the `spec-v20` draft wait for implementation in the same way: questions #86
(§9.4, first-directory miss), #87 (§9.4, macros before an included file's first key) and #88
(§9.4 and §12.5, the first name in a section object after text parsed in place). The fixtures
under `files/v4/` and `files/v20/` are identical to the ones in `cases/spec/09-macros/` and can
be dropped on the move; `files/v4` also serves as the directory that one case loads.

- `09-macros/include_path_first_miss_then_load_try_accepts_later`: a `.load(try=true)` of a
  missing file after a first-directory miss lets the document pass.
- `09-macros/include_path_first_miss_then_load_try_directory_accepts_later`: the same with a
  directory as the `.load` path.
- `09-macros/include_path_first_miss_then_load_try_accepts_later_file_input`: the first case,
  given as a file.
- `09-macros/include_path_first_miss_load_try_after_each_miss_accepts`,
  `09-macros/include_path_two_first_misses_then_load_try_accepts`,
  `09-macros/include_path_first_miss_url_then_load_try_accepts`: a skip covers the misses before
  it; a later miss needs a later skip.
- `09-macros/include_braced_file_nested_braced_then_entry`,
  `09-macros/include_braced_file_nested_braced_before_first_name`,
  `09-macros/include_braced_file_nested_close_brace_before_first_name`,
  `09-macros/include_key_nested_close_brace_before_first_name`,
  `09-macros/include_braced_file_nested_text_before_first_name`,
  `09-macros/include_braced_file_nested_path_before_first_name`: a nested file or text before
  the first key does not end the brace takeover, and its keys are not the file's first.
- `09-macros/comments_include_nested_braced_before_first_name`,
  `09-macros/comments_include_nested_text_before_first_name`: the comment after such a file goes
  to `z`.
- `09-macros/comments_include_first_name_in_section_object_after_text`: after text parsed in
  place in a section object, the first name's `}` leaves the value created most recently as it
  was.

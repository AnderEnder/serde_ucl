# Pending cases

Cases from `spec-v7` that the new core or its emitters do not pass yet. The runner does not read
this directory. When the core passes a case, move its files to `cases/spec/<same directory>/`.

- `06-strings/sq_line_continuation_lone_cr`: §6.2, a backslash before a lone CR (#53).
- `10-output/no_implicit_arrays_collection_key_bare`: §10.1, the key of a `no-implicit-arrays`
  collection array (#51).
- `10-output/multi_value_merge_quirk_layout_nan_inf`: §10.7, layout under the merge quirk (#50).

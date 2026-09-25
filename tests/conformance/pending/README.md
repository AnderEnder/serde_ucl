# Pending cases

Cases from `spec-v8` that the new core or its emitters do not pass yet. The runners do not read
this directory. When the core passes a case, move its files unchanged to
`cases/spec/<same directory>/`, then delete this directory once it is empty.

- `06-strings/heredoc_empty_name_end_rule`: §6.3, where a heredoc with an empty NAME ends (#55).
- `06-strings/heredoc_empty_name_variables`: §6.3, variable expansion in a heredoc with an empty
  NAME (#55).
- `06-strings/heredoc_empty_name_eof_error`: §6.3, a heredoc with an empty NAME that reaches the
  end of input (#55).
- `06-strings/heredoc_repeated_letter_name`: §6.3, a NAME made of one repeated letter (found
  while answering #55).

# Conformance corpus

The expected results in this directory come from the C reference implementation, libucl.
None of them is written by hand.

## Layout

- `libucl/basic/` holds libucl's own `tests/basic` corpus, copied verbatim from
  <https://github.com/vstakhov/libucl> at commit `24c8b399062ae4691168c243e3b7345ef7f31956`
  (2026-09-20). libucl is BSD-2-Clause; see `LICENSE-libucl` in the repository root. Every `*.in`
  file is a case. The `*.inc` files and `include_dir/` are included by the cases. The `*.res` files
  are libucl's config-emitter output; they are used by tier-2 conformance (PLAN.md P5.1).
- `cases/review/` holds the 30 cases behind REVIEW.md.
- `cases/additions/` holds the 53 cases behind PLAN.md §5.
- `cases/errors/` holds inputs libucl rejects.
- `cases/spec/NN-topic/` holds the cases cited by the behaviour spec in `docs/spec/`, one directory
  per spec section. `cases/spec/09-macros/files/` and `cases/spec/08-duplicates/files/` hold files
  that those cases include or load (`*.inc`); they are not cases themselves. Cases in `cases/spec/10-output/` also have libucl's own output
  in every text format: `<case>.config.golden`, `<case>.json.golden`, `<case>.json-compact.golden`
  and `<case>.yaml.golden` (exact bytes of `ucl_object_emit` for the parsed value).
- `cases/migrated/` holds inputs taken from the crate's older test suites. Their expected results
  are now libucl's, not the old hand-written assertions.
- `<case>.golden.json` next to each case is libucl's typed dump of that case, produced by
  `tools/ucl-dump`.
- `<case>.flags` (optional) lists parser settings, one per line:
  `key-lowercase`, `zerocopy`, `no-time`, `no-implicit-arrays`, `save-comments`, `disable-macro`,
  `no-filevars` (the parser flags); `dump-comments` (like `save-comments`, and the golden file also
  records the comments attached to each value: `"c"` for comments written before it, `"ca"` for
  comments written after it, see `tools/ucl-dump/ucl_dump.c`); `var:NAME=VALUE` (register an extra variable after `ABI`, in
  file order); `variable-handler` (install the test handler: any braced name starting with `H_`
  resolves to `[handled]`, every other name is refused); `priority:N` and `strategy:NAME` (the
  priority and duplicate strategy of the input chunk; strategies are `append`, `merge`, `rewrite`,
  `error`); `string-input` (do not set the file variables from the case path, as for a document
  given as a string: libucl then defines `FILENAME` as `undef`). See `tools/ucl-dump/ucl_dump.c`
  for the exact oracle options.
- Every case is parsed with the variable `ABI` registered as `unknown`, and with the file variables
  `FILENAME` and `CURDIR` set from the case path (unless `no-filevars`). The oracle runs each case
  from the case's own directory, so relative include paths resolve against that directory.
- `xfail.txt` lists the cases known to fail, with a reason.

## Regenerating

    scripts/regen-golden.sh              # clones and builds libucl at the pinned commit under target/
    LIBUCL_DIR=/path/to/libucl scripts/regen-golden.sh   # reuse an existing checkout at that commit

`scripts/create-cases.sh` recreates `cases/review/` and `cases/additions/` byte for byte.

## Running

    cargo test --test conformance
    UCL_CONFORMANCE_REPORT=1 cargo test --test conformance -- --nocapture   # per-case detail

The run fails when a case fails without an entry in `xfail.txt`, and also when a listed case passes.
So the list can only shrink; remove an entry as soon as its case passes.

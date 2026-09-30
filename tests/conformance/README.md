# Conformance corpus

The expected results in this directory come from the C reference implementation, libucl.
None of them is written by hand.

## Layout

- `libucl/basic/` holds libucl's own `tests/basic` corpus, copied verbatim from
  <https://github.com/vstakhov/libucl> at commit `24c8b399062ae4691168c243e3b7345ef7f31956`
  (2026-09-20). libucl is BSD-2-Clause; see `libucl/LICENSE`. Every `*.in`
  file is a case. The `*.inc` files and `include_dir/` are included by the cases. The `*.res` files
  are libucl's config output, produced by the two passes of spec §10.9; the emitter runner compares
  them (see *Running*).
- `cases/review/` holds 30 cases from the initial compatibility review.
- `cases/additions/` holds 53 cases added with the first implementation plan.
- `cases/errors/` holds inputs libucl rejects.
- `cases/spec/NN-topic/` holds the cases cited by the behaviour spec in `docs/spec/`, one directory
  per spec section. `cases/spec/09-macros/files/` and `cases/spec/08-duplicates/files/` hold files
  that those cases include or load (`*.inc` and others, among them an empty file, a directory and a
  symbolic link under `files/v4/`, and file names with `[` and `]` under `files/v5/g/`); they are
  not cases themselves (`files/v6/` and `files/v18/` among them, with fixtures for spec-v6 and
  the draft for spec-v18). Some `09-macros` cases include other cases, or themselves, by name, and
  some `10-output` and `08-duplicates` cases use `../09-macros/files/`.
  `cases/spec/13-inputs/files/` holds the further inputs of the §13 cases
  (`<case>.<n>.inc`, `n01.inc`…`n16.inc`) and the files they include.
- `pending/`, when present, holds cases from a spec draft or released version that the crate does
  not pass yet, with their golden files and a README; the runners do not read it, but
  `scripts/regen-golden.sh` regenerates its golden files like the others. Each case moves to
  `cases/spec/` when the crate passes it. Five cases of the draft for question #83 (§12.5) are
  pending under `pending/09-macros/` (see `pending/README.md`).
- `platform-dependent.txt` and `platform/<platform>/` hold the golden files that depend on the
  platform's C library; see *Golden files per platform* below.
- `cases/migrated/` holds inputs taken from the crate's older test suites. Their expected results
  are now libucl's, not the old hand-written assertions.
- `<case>.golden.json` next to each case is libucl's typed dump of that case, produced by
  `tools/ucl-dump`.
- Every case whose typed dump is not an error also has libucl's own output for the parsed value,
  byte for byte, in each text format of spec §10: `<case>.config.golden`, `<case>.json.golden`,
  `<case>.json-compact.golden` and `<case>.yaml.golden`. Cases whose `.flags` include
  `save-comments` or `dump-comments` also have `<case>.config-comments.golden`: the config output
  with the comments the parser saved (spec §10.10). An error case has none of these files. The
  bytes are exact, including NUL bytes and bytes that are not UTF-8, and there is no final line
  break unless libucl writes one. The emitter runner compares them byte for byte (see *Running*).
- `<case>.flags` (optional) lists parser settings, one per line:
  `key-lowercase`, `zerocopy`, `no-time`, `no-implicit-arrays`, `save-comments`, `disable-macro`,
  `no-filevars` (the parser flags); `dump-comments` (like `save-comments`, and the golden file also
  records the comments attached to each value: `"c"` for comments written before it, `"ca"` for
  comments written after it, see `tools/ucl-dump/ucl_dump.c`); `var:NAME=VALUE` (register an extra variable after `ABI`, in
  file order); `variable-handler` (install the test handler: any braced name starting with `H_`
  resolves to `[handled]`, every other name is refused); `priority:N` and `strategy:NAME` (the
  priority and duplicate strategy of the input chunk; strategies are `append`, `merge`, `rewrite`,
  `error`); `string-input` (do not set the file variables from the case path, as for a document
  given as a string: libucl then defines `FILENAME` as `undef`); `registered-macros` (register
  the test macros `.emit`, `.seen`, `.fail` and `.ctx` of spec §13.2) and
  `registered-priority-override` (also register the handler of `.seen` under the name
  `priority`, replacing the built-in macro). See `tools/ucl-dump/ucl_dump.c` for the exact oracle
  options.
- `<case>.inputs` (optional) lists further inputs that the same parser reads after the case's own
  file (spec §13.1), in order, one per line: `MODE PRIORITY STRATEGY PATH`, with `#` comments.
  MODE is `chunk` (the file's bytes are added as a document given as text; the file variables
  stay as they are) or `file` (the file is added by its path, as a file, which sets `FILENAME` and
  `CURDIR` from it); PRIORITY is a decimal number; STRATEGY is `append`, `merge`, `rewrite` or
  `error`; PATH is relative to the case's directory. The case's own `.flags` apply to the parser,
  and `priority:N` and `strategy:NAME` to the case's own file only. The golden files record the
  result after the last input; it is an error if any input reported one. Further inputs are named
  so that they are not cases themselves, under `files/` (for example `files/<case>.<n>.inc`).
- Every case is parsed with the variable `ABI` registered as `unknown`, and with the file variables
  `FILENAME` and `CURDIR` set from the case path (unless `no-filevars`). The oracle runs each case
  from the case's own directory, so relative include paths resolve against that directory.
- Known failures, one list per runner, each entry `<case-id> <reason>` with an optional `# note`:
  - `xfail-new.txt`: the crate's parser (`libucl_conformance_new_core`). Reasons are a spec
    version (`spec-vN`) for rules that version added, `divergence:<topic>` for a place where the
    project decided to differ from libucl (spec README, *Divergences decided by the project*), and
    `non-utf8`;
  - `xfail-emit.txt`: the output of the crate's emitters (`libucl_conformance_emitters`). It may
    hold only cases that the crate does not parse and that `xfail-new.txt` also lists, with
    reason `divergence:<topic>`; an entry covers every output of its case. Output that the crate
    writes differently for a case it parses cannot be listed, so such a case fails the run.

## Regenerating

    scripts/regen-golden.sh              # clones and builds libucl at the pinned commit under target/
    LIBUCL_DIR=/path/to/libucl scripts/regen-golden.sh   # reuse an existing checkout at that commit
    LIBUCL_COMMIT=<sha> scripts/regen-golden.sh          # build another libucl commit instead

`scripts/ci.sh pin <sha>` shows what moving the pin to another commit would change: it
regenerates every golden file at that commit, runs the conformance tests against them, and writes
a summary and a patch to `target/pin-move/`. The workflow *Golden files at another libucl commit*
(`.github/workflows/pin-move.yml`) runs it on demand and publishes the result; it commits nothing.

`scripts/create-cases.sh` recreates `cases/review/` and `cases/additions/` byte for byte.

### Golden files per platform

libucl leaves some work to the C library: glob matching and sorting (spec §9.4, *Globs*), and the
base name of a path. A few cases therefore give other results with another C library. The golden
files next to every case come from the oracle platform, macOS, and are what the crate must
produce on every platform; the conformance runners read only those.

For the drift check on other platforms, `platform-dependent.txt` lists the cases whose results
depend on the C library, one per line as `<case id> <reason>`, where the case id is the case's
path below `tests/conformance/` without its extension. On a platform other than macOS (the
platform name is `uname -s` in lower case, for example `linux`), `scripts/regen-golden.sh` writes
the golden files of those cases to `platform/<platform>/<case id>.*` instead of next to the case,
and those files are committed. Every other case writes next to itself on every platform. So
`scripts/ci.sh golden` compares, on each platform, with that platform's own expectation for the
listed cases and with the one expectation for all others; the workflow *Golden files*
(`.github/workflows/golden.yml`) runs it nightly on Linux and on macOS.

When the drift check on some platform fails for a case whose result depends on the C library,
add the case to `platform-dependent.txt` and commit the files that the regeneration wrote below
`platform/<platform>/`. The script rejects a listed id that is not a case.

## Running

    cargo test --test conformance
    UCL_CONFORMANCE_REPORT=1 cargo test --test conformance -- --nocapture   # per-case detail

The test target has three tests, each with its own known-failure list:

- `libucl_conformance_new_core` parses every case with the crate's parser and compares the result
  with `<case>.golden.json`. A case with a `.inputs` file is parsed as several inputs into one
  parser (`Parser::inputs`); the flags `registered-macros` and `registered-priority-override`
  register handlers equivalent to the oracle's test macros (spec §13.2).
- `libucl_conformance_emitters` takes every case that has output golden files, parses it with the
  crate's parser, writes the result in each format and compares the bytes with `<case>.config.golden`,
  `<case>.json.golden`, `<case>.json-compact.golden`, `<case>.yaml.golden` and, where present,
  `<case>.config-comments.golden`. For upstream cases it also reproduces the two passes of spec
  §10.9 and compares the result with the `.res` file. A case that parses without error but has no
  output golden files fails the run. The report line gives the number of matching outputs per
  format.
- `libucl_conformance_readback` takes every case that the crate parses, writes the result in each
  format with the crate's emitters, and parses the output again with nothing registered (no
  variables, no handler, `no-filevars`). The value read back must equal the value written, apart
  from the losses spec §10.8 lists; an output that §10.8 says is unreadable (keys written bare that
  cannot be read bare) may read back as anything. Differences that §10.8 does not list would be
  held in `READBACK_PENDING` in `tests/conformance.rs`, per case and format, each with the
  question in `docs/clean-room/QUESTIONS.md` that asks about it; the list is empty since
  spec-v10 answered #57 and #58. The report line gives, per format, the outputs that read back as
  the same value, those let through as unreadable, those rejected for a float, and the others.

A run fails when a case fails without an entry in its list, and also when a listed case passes.
So the lists can only shrink; remove an entry as soon as its case passes.

## serde corpus

`tests/serde_corpus/` holds values written by the crate's serde serialization, checked by
`cargo test --test serde_roundtrip`. Each value has one file per format: `<name>.ucl` (config),
`<name>.json`, `<name>.compact.json` and `<name>.yaml` (`dollar_strings` has the config file
only). Next to each file, `<file>.golden.json` is libucl's typed dump of reading that file back,
in the format of the conformance golden files.

The JSON files are valid JSON (`docs/clean-room/WORKLIST.md`, C4 decision 2): a time is written as
its number of seconds, so their dumps read it as a float, and NaN and infinite floats and times
have no JSON form. For `floats`, `times`, `typed_sample` and `typed_enums`, the `.json` and
`.compact.json` files therefore hold the value without those: the finite floats and times, a
finite `weight` in `typed_sample` and a finite `Circle` in `typed_enums`. The subnormal times of
`times`, written with `ms` (spec §10.8), are in its config and YAML files only.

- Without the oracle binary (`target/libucl-oracle/ucl-dump`), the test `corpus_reads_back` checks
  that the serializer still writes the same bytes, and that both the crate's reading and the
  stored libucl reading give the value that was serialized.
- With the binary present, it also checks that the stored readings are current, and the tests
  that give generated values to libucl (`oracle_reads_generated_values_back`,
  `oracle_reads_generated_typed_values_back`) run; without it they skip.
- `UCL_SERDE_REGEN=1 cargo test --test serde_roundtrip` regenerates the corpus files and their
  readings; it needs the binary.

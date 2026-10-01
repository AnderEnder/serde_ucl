# Differential fuzzer

`ucl-differential` parses generated inputs with the crate and with libucl, and reports every
input on which the two disagree. libucl runs as a black box: the oracle binary
`target/libucl-oracle/ucl-dump` that `scripts/regen-golden.sh` builds, one process per input.
The comparison is the conformance suite's (`tests/common/oracle.rs`, shared with
`tests/conformance.rs`): the crate's value is dumped in the oracle's schema, with the saved
comments under `dump-comments`, the priorities spec §8.7 calls unobservable are removed, and
floats are compared by their bits.

The fuzzer is a package of its own, outside `cargo test` and outside the crate's workspace.
`scripts/ci.sh` checks that it is formatted, passes clippy, and passes its unit tests; running it
needs the oracle, and is a separate step.

## Running

The oracle is built once, with git, CMake and a C compiler, on macOS or Linux:

    scripts/regen-golden.sh

Run the fuzzer on macOS, the oracle platform. libucl leaves glob matching and the base name of a
path to the C library, and the crate follows macOS on every platform (spec §9.4, *Globs*;
`tests/conformance/README.md`, *Golden files per platform*), so on Linux the fuzzer also reports
the differences of glibc, for example in `[^…]` patterns.

Then, from the repository root:

    cargo run --release --manifest-path fuzz/Cargo.toml --target-dir target/fuzz -- --seconds 600

or `scripts/ci.sh fuzz 600`, which also builds the oracle if it is missing. The manually
triggered workflow *Differential fuzzing* (`.github/workflows/fuzz.yml`) runs the latter and
uploads the findings.

Options (`--help` lists them):

| Option | Default | |
| --- | --- | --- |
| `--seconds N` | 60 | stop after N seconds; 0 for no limit |
| `--runs N` | none | stop after N inputs |
| `--seed N` | from the clock | the generator's seed; the run prints it, and the same seed gives the same inputs on each worker |
| `--jobs N` | available parallelism | worker threads, each with its own oracle processes |
| `--max-len N` | 4096 | the longest input, in bytes |
| `--timeout N` | 10 | seconds the oracle may take for one input |
| `--oracle PATH` | `target/libucl-oracle/ucl-dump` | |
| `--out DIR` | `target/fuzz-differential` | findings, work files and `summary.txt` |

It prints progress every 10 seconds and a summary at the end, and exits with status 1 if it
saved a finding.

## What it generates

- **Mutations of the conformance cases.** Every case of `tests/conformance/` is a seed, with its
  `.flags`. A mutation inserts, replaces or deletes UCL tokens, bytes and whitespace, copies
  pieces of the seed or of another seed, swaps in a line of another seed, or cuts the input
  short (`src/generate.rs`).
- **Documents from a small grammar**, a fifth of the inputs: entries with a few repeated keys,
  nested objects and arrays, every value form, comments, section names, `.priority`, `.inherit`
  and `.try_include`.
- A quarter of the inputs get one more parser flag (`key-lowercase`, `no-time`,
  `no-implicit-arrays`, `disable-macro`, `no-filevars`, `zerocopy`, a strategy, a priority,
  the test macros of spec §13.2, `dump-comments` or the test variable handler).

Both sides parse the input as a document given as text (the flag `string-input`), with the
seed's directory as the oracle's working directory and as the crate's base directory, so the
seeds' relative include paths find their files (spec §9.3). A seed's other flags are kept:
under `dump-comments` the saved comments are compared too (§12.5), and under
`variable-handler` the handler's results (§7.7).

The oracle runs with every flag but `zerocopy`, which the crate still parses with. Spec §12.2
(spec-v19) gives a document with `zerocopy` the result the spec gives it without the flag, all
other settings unchanged, while libucl's own result with it is undefined for the text of an
expanded `.emit` and what depends on it, and for documents that include a file with entries. So
the crate's result with `zerocopy` is compared with the oracle's without it, and the rules below
apply as for that run; a report then names the flags the oracle ran with.

## Verdicts

- **agree**: the same value, or both reject the input.
- **skipped**: not compared, or the difference is one the spec allows. The oracle crashed
  (undefined behaviour in the spec), timed out, or wrote a dump nested deeper than `serde_json`
  reads; the crate rejected the input for a project divergence (spec README, *Divergences
  decided by the project*): non-UTF-8 text, an unsupported feature (signatures), the limit on
  nested argument documents, or the nesting limit; or the results differ only in behaviour the
  spec marks **Uncertain** (`src/uncertain.rs`): a handler result that shares its string with
  other text (§7.7), a float outside the 64-bit range with `kb`, `mb` or `gb` (§5.4), the byte
  saved after a block comment that ends its unit and the comments of a value §8 replaced
  (§12.5), which may come before a later value's own comments, also with the same text, when
  the crate read them before that value's own (a value with no comments of its own is not
  ordered), the bytes after a NUL in a string, at the same length, when `.inherit` or a test
  macro that copies stands in some unit of the document: the input, a file it reads, or text
  parsed in place (§9.7, §13.2; any such string of that document is excused, copied or not), the
  key of a collection in `.seen`'s copy of ARGUMENTS (§13.2), and an included file or text in
  place that starts with `[` (§9.4, §13.2), which the crate rejects there. Four uncertain
  rules that the dumps cannot show are reported by the crate's parse
  (`Parser::uncertain_reached`, hidden from the crate's documentation): a macro after a name
  followed only by comments when the value created most recently is not an object (§9.1), a
  container of an ended unit at the check at the end of a later included file, a `}` in an
  included file that closes an array element, and a `/` at the end of a glob pattern that leaves
  out a symbolic link to a regular file, or at the end of a plain include path after the name of
  a file (§9.4, which depends on the operating system); any difference of such a parse is
  skipped.
  Nothing the spec specifies is skipped.
- **differ**: `crate-accepts` (libucl rejects the input), `crate-rejects` (libucl accepts it),
  `values-differ`, or `crate-panics`.

A difference is reduced to a smaller input with the same verdict, by deleting pieces and pairs
of brackets, and saved under `findings/<verdict>-<hash>/`:

- `input.ucl`, the reduced input, and `original.ucl`, the generated one;
- `flags` and `dir`, the flags and the working directory;
- `report.txt`: the difference, both results, and a command that runs the oracle on the input.

Only the first three differences of each rough class (the crate's error kind, the two value
types, or the seed's directory) are reduced and saved; the summary counts all of them.

If the fuzzer itself stops abnormally, `work/<n>/input.ucl` holds each worker's last input.

## Checking findings

    target/fuzz/release/ucl-differential --replay target/fuzz-differential/findings/*
    target/fuzz/release/ucl-differential --check some.ucl --flag no-time --dir tests/conformance

`--replay` runs saved findings again and prints their verdicts, for example after a fix; it
exits with status 1 while any of them still differs. `--check` runs one file through both and
prints both results.

A difference is a question for the spec, or a bug where the spec is clear: it goes to
`docs/clean-room/QUESTIONS.md` or gets fixed, with a test (`docs/clean-room/PROTOCOL.md`).

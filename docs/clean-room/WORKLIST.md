# Clean-Room Work List

Goals only, with no implementation guidance. Behaviour is defined by the released `docs/spec/` and
the conformance suite. Work items run in order. C0 is done by the spec team; C1–C6 by the
implementation team, under `PROTOCOL.md`.

## C0 — Behaviour spec (`docs/spec/`), by the spec team

The spec team reads libucl's source and public documentation and runs the oracle, then writes a
behaviour-only specification (see PROTOCOL.md, "What the spec may contain"). Every rule cites the
conformance cases that demonstrate it; where no case covers a rule, the team adds one and
regenerates golden files with `scripts/regen-golden.sh`.

Sections, organised by format feature:
01 document structure and separators, 02 whitespace and comments, 03 keys and named sections,
04 unquoted values, 05 numbers and suffixes, 06 strings (quoted, single-quoted, heredoc),
07 variables, 08 duplicate keys, priorities and strategies, 09 macros, 10 output formats,
11 errors and limits, 12 parser flags.

Done when every case in `tests/conformance/` is explained by some section, the reviewer finds no
prohibited content, and the result is tagged `spec-v1`. Later versions answer implementers'
questions.

## C1 — Duplicate-key insertion

Implement `UclObject::insert_with_strategy` and `UclObject::insert_slot_with_strategy`
(`src/value.rs`, currently `todo!()`) per spec §08. Done when `cargo test` is green again and the
conformance xfail lists have not grown.

## C2 — New parser core

A new byte-oriented parser in `src/parse/`, next to the existing parser: input `&[u8]`, UTF-8
validated where strings and keys are materialised (non-UTF-8 content is an error). It produces the
`src/value.rs` model and typed errors with positions, and implements spec §01–§08, §11 and §12,
with macros recognised but rejected until C3. The conformance runner also runs every case through
the new core, with its own `tests/conformance/xfail-new.txt`, which may only shrink. Done when the
only entries left in that file need macros, are the non-UTF-8 case, or are divergences justified
in the spec.

## C3 — Macros

Spec §09: `.include` (and its parameters), `.try_include`, `.priority`, `.inherit`, `.load`
(behind a default-off feature). `.includes` returns an "unsupported" error. Unknown macros are
errors. There is a loader abstraction with a filesystem loader (feature `fs`, default on) and an
in-memory loader. Done when no `xfail-new.txt` entry is left that needs macros.

Split into two work items: C3a `.priority` and `.inherit`; C3b the loader, `.include`,
`.try_include`, their parameters, globs and `.load`.

### Project decisions for C3 (settle spec-v4 README, *Known gaps*)

1. **Signatures (§9.4).** The crate never verifies signatures. `.includes`, and `sign=true` on
   any include macro, are rejected with the "unsupported" error; `sign=false` is accepted. The
   cases `includes_like_include` and `include_sign_param_no_effect` are justified divergences.
2. **URLs (§9.4).** The crate never fetches URLs. What `url=true` with `://` in the path does
   instead follows §9.4 in every mode, including `try=true` and `.try_include`; without
   `url=true` such a path is an ordinary path.
3. **Search paths (§9.4).** `path` is implemented as §9.4 describes, quirks included.
4. **Silent stop (§9.4, *Missing and unusable files*).** The API reports it as an error of its own
   kind, distinct from a syntax error, from which the partial tree built so far can be retrieved.
   The serde entry points report it as an error. Message wording is the implementer's choice.
5. **`no-filevars` when parsing a file (§12.7).** Parsing a file by path defines `FILENAME` and
   `CURDIR` from that path whatever the flag says. Parsing bytes honours the flag.
6. **Base directory.** Where relative include paths resolve and what `CURDIR` is for a document
   given as bytes are parser options, not the process working directory.

Known-failure reasons: a justified divergence is listed with reason `divergence:<topic>` and
stays listed; C3 is done when no entry's reason is `macro`.

## C4 — Output formats and serde serialization

1. Emitters for JSON, compact JSON, the UCL config format and YAML, per spec §10. In its
   default mode, config output is byte-identical to the upstream `.res` files.
2. serde serialization: `to_string` (config), `to_json_string`, `to_json_string_compact`,
   `to_yaml_string`, `to_writer`, `to_value`, `from_value`; `Serialize`/`Deserialize` for
   `UclValue`; the `time` module serializes `Duration`. Serde output must round-trip exactly,
   floats included, even where the default emitter mode does not, and libucl must read it back to
   the same value (checked through the oracle).

Split into two work items: C4a the emitters (item 1) and the runner's emitter comparison; C4b
serde serialization (item 2).

### Project decisions for C4 (settle spec-v6 README, *Known gaps*)

1. **Saved comments in output (§10.10).** The crate writes saved comments in config output only
   when the caller asks for it, through an emitter option; the default config output writes none.
   With the option on, output is byte-identical to the `config-comments` golden files.
2. **serde JSON output is valid JSON (RFC 8259).** `to_json_string` and
   `to_json_string_compact` never write UCL-only forms: a time is written as its number of
   seconds, and a NaN or infinite float or time is a serialization error. A Rust value still
   round-trips where its deserializer accepts a number of seconds (for example `Duration` through
   the `time` module). The config and YAML formats keep the C4b forms.

## C5 — Cut-over

Switch the public API (`from_str`, `from_slice`, `from_reader`, `from_file`, the parser builder)
to the new core. Delete `src/lexer.rs` and `src/parser.rs` (including the streaming lexer and the
plugin/hook system), and the tests, examples and benches that exercise constructs libucl doesn't
support. Rewrite the remaining examples on the new API. Done when the old conformance xfail list is
retired and `xfail-new.txt` holds only justified divergences.

Split into two work items after the first C5 session stopped (see LOG.md): C5a moves the public
API, the tests, the examples and the benches to the new core, finishing the stopped session's
uncommitted work; C5b deletes the old implementation, retires the old runner and `xfail.txt`, and
writes `CHANGELOG.md`.

### Project decisions for C5

1. **No file access from text input by default** (decided by the project owner, 2026-09-25).
   `from_str`, `from_slice`, `from_reader`, and any other entry point that parses a document
   given without a path, use a loader that holds no files by default. An `.include`,
   `.try_include` or `.load` in such input therefore finds no file and behaves as §9.4 and §9.6
   describe for a missing file. `from_file` and a parser built with the filesystem loader
   (feature `fs`) read files, resolving relative paths against the file's directory or a
   configured base directory. A caller can opt text input into filesystem access through the
   parser builder. The CHANGELOG states this prominently.

## C6 — Docs, packaging, CI

Make every Cargo feature real (referenced by `#[cfg]`) or remove it. Correct README and crate docs.
Add `docs/COMPATIBILITY.md` (divergences and quirks, each with its case). Set Cargo metadata
(`repository`, `authors`, `rust-version`) and add license files for `MIT OR Apache-2.0`. CI: clippy
with `-D warnings`, `cargo fmt --check`, no wall-clock assertions in tests, and a nightly job that
regenerates golden files and fails on drift.

Split into two work items: C6a brings the core up to `spec-v8` (the cases in
`tests/conformance/pending/`), makes clippy clean with `-D warnings`, makes every Cargo feature
real or removes it, and removes super-linear slowdowns on deeply nested input; C6b does the
README, crate docs, `CLAUDE.md`, Cargo metadata, license files and CI.

### Project decisions for C6

1. Crate and library names stay `ucl-rust-lexer` / `ucl_lexer`. `repository` is
   `https://github.com/AnderEnder/ucl-rust-lexer`; `authors` is `Andrii Radyk`.
2. CI runs the same commands a developer runs locally, through one script the workflows call.
   Tests stay independent so they run in parallel. The nightly drift job runs
   `scripts/regen-golden.sh` exactly as a developer would and fails if any golden file changes.

## C7 — Robustness follow-ups

1. No input the parser accepts may overflow the stack in serde deserialization or serialization.
   On a 2 MiB thread in a debug build, `from_str::<UclValue>`, `to_value`, `to_string` and the
   other serde entry points either handle every depth the parser accepts (spec §11.2) or return
   an error at a documented depth limit. A stack overflow is never acceptable.
2. The test `load_needs_its_feature` runs in `scripts/ci.sh`: the crate's dev-dependency on itself
   currently turns `load` on for every test build.
3. The version becomes `0.2.0`, since this release breaks the 0.1.0 API (pre-1.0 semver).

## C8 — Remaining API and tooling items

Items from the original review that are still open after C7. C8a needs no new spec; C8b waits for
the spec release that specifies several inputs and registered macros; C8c is tooling.

### C8a

1. Deserialization errors from `from_str`, `from_slice`, `from_reader` and `from_file` carry the
   position of the offending value, and its file for included input, as syntax errors do.
2. An optional parser setting caps the bytes read per document and in total across includes.
   Going over it is an error of its own kind. The default is documented.
3. The parser builder can set a list of search directories that include macros use from the
   start, as a `path` list already in effect would (§9.4).
4. For every conformance case that parses, emitting in each of the four formats and parsing the
   output again gives the same value, apart from the losses §10.8 lists; one test checks this.

### C8b

1. A parser can take several inputs in turn (bytes or files), each with its own priority and
   duplicate strategy, and produce one result, as the spec describes for several inputs.
2. An application can register its own macros by name through the parser builder, as the spec
   describes for registered macros. Names that are neither built in nor registered stay errors.

#### Project decisions for C8b (settle spec-v10 §13)

1. The deterministic quirks of §13.1 are reproduced as specified: the limit on the number of
   inputs and the include depth they share, a zero-byte first input, and the end of an input not
   being a separator. `docs/COMPATIBILITY.md` lists them as quirks users may hit.
2. Registered macro handlers can do both things §13.2 describes: add entries to the innermost
   open object, and have text parsed in place of the macro.
3. A handler can end the parse silently as §13.2 describes for a failing handler, and it can also
   return an error with its own message, reported as an error of its own kind. libucl has no way
   to report an error; this is an addition, not a change of libucl behaviour.
4. Include resolution follows C3 decision 6 and C5 decision 1: a configured base directory stands
   in for the working directory for every input; without one, a file input resolves against its
   own directory, and a text input finds files only through the loader it was given.
5. The conformance runner reads `.inputs` and registers handlers equivalent to the test macros
   the case format documents; the cases move from `pending/13-inputs/` to `cases/spec/13-inputs/`.

### C8c

1. A manually triggered workflow takes a libucl commit, regenerates every golden file at it and
   publishes the diff for review.
2. A fuzz target, outside `cargo test`, compares the crate's parse result with the oracle's on
   generated inputs, seeded from the conformance cases.
3. `tests/scaling.rs` stays the only timing-based test and says so; the test-profile opt-level
   override is documented as a speed choice.
4. The crate targets the latest stable Rust (decided by the project owner, 2026-09-25):
   `rust-version` is `1.98`, the current stable, in `Cargo.toml` and `tests/features/Cargo.toml`.
   CI runs on stable only, so the separate minimum-version job goes. The README, `CHANGELOG.md`,
   `CLAUDE.md`, `scripts/ci.sh` and test comments say "latest stable Rust (1.98 at this release)"
   instead of 1.88. Code may use features up to 1.98.

## C10 — Owner decisions of 2026-09-26

1. The crate follows `spec-v13` (answers to QUESTIONS #70–#78): the cases in
   `tests/conformance/pending/` pass and move to `cases/spec/`, and the runner reads per-platform
   golden files as `tests/conformance/README.md` describes.
2. The limit on how deep `.inherit` copies may nest a value (C7, QUESTIONS #56) is configurable on
   the parser and its builder. The default stays 1024. The largest accepted setting is the largest
   depth at which every entry point still runs within a 2 MiB stack in an unoptimised build, shown
   by `tests/stack_depth.rs`; it is documented.
3. The package and the library are renamed `serde_ucl`, and the version is `0.3.0`. Every
   reference follows: code, tests, examples, benches, `fuzz/`, `tests/features/`, the README
   (installation included), crate docs, `CHANGELOG.md` and `CLAUDE.md`. The GitHub repository is renamed
   too: `repository` and every link use `https://github.com/AnderEnder/serde_ucl`.
4. Coverage and release workflows are restored, working:
   - Coverage: `cargo llvm-cov` on Linux on push and pull request. The report goes to the job
     summary and an artifact, and to Codecov only when a `CODECOV_TOKEN` secret exists; a missing
     token never fails the job.
   - Release: on a `vX.Y.Z` tag, run `scripts/ci.sh` on Linux and macOS, fail unless the tag
     matches the `Cargo.toml` version, publish through crates.io Trusted Publishing (GitHub OIDC,
     environment `release`; the name was reserved with a placeholder `serde_ucl` 0.0.0), and create a
     GitHub release whose notes are that version's `CHANGELOG.md` section. Nothing publishes if a check
     fails.

## C11 — Parser performance (owner request of 2026-09-27)

Find out how to make parsing faster, and make the changes that are safe.

Reference points, measured black box on an Apple M4 Max: libucl, built with `-O3`, parses the
same UCL input about 2.4 times as fast as the crate and the same JSON input about 1.8 times as
fast; `serde_json` deserializes JSON into typed structs 3.4 to 8 times as fast as `serde_ucl` does
from the same text. The spec side reruns the libucl comparison after the work item.

1. Profile at least `parse/config/1000`, `parse/json/1000` and
   `serde/deserialize-1000/from_str`, with the release settings of `Cargo.toml` for the numbers
   that are reported.
2. Write a report, `target/perf/C11-report.md`: where the time goes (shares of the profile), each
   candidate change with its measured or estimated gain and its risk, ranked.
3. Make the candidate changes that are low risk, one commit each, with the criterion result
   before and after in the commit message.
4. Behaviour does not change: the conformance results and `xfail-*.txt` stay as they are, emitter
   output stays byte-identical, error kinds and positions stay the same, `tests/stack_depth.rs`
   passes unoptimised, `tests/scaling.rs` passes, `scripts/ci.sh` passes, and a differential fuzz
   run of at least five minutes (`scripts/ci.sh fuzz 300`) finds no difference after the last
   change.
5. New dependencies, `unsafe` code and public API changes are proposals in the report, not code;
   they are owner decisions.

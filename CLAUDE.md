# CLAUDE.md

Guidance for Claude Code in this repository.

## Style

Be direct and concise; no emojis or filler. Base statements on evidence, and use technical terms
precisely.

## Project

`serde_ucl`: UCL (Universal Configuration Language) for Rust with serde, format-compatible with
libucl, the C library used by FreeBSD. Minimum Rust is `rust-version` in `Cargo.toml` (the latest
stable at each release). Compatibility is defined by behaviour: the spec in `docs/spec/` and the
conformance suite in `tests/conformance/`, whose golden files come from libucl.
`docs/COMPATIBILITY.md` lists the deliberate differences and the quirks the crate reproduces.

- `src/parse/`: the parser. `mod.rs` (`Parser`), `builder.rs` (`ParserBuilder`), `core.rs`
  (document structure), `number.rs`, `string.rs`, `vars.rs`, `comments.rs`, `macros.rs` (macro
  syntax, built-in macros), `include.rs` and `glob.rs` (`.include`, `.try_include`, `.load`),
  `loader.rs` (`FsLoader`, `MemoryLoader`), `inputs.rs` (several inputs, spec §13.1),
  `registered.rs` (registered macros, spec §13.2), `facts.rs` and `tree.rs` (what the emitters need
  to know about the parse), `error.rs` (`Error`, `ErrorKind`).
- `src/emit/`: config, JSON, compact JSON and YAML output. `src/de.rs`, `src/de/`: serde
  deserialization; `src/ser/`: serialization; `src/handoff.rs`: passes a whole `UclValue` past
  serde's data model so deep values do not recurse. `src/value.rs`: the value model;
  `src/error.rs`: `UclError`, `Position`; `src/time.rs`: the `Duration` helper.
- `tests/conformance.rs`, `tests/conformance/`: conformance runners and cases;
  `tests/common/oracle.rs`: running a case as the oracle does, shared with the fuzzer.
- `fuzz/`: the differential fuzzer `ucl-differential`, a separate package (`fuzz/README.md`).
- `scripts/ci.sh`: everything CI runs. `scripts/regen-golden.sh`, `scripts/create-cases.sh` and
  `tools/ucl-dump/`: oracle tooling (spec team).
- `.github/workflows/`: `ci.yml` (`scripts/ci.sh`, Linux and macOS, stable), `coverage.yml`,
  `release.yml` (see *Release*), `golden.yml` (nightly drift check, Linux and macOS),
  `pin-move.yml` and `fuzz.yml` (manual).

## Clean room

The implementation in `src/` must not be derived from libucl's source code; tests may be.
`docs/clean-room/PROTOCOL.md` is the full protocol and wins over this summary. The spec team reads
libucl and writes the spec; the implementation team works only from released spec versions and has
never read libucl source.

| Area | Owner |
| --- | --- |
| `src/`, `tests/*.rs`, `tests/common/`, `tests/features/`, `fuzz/`, `examples/`, `benches/`, `xfail-*.txt` entries for the crate's behaviour | implementation team |
| `docs/spec/`, `docs/COMPATIBILITY.md`, conformance cases and golden files, `tests/conformance/README.md`, `scripts/regen-golden.sh`, `scripts/create-cases.sh`, `tools/`, `golden.yml`, `pin-move.yml` | spec team |
| `docs/clean-room/WORKLIST.md` (goals and decisions), `README.md`, `CLAUDE.md`, `CHANGELOG.md`, `Cargo.toml`, `scripts/ci.sh`, other workflows | either team, within its work item |

Rules for the implementation team:

- Forbidden inputs: libucl source (`*.c`, `*.h`, build files) anywhere, including
  `target/libucl-oracle/` and online; `tools/` (oracle tooling); `REVIEW.md`, `PLAN.md`,
  `PROGRESS.md`; branches `quarantine/*` and `backup/*`; the history of `CLAUDE.md` and of any
  revision containing `src/lexer.rs` or `src/parser.rs`; Claude Code's files under `~/.claude/`
  (session transcripts, memory), even when a context summary points at a transcript; anything
  else the spec team writes except work-item goals and conformance cases.
- Allowed inputs: the released spec, `docs/clean-room/`, libucl's public format documentation,
  the conformance cases and golden files, and the oracle run as a black box
  (`scripts/regen-golden.sh`, `target/libucl-oracle/ucl-dump`).
- The released spec is the latest tag: `git tag --sort=-v:refname -l 'spec-v*' | head -1`.
  `git diff <tag> -- docs/spec` shows edits made after it, which are not released.
- Search named directories only, never the repository root or `/tmp`.
- A sub-agent receives these rules word for word.
- Unclear or wrong spec: ask in `docs/clean-room/QUESTIONS.md`, choose a behaviour, record it; the
  answer comes as a new spec version.
- Exposure to a forbidden input: stop, record it in `docs/clean-room/LOG.md`, and hand over; code
  written after the exposure goes to a `quarantine/` branch.
- Every session appends a LOG entry: date, role, work item, inputs consulted, commits, and for
  implementers the attestation in PROTOCOL.md.

## Commands

```bash
cargo build
cargo test                                   # unit, integration, conformance, doctests
cargo test --test conformance
UCL_CONFORMANCE_REPORT=1 cargo test --test conformance -- --nocapture   # per-case detail
cargo bench --bench parse_benchmarks         # benches/README.md
scripts/ci.sh                                # run before pushing
```

| `scripts/ci.sh` mode | Does |
| --- | --- |
| *(none)* / `checks` | fmt, clippy with `-D warnings` over every feature set, all tests (stack depth also unoptimised, `tests/features/`, fuzzer unit tests), every example, bench build, `cargo doc -D warnings` |
| `golden` | rebuild libucl, regenerate every golden file and the serde corpus, fail on any change (golden files must be unmodified first) |
| `pin COMMIT` | the same at another libucl commit (full SHA); writes a summary and patch to `target/pin-move/`, commits nothing |
| `fuzz [SECONDS [SEED]]` | the differential fuzzer; fails on a difference |
| `coverage` | cargo-llvm-cov; report in `target/coverage/` |
| `release TAG` | the tag matches the `Cargo.toml` version and `CHANGELOG.md` has its section |

## Testing

- `cargo test --test conformance` runs three tests: `libucl_conformance_new_core` (parse result
  against `<case>.golden.json`), `libucl_conformance_emitters` (each format byte for byte) and
  `libucl_conformance_readback` (each output parsed again; only the losses of spec §10.8 are
  allowed, others go in `READBACK_PENDING`). Known failures are in `xfail-new.txt` and
  `xfail-emit.txt` with a reason; they hold only justified divergences and may only shrink, and a
  listed case that passes fails the run.
- Golden files come only from libucl and are never edited by hand. `scripts/regen-golden.sh`
  clones libucl at the pinned commit under `target/`, builds it and `tools/ucl-dump/`, and runs
  every case; it needs git, CMake and a C compiler. `LIBUCL_DIR` reuses a checkout,
  `LIBUCL_COMMIT` builds another commit. The macOS results are the expectation the crate is tested
  against; cases whose libucl result depends on the C library are listed in
  `tests/conformance/platform-dependent.txt`, with other platforms' files under
  `tests/conformance/platform/<platform>/`. Details: `tests/conformance/README.md`.
- serde: `tests/serde_roundtrip.rs` and the corpus in `tests/serde_corpus/`; `UCL_SERDE_REGEN=1`
  regenerates the corpus with the oracle binary.
- `tests/stack_depth.rs`: no entry point overflows a 2 MiB stack at the deepest accepted input.
  `cargo test` builds the crate at `opt-level = 2` for speed; `scripts/ci.sh` also runs these
  tests unoptimised.
- `tests/scaling.rs` is the only timing-based test (time ratios within one run); no other test
  uses wall-clock thresholds.
- `tests/features/` depends on the crate without the `load` feature, which the crate's
  dev-dependency on itself otherwise turns on.
- The fuzzer saves reduced differences under `target/fuzz-differential/findings/`; each goes to
  `QUESTIONS.md`, or is fixed where the spec is clear.
- README code blocks run as doctests (`ReadmeDoctests` in `src/lib.rs`). Unit tests are inline
  under `#[cfg(test)]`; integration tests in `tests/`.

## Code conventions

- Errors: the serde entry points return `UclError` (with `position()` and `file()`); the parse API
  returns `parse::Error` (`kind()`, `position()`, `file()`, silent stops via `is_stopped()`);
  registered macro handlers return `MacroError`.
- Behaviour follows the released spec; doc comments cite the section (for example §9.4).
- Text input reads no files unless the parser is given a file loader; `from_file` uses the
  filesystem.
- `cargo fmt`; clippy clean with `-D warnings`. Every Cargo feature (`fs`, `load`) is used by a
  `#[cfg]`.
- Profile with `cargo bench` before optimizing; release builds use fat LTO and one codegen unit.

## Commits, changelog, release

- Conventional commits: `type(scope): summary` (`feat`, `fix`, `perf`, `refactor`, `test`,
  `docs`, `build`, `ci`, `chore`), `!` for a breaking change. Clean-room work adds a footer
  `Work item: <ID>`; spec releases use `docs(spec): release spec vN` and tag `spec-vN`.
- `CHANGELOG.md`: newest first. Changes since the last release go under `## Unreleased` at the
  top; a release renames it `## X.Y.Z - YYYY-MM-DD`.
- Release: set the version in `Cargo.toml` (and the lock files, including `fuzz/` and
  `tests/features/`), rename the changelog section, run `scripts/ci.sh release vX.Y.Z`, commit
  `chore(release): X.Y.Z`, push `main`, wait for CI, then push an annotated tag `vX.Y.Z`.
  `release.yml` checks the tag, runs CI on Linux and macOS, publishes through crates.io Trusted
  Publishing (environment `release`) and creates the GitHub release from the changelog section.
  Never run `cargo publish` locally.

## Local agent definition

`.claude/agents/clean-implementer.md` is local and untracked. It embeds this file because it sets
`omitClaudeMd`; regenerate the embedded copy whenever this file changes.

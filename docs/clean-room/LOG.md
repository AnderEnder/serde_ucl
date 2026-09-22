# Clean-Room Provenance Log

## Before the clean room (oracle side)

- 2026-09-22 — REVIEW.md and PLAN.md written by participants who had read libucl source.
- 2026-09-22 — Phase 0 (conformance suite, `tools/ucl-dump`, `scripts/regen-golden.sh`, vendored
  upstream corpus): test-side work, allowed to be derived.
- 2026-09-22 — Phase 1 (fixes to the crate's existing parser and deserializer) and Phase 2 (value
  model, `src/de.rs`, `src/time.rs`) were written by oracle-side participants. The duplicate-key
  insertion in `src/value.rs` was a port of libucl logic. It was removed in the commit that added
  this log, leaving `todo!()` placeholders for C1. Comments that cited libucl internals were
  removed or made neutral.
- 2026-09-22 — A Phase 3 attempt that ported libucl's parser was stopped before any commit and
  moved to branch `quarantine/p3-source-port`. It must not be used.
- 2026-09-22 — `CLAUDE.md` (on this branch and in the main checkout) had its libucl implementation
  notes removed and the clean-room rules added.

- 2026-09-22 — Process changed to the classic two-team model: the spec team (which reads libucl
  source) writes `docs/spec/`, a reviewer checks it for prohibited content before release, and the
  implementation team (which has never read libucl source) works only from released spec versions.
  An earlier C0 attempt by a clean participant, using public docs and black-box runs only, was
  stopped before committing anything.

## Sessions

(Append entries here: date, role, work item, inputs consulted, commits, attestation.)

- 2026-09-22 — **Spec team, C0 (behaviour specification).**
  - Inputs consulted: libucl source at the pinned commit `24c8b399062ae4691168c243e3b7345ef7f31956`
    (parser, emitter and utility sources, character tables, `tests/test_basic.c`); libucl's public
    documentation (`README.md`, `doc/*.md`); the conformance suite; black-box runs of the oracle
    (`tools/ucl-dump`, `scripts/regen-golden.sh`, and libucl's `test_basic` to confirm that the
    upstream `.res` files reproduce).
  - Commits: `3927c3e` (oracle options `-e`, `-v`, `-H`, `-p`, `-s`; 385 spec cases), `415d06b`
    and `dab710f` (more cases; `-S` oracle option), `b56135b` (`docs/spec/` sections 01–12 and a
    coverage table for all 716 cases; more cases), and this entry.
  - Result: `docs/spec/` describes observable behaviour only: accepted inputs, resulting values,
    rejected inputs and output bytes. Each rule cites conformance cases whose golden files come from
    the oracle. It contains no libucl code or pseudo-code and no internal libucl names; the only
    libucl identifiers are the public parser-flag names in §12, plus case file names in the coverage
    table. Sections follow format features, not libucl's source files.
  - Test state: `xfail.txt` lists the 225 new cases that the pre-C1 parser (commit `03b2756`) fails,
    by the runner's suggested reason (parser 134, macro 58, flags 33). Checked by running the
    conformance test at `03b2756` with the new cases: 716 cases, 344 pass, 372 expected failures,
    green. At the branch head, `insert_with_strategy` is still `todo!()` until C1, so the runner
    reports extra failures there.
  - Not yet done: review of the spec for prohibited content, and the `spec-v1` tag (PROTOCOL.md,
    *Release*).

- 2026-09-22 — **Spec reviewer, C0 review (candidate `spec-v1`).**
  - Inputs consulted:
    - `docs/clean-room/PROTOCOL.md`;
    - `docs/spec/README.md` and `01-*.md` … `12-*.md` at `c9e1dd5`;
    - `tests/conformance/` (case inputs, `.flags`, golden files, `xfail.txt`, README);
    - `cargo test --test conformance`;
    - black-box runs of `target/libucl-oracle/ucl-dump` on scratch inputs, to probe claims that
      no case covers.
  - Not consulted: libucl source, `tools/ucl-dump/ucl_dump.c`, anything under
    `target/libucl-oracle/libucl/`, `REVIEW.md`, `PLAN.md`, `PROGRESS.md`, `quarantine/*`.
  - Present in this session's context without being sought: the main checkout's working copy of
    `CLAUDE.md` (uncommitted changes there). It still contains implementation notes that look
    libucl-derived: a number-parsing state machine with an internal-looking variable name, a
    "two-pass algorithm from C implementation", a character-flag table and in-place unescaping.
    This branch's `CLAUDE.md` does not contain them. Implementer sessions started from the main
    checkout would receive that content.
  - Commits: `5c98e5b` (`docs/clean-room/reviews/spec-v1.md`), `4cdca6e` (this entry), and a
    follow-up `C0 review:` commit that corrects the B-2 count in the review's verdict.
  - Result: verdict **Release after fixes**.
    - Bar A: seven minor wording findings, no blockers.
    - Bar B: six majors and fourteen minors.
    - Citations: all 636 resolve; 339 checked against golden content, including every citation in
      §05, §07, §08 and §10.
    - `spec-v1` is not tagged. The spec team fixes the bar-A findings and the bar-B majors first.
  - Attestation: I did not read libucl source code.

- 2026-09-22 — **Spec team, C0 fixes after review; release `spec-v1`.**
  - Inputs consulted: the review `docs/clean-room/reviews/spec-v1.md`; libucl source at the pinned
    commit (parser, emitter, utility and test-harness sources); black-box runs of the oracle; the
    conformance suite.
  - Commits: `52ae390` (41 new cases, golden files from the oracle; `object_depth_limit_error`
    changed to 1024 objects; 28 new `xfail.txt` entries for cases the pre-C1 parser fails),
    `2ddb2d6` (spec text: A-1 … A-7, B-1 … B-20), `3b2e183` (`docs/clean-room/reviews/spec-v1-response.md`,
    mapping each finding to its commit).
  - Tag: `spec-v1` → `3b2e183` (lightweight, local only).
  - Result: all review findings fixed, none deferred. *Known gaps* in `docs/spec/README.md` lists
    two rules without a committable case (B-10 file input, B-20 depth 1023). The spec still
    contains behaviour only. Self-check greps: no `ucl_`/`UCL_` outside §12's public flag names,
    no `.c`/`.h` references, no code blocks. All citations resolve, and every `cases/spec/` case is
    cited. At `03b2756` with the new cases, the conformance test is green: 757 cases, 357 pass,
    400 expected failures.
  - Cause of B-2, recorded for future sessions: the tool used to write files replaced each
    backslash-`u` escape followed by four hex digits with the decoded character. Text with such
    escapes must be written through a placeholder and checked afterwards.

- 2026-09-22 — **Implementation team, C1 (duplicate-key insertion).**
  - Inputs consulted:
    - `docs/clean-room/PROTOCOL.md`, `WORKLIST.md`, `LOG.md`, `QUESTIONS.md`;
    - spec `spec-v1` (`git show spec-v1:…`; the working-tree copy was identical): §8, §9.4–§9.7,
      §12 and `README.md`;
    - the current `src/value.rs` (the file edited), `tests/conformance.rs`,
      `tests/conformance/README.md` and the header of `xfail.txt`;
    - case inputs, `.flags` and golden files: `cases/spec/08-duplicates/` `strategy_merge_arrays_ignore_priority`,
      `strategy_merge_scalar_keeps_container_priority`, `priority_applies_to_containers`,
      `keeps_first_position`, `priority_higher_replaces`, `no_implicit_arrays_with_arrays`;
      `cases/spec/09-macros/` `include_merge_ignores_priority`,
      `include_merge_lower_priority_still_merges`, `inherit_replaced_whatever_priority`;
    - black-box runs of `target/libucl-oracle/ucl-dump`: its usage message, and 18 probes on
      scratch inputs (options `-s merge|error|rewrite` and `-I`), with the inputs and results
      recorded in `QUESTIONS.md` #1–#4, except these five:
      - `-s rewrite`, `.priority 3⏎a = 1⏎.priority 1⏎a = 2` → `a: int 2 @1`;
      - default strategy, `a = 1⏎a = 2⏎.include(duplicate="merge") "f"` with f = `a { z = 3 }`
        → `a: ⟨int 1 | int 2 | {z: int 3}⟩`;
      - `-I`, `.priority 3⏎a = 1⏎a = 2⏎.priority 1⏎a = 3` → `a: int 3 @1` (also added to #3);
      - `-I`, `a = 1⏎a = 2⏎.priority 2⏎a = 3` → `a: int 3 @2` (also added to #3);
      - `-I`, `a = [1]⏎a = 2⏎a = 3` → `a: [[int 1], int 2, int 3]`;
    - the worktree's `CLAUDE.md` (loaded automatically; the clean version);
    - `cargo build`, `cargo test`, `cargo clippy` output.
  - Seen without opening the file: a grep over `src/`, `tests/`, `examples/` and `benches/` for the
    insertion function names printed one line of `src/parser.rs` (line 2500, the only call site).
    `src/parser.rs` was not opened.
  - Not consulted: libucl source, anything under `target/libucl-oracle/` other than running
    `ucl-dump` (no `libucl/`, `build/` or `build.log`), the oracle tool sources,
    `scripts/regen-golden.sh` (neither read nor run), `REVIEW.md`, `PLAN.md`, `PROGRESS.md`,
    `quarantine/*`, history of `src/` before `ef8007e`, history of `CLAUDE.md`, `src/lexer.rs`,
    `src/parser.rs`. The PLAN.md references in existing comments and in `xfail.txt` were not
    followed.
  - Commits:
    - `9babe6e`: implementation and unit tests;
    - `fb7ebf3`: QUESTIONS.md #1–#4;
    - this entry.
  - Result:
    - `cargo test` is green (395 tests), and `cargo build --examples --benches` builds.
    - Conformance before: 757 cases, 96 pass, 400 expected failures, 261 unexpected failures
      (`todo!()` panics). After: 357 pass, 400 expected failures. `xfail.txt` is unchanged.
    - Where the spec is silent, behaviour follows the oracle probes (QUESTIONS.md #1–#4).
    - The `DuplicateKeyError` message, "duplicate element for key '…' found", is taken from the
      existing unit test `test_rewrite_and_error`.
  - Model gap for C2: `UclArray` elements carry no priority. Golden files with a priority per array
    element (`priority_applies_to_containers`, `strategy_merge_arrays_ignore_priority`, and the
    collection arrays of QUESTIONS.md #3) cannot be represented yet.
  - Attestation: I did not read libucl source code or any forbidden input listed in
    docs/clean-room/PROTOCOL.md.

- 2026-09-22 — Role: spec team. Item: C0, release `spec-v2` (tag on `17b0b85`). Answers implementer
  questions #1–#4 from C1 (`docs/clean-room/QUESTIONS.md`). Inputs consulted: libucl source at the
  pinned commit (duplicate-key handling), black-box oracle runs, the conformance corpus, and
  `spec-v1`. Changes: §8.3 (the inherited-value rule applies under `append` only; behaviour under
  `merge`, `rewrite`, `error`; values stay inherited after a merge), §8.4 (merge uses only the
  first value of an entry; nested entries are compared by priority), §8.5 (the collected array has
  priority 0, a quirk; the error after a merge replaces a collected array with a scalar, a quirk
  matched per D4), new §8.7 (root and array-element priorities have no observable effect), §11.1
  and README (error message wording is not specified), §12.4 (pointer to §8.5). 31 new cases
  (30 in `cases/spec/08-duplicates/`, 1 in `cases/spec/10-output/`) with 8 include files; all 31 fail
  on the current parser and are in `xfail.txt` (macro or flags). The conformance runner now ignores
  the root object's priority and array elements' priorities, as §8.7 states. Commits: `a7344ce`,
  `4d8e863`, `17b0b85`. The spec contains behaviour only: no libucl code, internal names, or
  procedures.

- 2026-09-23 — **Implementation team, C2 (new parser core).**
  - Inputs consulted:
    - `docs/clean-room/PROTOCOL.md`, `WORKLIST.md`, `LOG.md`, `QUESTIONS.md`;
    - spec `spec-v2`, read from the working tree after checking that `git diff spec-v2 -- docs/spec/`
      is empty: `README.md`, §01–§09, §11, §12 (§10 not read);
    - `tests/conformance.rs`, `tests/conformance/README.md`, the header of `xfail.txt`, case inputs
      and golden files (`cases/spec/08-duplicates/priority_applies_to_containers`,
      `cases/spec/07-variables/filename_for_string_input`, `cases/spec/11-errors/*`,
      `cases/spec/02-comments/hash_at_end_of_input`, `libucl/basic/22`), and a grep of all golden
      files for hex-encoded keys and strings (only `libucl/basic/22`);
    - the crate's own code: `src/value.rs`, `src/lib.rs`, parts of `src/error.rs` (`Position`,
      `UclError`, the outline of `with_source_context`), `examples/advanced_features.rs` lines
      355–385 (a `match` on `UclError`), `Cargo.toml`;
    - black-box runs of `target/libucl-oracle/ucl-dump`: its usage message; about 300 probes on
      scratch inputs, with options `-l`, `-T`, `-I`, `-M`, `-s merge|rewrite|error`; and
      differential runs of about 90,000 generated inputs through the oracle and the new core
      (a scratch tool outside the repository), across the same options. The findings are in
      `QUESTIONS.md` #5–#16 with their inputs and results. In the last runs every remaining
      difference falls under one of those questions;
    - the clean `CLAUDE.md` embedded in the agent definition; `cargo build`, `cargo test`,
      `cargo fmt --check` output.
  - Seen without opening the files:
    - one directory listing of the session scratchpad's root, shared with other sessions, showed
      other sessions' file names, among them `.c` files, `chartable.txt` and `parser_head.rs`.
      None was opened, and all work used a private subdirectory;
    - two greps over `src/` for `duplicate element` and `DuplicateKeyError` had `src/lexer.rs` and
      `src/parser.rs` in scope. Neither matched in them, so nothing from those files was printed;
    - the oracle's stderr messages, libucl's wording, in probe output. The core's messages are the
      project's own;
    - `tests/conformance/README.md` names `tools/ucl-dump/ucl_dump.c`, and `xfail.txt` notes name
      `PLAN.md` items. Neither was followed.
  - Not consulted: libucl source; anything under `target/libucl-oracle/` other than running
    `ucl-dump` (no `libucl/`, `build/` or `build.log`); the oracle tool's sources;
    `scripts/regen-golden.sh` (neither read nor run); `REVIEW.md`, `PLAN.md`, `PROGRESS.md`;
    `quarantine/*`; history of `src/` before `ef8007e`; history of `CLAUDE.md`; `src/lexer.rs`,
    `src/parser.rs`.
  - Commits, with the `xfail-new.txt` count after each:
    - `f8385a0` C2.1: `DuplicateKeyError` wording is the project's own; `test_rewrite_and_error`
      checks the error value and key (no `xfail-new.txt` yet);
    - `19e5b2d` C2.2: `src/parse/` skeleton and public API, the second conformance suite with
      `.flags` applied, `xfail-new.txt` with all 788 cases: 788;
    - `bb69379` C2.3: the parser core (§1–§8, §11, §12; macros recognised): 130;
    - `7333c5f` C2.4: two gaps found by differential runs (QUESTIONS.md #9, #15); unit tests: 130;
    - `19f0c68` C2.5: `UclError::Syntax`; QUESTIONS.md #5–#15: 130;
    - `abb5da0` C2.6: this entry: 130;
    - a follow-up C2.7 commit: QUESTIONS.md #16 and the corrections to this entry: 130.
  - Result:
    - `xfail-new.txt`: 130 entries, 129 `macro` (C3) and 1 `non-utf8` (`libucl/basic/22`).
      `xfail.txt` is unchanged at 431. `cargo test`: 431 tests pass (395 before C2).
      `cargo build --examples --benches` builds.
    - Design: containers are filled in place on an explicit frame stack, so nesting never grows
      the call stack and a nested object is already in its parent while it is parsed (which
      `.inherit` needs in C3). Supporting changes to the model: `Placement`,
      `UclObject::insert_slot_placed`, `Entry::value_at_mut`. Root and array-element priorities
      are not represented (§8.7). A container dropped for a lower priority is parsed into a
      detached value; that path needs C3's priority changes to be reached.
    - Spec gaps: where the spec is silent the core follows the oracle (QUESTIONS.md #5–#9, #15);
      where the spec states a rule that the oracle contradicts, the core follows the spec
      (#10–#14, #16). Two of these rest on a reading of the spec: #5 takes §1.6's "a bare key
      followed by a line break" to mean a line break directly after the key, and #15 takes §5.8
      to be silent on out-of-range numbers followed by other text.
    - Choices for the spec's *Uncertain* items: §4.8, fewer than four characters after
      backslash-u in an unquoted value: the backslash is dropped and the characters kept
      (`x\u12` gives `xu12`; the oracle gives `xu2`); §7.7, a handler's value is substituted in
      place like a registered variable's and counts as a replacement for §7.5; §12.5, every skipped
      comment is saved with its text and position, without attaching it to a value.
  - Attestation: I did not read libucl source code or any forbidden input listed in
    docs/clean-room/PROTOCOL.md.

- 2026-09-23 — Role: spec team. Item: C0, release `spec-v3` (tag on `ebd0b04`). Answers implementer
  questions #5–#16 from C2 (`docs/clean-room/QUESTIONS.md`). Inputs consulted: libucl source at the
  pinned commit (comment skipping, the start-of-document handling, container closing, macro-name
  handling, unescaping, variable expansion, number reading, comment attachment), black-box oracle
  runs, the conformance corpus, and `spec-v2`. Where the implementer had kept the spec against the
  oracle (#10–#14, #16), the spec was wrong and is corrected: §1.1 (a bracketed root only after
  whitespace alone or directly after a leading comment group), §2.2 (a last-byte `#` after
  whitespace before the first key is an error), §3.4 (left-open section objects close with the next
  bracketed container opened in them), §7.6 (`\$` blocks expansion only when every `$` is written
  `\$`), §9.1/§9.2 (a macro whose name runs to the end of input is ignored; a known name followed
  only by whitespace and comments to the end too), §12.1 (quoted keys are lowercased before escapes
  are decoded). Where the implementer followed the oracle (#5–#9, #15), the reading is confirmed
  and written down: §1.5, §1.6, §2.3 (corrected in one detail: a `"` directly after `\` never
  toggles a quoted part), §3.2, §3.4, §5.8. Also new: §5.2 quirk for `x` after a fraction or
  exponent. Uncertain items: §4.8 resolved (short `\u` escapes are deterministic); §12.5 resolved
  (comment text and attachment rules, recorded through a new oracle option `-c` /
  `dump-comments`), except the byte saved after a block comment that ends the input, which is
  undefined; §7.7 stays undefined in libucl (memory-dependent) and the implementer's choice
  becomes a project divergence (README). 94 new cases with golden files from the oracle; the old
  parser fails 56 of them (`xfail.txt`), the new core 37 (`xfail-new.txt`, reason `spec-v3`).
  Both runners green. Commits: `547a893`, `ebd0b04`. The spec contains behaviour only: no libucl
  code, internal names, or procedures.

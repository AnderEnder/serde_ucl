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

- 2026-09-23 — **Implementation team, C2.8 (new parser core to `spec-v3`).**
  - Inputs consulted:
    - `docs/clean-room/PROTOCOL.md`, `WORKLIST.md`, `LOG.md`, `QUESTIONS.md`;
    - spec `spec-v3`, read from the working tree after checking that `git diff spec-v3 HEAD --
      docs/spec/` is empty: the whole of `git diff spec-v2 spec-v3 -- docs/spec/` (§1, §2, §3,
      §4, §5, §7, §9, §12, README); in full §5 and §9.1–§9.3; in part §1.1–§1.5, §2.1–§2.2,
      §3.1–§3.4, §4.5–§4.9, §7.1–§7.6, §12.1–§12.4;
    - `tests/conformance/README.md`, `xfail-new.txt`, `tests/conformance.rs`; the inputs, `.flags`
      and golden files of the 37 `spec-v3` entries; the list of `.flags` files naming comments;
      the inputs of the `cases/spec/09-macros/` cases whose golden file is an error;
    - the crate's own code: `src/parse/*`, parts of `src/value.rs` (`Placement`, `Slot`, the
      `Entry` insertion rules, `insert_slot_placed`), `Cargo.toml`;
    - black-box runs of `target/libucl-oracle/ucl-dump`: its usage message; about 400 probes on
      scratch inputs under `target/c28/` (options `-c`, `-l`, `-T`, `-I`, `-s merge|rewrite`);
      differential runs of about 417,000 generated inputs through the oracle and the new core, with
      a scratch tool under `target/c28/` that is not committed; in the last runs no difference was
      left other than the §12.5 *Uncertain* byte. The findings that the spec does not
      cover are in `QUESTIONS.md` #17–#22 with their inputs and results;
    - the clean `CLAUDE.md` embedded in the agent definition; `cargo build`, `cargo test`,
      `cargo clippy`, `rustfmt --check` output.
  - Seen without opening the files:
    - `ls target/libucl-oracle/` printed `build`, `build.log`, `libucl`, `ucl-dump`;
    - the oracle's stderr message, libucl's wording, in one probe;
    - `tests/conformance/README.md` names `tools/ucl-dump/ucl_dump.c`, `PLAN.md` and `REVIEW.md`,
      and a doc comment in `tests/conformance.rs` names a `PLAN.md` item; none was followed;
    - the session's git status listed `PLAN.md` and `REVIEW.md` in the main checkout by name;
    - a grep over `src/` for uses of the `parse` module printed about ten lines of `src/lexer.rs`
      that matched `.parse::<…>()` or `.comments()`, and two `clippy` runs printed two lines of
      `src/parser.rs` (a test assertion on `parsing_hooks()`). Neither file was opened and nothing
      from them was used;
    - the harness captured the output of two background commands in files under the session
      scratchpad in `/private/tmp/`; neither file was opened (the runs were repeated with their
      output in `target/c28/`).
  - Not consulted: libucl source; anything under `target/libucl-oracle/` other than running
    `ucl-dump`; the oracle tool's sources; `scripts/regen-golden.sh` (neither read nor run);
    `REVIEW.md`, `PLAN.md`, `PROGRESS.md`; `quarantine/*`; history of `src/` before `ef8007e`;
    history of `CLAUDE.md`; `src/lexer.rs`, `src/parser.rs`; `docs/spec/` not edited, no golden
    file edited.
  - Changes:
    - §1.1 root bracket and §2.2 last-byte `#`; §2.3 quotes in block comments; §3.4 left-open
      section objects close with the next bracketed container that closes in them; §4.8 short
      `\u` escapes (this replaces the C2 choice `x\u12` → `"xu12"` with the spec's `"xu2"`);
      §5.2 `x` after a fraction or exponent and §5.8 malformed text before the range check; §7.6
      `\$` blocks expansion only when every `$` is written `\$`; §9.1/§9.2 macros at the end of
      input, macros after a section name, and the macro syntax (unbalanced arguments, arguments
      then the end of input, with a `"` after `\` not ending a quoted part) checked before C3;
      §12.1 quoted keys lowercased before their escapes are decoded, and keys compared without
      ASCII case, an entry keeping its first spelling (the model no longer lowercases a second
      time).
    - §12.5: comments are attached to values (`Parser::attached_comments`, `AttachedComments`,
      `PathSegment`, `CommentPlacement`), and a block comment's saved text includes the byte after
      `*/`. *Uncertain* item: at the end of input no byte is added.
    - The conformance runner dumps the new core's attached comments as `"c"`/`"ca"` for cases with
      `dump-comments`, so those cases are compared.
    - Where the oracle goes beyond the spec text, the core follows the oracle and the question is
      open: `QUESTIONS.md` #17 (comment lists and §8), #18 (known macro name at the end of input),
      #19 (§5.2/§5.8 details), #20 (`\U` under `key-lowercase`), #21 (§4.8 bytes, §7.6 `$`
      count), #22 (VT, FF and line breaks between section names).
  - Commits: `ea87ec3` (C2.8: the changes above, QUESTIONS.md #17–#22, this entry) and a
    follow-up C2.8 commit (key comparison under `key-lowercase` and the quote rule in macro
    arguments, found by probes after `ea87ec3`; #18 and #20 extended; header comments of
    `xfail-new.txt` and `tests/conformance.rs` corrected).
  - Result:
    - `xfail-new.txt`: 129 entries, 128 `macro` (C3) and 1 `non-utf8` (`libucl/basic/22`). All 37
      `spec-v3` entries are gone, and so is `cases/spec/09-macros/macro_unbalanced_args_error`,
      now rejected for its unbalanced arguments. New core: 882 cases, 753 pass.
    - `xfail.txt` is unchanged (old parser: 395 pass, 487 expected failures). `cargo test`: 440
      tests pass. `cargo build --examples --benches` builds.
  - Attestation: I did not read libucl source code or any forbidden input listed in
    docs/clean-room/PROTOCOL.md.
- 2026-09-23 — Role: spec team. Item: C0, release `spec-v4` (tag on `591b818`). Answers implementer
  questions #17–#22 from C2.8 and completes §9 for C3 (macros). Inputs consulted: libucl source at
  the pinned commit (the include, priority, load and inherit handlers; macro name, argument and
  value parsing; comment saving and attachment; number reading; character classes; the emitter's
  key quoting; file fetching, path resolution and file variables), black-box oracle runs, the
  conformance corpus, and `spec-v3`. All six questions confirm the implementer's reading and are
  written down: §12.5 (#17; a replaced value's comments reappearing later is **Uncertain**), §9.2
  and §2.2 (#18), §5.2 and §5.8 (#19; an out-of-range float truncated for `kb`/`mb`/`gb` is
  platform-dependent, **Uncertain**, §5.4), §12.1 (#20), §4.8 and §7.6 (#21), §3.4 (#22). §9.5 now
  states that a bare macro value keeps its trailing spaces. The §9 audit corrected three earlier
  statements: macro arguments do see `FILENAME` = `undef` and `CURDIR` (not "no variables");
  whitespace after a comment group before a macro value is part of the value; a braced included
  file takes over the enclosing object's brace. It adds parameter matching and types, value
  forms, include units (priority and strategy not inherited, file variables, `no-filevars`),
  braces in included files, missing and unusable files, globs, nesting under a key, `sign`/`url`/
  `path`, and details of `.priority`, `.load` and `.inherit`; §10.1 gains the output facts of
  values created by macros. README: Uncertain list, and *Known gaps* now lists three project
  decisions still open (`sign`/`url`/`path`; how the API reports a silent stop; file parsing under
  `no-filevars`). 261 new cases with golden files from the oracle (1143 in all); no existing
  golden file changed. The new core fails 193 of them, all reason `macro` (`xfail-new.txt`); the
  old parser fails 206 (`xfail.txt`). The conformance README notes the new helper files. Both
  runners green at every commit. Commits: `797072a`, `a81b420`, `591b818`. The spec contains
  behaviour only: no libucl code, internal names, or procedures.
- 2026-09-23 — **Implementation team, C3a (`.priority` and `.inherit` in the new parser core, `spec-v4`).**
  - Inputs consulted:
    - `docs/clean-room/PROTOCOL.md`, `WORKLIST.md` (with *Project decisions for C3*), `LOG.md`
      (the C2.8 and `spec-v4` entries), `QUESTIONS.md`;
    - spec `spec-v4`, read from the working tree after checking that
      `git diff --stat spec-v4 HEAD -- docs/spec/` is empty: §8, §9 and §12 in full; README lines
      1–150 (conventions, how the oracle runs every case, divergences, uncertain behaviour, known
      gaps);
    - `tests/conformance/README.md`, `xfail-new.txt`, `tests/conformance.rs`; the inputs, `.flags`
      and golden files of the 86 entries whose input uses only `.priority` and `.inherit`, and the
      inputs of `macro_value_then_comma_error` and `try_include_self_stops_parsing`;
    - the crate's own code: `src/parse/*`, `src/value.rs`, `Cargo.lock`; the C2.8 scratch
      differential tool `target/c28/difftool/` (implementation-team code, not committed), copied to
      `target/c3a/difftool/` and given generators for macro inputs;
    - the `indexmap` 2.12 source in the cargo registry, for the signature of `replace_index`;
    - black-box runs of `target/libucl-oracle/ucl-dump`: its usage message; about 130 probes on
      scratch inputs under `target/c3a/p/` (options `-c`, `-l`, `-I`, `-F`, `-v`, `-s`);
      differential runs of about 150,000 generated inputs through the oracle and the new core. The
      final runs (about 120,000 inputs, flags `-l`, `-I`, `-c`, `-F`, `-s merge|rewrite|error` and
      combinations) left no difference; the tool skips inputs the core reports as unsupported. The
      findings that the spec does not cover are in `QUESTIONS.md` #23–#27 with their inputs and
      results;
    - the clean `CLAUDE.md` embedded in the agent definition; `cargo build`, `cargo test`,
      `cargo clippy`, `rustfmt --check` output.
  - Seen without opening the files:
    - the session's git status listed `PLAN.md`, `REVIEW.md` and `.claude/` as untracked and
      `CLAUDE.md` as modified in the main checkout, by name;
    - `tests/conformance/README.md` names `tools/ucl-dump/ucl_dump.c`, `PLAN.md` and `REVIEW.md`;
      doc comments in `tests/conformance.rs` and `src/value.rs` name `PLAN.md` items (the one on
      `Slot::inherited` was removed with the attribute it explained); none was followed;
    - the oracle's stderr, libucl's wording, in the first probes, before stderr was discarded;
    - one `clippy` run printed six lines of `src/lexer.rs` (a collapsible `if`), and later runs
      printed the file and line of two `src/parser.rs` warnings. Neither file was opened and
      nothing from them was used;
    - `ls target/` and `ls target/c28/` printed directory names, `libucl-oracle` among them.
  - Not consulted: libucl source; anything under `target/libucl-oracle/` other than running
    `ucl-dump`; the oracle tool's sources; `scripts/regen-golden.sh` (neither read nor run);
    `REVIEW.md`, `PLAN.md`, `PROGRESS.md`; `quarantine/*`; history of `src/` before `ef8007e`;
    history of `CLAUDE.md`; `src/lexer.rs`, `src/parser.rs`; `/tmp` and the session scratchpad.
    `docs/spec/` and golden files were not edited.
  - Changes:
    - `src/parse/macros.rs` (new): the syntax every macro shares (§9.2): NAME, the skipping between
      the parts, ARGUMENTS parsed as a document of their own (same flags, priority 0, `append`,
      only `FILENAME` = `undef` and `CURDIR`, no handler, comments not saved; error positions in
      the enclosing input; at most `MAX_ARGUMENT_DEPTH` = 64 documents inside one another), the
      three VALUE forms with variables expanded, and the skipping after a macro. Parameter tables
      for `.include`/`.try_include`/`.includes`, `.load` and `.priority`, matched by prefix and
      type (`Arguments::resolve`), and the exact-name `replace` of `.inherit`
      (`Arguments::exact_bool`). `.priority` (§9.5) and `.inherit` (§9.7) run; `.include`,
      `.try_include`, `.includes` and `.load` are read in full, then reported as unsupported.
    - `src/parse/core.rs`: the macro code moved to `macros.rs`. Under `key-lowercase`, an entry
      whose values a new value replaces takes the new key's spelling (QUESTIONS.md #27).
    - `src/value.rs`: `UclObject::get_index`, `get_index_mut`, `index_of`, `rename_key`;
      `Slot::into_inherited`; under `no-implicit-arrays` a collection takes only the entry's first
      value (#25).
    - `src/parse/error.rs`: `UnterminatedMacroValue`, `ArgumentsTooDeep`, `InvalidPriority`,
      `InheritSourceMissing`, `InheritSourceNotObject`.
    - `xfail-new.txt`: 92 entries removed, the 86 that need only `.priority` and `.inherit` and 6
      include-family cases whose golden error lies in the macro syntax (argument parse error,
      quoted and braced value errors, a `,` after the value). The notes of the remaining `macro`
      entries name the C3b macros their input uses; header updated. `tests/conformance.rs` doc
      comment updated.
    - `QUESTIONS.md` #23–#27.
    - For C3b: the include family is read in full and the skipping after it is checked before the
      "unsupported" error; for the silent stops of #23 (`a = 1⏎.try_include()#`) the include has to
      run before that check. `bytes_curdir()` in `macros.rs` is the one place that gives `CURDIR`
      for argument documents (decision 6). `Resolved::bool`, `string` and `array` are in place for
      the include parameters.
  - Result: `xfail-new.txt` has 230 entries: 229 `macro` (C3b; notes: 174 `.include`, 41 `.load`,
    11 `.try_include`, 1 `.includes`, 1 `.include, .try_include`, 1 `.load, .include`) and 1
    `non-utf8`. New core: 1143 cases, 913 pass. `xfail.txt` is unchanged (old parser: 450 pass,
    693 expected failures). `cargo test`: 456 tests pass. `cargo build --examples --benches`
    builds.
  - Commit: the `C3a:` commit that adds this entry.
  - Attestation: I did not read libucl source code or any forbidden input listed in
    docs/clean-room/PROTOCOL.md.
- 2026-09-23 — **Implementation team, C3b (loader, `.include`, `.try_include`, `.includes`, `.load` in the new parser core, `spec-v4`).**
  - Inputs consulted:
    - `docs/clean-room/PROTOCOL.md`, `WORKLIST.md` (with *Project decisions for C3*), `LOG.md`
      (the `spec-v4` and C3a entries), `QUESTIONS.md`;
    - spec `spec-v4`, read from the working tree after checking that
      `git diff --stat spec-v4 HEAD -- docs/spec/` is empty: §9 in full; §7.1, §7.2, §7.8;
      §10.1; §11; §12 (flag table, §12.6, §12.7, §12.8); README lines 1–150;
    - `tests/conformance/README.md`, `xfail-new.txt`, `tests/conformance.rs`; the inputs,
      `.flags` and golden files of the 229 `macro` entries; the helper files under
      `cases/spec/09-macros/files/`, `cases/spec/08-duplicates/files/` and `libucl/basic/`
      (`*.inc`, `include_dir/`, `load.inc`);
    - the crate's own code: `src/parse/*`, `src/value.rs`, `src/error.rs` (`UclError`),
      `Cargo.toml`; the C3a scratch differential tool, copied to `target/c3b/difftool/` and
      given a base directory, silent stops, generators for include and `.load` inputs and a
      mode that prints both results (implementation-team code, not committed);
    - black-box runs of `target/libucl-oracle/ucl-dump`: its usage message; about 210 probes
      on scratch inputs and helper files under `target/c3b/p/` (a copy of
      `cases/spec/09-macros/files/` plus files of my own), options `-c`, `-l`, `-I`, `-F`,
      `-S`, `-v`, `-s`, stderr always discarded; differential runs through the oracle and the
      new core (include, `.load` and the C3a generators; flags `-c`, `-l`, `-I`, `-F`, `-S`,
      `-s merge|rewrite|error`, `-l -I -c`). The final runs generated 91,722 inputs (a run
      repeated with an identical seed counted once). The tool skipped 24,280 of them, inputs
      with a macro right after a section name (QUESTIONS.md #31), and compared 67,442, among
      them two runs with the skip turned off, 8,112 inputs in all, which found one difference, of
      that shape. The compared inputs leave no other difference. Several probes crashed the oracle (SIGSEGV, exit
      139): silent stops inside files included by a glob or a search path (#30), a `}` in a file
      nested under a key (#32), and `"s".include "files/a.inc" # [` (#31). The findings the
      spec does not cover are in QUESTIONS.md #28–#33 with their inputs and results;
    - the clean `CLAUDE.md` embedded in the agent definition; Cargo documentation on features
      and dev-dependencies (general knowledge); `cargo build`, `cargo test`, `cargo clippy`,
      `rustfmt --check` output.
  - Seen without opening the files:
    - the session's git status listed `PLAN.md`, `REVIEW.md` and `.claude/` as untracked and
      `CLAUDE.md` as modified in the main checkout, by name;
    - `tests/conformance/README.md` names `tools/ucl-dump/ucl_dump.c`, `PLAN.md` and
      `REVIEW.md`; the doc comments of `tests/conformance.rs` name `PLAN.md` items; none was
      followed;
    - `clippy` printed the file and line of two `src/parser.rs` warnings, and one run printed
      one code line of a pre-existing file outside `src/parse/` (`character: ch,`, line 627,
      so `src/lexer.rs` or `src/parser.rs`). Neither file was opened and nothing from them was
      used;
    - `cargo test` runs a pre-existing unit test that writes and removes a scratch file in
      the system temporary directory; nothing there was listed or opened.
  - Not consulted: libucl source; anything under `target/libucl-oracle/` other than running
    `ucl-dump`; the oracle tool's sources; `scripts/regen-golden.sh` (neither read nor run);
    `REVIEW.md`, `PLAN.md`, `PROGRESS.md`; `quarantine/*`; history of `src/` before `ef8007e`;
    history of `CLAUDE.md`; `src/lexer.rs`, `src/parser.rs`; `/tmp`, `/private/tmp` and the
    session scratchpad. `docs/spec/` and golden files were not edited.
  - Changes:
    - `src/parse/loader.rs` (new): the `Loader` trait (`current_dir`, `canonicalize`, `kind`,
      `read`, `read_dir`), `FileKind`, `FsLoader` (Cargo feature `fs`, on by default) and
      `MemoryLoader`.
    - `src/parse/include.rs` (new): `.include`, `.try_include` and `.includes` with every
      parameter, the search path, the `try`/`.try_include` matrix of the oracle runs, nesting
      under a key, and `.load` (feature `load`, off by default; without it `.load` is
      unsupported). `src/parse/glob.rs` (new): glob patterns as the oracle matches them (#29).
    - `src/parse/core.rs`: an included file is an input unit with a `Core` of its own that
      takes over the container stack, tree and comments and hands them back; frames record
      their unit; an included file's leading `{` takes over the object's brace, and its `}`
      keeps root and braced objects open but closes section objects (#32); the end-of-unit
      checks; section objects of both kinds close together.
    - `src/parse/macros.rs`: the include macros run before what follows them is checked
      (goal 6, QUESTIONS.md #23), so `a = 1⏎.try_include()#` stops silently; argument lists may
      include files, and a stop there makes the macro fail; their `CURDIR` is the base
      directory.
    - `src/parse/vars.rs`: the expander owns its variables; an included file sets `FILENAME`
      and `CURDIR`, which move to the end of the lookup order (#28) and are restored after it,
      or kept under `NO_FILEVARS`.
    - `src/parse/comments.rs`: comments are collected across units, each with its text and
      position in its own input.
    - `src/parse/error.rs`: `Error::file`, `Error::is_stopped`, `Error::partial`,
      `Error::into_partial`; kinds `FileNotFound`, `NotAFile`, `IncludeSelf`,
      `IncludeTooDeep`, `IncludeArrayRoot`, `IncludeTargetNotObject`, `UrlNotSupported`,
      `LoadKeyMissing`, `LoadKeyExists`, `Stopped`, `StoppedInArguments`. `Error` is no longer
      `Eq` (it may hold the partial tree).
    - `src/parse/mod.rs`: `Parser::set_loader`, `Parser::set_base_dir`, `Parser::base_dir`;
      `parse` uses the base directory for `CURDIR`; `parse_file` reads through the loader and
      defines `FILENAME` and `CURDIR` whatever `NO_FILEVARS` says (decision 5);
      `MAX_INCLUDE_DEPTH` = 16; re-exports of the loader types.
    - `src/error.rs`: `UclError::Stopped`, and a `From<parse::Error>` that maps stops there
      (decision 4). `src/value.rs`: `Slot::collection` (crate-internal).
    - `Cargo.toml`: features `fs` (default) and `load`; the crate is its own dev-dependency with
      `load`, so plain `cargo test` builds and tests the core with `.load` while `cargo build`
      leaves it off (checked with a temporary `compile_error!`).
    - `tests/conformance.rs`: the case's directory is the parser's base directory (the process
      working directory is never changed); cases with `string-input` or `no-filevars` are
      parsed as bytes, the others as files; a stop is compared through its partial tree;
      entries with reason `divergence:signature` must fail with the unsupported error; an
      unsupported outcome is hinted `unsupported`.
    - `xfail-new.txt`: 227 `macro` entries removed; `includes_like_include` and
      `include_sign_param_no_effect` relabelled `divergence:signature`; header rewritten.
    - Choices: `url=true` with `://` is an error with `try=true` and for `.try_include` too
      (decision 2 as written; the oracle skips there). A stop inside a file included by a glob
      or a search path ends the parse (#30). A macro right after a section name keeps the
      section object open for later entries (#31).
    - `QUESTIONS.md` #28–#33.
  - Result: `xfail-new.txt` has 3 entries: 2 `divergence:signature`, 1 `non-utf8`; none has
    reason `macro`. New core: 1143 cases, 1140 pass. `xfail.txt` is unchanged (old parser: 450
    pass, 693 expected failures). `cargo test`: 476 tests pass. `cargo build --examples
    --benches` builds; `cargo build --no-default-features` and `--features load` build.
  - Commits: the `C3b:` commits `2d339c3`, `f18d87c` and the one that adds this entry.
  - Attestation: I did not read libucl source code or any forbidden input listed in
    docs/clean-room/PROTOCOL.md.
- 2026-09-23 — Role: spec team. Item: C0, release `spec-v5` (tag on `e2abbea`). Answers implementer
  questions #23–#33 from C3a and C3b, and specifies URL includes in every mode (WORKLIST C3,
  decision 2). Inputs consulted: libucl source at the pinned commit (macro value and argument
  parsing, the include, load and inherit handlers, object copying, element insertion and implicit
  arrays, key parsing and the section-name lookahead, container opening and closing, the end-of-input
  check, variable registration, URL fetching without URL support), black-box oracle runs, the
  conformance corpus, and `spec-v4`. Confirmed and written down: #23 (§9.2), #24 (§9.7, §12.5), #25
  (§8.5), #28 (§9.4, §7.1), #29 (§9.4 *Globs*, the oracle C library's rules), #32 (§9.4), #33
  (§9.4, §9.6, §12.5; one correction: under `try=false` a glob match that is the including file is
  still skipped, and only an include of no match at all is an error). Corrected: #27 (§12.1, a
  replacing value gives the entry its spelling). Specified: #31 (§9.1, a key with a separator after
  such a macro is a name; comments to the end reopen the value created most recently). Marked
  **Uncertain** (undefined in libucl), with the project's choice where the implementer made one:
  argument documents nested until libucl crashes (#26; the 64-document limit is a project
  divergence), any failure inside an included file after which libucl goes on with the same macro
  (#30; crash; an error fails, a stop ends the parse), a non-object value reopened after a macro
  following a name (#31), a `}` in a key-nested file under an object with its own bracket (#32,
  crash), and which unit a container of an ended unit belongs to at the end-of-unit check (#32,
  memory reuse). README: *Known gaps* no longer lists open decisions; *Divergences decided by the
  project* records the WORKLIST C3 decisions; the Uncertain list and the coverage table (1254
  cases) are updated. 111 new cases with golden files from the oracle; no existing golden file
  changed. The new core fails 12 of them (11 `spec-v5`, 1 `divergence:argument-depth`); the old
  parser fails 91. Cases that crash the oracle are not committed. Both runners green at every
  commit. Commits: `71c7dd4`, `8c9eb8e`, `e2abbea`. The spec contains behaviour only: no libucl
  code, internal names, or procedures.
- 2026-09-23 — **Implementation team, C3c (the new parser core brought up to `spec-v5`).**
  - Inputs consulted:
    - `docs/clean-room/PROTOCOL.md`, `WORKLIST.md` (C3 decisions, decision 2 as amended),
      `LOG.md` (the C3b and `spec-v5` entries), `QUESTIONS.md` (#23–#33 in full);
    - spec `spec-v5`, read from the working tree after checking that
      `git diff --stat spec-v5 HEAD -- docs/spec/` is empty: `git diff spec-v4 spec-v5 --
      docs/spec/` in full; §9.1, §9.4 (all subsections), §3.4, §12.5, §8.4; single lines of §1.1,
      §1.6 and §3.1 found with `grep`;
    - `tests/conformance/xfail-new.txt`, the header of `tests/conformance.rs`; the inputs and
      golden files of the 11 `spec-v5` entries, of `cases/spec/03-keys/section_path_left_open_*`
      and of `cases/spec/09-macros/comments_include_key_*`, `comments_end_of_included_file`,
      `comments_carry_into_included_file`; the helper files under `cases/spec/09-macros/files/`
      (copied to `target/c3c/p/files/`) and the first byte of the helper files there and under
      `libucl/basic/`;
    - the crate's own code: `src/parse/core.rs`, `macros.rs`, `include.rs`, `comments.rs`,
      `error.rs`, parts of `mod.rs` and `src/value.rs`; my C3b scratch differential tool, copied
      to `target/c3c/difftool/` and changed (the skip of macros after a name removed, oracle
      crashes counted apart and not compared, a two-unit mode in which part of the input becomes
      an included file, generators for name runs and for two-unit include inputs), a runner
      `target/c3c/run.sh` and a bucketing script `target/c3c/triage.py` (implementation-team
      code, not committed);
    - black-box runs of `target/libucl-oracle/ucl-dump`, stderr discarded: about 300 probes on
      scratch inputs and on 85 helper files of my own under `target/c3c/p/files/c3c/`, options
      `-c`, `-l`, `-I`, `-s`, `-v FILE=f`; differential runs (below);
    - general Rust documentation (general knowledge); `cargo build`, `cargo test`,
      `cargo clippy`, `rustfmt` output.
  - Seen without opening the files:
    - the worktree's `CLAUDE.md` was loaded into the session automatically; its text is the
      clean version embedded in the agent definition. The session's git status listed
      `PLAN.md`, `REVIEW.md` and `.claude/` as untracked and `CLAUDE.md` as modified in the main
      checkout, by name;
    - the header of `tests/conformance.rs` names `PLAN.md` P0.5; not followed;
    - `cargo clippy` printed two code lines of a pre-existing file outside `src/parse/`
      (`if self.nesting_depth == 0 {`, `return Err(LexError::UnexpectedCharacter {`, so
      `src/lexer.rs`); the file was not opened and nothing from it was used;
    - the harness writes background-command output under `/private/tmp`; those files were not
      opened (the runs write their own summaries under `target/c3c/runs/`).
  - Not consulted: libucl source; anything under `target/libucl-oracle/` other than running
    `ucl-dump`; `scripts/regen-golden.sh` (neither read nor run); `REVIEW.md`, `PLAN.md`,
    `PROGRESS.md`; `quarantine/*`; history of `src/` before `ef8007e`; history of `CLAUDE.md`;
    `src/lexer.rs`, `src/parser.rs`; `/tmp`, `/private/tmp` and the session scratchpad.
    `docs/spec/` and golden files were not edited.
  - Changes (spec-v5):
    - `include.rs`: a `url=true` include with `://` is decided before the search path, globs
      and key nesting: skipped with `try` (and for `.try_include`, not a silent stop), an error
      without; its `path` list still takes effect (#37). `.try_include(glob=true, try=false)`
      skips a match that is the including file and fails when it included no match (#33).
    - `core.rs`: the object a key-nested file goes into shares a brace taken over by the object
      where the macro stands (#39). A macro directly after a name starts a name run: the next key
      read in the unit counts as a word after a name, a name when a separator follows (§9.1,
      #38). At the end of a unit where whitespace and at least one comment followed the run's
      last macro, the value created most recently is reopened as a left-open object of that
      unit (#34–#36); for this the core keeps the value created most recently, by path, while a
      name run is in effect, also without `save-comments`. The block-comment scanner is shared
      by the skipper and that check. #27 (key spelling) needed no change.
  - Changes (differences found by the differential runs): the end-of-unit check stops at the
    first container any other unit opened, as §9.4 says (#41); a section object whose brace a
    file took over closes with a bracketed container opened in it (#40); the first key of a file
    whose leading `{` took over a brace still held gives its first name's object a share (#42);
    a file of zero bytes attaches no pending comments at its end (#43); under `merge` the `null`
    that ends a unit merges into a container first value (#44); a name whose bracket is in a
    comment after VT or FF lets the next name come on a later line, and the end of input keeps
    the objects (#46); after a macro whose file closed the braced root only whitespace and `;`
    may follow (#47, new `ErrorKind::AfterRootClosedByInclude`); saved comments follow a value
    that `target="array"` moves into a new array (§12.5). Unit tests for all of these.
  - Choices where the spec leaves the behaviour Uncertain or it crashes the oracle: the
    comment-to-end quirk leaves a value that is not an object alone (§9.1); a container of an
    ended unit counts as opened by a later unit at the same include depth (§9.4, unchanged); an
    included file starting with `[` is an error at the `[` (#45); a key-nested file's `}` under
    an object with only its own bracket is an error (§9.4, unchanged); an array element closed
    by an included file leaves the array open for the including unit (#48).
  - Differential runs: 83 runs, flags none, `-c`, `-l`, `-I`, `-s merge|rewrite|error`, `-F`,
    `-S`, `-l -I -c`; generators for name runs, single unit and two units (a name directly before
    `.priority`, `.inherit`, `.include` with and without `try`, `glob`, `key`, `prefix`,
    `target="array"` and URLs, `.try_include` of missing files, with `try=false` and with URLs,
    `.load` and `.foo`), two-unit includes, and the C3a/C3b generators (the include generator
    with bare names added), the C3b flag matrix for includes included. Each run as last made:
    476,713 inputs generated (runs that share a seed and generator but differ in flags reuse
    the same inputs); 707 crashed the oracle and were not compared; 476,006 compared, with no
    macro-after-name skip (as in C3b, an input whose oracle result holds non-UTF-8 bytes, or that
    the crate rejects as unsupported, counts as no difference). 32 differences, all bucketed by
    hand: 25 an included file starting with `[` that holds an object (#45; the same files crash
    the oracle at the top level), 6 comments of a value replaced by a higher priority
    reappearing on a later value (§12.5, Uncertain), 1 a VT before a block comment between array
    elements (#49, §1.5, not a macro behaviour, left unchanged). One earlier run also showed 2
    differences of the §9.4 Uncertain unit case, which the oracle did not reproduce under a
    different file name. The first campaign, before the changes above that came from it, found
    141 differences; the extended generators then found the lost comments of a value that
    `target="array"` moves (fixed, below), and the `-c` runs were made again after that fix.
  - Questions: `QUESTIONS.md` #34–#49.
  - Result: `xfail-new.txt` has 4 entries: 2 `divergence:signature`, 1
    `divergence:argument-depth`, 1 `non-utf8`; the 11 `spec-v5` entries are removed. New core:
    1254 cases, 1250 pass. Existing parser: 470 pass, 784 expected failures; `xfail.txt` is
    unchanged. `cargo test`: 481 tests pass. `cargo build --examples --benches`,
    `cargo check --no-default-features` and `cargo check --features load` succeed.
  - Commits: the `C3c:` commits `a028700`, `e7fe4cc`, `5acdf73`, `05ed763`, and those that add
    and amend this entry.
  - Attestation: I did not read libucl source code or any forbidden input listed in
    docs/clean-room/PROTOCOL.md.
- 2026-09-23 — Role: spec team. Item: C0, release `spec-v6` (tag on `c38da8e`). Answers implementer
  questions #34–#49 from C3c, and audits §10 for the C4 output work. Inputs consulted: libucl
  source at the pinned commit (the emitter and its string, number and comment writers, the
  character table, the key-escape and string-origin marks, the parser's key, macro and
  after-value handling, container opening and closing, the end-of-unit check, the include handler's
  URL, search-path and key-nesting code, empty-chunk handling), black-box oracle runs, the
  conformance corpus, and `spec-v5`. Confirmed and written down: #34, #35, #36, #38 (§9.1), #37
  (§9.4 URLs), #39 (§9.4 *Nesting under a key*), #40, #41, #42, #47 (§9.4 *Where the entries go*,
  *The check at the end of a unit*), #43 (§9.4, §12.5), #44 (§8.4), #46 (§3.4). Specified: #49
  (§1.5). Marked **Uncertain** with the crate's behaviour recorded as the project's choice: #45 (an
  included file starting with `[`) and #48 (a `}` in an included file closing an array element).
  §10: the output of multi-value entries with per-value keys, the empty key, the exact digits of
  floats and the `%.15g` form, nested layouts, the inline layout for every first-value kind, what
  libucl reads back and which forms read back exactly (§10.8, found by reading back the output of
  every case), and the config output with saved comments (§10.10). Oracle tool: output written by
  length, so NUL bytes are kept, and a `config-comments` format. Regen script: every case that
  parses gets `<case>.config.golden`, `.json.golden`, `.json-compact.golden` and `.yaml.golden`,
  and cases that save comments `<case>.config-comments.golden`; two regenerations gave identical
  files, and no golden file contains the checkout path. 121 new cases (1375 in all); no existing
  golden file changed. The new core fails 4 of them (`spec-v6`, #49); the old parser fails 89.
  Both runners and full `cargo test` green at every commit. Commits: `1e2604a`, `c38da8e`. The spec
  contains behaviour only: no libucl code, internal names, or procedures.

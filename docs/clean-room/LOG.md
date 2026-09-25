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
- 2026-09-23 — Role: implementation team. Item: C4a, the output emitters (spec-v6 §10) and the
  parse-side fix of §1.5 (QUESTIONS.md #49).
  - Inputs consulted: `docs/spec/` at `spec-v6` (§10 in full, §1.5, §8.4, §9.4 *Nesting under a
    key*, §9.6, §12.5, the README) and `git diff spec-v5 spec-v6 -- docs/spec/`;
    `docs/clean-room/` (PROTOCOL, WORKLIST with the C4 decisions, QUESTIONS #34–#49, LOG); the
    conformance cases, their `.flags`, golden files and `.res` files, `tests/conformance/README.md`
    and `tests/conformance.rs`; the crate's own code (`src/value.rs`, `src/parse/`); black-box
    runs of `target/libucl-oracle/ucl-dump` (`-e config|json|json-compact|yaml|config-comments`
    with `-l`, `-I`, `-C`, `-s`); Rust standard library documentation. The source of
    `scripts/regen-golden.sh` and of `tools/ucl-dump` was not opened.
  - `src/emit/` (new): `Format` (`Json`, `JsonCompact`, `Config`, `Yaml`), `Emitter` with
    `with_facts` and `with_comments` (saved comments are written only when asked for, in the
    config format, decision C4.1), `emit`, the free functions `to_json`, `to_json_compact`,
    `to_config`, `to_yaml`, and `key_needs_quoting`. Floats follow §10.3 through Rust's exact
    formatting (ties to even), `%.15g` built from the rounded exponent.
  - `src/parse/facts.rs` (new): `OutputFacts` and `ValueFacts`, the output facts of §10.1 by value
    path, recorded by the core for every parse (`Parser::output_facts`, `Parser::emitter`). Only
    facts that differ from the emitter's defaults are stored. They follow values the way saved
    comments do: replaced by priority or `rewrite`, collected under `no-implicit-arrays`, moved
    by `.include(key, target="array")`, copied by `.inherit` (taken before the copy), the
    `merge` scalar that takes a container's place, and included units; `.load` records
    `multiline=true`.
  - §1.5: after an element, a VT or FF makes the next element be read like the first. The 4
    `spec-v6` entries of `xfail-new.txt` pass and are removed.
  - Runner: `libucl_conformance_emitters` compares, for every case that parses, the four formats
    with `<case>.<format>.golden`, config with comments with `<case>.config-comments.golden`
    (cases with `save-comments` or `dump-comments`), and for the 25 upstream cases with a `.res`
    the two passes of §10.9 (case 14 from another directory); it asserts that every non-error
    case has its output golden files and no error case has any. Known failures in the new
    `tests/conformance/xfail-emit.txt`: the 4 cases the core does not parse (2
    `divergence:signature`, 1 `divergence:argument-depth`, 1 `divergence:non-utf8`, the last with
    its `.res`).
  - Behaviour chosen from oracle runs where §10 is silent: QUESTIONS.md #50 (layout and key of a
    scalar that takes a container's place under `merge`; the golden files of two §8 cases show
    it), #51 (keys of `no-implicit-arrays` collections and of arrays that
    `.include(target="array")` builds), #52 (empty root array).
  - Differential runs against the oracle, every format and config with comments: about 35
    hand-written inputs on the places where facts move, then every case without a `.flags`
    file (1212) under the flag sets `-C`, `-l`, `-I`, `-s merge`, `-C -I`, `-C -s merge`,
    `-C -l`, `-s rewrite`, `-I -s merge` and `-C -I -l` (60,600 comparisons). Differences left:
    the 4 divergence cases; 3 cases whose input ends with a block comment, after whose `*/`
    libucl writes a NUL byte in config-comments output (§12.5 *Uncertain*: the core saves no
    byte there); and under `rewrite`, comments of a replaced value reappearing on a later value
    (7 cases; §12.5 says no case pins this; comment attachment, not output). One difference
    found by the runs was fixed: the key of the array `.include(key, target="array")` builds.
    The facts are kept in a map ordered by path, so that a value's facts are one range: with a
    scan of every fact per replacement, 20,000 single-quoted keys overridden by `.priority` took
    2.4 s, now 0.04 s. After that change the sweep was repeated under 8 of the flag sets (48,480
    comparisons) with the same differences.
  - Result: emitters 1023 cases with output, 1019 match in every format (config 1019/1019,
    json 1019, json-compact 1019, yaml 1019, config-comments 53/53, `.res` 24/24 compared), 4
    expected failures. New core: 1375 cases, 1371 pass, `xfail-new.txt` has 4 entries. Existing
    parser: 502 pass, 873 expected failures, `xfail.txt` unchanged. `cargo test`: 499 tests pass.
    `cargo build --examples --benches`, `cargo check --no-default-features` and
    `cargo check --features load` succeed.
  - Not done here: `tests/conformance/README.md` still says the runner does not compare the output
    golden files; that file belongs to the spec team.
  - Commits: `8dc679d` (emitters, facts, §1.5, runner, this entry) and the `C4a:` commit after
    it, in which the runner accepts only cases the core does not parse in `xfail-emit.txt`.
  - Attestation: I did not read libucl source code or any forbidden input listed in
    docs/clean-room/PROTOCOL.md.
- 2026-09-23 — Role: implementation team. Item: C4b, serde serialization (WORKLIST C4 item 2,
  spec-v6 §10.8).
  - Inputs consulted: `docs/spec/` at `spec-v6` (checked identical to the tag: §10 in full,
    §1.1, §3.1–§3.2, §4, §5, §6, §7, §11, the README); `docs/clean-room/` (PROTOCOL, WORKLIST with
    the C4 decisions, QUESTIONS #48–#52 and the table format, LOG); `tests/conformance/README.md`, `tests/conformance.rs`
    (dump schema and comparison, copied into the new test) and the inputs of the `10-output`
    read-back cases; the crate's own code (`src/de.rs`, `src/time.rs`, `src/value.rs`,
    `src/error.rs`, `src/emit/`, the public API of `src/parse/`); black-box runs of
    `target/libucl-oracle/ucl-dump` (typed dump, `-e config`, `-v`) on probe inputs under
    `target/c4b/` and on the serializer's output; Rust standard library and serde documentation.
    The source of `scripts/regen-golden.sh` and of `tools/ucl-dump` was not opened; nothing under
    `/tmp` or `/private/tmp` was used.
  - `src/ser/` (new): `to_value`, `to_string` (config), `to_json_string`,
    `to_json_string_compact`, `to_yaml_string`, `to_writer` (config), all
    `Result<_, UclError>`; `Serialize` for `UclValue` and `UclObject`. A private serializer builds
    a `UclValue`; the text is written by a crate-private round-trip mode of the C4a emitters
    (`Emitter::round_trip`, `try_emit`), which keeps libucl's layouts and changes the forms.
    Time values and the values of a multi-value entry travel as newtype structs with private
    names, which other serializers see as an `f64` and a sequence.
  - `src/de/value.rs` (new, split from `src/de.rs`, no parser imports): `from_value`; the value
    deserializer; `Deserialize` for `UclValue` (from this crate's deserializer a time stays a
    time and every value of a multi-value entry is kept) and `UclObject`; map keys parsed into
    integer, `bool`, `char` and float targets; bytes from arrays of integers.
  - `src/time.rs`: `serialize` writes a `Duration` as a time; the `time` tests now parse with the
    new core. `src/error.rs`: `SerdeError::Unrepresentable`, `serde::ser::Error for UclError`.
  - Choices under goal 2, documented in `src/ser/mod.rs`:
    1. Floats: the shortest digits that identify the double, with `.` or an exponent (Rust's
       `Debug` form: `0.1`, `-0.0`, `1e16`); NaN `nan` (sign and payload not kept), +∞ `inf`,
       −∞ `-1e308k` (§10.8). Subnormal floats and times: error (§5.3, no literal reads back).
    2. Times: the float digits followed by `s`; ±∞ `1e308ks` and `-1e308ks` (oracle and core
       read them; QUESTIONS.md #54); NaN time: error. The same forms in JSON, so JSON output with
       a time or a non-finite float is not JSON (documented).
    3. Integers outside `i64` (from `u64`, `i128`, `u128`): error, in `to_value` too.
    4. Strings: double-quoted with the §6.1 escapes (every byte). In the config format a string
       containing `$` is single-quoted, so the output reads back exactly whatever variables the
       reader registers; one that single quotes cannot hold (a backslash, paired from the left,
       before `'`, LF, CR or the end; QUESTIONS.md #53) is an error in the config format. JSON,
       compact JSON and YAML always use double quotes; there a string that refers to `FILENAME`
       or `CURDIR` (`$NAME…`, `${NAME}`), which libucl and the core define by default (§7.8), is
       an error (conservative: also when `$$` would keep it), and variables that the application
       registers when reading are a documented reader precondition, like the reader's flags.
    5. Keys: bare where §3.1 allows (config, YAML), otherwise double-quoted; the empty key: error.
    6. Multi-value entries: one entry per value with the same key in every format (libucl's
       JSON and YAML array form of §10.7 loses values).
    7. Root: must be an object or an array (§1.1), else error; more than 1024 nested containers
       (§11.2): error. `Duration` that no `f64` of seconds holds exactly: error.
    8. Reader assumptions, documented: default flags, the `append` strategy, and for the
       double-quoted formats no registered variable that a string refers to.
  - Tests (`tests/serde_roundtrip.rs`): values compared as typed dumps (entry order, every value,
    floats by bits), not with `PartialEq`. Generated (SplitMix64, fixed seed, `UCL_SERDE_SEED`):
    3000 `UclValue` roots through the core: config 2977 read back exactly and 23 fail as they
    must; JSON, compact JSON and YAML 2500 each read back exactly, 366 fail as they must (file
    variables), 134 are written but not compared (they refer to `ABI`, which the test readers
    register). 600 typed structs (every serde shape, `Duration`, a `UclValue` field): config 575
    exact and 25 errors; the other formats 386 exact, 173 errors, 41 not compared. Oracle (skips
    when the binary is missing): of 400 generated values, the 397 with a config form and the 366
    with a JSON form that the oracle's variables leave alone, and of 200 typed values 194 and
    119, in root objects of 100 entries and a root array, one `ucl-dump` run per document, 32
    documents: all read back exactly. Corpus
    `tests/serde_corpus/`: 13 values, 49 files, each with libucl's typed dump
    (`<file>.golden.json`), checked without the binary (serializer output unchanged, core
    reading and libucl's dump equal the value) and, with it, the dumps checked current;
    `UCL_SERDE_REGEN=1` rewrites them and refuses a dump that differs or contains the checkout
    path. Also error cases, the nesting limit, `from_value` on core-parsed documents (one-or-many,
    object into a sequence, enum shapes, floats into integers, keys into integers), and
    `UclValue` through `serde_json`.
  - Result: `cargo test` 517 tests pass. Conformance unchanged: new core 1371 of 1375,
    emitters 1019 of 1023, existing parser 502; no xfail list changed.
    `cargo build --examples --benches`, `cargo check --no-default-features` and
    `cargo check --features load` succeed.
  - Found and not changed: the core keeps a backslash and a lone CR inside single quotes, where
    libucl removes both (QUESTIONS.md #53; §6.2 says CR LF only and no case pins it).
  - Questions: #53, #54.
  - Commits: `d66f3c7`, and the `C4b:` commit after it, in which strings that refer to
    `FILENAME` or `CURDIR` are an error in the double-quoted formats instead of a documented
    exception.
  - Attestation: I did not read libucl source code or any forbidden input listed in
    docs/clean-room/PROTOCOL.md.
- 2026-09-23 — Role: spec team. Item: C0, release `spec-v7` (tag on `6a8b1d9`). Answers implementer
  questions #50–#54 from C4a and C4b. Inputs consulted: libucl source at the pinned commit (the
  parser's single-quote lexer and unescape, number and suffix parsing, atom parsing of `null`,
  `nan` and `inf`, the duplicate-key handling under `merge` and `no-implicit-arrays`, the key
  parser's quoting mark, the include handler's `target="array"` conversion, and the emitter's
  key and array writers), black-box oracle runs, the conformance corpus, and `spec-v6`.
  Confirmed and written down: #50 (§10.7, §8.4; the layout rule extended to `nan`, `inf`, strings
  and `.inherit` copies, and the scalar keeps the container's key), #51 (b) (§10.1), #52 (§10.4–
  §10.6), #54 (§5.4 time-suffix overflow to ±∞, `ms` giving subnormal times, no NaN time; §10.8
  forms). Corrected: #51 (a), the key of a `no-implicit-arrays` collection array is never quoted;
  #53 (a), a backslash before a lone CR is removed with it (§6.2, plus the pairing rule); #53 (b),
  the exact single-quote read-back condition (§10.8). Also: the conformance README now describes
  the three runners, the three known-failure lists and the serde corpus; spec README *Known gaps*
  and *Divergences decided by the project* carry the C4 decisions. 15 new cases (1390 in all),
  with typed and output golden files; no existing golden file changed. The new core and its
  emitters pass all 15; the old parser fails 12 (`xfail.txt`). Held back, not committed, because
  the new core writes their output differently and `xfail-emit.txt` cannot list them:
  `sq_line_continuation_lone_cr` (§6.2, #53 a), `no_implicit_arrays_collection_key_bare` (§10.1,
  #51 a) and `multi_value_merge_quirk_layout_nan_inf` (§10.7, #50). Both runners and full
  `cargo test` green at every commit. Commits: `f4c8fac`, `6a8b1d9`. The spec contains behaviour
  only: no libucl code, internal names, or procedures.
- 2026-09-23 — Role: implementation team. Item: C4c, the core, emitters and serde serializer
  brought up to `spec-v7` and to WORKLIST C4 decision 2 (serde JSON output is valid JSON).
  - Inputs consulted: `docs/spec/` at `spec-v7` (checked identical to the tag; `git diff spec-v6
    spec-v7 -- docs/spec/` in full, and §10 in full); `docs/clean-room/` (PROTOCOL, WORKLIST with
    the C4 decisions, QUESTIONS #50–#54 and their answers, LOG); `tests/conformance/README.md`,
    `tests/conformance/pending/` and its README, the xfail lists and their headers; the crate's
    own code (`src/emit/`, `src/ser/`, `src/time.rs`, `src/lib.rs`, `src/parse/core.rs`,
    `src/parse/string.rs`, `src/parse/facts.rs`, `src/parse/number.rs`, `src/value.rs`,
    `tests/serde_roundtrip.rs`); black-box runs of `target/libucl-oracle/ucl-dump` (typed dump,
    `-e config|json|yaml`, `-I`, `-s merge`) on probe inputs under `target/c4c/`, and through
    `UCL_SERDE_REGEN=1` and the oracle tests; Rust standard library, serde and serde_json
    documentation. The source of `scripts/regen-golden.sh` and of `tools/ucl-dump` was not opened;
    nothing under `/tmp` or `/private/tmp` was used; `src/lexer.rs` and `src/parser.rs` were not
    read.
  - Core (`src/parse/`):
    1. §6.2 (#53): in single quotes, a backslash before a CR that no LF follows is removed with
       the CR, and a further CR stays (`string::single_quoted`).
    2. §10.1 (#51): when repeated keys make a `no-implicit-arrays` collection, its key is
       recorded as never needing quoting (`Core::collection_key`), also when no other facts
       exist; a scalar that replaces the collection under `merge` keeps that key (oracle:
       `"x y" = 1⏎"x y" = 2⏎"x y" = 3` with `-I -s merge` → `x y = 3;`, and `"a=b" = [1]⏎"a=b" = 2`
       → `"a=b" = 2;`).
    3. §10.7 (#50): a value's origin records whether it is a keyword (§4.5); under the §8.4 merge
       quirk `nan` and `inf` follow their own kind (inline), while a float written with digits,
       also `1e308k`, keeps the replaced container's layout. The flag is not a recorded fact.
  - Serializer (`src/emit/number.rs`, `src/emit/mod.rs`, `src/ser/`):
    1. §5.4, §10.8 (#54): time ±∞ stays `1e308ks`/`-1e308ks`, a NaN time an error. A subnormal
       time is written as a normal float followed by `ms` whose quotient by 1000 is the time: the
       time's shortest digits with the exponent raised by 3 when they give it (`1e-307ms`),
       otherwise time × 1000, or the smallest normal float for ±`MIN_POSITIVE / 1000`
       (`2.2250738585072014e-308ms`); a time closer to zero than `2.2250738585069563e-311` is an
       error. libucl reads the committed forms back exactly (corpus `times.ucl`, `times.yaml`) and
       the generated ones in the oracle tests.
    2. Decision 2: in JSON and compact JSON a float is a JSON number, a time its seconds as a JSON
       number, and a NaN or infinite float or time an error; a subnormal time is an error there,
       like a subnormal float. YAML and config keep the C4b forms. Repeated member names for a
       multi-value entry are unchanged (RFC 8259 grammar allows them).
  - Tests: `tests/serde_roundtrip.rs` compares JSON readings with times as floats
    (`expected_reading`); generators give subnormal times (writable and not) and, for every other
    value or sample, no value that JSON cannot write; new test `json_output_is_json` parses the
    JSON and compact JSON of the 3000 generated values and 600 typed values with `serde_json`
    (dev-dependency feature `float_roundtrip`) into an order-preserving tree that keeps repeated
    members, and compares it with the value. Unit tests for the three core rules and the forms.
  - Corpus: `UCL_SERDE_REGEN=1`; changed only `*.json`, `*.compact.json` and their dumps (times as
    seconds; `floats`, `times`, `typed_sample`, `typed_enums` without NaN and infinities in the
    JSON files), and `times.ucl`, `times.yaml` with their dumps (new `subnormal` entry). Still 49
    files. Documented in `tests/conformance/README.md`, `src/ser/mod.rs`, `src/time.rs`,
    `src/lib.rs`.
  - Pending cases: `sq_line_continuation_lone_cr`, `no_implicit_arrays_collection_key_bare` and
    `multi_value_merge_quirk_layout_nan_inf` moved unchanged to `cases/spec/06-strings/` and
    `cases/spec/10-output/`; `pending/` deleted. The existing parser fails all three, so they were
    added to `xfail.txt` with the cases (reasons `parser`, `flags`, `flags`), as its header allows
    for new cases; `xfail-new.txt` and `xfail-emit.txt` unchanged. spec README *Known gaps* still
    says these rules have no committed case (spec team).
  - Result: `cargo test` 519 tests pass. Conformance: new core 1389 of 1393 (4 listed
    divergences), emitters 1037 of 1041 cases with output (config, JSON, compact JSON, YAML 1037
    of 1037 each, config-comments 53 of 53, `.res` 24 of 24), existing parser 505 of 1393.
    Round trips through the core, generated values (config, JSON, compact, YAML): exact
    2961/2351/2351/2515, errors as required 39/541/541/357, not compared 0/108/108/128; typed:
    exact 568/325/325/387, errors 32/263/263/167, not compared 0/12/12/46. serde_json reads 2459
    values and 337 typed values in each JSON format as written. Oracle: config 396, JSON 349,
    YAML 361 values plus a root array of 100 each, typed 187/109/128 plus 100; all read back
    exactly. `cargo build --examples --benches`, `cargo check --no-default-features` and
    `cargo check --features load` succeed.
  - Questions: none.
  - Commits: `8282ffd`, `3336006` (JSON error messages for NaN and subnormal times, docs), and
    the `C4c:` commit after them, which records these commits here.
  - Attestation: I did not read libucl source code or any forbidden input listed in
    docs/clean-room/PROTOCOL.md.
- 2026-09-23 — Role: implementation team. Item: C5, the cut-over. **Stopped after an exposure;
  not completed, nothing committed.**
  - Exposure to `src/lexer.rs` contents (the C5 brief forbids opening `src/lexer.rs` and
    `src/parser.rs`): a recursive grep for the names of the crate's entry points, run over `src/`
    without excluding the two files, printed three lines of `src/lexer.rs`: the signatures of the
    two streaming-lexer entry points that `src/lib.rs` re-exports, and one line of the body of
    one of them. A later `grep -c` over the two files printed only match counts. Nothing from
    them is used in the new code; both functions were to be deleted in this item.
  - Exposure to forbidden inputs (fork porting `tests/integration_tests.rs` and
    `tests/performance_tests.rs`): after both files were ported and verified, a grep for the
    name of the renamed test file ran over the whole worktree and printed one line of
    `PROGRESS.md` and three lines of `PLAN.md`: planning and process notes about how the
    `cases/migrated/` inputs were captured and about the handling of three test and example
    files. None of the lines names libucl internals. All code of that fork was written before
    the grep; afterwards it wrote only its notes under `target/c5/` and a note here, which this
    entry replaces because it reproduced the lines.
  - Exposure of the session lead (the participant writing this entry): the fork's report and
    its note here reproduced those lines, and the lead read them. `PLAN.md`, `PROGRESS.md` and
    `REVIEW.md` are tracked files in the worktree root, so any search over `.` reaches them;
    later participants must search explicit directories only. The lead stopped at that point
    and afterwards wrote only this record and stop messages to the forks still running.
  - State of the worktree at the stop, all uncommitted:
    1. Written by the lead before the exposure: `src/parse/builder.rs` (new, `ParserBuilder`),
       the cut-over of `src/de.rs` (`from_str`, `from_slice`, `from_reader`, `from_file` and
       `UclDeserializer` on the new core; the old `from_str_with_*` functions left on the old
       parser as a transition), `src/parse/mod.rs` (the builder, `Parser::builder`,
       `read_file`/`parse_read_file`, module docs), `src/error.rs` (`UclError::parse_error`,
       `UclError::position`), `src/lib.rs` (crate docs, re-exports), and new benches
       `benches/common/mod.rs`, `benches/parse_benchmarks.rs`, `benches/emit_benchmarks.rs`,
       `benches/serde_benchmarks.rs` (not yet run, not in `Cargo.toml`).
    2. A temporary input-recording hook in `src/parse/mod.rs`, between `C5-PROBE-HOOK begin`
       and `C5-PROBE-HOOK end`. It must be removed and never committed.
    3. Completed by forks that were not exposed: `tests/bare_word_tests.rs`,
       `tests/heredoc_tests.rs`, `tests/nginx_syntax_tests.rs`, `tests/implicit_array_tests.rs`
       (notes in `target/c5/notes/A.md`); `tests/unicode_escape_tests.rs`,
       `tests/cpp_comment_tests.rs`, `tests/real_world_ucl_configs.rs` (notes in
       `target/c5/notes/B.md`); `tests/integration_tests.rs` and `tests/performance_tests.rs`
       renamed to `tests/generated_documents.rs` (fork C, code written before its exposure;
       notes in `target/c5/notes/C.md`).
    4. Stopped mid-edit by the lead's stop message; nothing below was reviewed. Each of the
       three example forks reports that it searched no path outside its files, `docs/spec/`,
       `src/emit/`, `src/parse/facts.rs` and the oracle, and opened no forbidden file.
       - Rewritten, built and run with exit 0: `examples/basic_usage.rs`,
         `examples/complete_ucl_syntax.rs`, `examples/web_server_config.rs`,
         `examples/framework_integration.rs`, `examples/nginx_style_framework_integration.rs`.
       - Rewritten, then one literal changed and not rebuilt: `examples/number_parsing.rs`.
       - Rewritten, not built or run: `examples/advanced_features.rs`,
         `examples/real_world_configurations.rs`.
       - Unchanged (still on the old API): `examples/configuration_management.rs`,
         `examples/real_world_usage.rs`.
       - Not run through `rustfmt`; the README descriptions for `examples/README.md` were not
         written. Scratch oracle inputs are under `target/c5/ex-D/`, `target/c5/ex-E/` and
         `target/c5/ex-F/`.
    5. Tooling under `target/c5/`: a differential probe crate (`target/c5/probe`) comparing the
       new core's typed dump with the oracle's on inputs the hook records, and a crate that
       writes the bench documents (`target/c5/benchgen`).
  - Inputs consulted before the stop: `docs/spec/` at `spec-v7` (README, §7), `docs/clean-room/`
    (PROTOCOL, WORKLIST, QUESTIONS, LOG), `tests/conformance/README.md` and `tests/conformance.rs`,
    the crate's own code other than the two old files, tests, examples and benches, and black-box
    runs of `target/libucl-oracle/ucl-dump`.
  - Attestation: I did not read libucl source code. I was exposed to the forbidden inputs
    recorded above, as described.
- 2026-09-23 — Role: session lead (oracle side). Item: C5 handover after the exposure above.
  - Decision: the uncommitted work listed in items 1, 3 and 4 of the entry above stands. Per
    PROTOCOL.md only code written after an exposure is quarantined; the lead and fork C wrote
    their code before their exposures, the other forks were not exposed, and the exposed lines
    were process notes about test capture, not libucl internals. The participant is replaced by a
    new implementer, who reviews that work before committing it.
  - `target/c5/notes/C.md` was written by fork C after its exposure and was removed from the
    worktree unread by the implementation team.
  - `REVIEW.md`, `PLAN.md` and `PROGRESS.md` are removed from the working tree and kept only on
    branch `quarantine/oracle-notes`, so that searches of the tree cannot reach them. References
    to them in `scripts/`, `tools/`, `tests/conformance/README.md` and `PROTOCOL.md` are
    reworded; references in `src/` and `tests/*.rs` are left to the implementer.
  - `.claude/agents/clean-implementer.md` gains a search rule (named directories only, old
    implementation excluded) and a rule that sub-agents receive the forbidden list verbatim.
- 2026-09-25 — Role: implementation team (new participant, replacing the implementer of the
  stopped C5 session). Item: C5a, the public API, tests, examples and benches on the new core,
  finishing the stopped session's uncommitted work (WORKLIST C5, project decision 1).
  - Inputs consulted: `docs/spec/` at `spec-v7` (checked identical to the tag; README
    *Divergences decided by the project*, §5.5, §6.1, §6.3, §7, §9.6, §12.3, §12.6);
    `docs/clean-room/` (PROTOCOL, WORKLIST with the C5 decision, QUESTIONS, the two C5 entries of
    this log); `target/c5/notes/A.md` and `B.md`; `tests/conformance/README.md`, case file names
    and golden files; the serde corpus; the crate's own code other than `src/lexer.rs` and
    `src/parser.rs`, including `git show` of `src/de.rs` at `HEAD` (after `ef8007e`) and of the test,
    example and bench files at `cb51df2`; black-box runs of `target/libucl-oracle/ucl-dump`
    (typed dump, `-e config|config-comments|json|json-compact|yaml`, `-C`, `-H`, `-I`, `-l`,
    `-T`, `-s`, `-v`) on inputs under `target/c5a/`; Rust, serde and criterion documentation.
    Not used: `target/c5/probe-C.txt`, `target/c5/rec-C`, anything under `/tmp` or
    `/private/tmp`. Searches named `src/` (always excluding `lexer.rs` and `parser.rs`), `tests/`,
    `examples/`, `benches/`, `docs/spec/`, `docs/clean-room/` and `target/c5*` only.
  - Tooling: `target/c5/probe` (the differential probe of the stopped session) and
    `target/c5/benchgen`. The recording hook was removed from `src/parse/mod.rs` before any
    commit and saved to `target/c5/probe-hook.txt`; it was applied to the working tree twice,
    only to record the inputs of the test and example runs, and removed right after each run
    (checked with `grep` before every commit). Compiler output was filtered to drop every line
    that names `src/lexer.rs` or `src/parser.rs`; `cargo fmt` changed neither file (checked with
    `git status`, no diff).
  - Two incidents, neither an exposure of source text: (1) changing the signature of
    `from_str_with_variables` broke two call sites inside the old modules' inline tests; the
    errors were counted, never displayed, and located by bisecting on the count, which showed
    that the old tests call `crate::from_str_with_variables(text, Box<handler>)`; a test-only
    stand-in keeps them compiling. (2) One `cargo test` listing, filtered on the word "error",
    printed the names of two inline tests of the old lexer (test names only, from the test
    harness); later filters also drop lines naming the old modules.
  - Review of the inherited work, and what was wrong:
    1. `src/de.rs` and `src/parse/mod.rs`: the serde entry points parsed with a parser whose
       default loader was the filesystem loader, so text input read files, against decision 1;
       `from_file` resolved includes against the working directory, not the file's directory;
       the module docs recommended `disable-macro` for untrusted input. The old `from_str_with_*`
       functions still ran on the old parser.
    2. Doc examples: three failed. `lib.rs` and `ParserBuilder` combined `disable-macro` with
       variables, which that flag disables (§12.6); `lib.rs` put `# comments` after `30s` and
       `512kb`, which makes those values strings (§5.5).
    3. `lib.rs` still re-exported the old lexer, parser, plugins, hooks and handler types.
    4. Examples: `advanced_features` failed an assertion (the column of a raw line break);
       `complete_ucl_syntax`, `real_world_configurations` and `web_server_config` described the
       filesystem loader as the default, and `web_server_config`'s comment on relative include
       paths no longer held. `configuration_management` and `real_world_usage` were not ported;
       both relied on `${NAME:-default}` and on dotted keys as overrides, which UCL lacks, and
       `real_world_usage` also on `window = 1min }` (a string in libucl, §5.5) and a struct that
       did not fit its document.
    5. Tests: `unicode_escape_tests.rs` cited `"ὤ0"` for an input `"ὠ0"`; otherwise the
       ported files held. Every input they parse was recorded and compared with the oracle: 137
       same, 1 different (the non-UTF-8 divergence, `emoji_pair`).
    6. Benches: correct; the documents they generate were checked with the oracle (4 same; the
       500-deep one is too deep for the probe's JSON reader and was checked directly).
  - Public API after C5a: `from_str`, `from_slice`, `from_reader`, `from_file` (feature `fs`),
    `from_str_with_variables`, `from_str_with_map`, `from_str_with_env`, `from_value`,
    `UclDeserializer` (`new`, `from_slice`, `from_parser`, `parser`, `parser_mut`), `parse::Parser`
    and `parse::ParserBuilder` (`Parser::builder`), the loaders, `UclError` with `parse_error` and
    `position`, the emitters and serializers as before. `lexer` and `parser` are
    `#[doc(hidden)]`, kept only for the old conformance runner; the library builds without them
    (checked by removing the two module declarations, with and without default features).
    Old error types (`LexError`, `ParseError`, `Span`, `ErrorContext`, `EnhancedError`,
    `UclError::Lex`, `UclError::Parse`, `with_source_context`) are `#[doc(hidden)]`.
  - Decision 1: a parser's default loader is an empty `MemoryLoader` in every build; `from_str`,
    `from_slice`, `from_reader`, `UclDeserializer::new` and `parse::parse` therefore read no
    files. `from_file` uses `FsLoader`; without a configured base directory, relative include
    paths in a file given to `Parser::parse_file` resolve against that file's directory, also
    inside included files; that directory is then also `CURDIR` in macro argument lists (§9.2),
    where libucl uses the working directory. Nested includes are anchored to the directory of the
    file given to the parser, not of the including file: the decision says "the file's
    directory", and §9.3 never resolves against an including file; the tests pin this reading.
    Text input opts in with `ParserBuilder::with_loader(FsLoader::new())`. `CURDIR` of text input
    is the base directory or the loader's current directory (`/`).
    Tested in `tests/api_tests.rs` (`text_input_reads_no_files`, `text_input_loads_no_files`,
    `file_variables_of_text_input`, `from_file_resolves_includes_against_the_file_directory`)
    and `src/parse/mod.rs`. The conformance runners set `FsLoader` explicitly.
  - Tests, per file (test functions before → after):
    - `tests/bare_word_tests.rs` 12 → 11, `heredoc_tests.rs` 17 → 16, `nginx_syntax_tests.rs`
      8 → 8, `implicit_array_tests.rs` 13 → 13: as in `target/c5/notes/A.md` (vague "either"
      assertions replaced with libucl's results; message-wording checks replaced with error
      kinds; `//` line in an input replaced with `#`), reviewed.
    - `tests/unicode_escape_tests.rs` 11 → 12, `cpp_comment_tests.rs` 11 → 9,
      `real_world_ucl_configs.rs` 5 → 5: as in `target/c5/notes/B.md` (`\u{…}` and `//` comments
      are not UCL: their parse assertions became error or entry assertions; old-lexer comment
      types deleted), reviewed; one comment corrected.
    - `tests/integration_tests.rs` 10 → 9: ported: the large-configuration, deeply-nested and
      large-array tests without their timing and throughput assertions, with the values checked;
      FreeBSD pkg values asserted; the rspamd document's error is `FileNotFound` for
      `$CONFDIR/common.conf`; the two C++-comment tests assert `//` entries. Deleted:
      `test_cpp_comments_preservation` (old-lexer comment types, C++ comments).
    - `tests/performance_tests.rs` (9) → `tests/generated_documents.rs` (5): wall-clock and
      throughput assertions deleted; the small, medium, large and scaling tests became one test
      of generated documents of several sizes with every value checked; string-heavy,
      number-heavy and deeply nested documents checked value by value. Deleted: the old lexer's
      token-count test and the streaming-lexer comparison.
    - `tests/extensibility_tests.rs` (25) deleted: plugins, hooks, suffix handlers, string
      processors, validation hooks and `VariableContext` have no libucl counterpart. The
      handler tests were ported to `tests/api_tests.rs` (`variable_handlers`: registered
      variables win, braced references only, refusal keeps the text, handlers chained as
      closures; `from_str_with_env_reads_braced_references_only`), checked with the oracle's
      `-H` handler.
    - `tests/api_tests.rs` (new, 13): entry points, decision 1, `from_file` errors with the
      included file's name and position, non-UTF-8, borrowed `&str` fields, readers, the
      document deserializer, `from_str_with_variables`/`_map`/`_env`, handlers.
    - `tests/conformance.rs`: the new-core and emitter runners set `FsLoader`; the old runner
      imports from the hidden module; stale references removed. Counts unchanged.
    - `src/error_tests.rs` 13 → 8: kept the `Position` test; ported, corrected to libucl:
      surrogate escapes are `InvalidUtf8`, TAB in double quotes is an error while 0x1F and DEL
      are allowed (§6.1), `\q` gives `q`, `123e`, `0x`, `1e+` and a 2000-digit run are strings
      (§5.5, §5.8), `@` as a key is an error but as a value a string, unterminated string and
      comment, heredoc NAME rules. Deleted: `LexerConfig` limits (string length, tokens, comment
      length, nesting depth 3; libucl has none, the core's nesting limit is tested in
      `src/parse/mod.rs`), lexer recovery and number-format validation (lexer internals), and
      the assertion that `<<⏎content⏎` is an error, which libucl gives and the core does not
      (QUESTIONS.md #55).
    - `src/de.rs` 24 → 23: the two old error tests merged into one on kinds and positions;
      variable, environment and convenience tests ported to the new functions; the lexer-config
      test became `test_parser_settings_are_honoured` (`no-time`).
    - `src/parse/mod.rs` 20 → 23: decision-1 tests, `CURDIR` of text input; the file-variables
      test split so the filesystem part is `fs`-gated.
  - Examples (all build, run with exit 0, and the inputs they parse agree with the oracle: 51
    same, 0 different, 7 handler inputs checked by hand): kept and corrected `basic_usage`,
    `complete_ucl_syntax`, `number_parsing`, `web_server_config`, `framework_integration`,
    `nginx_style_framework_integration`, `real_world_configurations`, `advanced_features`;
    rewritten `configuration_management` (layers by include priority, variables and a handler
    with fallbacks, validation, reload diff); ported `real_world_usage` (registered variables
    and a handler instead of `${NAME:-default}`, literals for numbers and booleans, fixed
    document/struct mismatches). Deleted `cpp_comments_demo` (C++ comments),
    `extensibility_demo` (plugins, hooks), `performance_comparison` (streaming lexer).
    `examples/README.md` rewritten. `CLAUDE.md` still names `performance_comparison`; left for
    the project owner.
  - Benches: deleted `lexer_benchmarks`, `parser_benchmarks`, `zero_copy_benchmarks`,
    `memory_efficiency_benchmarks`, `ucl_compatibility_benchmarks`; wired `parse_benchmarks`,
    `emit_benchmarks`, `serde_benchmarks`; `benches/README.md` rewritten. One run
    (`cargo bench`, rustc 1.98.1, Apple M4 Max, median of 30 samples):

    | Group | Time | Throughput |
    | --- | --- | --- |
    | parse/config/10 (5.1 KB) | 66.7 µs | 73.2 MiB/s |
    | parse/config/100 | 693.6 µs | 69.5 MiB/s |
    | parse/config/1000 (510 KB) | 7.35 ms | 66.2 MiB/s |
    | parse/config-100-flags/save-comments | 1.144 ms | 42.2 MiB/s |
    | parse/config-100-flags/no-implicit-arrays | 728.5 µs | 66.2 MiB/s |
    | parse/config-100-flags/key-lowercase | 695.8 µs | 69.3 MiB/s |
    | parse/json/100 (14 KB) | 298.2 µs | 45.2 MiB/s |
    | parse/json/1000 | 3.115 ms | 44.4 MiB/s |
    | parse/nested/10 | 3.84 µs | 19.9 MiB/s |
    | parse/nested/500 (4.4 KB) | 1.088 ms | 3.9 MiB/s |
    | parse/variables/1000 | 1.172 ms | 76.0 MiB/s |
    | emit/config-1000/config | 5.747 ms | 81.1 MiB/s (output) |
    | emit/config-1000/json | 4.204 ms | 136.7 MiB/s (output) |
    | emit/config-1000/json-compact | 4.156 ms | 68.3 MiB/s (output) |
    | emit/config-1000/yaml | 6.747 ms | 69.9 MiB/s (output) |
    | serde/deserialize-1000/from_str | 8.139 ms | 59.8 MiB/s |
    | serde/deserialize-1000/from_value | 774.4 µs | 628.7 MiB/s |
    | serde/serialize-1000/to_string | 2.611 ms | 181.3 MiB/s (output) |
    | serde/serialize-1000/to_json_string | 2.667 ms | 214.7 MiB/s (output) |
    | serde/serialize-1000/to_json_string_compact | 3.036 ms | 92.8 MiB/s (output) |
    | serde/serialize-1000/to_yaml_string | 5.519 ms | 85.1 MiB/s (output) |
    | serde/to_value-1000 | 1.849 ms | – |

    Deep nesting costs far more per byte than flat input (nested/500 against nested/10); not
    investigated in C5a.
  - Stale references to the oracle-side planning documents removed from `src/de.rs`,
    `src/value.rs` and `tests/conformance.rs`; none remain in `src/` (old files excluded) or
    `tests/*.rs`.
  - Left for C5b: delete `src/lexer.rs`, `src/parser.rs`, their module declarations and the
    test-only `old_api_for_old_tests` in `src/lib.rs`; the old error types listed above and their
    tests in `src/error.rs`; the old runner in `tests/conformance.rs` and `xfail.txt`; write
    `CHANGELOG.md` from the notes below.
  - CHANGELOG notes (breaking and behaviour changes of C5):
    1. **No file access from text input.** `from_str`, `from_slice`, `from_reader`,
       `UclDeserializer::new`/`from_slice`, `parse::parse` and any `Parser` left with its default
       loader read no files: `.include` of any path is a missing-file error, `.try_include` a
       silent stop (`UclError::Stopped`), `.load` an error or, with `try=true`, nothing. A
       default `Parser::parse_file` finds no file either. Opt in with
       `ParserBuilder::with_loader(FsLoader::new())` or `Parser::set_loader`. `Parser::new()`'s
       loader was the filesystem loader under feature `fs`; it is now always empty.
    2. **`from_file`** reads from the filesystem and resolves relative include paths against the
       file's directory, also inside included files (libucl uses the process's working
       directory); a configured base directory takes precedence.
    3. **`$FILENAME` and `$CURDIR`** are defined (spec §7.8): for text input `undef` and the
       base directory, or `/` without one (never the process's working directory); for a file,
       its canonical path and directory, which registered variables cannot override. As
       registered names they also match unbraced prefixes (`$CURDIRx` → `/x`). `no-filevars`
       turns them off for text input.
    4. **The variable handler sees only braced references** `${NAME}` whose name is not
       registered (§7.7); `$NAME` never reaches it. It is a closure
       `FnMut(&str) -> Option<String>` (`ParserBuilder::with_variable_handler`); the
       `VariableHandler` trait and `MapVariableHandler`, `EnvironmentVariableHandler`,
       `ChainedVariableHandler`, `VariableContext` are gone. `from_str_with_env` expands
       `${HOME}` but leaves `$HOME`. There is no `${NAME:-default}` form, and expansion never
       produces a number or boolean (§7.2).
    5. **Non-UTF-8 input is an error**: keys and strings must be valid UTF-8
       (`ErrorKind::InvalidUtf8`), including strings made invalid by `\u` escapes of surrogates;
       comments may hold any bytes. libucl accepts such bytes.
    6. **Borrowed `&str` fields** are not supported: values are owned, and a `&'a str` target
       fails with "invalid type: string …, expected a borrowed string". Use `String` or
       `Cow<str>`.
    7. API: `from_str_with_variables` takes `(name, value)` pairs, registered in order, instead
       of a boxed handler; `from_str_with_map` registers longest name first; `from_str_with_config`
       and `from_str_with_config_and_variables` are removed (use `ParserBuilder` with
       `UclDeserializer::from_parser` or `from_value`); `UclDeserializer::with_lexer_config` and
       `with_variable_handler` are removed and `from_parser` takes a `parse::Parser` and the
       input; the root no longer re-exports the lexer, tokens, the streaming lexer, `UclParser`,
       `ParserConfig`, `DuplicateKeyBehavior`, `UclParserBuilder`, plugins, hooks, handler types,
       `LexError`, `ParseError` or `Span`. Added: `from_slice`, `from_reader`, `from_file`,
       `parse::ParserBuilder`, `Parser::builder`, `UclError::parse_error`, `UclError::position`.
       Parse errors are `UclError::Syntax` with an `ErrorKind`, a position and, for an included
       file, its path; `UclError::Lex` and `UclError::Parse` are no longer produced.
    8. Syntax now follows libucl (docs/spec): `//` is not a comment; `\u{…}` is an error;
       `NULL` and `Null` are strings; `inf` and `nan` are lowercase only; a heredoc NAME is
       uppercase letters; a suffixed number followed by a space is a string; unquoted values run
       to the end of the line; `key name { }` sections nest; repeated keys keep every value;
       unknown escapes drop the backslash; TAB in double quotes is an error; macros run; the old
       lexer's configurable limits are gone (nesting is limited to 1024).
    9. Removed examples and benches listed above.
  - Result: `cargo test` passes (lib 366, doc 14, integration files as above). Conformance: new
    core 1389 of 1393 (4 listed divergences), emitters 1037 of 1041, old parser 505 of 1393,
    all unchanged; `xfail-new.txt` and `xfail-emit.txt` unchanged. `cargo build --examples
    --benches`, `cargo check --no-default-features`, `cargo check --features load` and
    `cargo doc --no-deps` succeed.
  - Questions: #55 (§6.3, a heredoc with an empty NAME whose content no empty line follows
    before the end of input).
  - Commits: `7a3e3f8`, `b77cdfe`, `6fc5050`, `58100af` (this entry, `cargo fmt`), and the
    `C5a:` commit after it (loader, include and base-directory docs for decision 1, a test of
    `CURDIR` in macro argument lists, this line).
  - Attestation: I did not read libucl source code or any forbidden input listed in
    docs/clean-room/PROTOCOL.md.
- 2026-09-25 — Role: implementation team. Item: C5b, deletion of the old implementation,
  retirement of the old conformance runner, `CHANGELOG.md` (WORKLIST C5, project decision 1).
  - Inputs consulted: `docs/spec/` (checked identical to `spec-v7`; no section needed);
    `docs/clean-room/` (PROTOCOL, WORKLIST C5, this log's first section and the C5a entry with
    its CHANGELOG notes); `tests/conformance/README.md` (runner section), `xfail-new.txt`,
    `xfail-emit.txt` and the header of `xfail.txt` before deleting it; the crate's own code
    other than the two old files; `git show` of `src/lib.rs`, `src/de.rs` and `src/error.rs` at
    `cb51df2`, of `src/lib.rs` and the public items of `src/value.rs` and `src/time.rs` at
    `ef8007e`, of `Cargo.toml` at `ef8007e`, and the names of the `src/` files added in
    `ef8007e..HEAD` (all at or after `ef8007e`), for the lists of removed and added API;
    `README.md` and `CLAUDE.md`, searched by file name for stale lines; runs of a scratch crate
    (`target/c5b/check`) that checks every behaviour the CHANGELOG states. No oracle runs.
    Searches named `src/` (excluding `lexer.rs` and `parser.rs` while they existed), `tests/`,
    `examples/`, `benches/`, `docs/clean-room/`, `target/c5b/` and the files above only.
  - Search-rule breach: before the deletion, one command piped both old files into `grep -c`
    for six patterns (three dependency names, three feature names). It printed six counts, all
    0, and no line of either file. The counts were not used: dependency and feature use was
    established from the other files and from the clean build after the deletion. Also, a
    `git show ef8007e --stat` printed the file name of `PLAN.md` with a line count, not its
    content.
  - Method: `git rm src/lexer.rs src/parser.rs` without opening either file, then what the
    compiler and searches of the other files pointed to. Diffs of the deletion commit were viewed
    only with `--stat` or with pathspecs naming other files.
  - Removed:
    - `src/lexer.rs`, `src/parser.rs` (streaming lexer, plugins, hooks), their hidden module
      declarations and the test-only `old_api_for_old_tests` in `src/lib.rs`
      (`from_str_with_variables` is now re-exported in every build).
    - `src/error.rs`: `Span`, `ErrorContext`, `EnhancedError`, `LexError`, `ParseError`,
      `UclError::Lex`, `UclError::Parse`, `From<ParseError> for UclError`,
      `UclError::with_source_context`, `UclError::format_with_context`; `Position::advance` and
      `advance_by` (no users; a carriage return reset their column, which the parser's
      positions do not); `SerdeError::TypeMismatch`, `MissingField`, `UnknownField` (never
      constructed). Tests: `error.rs` 7 → 2, `src/error_tests.rs` 8 → 7.
    - `tests/conformance.rs`: `libucl_conformance`, `parse_with_crate`, the old-parser import and
      the `non-utf8, D2` hint; `dump` now takes the comment map and serves the new core.
      `tests/conformance/xfail.txt` deleted.
    - Stale comments: `tests/generated_documents.rs` header; the `fs` feature comment in
      `Cargo.toml` now names `from_file`.
  - Kept after review: `UclValue::is_time` and `value_mut`, the only public functions without a
    user in the crate (value-model accessors); the features `std`, `save-comments` and
    `strict-unicode`, which no `#[cfg]` uses (C6); the `description` in `Cargo.toml`, which
    still says "lexer and parser" (Cargo metadata, C6); the words "UCL lexer" inside a generated
    test document's string. `docs/clean-room/` mentions the old files only in records (this
    log, `reviews/`), work-item goals (WORKLIST) and PROTOCOL's forbidden-input rule, which
    still binds their history.
  - Public API after C5b: modules `de`, `emit`, `error`, `parse`, `ser`, `time`, `value`; at the
    root `from_str`, `from_slice`, `from_reader`, `from_file` (feature `fs`),
    `from_str_with_variables`, `from_str_with_map`, `from_str_with_env`, `from_value`,
    `UclDeserializer`, `to_string`, `to_json_string`, `to_json_string_compact`,
    `to_yaml_string`, `to_writer`, `to_value`, `UclError` (`Serde`, `Io`, `Syntax`, `Stopped`;
    `parse_error`, `position`), `Position`, and the value model (`UclValue`, `UclObject`,
    `UclArray`, `Entry`, `Slot`, `Values`, `Placement`, `DuplicateStrategy`,
    `DuplicateKeyError`, `ParserFlags`); `error::SerdeError` (`Custom`, `Unrepresentable`).
  - `CHANGELOG.md`: written from the C5a notes, each claim checked with the scratch crate
    (default features and `load`). Its baseline is the public API at `ef8007e` and `cb51df2`
    (`src/` history before `ef8007e`, and so 0.1.0 itself, is forbidden); the value model and
    `time` were compared item by item and signature by signature between `ef8007e` and `HEAD`:
    additions only. Corrections to the notes: text input with `FsLoader` and no
    base directory resolves relative paths and `CURDIR` against the process's working
    directory; `.load` without feature `load` is an "unsupported" error even with `try=true`;
    an unquoted value ends at a line end, `,`, `;` or comment, not only at the line end; a
    repeated key fails to deserialize into a scalar field; `parse::VariableHandler` still exists
    as the closure type, so the entry names the removed trait; a default `Parser::parse_file`
    fails with `ErrorKind::Io`; `.include(try=true)` of a missing file does nothing, so
    `.include` is not an error in every form. Migration snippets compiled in the scratch crate.
  - Stale lines in files this item does not own: `tests/conformance/README.md` 49–50 (`xfail.txt`
    and the existing parser), 72 ("three tests"), 74–76 (`libucl_conformance`, "does the same");
    `CLAUDE.md` 44 (the two old files "are being replaced"), 88 (`performance_comparison`);
    `README.md` 8, 15–17, 59, 62, 122–204 (variable-handler types, `UclLexer`, `LexerConfig`,
    the streaming lexer, `UclParser` and hooks with `src/parser.rs` line references), 213,
    217–218, 272, 301, 326–330, 340–354, 386, 393, 453. `scripts/` and `tools/` are outside the
    directories this item may search and were not checked for `xfail.txt`.
  - Result: `cargo test` passes: 295 tests (lib 166, was 366 with the old modules' 194 inline
    tests; integration 115; doc 14). Conformance: new core 1389 of 1393, emitters 1037 of 1041,
    unchanged; `xfail-new.txt` and `xfail-emit.txt` unchanged, four justified divergences each.
    `cargo build --examples --benches`, `cargo check --no-default-features`,
    `cargo check --features load` and `cargo doc --no-deps` succeed without warnings.
    `cargo clippy --all-targets --keep-going`: 5 warnings (`src/parse/number.rs:15`,
    `src/parse/mod.rs:1044`, `tests/conformance.rs:197`,
    `examples/configuration_management.rs:257`, `examples/real_world_usage.rs:418`) and 2 errors
    of the deny-by-default `approx_constant` lint (`tests/bare_word_tests.rs:326`,
    `tests/implicit_array_tests.rs:111`), which stop clippy for those two targets; left for C6.
  - Questions: none.
  - Commits: `fc7f529`, `009b1d6`, `6db4123`, `fe85dbc`, `e0a6ea4` (this entry), `cfb196c`
    (CHANGELOG corrections, the baseline note), and the `C5b:` commit that records this line.
  - Attestation: I did not read libucl source code or any forbidden input listed in
    docs/clean-room/PROTOCOL.md. I breached the search rule once, as recorded above, without
    reading any content of the two old files.
- 2026-09-25 — Role: session lead (oracle side). Item: C5b review.
  - The C5b search-rule breach (a `grep -c` over the two old files for six dependency and feature
    names, printing six zero counts and no text) is not an exposure: no content of either file
    was shown and the result was not used. No action beyond this record.
- 2026-09-25 — Role: spec team. Item: C0, `spec-v8` (tag on `521b860`), and the oracle-side
  cleanup after the C5 cut-over.
  - Inputs: libucl source at the pinned commit (the heredoc reader), black-box runs of
    `target/libucl-oracle/ucl-dump`, `docs/clean-room/QUESTIONS.md` #55, the existing spec,
    `tests/conformance/`, `scripts/` and `tools/`, and the crate's `tests/api_tests.rs` test names
    for `docs/COMPATIBILITY.md`.
  - #55 answered in §6.3: the end rule of a heredoc with an empty NAME is deterministic (first LF,
    `;` or `,` after the first content line's LF; last byte dropped; unterminated at the end of
    input; `$` after the first LF does not turn on expansion, also noted in §7.2). Added the quirk
    of a NAME of one repeated letter. Four new cases with all golden files; the crate fails all
    four, so they are held in `tests/conformance/pending/06-strings/` with a README.
  - `tests/conformance/README.md`: two runners, no `xfail.txt`, `pending/` described.
  - `scripts/run_benchmarks.sh`, `scripts/profile_performance.sh` and `scripts/memory_analysis.sh`
    removed: they ran only benches deleted in C5 and nothing referred to them. `tools/` had no
    stale references.
  - Spec README: coverage rows for the three cases moved from `pending/` in C4c and for the four
    new pending cases (with their final paths); the table maps all 1393 cases in `cases/` and
    `libucl/`. *Known gaps* updated; the C5 decisions recorded under *Divergences decided by the
    project*.
  - `PROTOCOL.md`: the forbidden-input line now covers the git history of the deleted
    `src/lexer.rs` and `src/parser.rs`.
  - `docs/COMPATIBILITY.md` written for crate users: deliberate differences with reasons and
    cases or tests, choices that match libucl, and 30 notable quirks with spec sections and cases.
  - No existing golden file changed; full `cargo test` passes at every commit.
  - The spec contains behaviour only.
- 2026-09-25 — Role: implementation team. Item: C6a (spec-v8, clippy, Cargo features, linear
  time on nested input).
  - Inputs consulted: `git diff spec-v7 spec-v8 -- docs/spec/` and §6.3, §7.2 at `spec-v8`
    (`docs/spec/` at HEAD is identical to the tag); `docs/clean-room/` (PROTOCOL, WORKLIST C6
    and its project decisions, QUESTIONS #55, the C5b and spec-v8 entries of this log);
    `docs/COMPATIBILITY.md` (searched for heredoc, feature and nesting lines; nothing stale);
    the conformance cases and golden files, `tests/conformance/pending/` with its README,
    `tests/conformance/README.md`, `xfail-new.txt`, `xfail-emit.txt`; the crate's own files;
    `git show ef8007e:Cargo.toml` (the features of the CHANGELOG baseline) and
    `git ls-tree -r --name-only ef8007e -- src/` (file names only, to confirm that `src/parse/`,
    and so `OutputFacts` and `AttachedComments`, did not exist in 0.1.0). About 33 black-box runs
    of `target/libucl-oracle/ucl-dump` on scratch inputs under `target/c6a/probe/`: heredoc
    names of one repeated letter (14), heredocs with an empty name (11), `.inherit` of an object
    that holds the current one at depth 2–4 (5), and the validity of the new bench and test
    documents (3). Searches named `src/`, `tests/`, `examples/`, `benches/`, `docs/spec/`,
    `docs/clean-room/` and `target/c6a/` only. The tooling showed me the worktree's current
    `CLAUDE.md` once; it matches the clean version I was given. The main checkout's initial git
    status listed the names `PLAN.md` and `REVIEW.md`; neither was opened. `scripts/` and `tools/`
    were not opened.
  - spec-v8 (§6.3, §7.2): a heredoc NAME of one repeated letter is also ended by a longer line
    of that letter; a heredoc with an empty NAME ends at the first LF, `;` or `,` after its first
    line, drops its last byte, leaves that byte to be read next, and expands variables only
    when its first line has a `$` (`parse::string::heredoc` returns the content, where the input
    continues, and whether to expand). The four pending cases moved unchanged to
    `cases/spec/06-strings/`; `pending/` is deleted. The assertion held back for #55
    (`k = <<⏎content⏎` is an unterminated heredoc) is back in `src/error_tests.rs`. The
    conformance README now says `pending/` exists only when there are pending cases.
  - Clippy: `cargo clippy --all-targets --all-features -- -D warnings`, the same with
    `--no-default-features`, and `cargo clippy --lib --no-default-features -- -D warnings` pass
    (the crate's dev-dependency on itself turns `fs` and `load` back on for `--all-targets`,
    hence the `--lib` run). Fixed: `enum_variant_names` (`Number::NotNumber` → `Number::Text`),
    `redundant_locals` in a test, `collapsible_if` in the runner, `bool_assert_comparison` and
    `len_zero` in two examples, and `approx_constant` in two tests, whose input floats `3.14`
    are now `2.75`. No `allow` was added. The eight existing `allow(dead_code)` attributes stay,
    each now with a `reason`: deserialization targets whose fields are never read (error tests
    and examples) and the bench module that each bench uses in part.
  - Cargo features (decisions): `fs` (default) is real: it gates `parse::FsLoader` and
    `from_file`; kept. `load` is real: it gates `.load`, which otherwise fails with the
    "unsupported" error; kept, its comment now says so. `std` had no `#[cfg]` and the crate
    always needs the standard library (I/O, paths, the filesystem loader, serde's std support):
    removed, `default = ["fs"]`. `save-comments` had no `#[cfg]`; comments are saved at run time
    with `ParserFlags::SAVE_COMMENTS`, which the conformance runner needs in every build:
    removed. `strict-unicode` had no `#[cfg]`; keys and strings are always checked to be UTF-8
    (a divergence the project decided, spec README): removed. `CHANGELOG.md` records the three
    removals under *Removed features*.
  - Nesting, cause: the parser filled open containers in place in the tree and, for every
    entry or element it inserted, walked from the root through every open container to the
    current one, with a hash lookup of each level's key (`resolve`): O(depth) per value, so
    O(n × depth) for a document. More costs grew with depth: output facts (single-quoted
    strings, heredocs, keys that need quoting) were stored under full paths, so recording one
    and finding one in the emitters cost O(depth) path copies or comparisons per value; with
    `SAVE_COMMENTS`, every value's full path was copied, and every attached comment group was
    found and stored by its full path; and the bracket scan that decides whether a word starts a
    section path re-read the rest of the line for every name.
  - Nesting, fix: an open container now lives in its frame; an empty container of its kind
    keeps its place in the parent and it goes back when it closes, so the current container is
    one step away. `.inherit`, the only reader of the tree beyond the current container, finds
    open containers through the frames and copies them as they are (§9.7, *Quirk*;
    `Core::value_at`, `Core::filled_copy`; unit test with five oracle runs). A `debug_assert`
    checks, in every test run, that only the top frame's container changes. Paths are shared
    lists (`PathRef`) worked out once per frame when needed. Output facts and saved comments are
    trees of path segments (`OutputFacts`; `comments::CommentGroups`) whose children are found
    directly by key and value index (`parse::tree::Children`); the parser keeps each frame's
    facts node, each path node remembers its place in the comment tree, and the emitters walk
    the facts tree, and a comment tree built once per emit, node by node. The paths of
    `AttachedComments` are written out on the first call of `Parser::attached_comments`. The
    bracket scan's result is reused for every position before the byte it found. Both types
    are new in this release (not in 0.1.0), so `OutputFacts::iter` yielding owned paths needs no
    CHANGELOG entry; the public signatures of the comment API are unchanged.
  - Also found with a scratch harness (`target/c6a/scale/`) and fixed, on flat input: under
    `KEY_LOWERCASE`, after a key whose escape gives an uppercase letter, each new key was compared
    with every key of its object (quadratic in object width; frames now keep an index of keys by
    lowercase form, extended as keys are added, since the parser never removes or reorders
    entries); with saved comments, each replaced or collected value scanned and re-indexed every
    comment group (quadratic in comment groups; the comment tree above removes the scan).
  - Not changed: the indented output formats (config, JSON, YAML) are as large as depth ×
    lines; their time is linear in the output.
  - Test: `tests/scaling.rs` times each operation on two documents about 8 times apart in size
    (and depth, for nested ones) in one run, takes the fastest of 7 interleaved samples, and fails
    when the time grows more than 3 times the size ratio (linear ≈ 8, quadratic ≈ 64). Parse:
    nested objects with facts, comments and `.inherit`, with and without `SAVE_COMMENTS`; nested
    arrays; a section path; keys compared regardless of case; replaced values with saved
    comments; many values of one key with facts and comments. Emit: all four formats with facts,
    and with comments (sizes taken from the output); many values of one key with comments. Serde:
    `from_str`, `from_value`, `to_value`, `to_json_string_compact`, `to_string`. It runs in about
    0.5 s. It fails against `5484bb5` (time ×59–63 for size ×8.3); the width checks against
    `8bf25c5` (×54 for ×9.4, ×65 for ×8.9); the comment checks against `f19341f` (×45.7 for
    ×8.4, ×47.3 for ×8.7). The final file passed 10 of 10 runs, 5 of them with 16 busy
    processes on 14 cores.
  - Benches (criterion, median time). Before: the library code of `04dd3cf`, run with the bench
    groups that were then committed as `81859c3`. After: `028d289`.

    | Bench | Before | After |
    | --- | --- | --- |
    | parse/nested/10 | 3.73 µs (20.4 MiB/s) | 3.67 µs (20.8 MiB/s) |
    | parse/nested/500 | 1.366 ms (3.07 MiB/s) | 157 µs (26.8 MiB/s) |
    | parse/nested/1000 | 6.71 ms (1.26 MiB/s) | 303 µs (28.0 MiB/s) |
    | parse/nested-mixed-1000/default | 90.6 ms (448 KiB/s) | 1.15 ms (34.4 MiB/s) |
    | parse/nested-mixed-1000/save-comments | 173 ms (234 KiB/s) | 1.87 ms (21.2 MiB/s) |
    | parse/config/1000 | 7.36 ms (66.1 MiB/s) | 5.37 ms (90.7 MiB/s) |
    | parse/config-100-flags/save-comments | 1.12 ms (42.9 MiB/s) | 792 µs (60.9 MiB/s) |
    | parse/config-100-flags/key-lowercase | 701 µs (68.8 MiB/s) | 535 µs (90.2 MiB/s) |
    | parse/json/1000 | 3.06 ms (45.1 MiB/s) | 2.91 ms (47.5 MiB/s) |
    | parse/variables/1000 | 1.15 ms (77.2 MiB/s) | 1.15 ms (77.2 MiB/s) |
    | emit/config-1000/config | 5.74 ms | 1.20 ms |
    | emit/config-1000/json-compact | 4.12 ms | 1.43 ms |
    | emit/nested-mixed-1000/config | 78.7 ms | 4.81 ms |
    | emit/nested-mixed-1000/json-compact | 54.0 ms (501 KiB/s) | 233 µs (113 MiB/s) |
    | emit/nested-mixed-1000/yaml | 104 ms | 3.02 ms |
    | serde/nested-mixed-1000/from_str | 93.2 ms (436 KiB/s) | 1.86 ms (21.3 MiB/s) |
    | serde/deserialize-1000/from_str | 7.95 ms | 5.91 ms |
    | serde/nested-mixed-1000/to_json_string_compact | 481 µs | 469 µs |
    | serde/nested-mixed-1000/to_string | 5.28 ms | 6.05 ms |
    | serde/serialize-1000/to_string | 2.51 ms | 2.49 ms |

    `serde/nested-mixed-1000/to_string` writes 7.5 MB and varies with page faults: a
    side-by-side probe of the old and new crate had either one ahead from run to run; its code
    path does not use facts or comments. All numbers are in `target/c6a/bench-{before,final}.txt`
    on the machine that ran them.
  - Result: `cargo test` passes, 302 tests (lib 169, integration 119 with `scaling` 4, doc 14).
    Conformance: new core 1393 of 1397, emitters 1040 of 1044; `xfail-new.txt` and
    `xfail-emit.txt` unchanged, the four justified divergences each. `cargo fmt --check`,
    `cargo build --examples --benches`, `cargo check --no-default-features`,
    `cargo check --features load` and `cargo doc --no-deps` succeed without warnings.
    `cargo test` was run at each commit from `3ada2e5` to `028d289` in a scratch worktree: no
    failure at any of them.
  - Stale lines in files this item does not own: `docs/spec/README.md` 192–195 and 206–209
    (the four cases are no longer in `pending/`; spec team); `README.md` *Feature Flags*
    (`std`, `zero-copy`, `save-comments`, `strict-unicode`; C6b).
  - Questions: none.
  - Commits: `3ada2e5`, `004ddbc`, `5484bb5`, `81859c3`, `30020db`, `8bf25c5`, `3fdb123`,
    `a23533b`, `4a632be` (the first version of this entry), `f19341f`, `028d289`, and the
    `C6a:` commit that records this version.
  - Attestation: I did not read libucl source code or any forbidden input listed in
    docs/clean-room/PROTOCOL.md.
- 2026-09-25 — Role: implementation team. Item: C6b (README, crate docs, `CLAUDE.md`, Cargo
  metadata, license files, CI).
  - Inputs consulted: `docs/clean-room/` (PROTOCOL, WORKLIST C6 with its split and project
    decisions, the C6a entry of this log); `docs/spec/` at `spec-v8` through `git show spec-v8:…`
    (§7.1–7.2, §8.3–8.4, §12; at HEAD only the spec README differs from the tag);
    `docs/COMPATIBILITY.md`; `tests/conformance/README.md`; the crate's own files (`src/`,
    `tests/`, `examples/` and `benches/` with their READMEs); the root `README.md`, `CHANGELOG.md`,
    `Cargo.toml` and the worktree's current `CLAUDE.md` (no history); `scripts/regen-golden.sh`,
    the header of `scripts/create-cases.sh` and the old `.github/workflows/`. `tools/ucl-dump/`
    was not opened; a Linux compiler error quoted three lines of `ucl_dump.c`. Outside the
    repository: the Apache-2.0 text from apache.org and the MIT text from SPDX's
    license-list-data, `cargo search` and `cargo info` (the crate is not on crates.io), and
    `gh run list` for the repository (run status only). The tooling showed me the worktree's
    current `CLAUDE.md` once, and the main checkout's initial git status listed the names
    `PLAN.md` and `REVIEW.md`; neither was opened. Background command output that the tooling
    wrote under `/private/tmp` was not opened; every log I read is under `target/c6b/`.
  - Black-box oracle runs: about 30 runs of `target/libucl-oracle/ucl-dump` on scratch inputs in
    `target/c6b/probe/`, one or more per README example (the documents, the config and compact
    JSON output, saved comments, includes, the silent stop, the variable handler through `-H` with
    an `H_` name), the format notes and the suffix rule. `scripts/regen-golden.sh`, through
    `scripts/ci.sh golden`, in two scratch clones: on macOS it regenerated 5626 golden files and
    the serde corpus with no change, and failed with the diff after one committed golden file was
    tampered with; on Linux (Docker, `rust:1-bookworm`, aarch64) `ucl_dump.c` does not compile
    under the script's `-std=c99` (`getopt`, `optarg`, `optind` undeclared), and with
    `-D_POSIX_C_SOURCE=200809L` added to a scratch copy of the script (not committed) 15 golden
    files of three cases differ: `include_glob_caret_is_not_negation`,
    `include_glob_no_character_classes` and `include_glob_prefix_key_from_first_file` in
    `cases/spec/09-macros/`. The libucl clones those runs made under `target/c6b/golden-*/` were
    not opened, and were deleted afterwards.
  - `rust-version = "1.88"`: 1.85.0 and 1.87.0 fail to build the library (9 × E0658, let chains);
    1.88.0 builds it and every target. `scripts/ci.sh` passes with 1.88.0 and 1.98.1 on macOS,
    and in Docker on Linux with `rust:1.88-bookworm` and `rust:1-bookworm` (1.98.1), at `5d24627`
    plus the first version of this entry. `RUSTUP_TOOLCHAIN=1.88`, as documented, installs and
    selects 1.88.0 with clippy and rustfmt. The packaged crate builds with 1.88.0 (`--locked`,
    default features and `--no-default-features --features load`).
  - Clippy 1.88 has `uninlined_format_args` on by default: 10 format strings inlined, in
    `src/error.rs` (a test), two tests and one example.
  - `tests/scaling.rs`: `serde_time_grows_linearly` overflowed its 2 MiB test-thread stack with
    1.88.0 (serde recursion over the document nested 961 deep). The tests now run one at a time
    and each on a 64 MiB thread; samples last at least 5 ms (was 1 ms); a check fails only if all
    three attempts fail; the file header says why. It takes about 2.1 s. It passed 3 of 3 runs
    with 16 busy loops and 5 of 5 with 28 busy loops on 14 cores, and fails against `5484bb5`
    (×54.6–64.4 for ×8.4–9.4, every attempt). No other test in `src/`, `tests/`, `examples/` or
    `benches/` reads the clock.
  - Stack depth measured with a scratch binary (`target/c6b/stack/`, a 2 MiB thread, the
    nested-objects document of `tests/scaling.rs`): parsing, dropping and the emitters (all four
    formats, and config with saved comments) reach 1023 levels in debug and release builds with
    1.88.0 and stable; `from_str::<UclValue>` and `from_value` reach 239 (stable) and 301
    (1.88) in debug builds, 997 in a 1.88 release build; `to_value`, `to_string` and
    `to_json_string_compact` 604 and 733 in debug. The README's *Limits* section says so in
    general terms.
  - CI: `scripts/ci.sh` runs `cargo fmt --all --check`; clippy with `-D warnings` for all targets
    and features, and for the library without default features, with `fs` alone and with `load`
    alone; `cargo test`; `cargo build --examples --benches`; every example; `cargo doc --no-deps`
    with `RUSTDOCFLAGS='-D warnings'`. `scripts/ci.sh golden` refuses to start when
    `tests/conformance/` or `tests/serde_corpus/` has changes, runs `scripts/regen-golden.sh`
    and `UCL_SERDE_REGEN=1 cargo test --test serde_roundtrip`, and fails with `git status` and
    `git diff` if either directory changed. `.github/workflows/ci.yml` runs `scripts/ci.sh` on
    push, pull request and on demand, on `ubuntu-latest` and `macos-latest`, with stable and with
    the `rust-version` read from `Cargo.toml`. `.github/workflows/golden.yml` runs
    `scripts/ci.sh golden` nightly and on demand on `macos-latest` (Linux: see above). Removed:
    `build.yml` and `clippy.yml` (replaced), `coverage.yml` (every run in `gh run list` failed),
    `release.yml` (published on any `v*` tag without checks). `actionlint` 1.7.12 and
    `shellcheck` report nothing.
  - Package: `include` lists `/src/**`, `/Cargo.toml`, `/README.md`, `/CHANGELOG.md`,
    `/LICENSE-MIT` and `/LICENSE-APACHE`; `cargo package --list` gives those files plus
    `Cargo.lock`, `Cargo.toml.orig` and `.cargo_vcs_info.json`, which cargo adds. `cargo publish
    --dry-run` on the committed tree at the end of the session succeeds; the packaged manifest
    drops the self dev-dependency and the benches, and cargo warns that the tests, examples and
    benches are not in the package. Not published. `LICENSE-MIT` is SPDX's text with
    "Copyright (c) 2026 Andrii Radyk"; `LICENSE-APACHE` is apache.org's text unchanged.
  - README: every Rust example runs as a doctest (`ReadmeDoctests` in `src/lib.rs`, 11 blocks,
    two of them `no_run` because they read `/etc/myapp`), and every value and output it states
    was checked with the oracle.
  - For the spec team: `tools/ucl-dump/ucl_dump.c` does not build on Linux with glibc (above);
    the three glob cases above give other results on Linux, so their golden files hold only for
    macOS. `tests/conformance/README.md` already refers to `LICENSE-libucl`; no change needed.
  - For the lead: `version` is still `0.1.0`, while the CHANGELOG says the release is not
    compatible with 0.1.0 (never published); the README installs from git until a release, and
    a git dependency without a branch resolves to the default branch, which still has the old
    API, until this branch is merged; the crate docs link `COMPATIBILITY.md` at `blob/HEAD`,
    which likewise resolves only after the merge. The four tests of `tests/scaling.rs` now wait
    for one another (a mutex), a deliberate exception to "tests run in parallel" (WORKLIST C6
    decision 2) so that they do not take CPU time from one another's samples; they stay
    independent of order. serde (de)serialization recursion limits the depth of documents on
    small stacks (above);
    `parse::include::tests::load_needs_its_feature` never runs, because the self dev-dependency
    turns `load` on for every test build. `CLAUDE.md` lists the history of `CLAUDE.md` as
    forbidden, as my instructions do; PROTOCOL.md does not.
  - Questions: none.
  - Commits: `7fbc78c`, `4ec5612`, `55f2bd0`, `b3d5f91`, `c641506`, `d0a0be5`, `630deda`,
    `5d24627`, and the `C6b:` commit that adds this entry. `scripts/ci.sh` passed at each commit
    in a scratch worktree with stable (1.98.1), and from `55f2bd0`, which sets `rust-version`, with
    1.88.0 as well; `5d24627` in this worktree with both.
  - Attestation: I did not read libucl source code or any forbidden input listed in
    docs/clean-room/PROTOCOL.md.
- 2026-09-25 — Role: implementation team. Item: C7 (robustness follow-ups: stack depth in serde,
  the `load` feature test, version 0.2.0).
  - Inputs consulted: `docs/clean-room/` (PROTOCOL, WORKLIST C6 and C7, QUESTIONS in full, the
    C6a and C6b entries of this log); `docs/spec/` at `spec-v8` through `git show spec-v8:…`
    (§11.2, §9.7; at HEAD only the spec README differs from the tag); `docs/COMPATIBILITY.md`
    (its first 60 lines, to word a suggestion; not edited); the crate's own files (`src/`,
    `tests/scaling.rs`, parts of `tests/serde_roundtrip.rs`, the conformance runner's report,
    `README.md`, `CHANGELOG.md`, `Cargo.toml`, `.gitignore`), `scripts/ci.sh` and
    `.github/workflows/ci.yml`; serde's API as used by the crate. The tooling showed me the
    worktree's current `CLAUDE.md` once, and the main checkout's initial git status listed the
    names `PLAN.md` and `REVIEW.md`; neither was opened. Background command output that the
    tooling wrote under `/private/tmp` was not opened; every log I read is under `target/c7/`.
    One search command I issued named `.` (the repository root), against the search rule; zsh
    rejected its unquoted `--include=*.rs` glob before grep started, so nothing was searched or
    read. The same search was then run on the allowed directories.
  - Black-box oracle runs: 5 runs of `target/libucl-oracle/ucl-dump -e json-compact` on scratch
    inputs in `target/c7/probe/`: chains of objects nested 1000 deep, each with `.inherit` of the
    one before in its innermost object (2, 5 and 20 objects: accepted, output nested 2000, 4997
    and 19,982 deep, from 12 KB, 30 KB and 120 KB of input), and an object nested 500 or 1000
    deep inherited at the top level (accepted).
  - Cause of the overflows, measured with a scratch binary (`target/c7/stack/`, a 2 MiB thread,
    Rust 1.98.1, the crate unoptimised, object chains): `from_value::<UclValue>` reached 240
    levels, `to_value` of a `UclValue` 606 (and so `to_string`), `from_value::<serde_json::Value>`
    717, a derived `Clone` 549, `Debug` 1026, `==` 1825, drop 3056, the JSON emitter 2344, the
    config emitter more than 4096. serde passes a value through its data model one level at a
    time, several frames per level. Two further findings: `.inherit` copied values with the
    derived `Clone`, so parsing itself overflowed, also inside §11.2 (an object nested 1000 deep
    and `r1 { .inherit "r0" }`, 1001 levels); and a chain of copies nests a value without bound
    in libucl, so "every depth the parser accepts" was not bounded by §11.2.
  - Fix, parser: `Clone for UclValue` is written out with a heap stack; a copy by `.inherit`
    that would nest a value more than 1024 containers deep, the root included, is
    `ErrorKind::NestingTooDeep` at the macro (`Core::check_nesting`), so no parsed value is
    deeper than containers can be open. This is a divergence; QUESTIONS.md #56 asks whether it
    is acceptable. `NestingTooDeep`'s message now reads "containers nested inside one another".
  - Fix, serde: the crate's serializer and deserializer move a `UclValue` or `UclObject` whole,
    past serde's data model, through a thread-local slot (`src/handoff.rs`), asked for by the
    private newtype struct `marker::VALUE`; other serializers and deserializers see an ordinary
    newtype struct. `from_value::<UclValue>(v)` and `to_value(&v)` now give `v` back with
    priorities and marks (the docs said these were lost). The entry and time markers of the
    deserializer, which the move makes unreachable, are removed. Types other than `UclValue` are
    read and written by recursion through their own impls; the serializer and deserializer
    count the maps and sequences they enter and fail past `MAX_SERDE_NESTING` = 128 (the limit
    `serde_json` uses) with the new `SerdeError::TooDeep`, dropping the rest of a value too deep
    for them with a heap stack; the text functions drop their copy the same way.
  - Depth now, 2 MiB thread, debug: `from_str`, `from_slice`, `from_reader`, `from_file`, the
    `from_str_with_*` functions and `from_value` into `UclValue` or `UclObject`, and `to_value`
    of one: any depth (5000 checked; parsed values are at most 1024 deep); `to_string`,
    `to_json_string`, `to_json_string_compact`, `to_yaml_string`, `to_writer` of one: 1024 levels
    with the root, deeper is `Unrepresentable` as before; all of them into or from any other
    type: 128 maps and sequences, deeper is `TooDeep`. Least stack measured with the crate
    unoptimised (stable): typed serialization at 128 levels 353 KiB (`serde_json::Value`) and
    433 KiB (a recursive struct), typed deserialization 417 and 673 KiB, `to_json_string` of a
    `UclValue` 1024 deep 673 KiB, dropping such a value 673 KiB. `tests/stack_depth.rs` passes on
    1 MiB threads with the crate unoptimised, with 1.98.1 and 1.88.0 (checked by editing a
    scratch copy; committed at 2 MiB).
  - Tests: `tests/stack_depth.rs` (8 tests, 2 MiB threads): parsing and the emitters, with saved
    comments and output facts, and every serde entry point listed in the work item, on the
    deepest accepted documents (objects, arrays, a root array, multi-value entries, commented
    objects, and an `.inherit` copy reaching 1024), typed targets at 128 and 129 levels, a
    `UclValue` field of a typed target, and a value built 1025 deep. Unit tests: the `.inherit`
    limit and its position (`macros.rs`), clone order, priorities and marks, and clone at 20,000
    levels (`value.rs`). `cargo test` builds the crate optimised (`profile.test`), so
    `scripts/ci.sh` also runs `cargo test --lib --test stack_depth` with
    `--config 'profile.test.package.ucl-rust-lexer.opt-level=0'`; the two builds use separate
    artifacts.
  - `load_needs_its_feature`: moved from `src/parse/include.rs` to `tests/features/`, a package
    and workspace of its own that depends on the crate without `load` (lock file seeded from the
    crate's, same versions). `scripts/ci.sh` checks its format and clippy and runs its test with
    the crate's default features and with none, in `target/features`. With `--features
    ucl-rust-lexer/load` the test fails and cargo exits non-zero, which stops the script.
    `.gitignore` ignores `tests/features/target/`, where cargo builds when run in that directory.
  - Version 0.2.0 in `Cargo.toml`, both lock files and the CHANGELOG heading; the README and the
    docs name no crate version. CHANGELOG: the serde move, `MAX_SERDE_NESTING`,
    `SerdeError::TooDeep`, and the `.inherit` limit.
  - Results: `cargo test` 324 tests in 16 binaries. Conformance unchanged: new core 1393 of
    1397, emitters 1040 of 1044, `xfail-new.txt` and `xfail-emit.txt` unchanged. `scripts/ci.sh`
    passes with 1.98.1 and 1.88.0 at `c2c1681`, and with 1.98.1 at `ce3d5d8` in a scratch
    worktree. `cargo publish --dry-run --target-dir target/c7/publish` succeeds at `e350359`
    (39 files), as it did at `f9c3066`, the depth commit before a README wording fix was amended
    into it (now `c2c1681`); not published.
  - For the lead: `cargo publish --dry-run` with the default target directory (C6b) left a
    fingerprint under `target/debug` that plain `cargo build` in this worktree kept using, with
    sources under `target/package/`, so `cargo build` reported the crate fresh after source
    changes (`cargo check`, `cargo test` and clippy were unaffected). `cargo clean -p
    ucl-rust-lexer` removed it; `--target-dir` avoids it. Not changed and not serde: the derived
    `Debug` of a `UclValue` 1024 deep needs about 2 MiB unoptimised, and drop and `==` recurse
    (3056 and 1825 levels on 2 MiB). The stack figures and margins are from aarch64 macOS only;
    CI's Linux runners are x86_64, not measured. `CLAUDE.md` does not mention `tests/features/`
    or `tests/stack_depth.rs`; I did not edit it.
  - Questions: #56 (the `.inherit` depth limit as a divergence).
  - Commits: `7fff0b5`, `ce3d5d8`, `c2c1681`, `e350359`, and the `C7:` commit that adds this
    entry.
  - Attestation: I did not read libucl source code or any forbidden input listed in
    docs/clean-room/PROTOCOL.md.
- 2026-09-25 — Role: session lead (oracle side). Item: spec-v9 and C7 review.
  - QUESTIONS #56 answered as a project decision: the 1024-level limit on values nested by
    `.inherit` copies is accepted, following the precedent of #26; recorded in the spec README,
    `docs/COMPATIBILITY.md` and QUESTIONS.md, tagged `spec-v9`. No behaviour of libucl changed in
    the spec. `CLAUDE.md` names `tests/stack_depth.rs` and `tests/features/`; the
    clean-implementer agent embeds it.
  - The C7 grep naming `.` was rejected by the shell before it ran; not an exposure.
- 2026-09-25 — Role: implementation team. Item: C8a (deserialization error positions, input
  limit, default search directories, emitter readback test).
  - Inputs consulted: `docs/clean-room/` (PROTOCOL, WORKLIST C8, QUESTIONS, the C7 entries of this
    log); `docs/spec/` at HEAD, which `git diff --stat spec-v9 HEAD -- docs/spec/` shows equal to
    `spec-v9`: README, §3.1, §7.2, §7.5, §9.4 (*Signatures, URLs and search paths*), §10.1–§10.8,
    §11; the conformance suite (cases, golden files, `tests/conformance.rs`,
    `tests/conformance/README.md`); `scripts/regen-golden.sh` (only its flag-to-option mapping,
    to run the oracle) and `scripts/ci.sh`; the crate's own files; serde's API as the crate uses
    it. `rg`/`grep` ran on `src/`, `tests/`, `examples/`, `benches/`, `docs/spec/`,
    `docs/clean-room/`, `target/c8a/`, `scripts/`, and on single named files of the crate
    (`README.md`, `CHANGELOG.md`, `Cargo.toml`, `CLAUDE.md` at HEAD). Git: `git log --oneline`,
    `git tag`, `git show --stat cf800e8` (WORKLIST only), commit subjects of
    `tests/conformance/README.md`, `git show cf800e8:src/de/value.rs` (HEAD at the start of the
    session, after `ef8007e`) to bisect a slowdown, and `git archive` of `cf800e8` and `3da31c4`
    (a commit of this session) into `target/c8a/` for A/B benchmarks; no file of those trees was
    opened, and they were deleted afterwards. The tooling showed me the worktree's current
    `CLAUDE.md` once; the main checkout's initial git status listed the names
    `PLAN.md` and `REVIEW.md`, which I did not open. Background command output that the tooling
    wrote under `/private/tmp` was not opened; every log I read is under `target/c8a/`.
  - Black-box oracle runs: 14 runs of `target/libucl-oracle/ucl-dump -F` in
    `target/c8a/scratch/rb/`: libucl's own golden output (config, JSON, YAML) of
    `time_suffix_overflow_infinite`, `floats_boundaries` and `floats_exact_decimal_expansion`
    (9 runs: `-inf` reads back as the string `"-inf"`; the 309-character `%f` output of −1e300
    reads back as a string), of `time_subnormal_through_ms` and `min_normal` (config, JSON: 4
    runs, rejected), and one input with the `%f` outputs of ±1e117 to ±1e120 (the `-` is not
    counted: 127 characters without it read back as a string).
  - Item 4, readback test: `libucl_conformance_readback` in `tests/conformance.rs`. For every
    case the crate parses (1040), the output in each of the four formats is parsed again with
    nothing registered (no variables, no handler, `no-filevars`, so by §7.5 nothing expands) and
    compared with the value written by a comparator that allows only the §10.8 losses: times as
    floats, bytes written `\uFFFD` as U+FFFD, float precision by the row of §10.3 (expected value
    from Rust's `{:.1}`, `{:.14e}` and `{:.6}`, checked against the spec's tie examples), a `%f`
    output of 127 or more characters as a string, the rejection of the smallest normal float's
    `%.15g` output, JSON and YAML multi-value entries as arrays with the first-array loss of
    §10.7, the empty key as `null`, values of different spellings under separate keys in config,
    and no priorities, comments or marks. Keys written bare that cannot be read bare make an
    output unreadable, and may read back as anything, only when each such key is in one of the
    §10.8 categories (`;}#,`, a first byte that cannot start a key, the empty key, a key that a
    macro or a collection makes bare); any other such key is a difference. A mutation check
    (reading back with `key-lowercase`) makes 39–45 outputs per format fail. Result per format
    [same value, unreadable, rejected float, pending]: config [1022, 11, 2, 5], JSON and compact
    JSON [1033, 0, 2, 5], YAML [1024, 9, 2, 5]. The 5 pending cases (all four formats) are in
    `READBACK_PENDING` with QUESTIONS.md #57 (−∞ is written `-inf`, which reads back as a string:
    `time_infinite_in_json_form`, `time_suffix_overflow_infinite`, `floats_boundaries`) and #58
    ((a) the 127-character rule does not count the `-`: `floats_exact_decimal_expansion`; (b) the
    `%.15g` output of a subnormal time is rejected, which the bullet names only for the smallest
    normal float: `time_subnormal_through_ms`). The oracle reads the golden files the same way,
    so these are spec gaps, not crate bugs.
  - Item 3, search directories: `Parser::set_search_path`, `clear_search_path`, `search_path`,
    `ParserBuilder::with_search_path`. The list starts every parse as a `path` list would (§9.4),
    also in macro argument documents (my choice: a parser setting, like the loader and base
    directory, which argument documents keep; a document's own `path` list does not reach them,
    as before). Directories are text, as in `path`. Tests mirror each in-document `path=` case.
  - Item 2, input limit: `Parser::set_max_input_bytes`, `max_input_bytes`,
    `ParserBuilder::with_max_input_bytes`, `ErrorKind::InputTooLarge { limit, path }`, and
    `Loader::read_limited` (default: `read`; `FsLoader` and `MemoryLoader` stop after `limit + 1`
    bytes). One budget per parse for the document and every file `.include`, `.try_include` and
    `.load` read, repeats and glob matches counted, shared with macro argument documents; not
    softened by `try=true` or `.try_include`. Default: no limit, as libucl, whose limits are
    opt-in (§11.2); documented in the README, the crate docs and the setter.
  - Item 1, error positions. API: `UclError::Deserialize(error::DeserializeError)` for every
    deserialization error (`UclError::Serde` is left for serialization);
    `DeserializeError::{error, into_error, path, at_key, position, file}`; `UclError::file`;
    `UclError::position` covers it. Design, final: the value deserializer has two variants
    (`ValueDeserializer<const PATHS: bool>`). Without paths it is the code of `cf800e8`. With
    paths, each map and sequence access puts its step in front of an error as it returns (the
    values of a multi-value entry become `Key { key, index }`, a single value read as a sequence
    adds no step, an array read as a map uses `Index`), keys are lent to the target and kept for
    the path, and an owned-string map key gets a copy. The text entry points (`from_str`,
    `from_slice`, `from_reader`, `from_file`, `from_str_with_*`) deserialize without paths; only
    when that fails do they parse the document again in a locating mode and deserialize the
    target again with paths, and they use the second error, with its path and position, only if
    its message equals the first's (a target that fails differently, or not at all, leaves the
    first error without them). `UclDeserializer`, which cannot run its visitor twice,
    deserializes with paths and then locates. `from_value` deserializes without paths and gives
    neither path nor position. Locating mode: the output-facts tree records a location (input
    unit, value offset, key offset) for every value in a side vector, and the operations that
    keep facts with their values (replacement, `no-implicit-arrays` collection, the merge scalar
    quirk, `.inherit` copies, included units) move the locations; the first value of a merged
    container keeps its place; a value with no location of its own gets the nearest around it.
    The second parse asks the loader for included files again and gives the variable handler's
    answers from the first parse instead of calling it (the first parse records them). Costs
    when nothing fails: no per-value memory (value model unchanged, `UclError` 96 bytes, facts
    nodes unchanged), a copy of each variable-handler answer, and for `UclDeserializer` only,
    the path-recording pass. Benchmarks, A/B against `cf800e8` built in its own target
    directory, run back to back (criterion, 30 samples, medians): every `parse/*` group within
    −3.9% to +0.6%; `serde/deserialize-1000/from_str` 5886 → 5811 µs, `from_value` 768 → 752 µs;
    `UclDeserializer` 6008 µs in the same run (+3.4% over `from_str`); serialization groups
    within noise (−2.8% to −0.6%, and −13.9% for `nested-mixed-1000/to_string`, whose code is
    unchanged). Error path (new group `serde/deserialize-error-1000`, the last service's `ratio`
    wrong): `from_str` 14.56 ms, about 2.5 times its success path; `from_value` 964 µs.
  - Benchmark history, for the record: the first design recorded paths in every run (`fee6801`).
    Against a baseline saved at the start of the session it showed `from_value` +15.8% and
    `from_str` +2.5%. I then wrongly concluded that this was machine drift: my A/B runs of the
    archived `cf800e8` shared the worktree's target directory, and `git archive` gives files the
    commit time as mtime, so cargo reused the worktree's bench binary ("Finished in 0.04s") and
    both sides were the new code. With a separate target directory the regression was real
    (`from_value` 763 → 884 µs). Bisecting (`git show cf800e8:src/de/value.rs` in the current
    tree: 748 µs) put the whole cost in the deserializer's path recording, spread over key
    copies, per-entry error checks and per-entry state, and no variant recorded paths for less
    than 10–20% of `from_value`'s time; hence the final design, in `bb7cca2`.
  - Tests: `tests/error_positions.rs` (scalars in every form, nesting, elements,
    missing fields at the object or section name, the document, multi-value entries, sequences of
    one, arrays and values as maps, unknown fields and bad map keys at the key, enums, priorities,
    merged objects and arrays, the merge quirk, collections, `key-lowercase` renames, included
    files at depth, `duplicate="merge"` and `key=` includes, `.inherit` copies from the document
    and from an included file, `TooDeep`, `from_value` without path, `UclDeserializer` with path
    and position, a target that fails differently the second time, a stateful handler asked
    once, every text entry point, `from_file` with an include; 15 tests); unit tests for
    locations in the facts tree, the limit, the loaders' limited reads and search directories.
    `tests/api_tests.rs` and `tests/stack_depth.rs` updated for the new variant; three examples
    and the README assert positions.
  - Results: `cargo test` 350 tests in 17 binaries; conformance unchanged: new core 1393 of 1397,
    emitters 1040 of 1044, `xfail-new.txt` and `xfail-emit.txt` unchanged; the readback test as
    above. `scripts/ci.sh` passes with Rust 1.98.1 and with 1.88.0 at `3c0393f`.
  - For the lead: `CLAUDE.md` says the conformance target runs two tests; it now runs three
    (`tests/conformance/README.md` is updated). I did not edit `CLAUDE.md`. `CHANGELOG.md`: the
    additions and the changed error variant are in the unpublished 0.2.0 section.
  - Questions: #57, #58.
  - Commits: `0e98c29`, `e4f790e`, `3da31c4`, `fee6801`, `9dd9b66`, `bb7cca2`, `d36d5a6`,
    `f80a3a5`, `3c0393f`, and the `C8a:` commit that adds this entry.
  - Attestation: I did not read libucl source code or any forbidden input listed in
    docs/clean-room/PROTOCOL.md.
- 2026-09-25 — Role: spec team. Item: C0, release `spec-v10`.
  - Inputs: libucl source at the pinned commit (the parser's input, macro and chunk handling and
    the header's documented API), black-box runs of the oracle, `docs/spec/`,
    `docs/clean-room/QUESTIONS.md` #57–#58 and WORKLIST C8b.
  - Oracle: `tools/ucl-dump` gains `-i MODE:PRIORITY:STRATEGY:PATH` (further inputs, as text or as
    a file), `-R` (test macros `.emit`, `.seen`, `.fail`, `.ctx`) and `-O` (the `.seen` handler
    registered as `priority`). `scripts/regen-golden.sh` maps `<case>.inputs` and the flags
    `registered-macros` and `registered-priority-override`, and regenerates `pending/` too. A
    regeneration changed no existing golden file, and two runs gave identical files.
  - Spec: new §13 (several inputs into one parser; registered macros); §10.8 answers #57 (−∞ reads
    back as the string `"-inf"`) and #58 (the `%f` length rule of §5.3; every `%.15g` output below
    the normal range is rejected); cross-references in §9.2, §9.4 and §12.8; README sections,
    *Known gaps* and coverage (1400 committed cases, 101 pending).
  - Cases: 3 in `cases/spec/10-output/` (`readback_minus_infinity_is_string`,
    `readback_percent_f_length_limit`, `readback_fifteen_digit_subnormal_error`), which the crate
    passes; 101 in `tests/conformance/pending/13-inputs/`, which the runners do not read yet. Not
    committed: a `{` after a zero-byte first input (crashes libucl) and the file variables set by a
    file input (the golden file would hold the checkout path).
  - Commits: `3c30bf3` (oracle, script, cases), `b8f338d` (spec; tagged `spec-v10`).
  - The spec contains behaviour only.
- 2026-09-25 — Role: implementation team. Item: C8b (several inputs into one parser, registered
  macros; spec-v10 §13).
  - Inputs consulted: `docs/clean-room/` (PROTOCOL, WORKLIST C8b and its project decisions,
    QUESTIONS, this log); `docs/spec/` at HEAD, which `git diff --stat spec-v10 HEAD --
    docs/spec/` shows equal to `spec-v10`: §13 in full, and `git diff spec-v9 spec-v10 --
    docs/spec/` (§9.2, §9.4, §10.8, §12.8, README); the conformance suite (the 101 cases of
    `pending/13-inputs/` with their `.flags`, `.inputs`, `files/` and golden files,
    `pending/README.md`, `tests/conformance/README.md`, `tests/conformance.rs`);
    `scripts/regen-golden.sh` (its flag and input mapping and case loop, to run the oracle as the
    golden files were made) and `scripts/ci.sh`; the crate's own files (`src/parse/`,
    `src/value.rs`, `src/de.rs`, `src/error.rs`, `src/lib.rs`, `README.md`, `CHANGELOG.md`).
    `grep` ran on `src/` (the old `lexer.rs` and `parser.rs` no longer exist; excluded anyway),
    `tests/`, `examples/`, `scripts/` and single named files. Git: `git log --oneline`, `git tag`,
    `git status`, `git show --stat` of HEAD at the start (`f6453ce`, WORKLIST only), the spec
    diffs above, and `git show --stat` and the WORKLIST diff of `6c37b7c` (see below); scratch
    worktrees of this session's own commit under `target/c8b/wt`, to run `scripts/ci.sh` on it.
    The tooling showed me the worktree's current `CLAUDE.md`; the main checkout's initial git
    status listed the names `PLAN.md` and `REVIEW.md`, which I did not open. One background
    command's output went under `/private/tmp`; I did not open it, and wrote every later log
    under `target/c8b/`. The advisor tool (a reviewer that sees this session's transcript) was
    called twice: the first call, before the design, timed out; the second reviewed the finished
    work.
  - Black-box oracle runs (`target/libucl-oracle/ucl-dump` with `-i`, `-R`, `-O`, `-c`, `-F`,
    `-v`, `-e`), in `target/c8b/probe/`: about 400 single probes of the joins between inputs and
    of the test macros (`target/c8b/p1.py`–`p22.py`), and four differential rounds that compare
    the crate with the oracle on 39,017 generated combinations of two or three inputs (entries,
    separators, whitespace, comments, closed roots, stops, includes, inputs given as files, the
    test macros, priorities and strategies; `target/c8b/gen_diff*.py`). After the fixes below
    they match everywhere, apart from the byte saved after a block comment that ends an input,
    which §12.5 leaves uncertain.
  - Work: `Parser::inputs`, `Inputs` and `Input` (§13.1), with `Parser::parse` and
    `Parser::parse_file` now parses of one input; `Parser::register_macro`,
    `Parser::register_context_macro`, `ParserBuilder::with_macro` and
    `ParserBuilder::with_context_macro`, `MacroCall`, `MacroError` and `MacroHandler` (§13.2);
    new error kinds `TooManyInputs`, `AfterRoot`, `UnseparatedInput`, `MacroFailed` and
    `MacroStopped` (`Error::is_stopped` covers the last). The parser core keeps a document state
    from one input to the next (`Document`, `Boundary` in `src/parse/core.rs`). The oracle runs
    showed rules at the joins that §13 does not state, implemented as observed and asked as
    QUESTIONS.md #59: whitespace-only later inputs, values on a following line in a later input,
    section names after a separator at the end of an input, what may follow a closed root,
    comments at the joins, included files that stop and stay open, the file of a text input,
    and the copies the test macros make. The conformance runner reads `.inputs`, maps
    `registered-macros` and `registered-priority-override`, and registers handlers equivalent to
    the four test macros; `tests/inputs_and_macros.rs` covers the API (21 tests).
  - Results: the 101 cases, now in `cases/spec/13-inputs/`, all pass: new core 1501 cases, 1497
    pass (was 1400 and 1396), 4 expected failures unchanged; emitters 1113 cases with output,
    1109 match in every format (was 1046 and 1042; config with comments 56 of 56); readback
    1109 cases parse, the other differences unchanged at 5 per format (#57 and #58, whose
    `READBACK_PENDING` entries this item leaves alone). `xfail-new.txt` and `xfail-emit.txt`
    unchanged. `scripts/ci.sh` passes with stable Rust and with `RUSTUP_TOOLCHAIN=1.88`.
  - For the lead: the case move (`git mv` of `pending/13-inputs/` to `cases/spec/13-inputs/`
    and `git rm` of `pending/README.md`) was staged in this worktree's index when another
    session's commit `6c37b7c` ("C8c: target the latest stable Rust (1.98)", WORKLIST only)
    was made, and that commit took it in. `6c37b7c` therefore holds the move without the runner
    that reads the cases, so `scripts/ci.sh` fails at `6c37b7c` itself; it passes at this
    item's commits. I did not rewrite `6c37b7c`. `docs/COMPATIBILITY.md` and the §13 note
    "the cases are in `pending/13-inputs/`" belong to the spec team and are unchanged; wording
    for COMPATIBILITY.md is in my report.
  - Questions: #59.
  - Commits: `e8af70e` (API, core, tests, runner), `91a9388` (README), and the `C8b:` commit
    that adds this entry.
  - Attestation: I did not read libucl source code or any forbidden input listed in
    docs/clean-room/PROTOCOL.md.

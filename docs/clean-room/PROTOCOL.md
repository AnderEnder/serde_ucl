# Clean-Room Protocol

The implementation in `src/` must not be derived from libucl's source code. Tests, test tooling
and golden files may be derived from libucl.

## Roles

The classic two-team model: one team reads libucl's code and writes a specification; a separate
team, which has never seen the code, implements from that specification.

| Role | Who | May read libucl source | Writes |
| --- | --- | --- | --- |
| Spec team | Participants who read libucl source (including everyone involved before the clean room started) | yes | `docs/spec/`, conformance cases, oracle tooling. **Never `src/`.** |
| Spec reviewer | An independent reviewer; counsel before release | no need | review notes; releases a spec version |
| Implementation team | Fresh participants who have never read libucl source | **no** | `src/`, tests, examples, docs |

### What the spec may contain

Only observable behaviour: which inputs are accepted, what value each one produces, what is
rejected, and what the output formats look like. Every rule cites the conformance cases that show
it, and every example is checked against the oracle.

The spec must not contain libucl code, pseudo-code that follows its control flow, internal names
(functions, variables, internal flags or states), or an organisation that mirrors its source
files. Rules are organised by format feature. Names from libucl's public interface may appear
where they help, such as public parser-flag names in the flags section.

### Release

A spec version reaches implementers only after review: the reviewer checks for the prohibited
content above, the spec team fixes any findings, and the version is committed and tagged
`spec-vN`. Implementers work from the latest released tag. Questions from implementers go to the
spec team in writing (`docs/clean-room/QUESTIONS.md`); answers land as a new spec version, never as
code or hints.

## Allowed inputs for the implementation team

- The released `docs/spec/` (latest `spec-vN` tag) and `docs/clean-room/` (this protocol, the
  work list, the log, QUESTIONS.md).
- libucl's public documentation: its `README.md` and `doc/*.md`. Implementers get this as a copy
  that contains no source files.
- The conformance suite: case inputs, `.flags` files, golden files, xfail lists, and
  `tests/conformance.rs`.
- Black-box runs of the oracle: `scripts/regen-golden.sh` (builds libucl and dumps every case) and
  the built `target/libucl-oracle/ucl-dump`. Running them is allowed; opening the libucl files the
  script checks out is not.
- The crate's own code, except the files listed below, and general Rust, serde and crate documentation.

## Forbidden inputs

- libucl source files (`*.c`, `*.h`, build files) anywhere: `target/libucl-oracle/libucl/`, any
  other clone, GitHub source views, and copies in search results or mirrors.
- `REVIEW.md`, `PLAN.md` and `PROGRESS.md`. They were written by the oracle side and contain
  references to libucl internals. Since C5 they are kept only on branch `quarantine/oracle-notes`,
  not in the working tree.
- Branches under `quarantine/`.
- Git history of `src/` before commit `ef8007e`: no `git log -p`, `git show` or `git diff` against
  earlier commits for `src/` paths. Earlier revisions of `src/value.rs` contain the removed code
  that was derived from libucl.
- The old `src/lexer.rs` and `src/parser.rs`, deleted at the cut-over (C5b): their contents in
  git history (`git show`, `git log -p`, `git diff` or checkouts of any revision that has them).
  They predate the clean room and are not a source of design or code.
- Git history of `CLAUDE.md`: earlier revisions contain notes on libucl internals.
- Anything else an oracle-side participant writes, other than work-item goals in
  `docs/clean-room/WORKLIST.md` and conformance cases.

If an implementer reads a forbidden input, even by accident, it stops, records the exposure in
`docs/clean-room/LOG.md`, and is replaced by a new participant. Code written after the exposure is
moved to a `quarantine/` branch.

## Provenance

Every work session, on either team, appends an entry to `docs/clean-room/LOG.md` with the date, role, work
item, the inputs consulted, and the commits produced. Implementers add this attestation: *"I did not
read libucl source code or any forbidden input listed in docs/clean-room/PROTOCOL.md."* Spec-team
entries state that the spec contains behaviour only.

Commit messages start with the work-item ID (`C0: …` for the spec, `C2.3: …` for implementation).

## Legal review

This protocol is an engineering control, not legal advice. Before release, counsel should review
`docs/spec/`, this protocol and `LOG.md`.

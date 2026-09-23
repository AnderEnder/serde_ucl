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

## C5 — Cut-over

Switch the public API (`from_str`, `from_slice`, `from_reader`, `from_file`, the parser builder)
to the new core. Delete `src/lexer.rs` and `src/parser.rs` (including the streaming lexer and the
plugin/hook system), and the tests, examples and benches that exercise constructs libucl doesn't
support. Rewrite the remaining examples on the new API. Done when the old conformance xfail list is
retired and `xfail-new.txt` holds only justified divergences.

## C6 — Docs, packaging, CI

Make every Cargo feature real (referenced by `#[cfg]`) or remove it. Correct README and crate docs.
Add `docs/COMPATIBILITY.md` (divergences and quirks, each with its case). Set Cargo metadata
(`repository`, `authors`, `rust-version`) and add license files for `MIT OR Apache-2.0`. CI: clippy
with `-D warnings`, `cargo fmt --check`, no wall-clock assertions in tests, and a nightly job that
regenerates golden files and fails on drift.

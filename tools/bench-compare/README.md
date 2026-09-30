# bench-compare

The comparison of serde_ucl with libucl and serde_json in the README (*Against libucl and
serde_json*), as a script that reproduces it (clean-room work item C14). Spec-team tooling: it
builds and runs libucl.

    scripts/bench-compare.sh [ROUNDS]    # default 3 rounds

The script:

1. builds libucl at the oracle's pinned commit (`scripts/regen-golden.sh`) as a CMake Release
   build (`-O3`) under `target/bench-compare/libucl-build`, reusing the oracle's checkout
   `target/libucl-oracle/libucl` or cloning it (`LIBUCL_DIR`, `LIBUCL_COMMIT` override);
2. builds `lucl.c` against it (`-O3`), and this package with the crate's release settings (fat
   LTO, one codegen unit);
3. builds a second copy of this package against the previous release, the latest `v*` tag
   reachable from HEAD (`BASELINE` overrides it; `BASELINE=none` leaves it out), taken from git
   with `git archive`; the copy uses this tree's documents module, so both time the same
   documents, and runs as `serde_ucl@VERSION`, without serde_json;
4. writes the benchmarks' generated documents (`benches/common/mod.rs`, including both seeded
   irregular configurations) to `target/bench-compare/documents/` and reads any pinned JSON
   documents already in `target/bench-corpus/`;
5. runs this package, the copy and `lucl` in turns for ROUNDS rounds, the order rotated by one
   each round, and appends their lines to `target/bench-compare/results.txt`, printing the load
   with each run;
6. prints the medians over the rounds as tables (`bench-compare summarize`): this version, the
   previous release with the change against it, libucl and serde_json.

What is timed, each the median of 31 samples of at least 5 ms of repetitions:

| Kind | serde_ucl | libucl | serde_json |
| --- | --- | --- | --- |
| `parse` | `parse::parse`, or a `Parser` with a file loader, corpus base directory, `ABI=unknown` and the document's variables for rspamd | `ucl_parser_new(0)`, `ucl_parser_add_chunk`, `ucl_parser_get_object`, `ucl_parser_free`; for rspamd, the process runs from the corpus directory with matching variables | `from_str::<serde_json::Value>` |
| `parse-nofree` | the same, the values kept and freed after the sample's timing | the same | – |
| `typed` | `from_str` into a struct with `String` fields | – | `from_str`, the same struct |
| `typed-borrowed` | `from_str` into a struct with `&str` fields | – | `from_str`, the same struct |

The results include freeing the value (except `parse-nofree`), on both sides. The generated
documents are JSON with 10,000 and 1,000 records, a configuration with 1,000 services, the
three-entry document in UCL and JSON, and the seeded irregular configurations of 78,466 and
639,049 bytes. The three pinned JSON files (`twitter.json`, `citm_catalog.json`, `canada.json`)
are timed when present in `target/bench-corpus/`; each missing file is skipped with a message.
`benches/fetch-documents.sh` fetches and verifies them, but building and running the comparison
does not fetch them. The remaining inputs are three documents of the rspamd corpus
(`benches/corpus/README.md`): `groups.conf` with the 14 files it includes, `composites.conf` and
`scores.d/rbl_group.conf`. All applicable tools read the same document files in each round.
The Rust corpus parser follows `benches/common/files.rs` for this version and `v0.5.0`,
including optional include attempts in `composites.conf`. libucl's chunk API has no separate
base-directory argument in this harness, so its corpus run changes the process working directory
to the same directory. `CONFDIR=.` is registered only for `groups.conf`; `ABI=unknown` is
registered for all three documents. This matches the files and variable values used by the
benchmark and document check, while the languages' include implementations remain their own.

Each tool's output is collected before it is added to a round. A failing tool stops the script
without printing a summary or replacing the last complete `results.txt`; the incomplete run is
left in `results.incomplete.txt`. The summarizer rejects malformed lines and rounds with
different sets of measurements.

Each line of a run is `tool|kind|document|seconds`, with the round number in front in
`results.txt`. `bench-compare summarize FILE...` also reads the files of several runs.

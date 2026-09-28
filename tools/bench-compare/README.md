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
4. writes the benchmarks' generated documents (`benches/common/mod.rs`, which this package
   includes as a module) to `target/bench-compare/documents/`;
5. runs this package, the copy and `lucl` in turns for ROUNDS rounds, the order rotated by one
   each round, and appends their lines to `target/bench-compare/results.txt`, printing the load
   with each run;
6. prints the medians over the rounds as tables (`bench-compare summarize`): this version, the
   previous release with the change against it, libucl and serde_json.

What is timed, each the median of 31 samples of at least 5 ms of repetitions:

| Kind | serde_ucl | libucl | serde_json |
| --- | --- | --- | --- |
| `parse` | `parse::parse`, or a `Parser` with a file loader and `CONFDIR` for the corpus document that includes files | `ucl_parser_new(0)`, `ucl_parser_add_chunk`, `ucl_parser_get_object`, `ucl_parser_free` | `from_str::<serde_json::Value>` |
| `parse-nofree` | the same, the values kept and freed after the sample's timing | the same | – |
| `typed` | `from_str` into a struct with `String` fields | – | `from_str`, the same struct |
| `typed-borrowed` | `from_str` into a struct with `&str` fields | – | `from_str`, the same struct |

The results include freeing the value (except `parse-nofree`), on both sides. The documents are
the generated JSON documents of 10,000 and 1,000 records, the configuration of 1,000 services,
the three-entry document in UCL and in JSON, and three documents of the rspamd corpus
(`benches/corpus/README.md`): `groups.conf` with the 14 files it includes, `composites.conf` and
`scores.d/rbl_group.conf`.

Each line of a run is `tool|kind|document|seconds`, with the round number in front in
`results.txt`. `bench-compare summarize FILE...` also reads the files of several runs.

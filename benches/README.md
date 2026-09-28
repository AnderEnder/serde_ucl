# Benchmarks

Three [criterion](https://docs.rs/criterion) benchmarks measure the throughput of the crate's
public API. Every generated document is valid libucl (checked with the libucl oracle); the
generators are in `common/mod.rs` and `common/irregular.rs`.

| Benchmark | Groups | Measures |
| --- | --- | --- |
| `parse_benchmarks` | `parse/config/{10,100,1000}`, `parse/small/{reused-parser,new-parser}`, `parse/config-100-flags/{save-comments,no-implicit-arrays,key-lowercase}`, `parse/json/{100,1000}`, `parse/nested/{10,500,1000}`, `parse/nested-mixed-1000/{default,save-comments}`, `parse/variables/1000`, `parse/irregular/{60k,600k}`, `parse/json-corpus/{twitter,citm_catalog,canada}`, `parse/corpus/{rspamd-groups,rspamd-composites,rspamd-rbl_group}` | `parse::Parser::parse`, in input bytes per second |
| `emit_benchmarks` | `emit/config-1000/{config,json,json-compact,yaml}`, `emit/nested-mixed-1000/{config,json,json-compact,yaml}` | `emit::Emitter::emit` of a parsed value, in output bytes per second |
| `serde_benchmarks` | `serde/deserialize-1000/{from_str,from_str-borrowed,UclDeserializer,from_value}`, `serde/deserialize-small/{from_str,from_str-borrowed}`, `serde/deserialize-error-1000/{from_str,from_value}`, `serde/serialize-1000/{to_string,to_json_string,to_json_string_compact,to_yaml_string}`, `serde/to_value-1000`, `serde/nested-mixed-1000/{from_str,to_string,to_json_string_compact}`, `serde/irregular-{60k,600k}/{UclValue,IgnoredAny}`, `serde/json-corpus-{twitter,citm_catalog,canada}/{UclValue,IgnoredAny}`, `serde/corpus-{rspamd-groups,rspamd-composites,rspamd-rbl_group}/{UclValue,IgnoredAny}` | deserializing into a typed struct (`-borrowed`: one that borrows its keys and strings from the input), or a `UclValue` for the nested document (input bytes per second), and serializing it (output bytes per second); `deserialize-error` deserializes a `config(1000)` whose last value does not fit, where `from_str` also finds the value's path and position by parsing and deserializing again. The documents without a typed struct go through `from_str` (the corpus: `UclDeserializer`) into a `UclValue`, which takes a parse that owns its strings, and into `IgnoredAny`, which takes one that borrows them |

The documents:

- `config(n)`: a configuration in the style UCL is written in, about 600 bytes per service:
  braced sections, nginx-style `key value` entries, a repeated key, arrays, multipliers, times,
  the three quoted string forms, a heredoc, and `#` and nested block comments.
- `SMALL`: three entries (a string, an integer and a boolean), where the fixed cost of a parse
  dominates; `parse/small/new-parser` includes making the parser.
- `json(n)`: a JSON document, an array of `n` records.
- `nested(n)`: objects nested `n` deep.
- `nested_mixed(n)`: objects nested `n` deep, one per line, each with a comment, a number and a
  single-quoted string. The indented output formats of this document grow with depth times lines.
- `variables(n)`: `n` lines of strings with braced and unbraced variable references.
- `irregular(seed, size)` (`common/irregular.rs`): a configuration of at least `size` bytes that
  varies where `config(n)` does not: keys and their forms, section sizes (from empty to 120
  entries, on both sides of the 16 keys of a small object), the order of entries, the depth
  (sections up to 7 deep, named sections, and arrays and objects inside them), and strings
  from one byte to 16 KB, in several alphabets, with and without escapes. It uses every value
  form of `config(n)` as well, and JSON-style objects. The same seed and size give the same
  document; the module documentation lists the distributions. The benchmarks use seed 1 at
  60 000 bytes (78 466 bytes) and seed 2 at 600 000 bytes (639 049 bytes), somewhat more than
  `config(100)` (50 571 bytes) and `config(1000)` (510 495 bytes); `tests/bench_documents.rs`
  pins them by digest.
- The JSON documents that serde_json and simd-json publish figures for, `twitter.json` (632 KB),
  `citm_catalog.json` (1.7 MB) and `canada.json` (2.3 MB), in `target/bench-corpus/`. They are
  not committed, because their licences are unclear: `benches/fetch-documents.sh` fetches them
  from [serde-rs/json-benchmark](https://github.com/serde-rs/json-benchmark) at a pinned commit
  and checks their SHA-256 digests.
- Three rspamd configurations from `corpus/` (source, commit and licence in
  `corpus/README.md`), listed in `common/files.rs` (`CORPUS`): `rspamd-groups`, `groups.conf`
  with the 14 files of `scores.d/` it includes (55 215 bytes read, the throughput counts them
  all); `rspamd-composites`, `composites.conf` (9 155 bytes); and `rspamd-rbl_group`,
  `scores.d/rbl_group.conf` (13 404 bytes). They are parsed as their check parses them: with
  the filesystem loader, `corpus/rspamd/` as the base directory, the variable `ABI` =
  `unknown`, and for `groups.conf` the variable `CONFDIR` = `.`. The `serde/corpus-*` groups
  therefore go through `UclDeserializer::from_parser`, and make that parser in each iteration.

The groups of the JSON documents skip a document that is missing or does not parse, with a
message, so the benchmarks build and run without the network. The corpus is committed and
checked, so a corpus document that does not parse makes its benchmark fail.

## Checking the documents

Every document is checked against libucl the way the differential fuzzer checks one
(`ucl-differential --check`, `fuzz/README.md`): libucl parses it, and the crate gives the same
result.

```bash
benches/fetch-documents.sh        # the JSON documents, into target/bench-corpus/
benches/check-documents.sh        # the generated documents, the JSON documents and the corpus
benches/check-documents.sh 50     # and 50 more generated documents, to test the generator
```

The check needs the oracle, `target/libucl-oracle/ucl-dump`, which `scripts/regen-golden.sh`
builds. A document passes when libucl accepts it and the verdict is `agree`; each result is in
`target/bench-documents/checks/`. A difference goes to `docs/clean-room/QUESTIONS.md`. After a
change to the generator, run the check and update the digests in `tests/bench_documents.rs`.

## Running

```bash
# All benchmarks
cargo bench

# One benchmark, or one group
cargo bench --bench parse_benchmarks
cargo bench --bench serde_benchmarks -- serde/deserialize

# Criterion's options apply to every benchmark: without plots, or a quick run
cargo bench -- --noplot
cargo bench -- --warm-up-time 0.1 --measurement-time 0.5
```

Criterion writes HTML reports to `target/criterion/report/index.html`. Each group runs 30 samples
after one second of warm-up, over three seconds; results vary with the machine and its load.

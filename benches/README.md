# Benchmarks

Three [criterion](https://docs.rs/criterion) benchmarks measure the throughput of the crate's
public API, and `alloc_counts` counts its allocations. Every generated document is valid libucl
(checked with the libucl oracle); the generators are in `common/mod.rs`, `common/irregular.rs`
and `common/workloads.rs`.

| Benchmark | Groups | Measures |
| --- | --- | --- |
| `parse_benchmarks` | `parse/config/{10,100,1000}`, `parse/small/{reused-parser,new-parser}`, `parse/config-100-flags/{save-comments,no-implicit-arrays,key-lowercase}`, `parse/json/{100,1000}`, `parse/nested/{10,500,1000}`, `parse/nested-mixed-1000/{default,save-comments}`, `parse/variables/1000`, `parse/irregular/{60k,600k}`, `parse/json-corpus/{twitter,citm_catalog,canada}`, `parse/corpus/{rspamd-groups,rspamd-composites,rspamd-rbl_group}`, `parse/json-compact/{json-1000,twitter,citm_catalog,canada}`, and the workloads: `parse/strings/*`, `parse/keys/*`, `parse/numbers/*`, `parse/containers/*`, `parse/deep/*`, `parse/comments/*`, `parse/whitespace/*`, `parse/objects/*` (below) | `parse::Parser::parse`, in input bytes per second |
| `emit_benchmarks` | `emit/config-1000/{config,json,json-compact,yaml}`, `emit/nested-mixed-1000/{config,json,json-compact,yaml}` | `emit::Emitter::emit` of a parsed value, in output bytes per second |
| `serde_benchmarks` | `serde/deserialize-1000/{from_str,from_str-borrowed,UclDeserializer,from_value,UclValue,IgnoredAny}`, `serde/deserialize-small/{from_str,from_str-borrowed,UclValue,IgnoredAny}`, `serde/deserialize-error-1000/{from_str,from_value}`, `serde/serialize-1000/{to_string,to_json_string,to_json_string_compact,to_yaml_string}`, `serde/to_value-1000`, `serde/nested-mixed-1000/{from_str,to_string,to_json_string_compact}`, `serde/irregular-{60k,600k}/{UclValue,IgnoredAny}`, `serde/json-corpus-{twitter,citm_catalog,canada}/{UclValue,IgnoredAny,typed}`, `serde_json/json-corpus-{twitter,citm_catalog,canada}/{Value,IgnoredAny,typed}`, `serde/json-1000/{UclValue,IgnoredAny,typed}`, `serde_json/json-1000/{Value,IgnoredAny,typed}`, `serde/corpus-{rspamd-groups,rspamd-composites,rspamd-rbl_group}/{UclValue,IgnoredAny}` | deserializing into a typed struct (`-borrowed`: one that borrows its keys and strings from the input), or a `UclValue` for the nested document (input bytes per second), and serializing it (output bytes per second); `deserialize-error` deserializes a `config(1000)` whose last value does not fit, where `from_str` also finds the value's path and position by parsing and deserializing again. `from_str` also takes each document into a `UclValue`, which takes a parse that owns its strings, and into `IgnoredAny`, which takes one that borrows them (the corpus: through `UclDeserializer`). The JSON documents go into their typed forms (`typed`, `common/typed.rs`) too, and through `serde_json::from_str` into `serde_json::Value`, `IgnoredAny` and the same typed forms |
| `alloc_counts` | the documents of the serde groups, the compact JSON documents and the workloads | not a timing: for each document and entry point (`parse` with a reused parser, `parse-new`, `from_str` into `UclValue`, the typed form and `IgnoredAny`), the allocations, reallocations, frees, bytes requested, peak and kept live bytes and allocations per KB of one call, with a counting allocator; a Markdown table |

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

- `json-compact`: the JSON documents without the whitespace outside their strings
  (`common::compact_json`), every other byte as it is, so their numbers and escapes stay as
  written: `json(1000)` (121 961 bytes) and the three documents above (466 906, 500 299 and
  2 251 027 bytes).
- The workloads (`common/workloads.rs`, clean-room work item C16), each of which stresses one
  scan or one structure, so that a change to it can be measured where it matters and shown to
  make no input of its kind slower. Each has at least 200 000 bytes:
  - `parse/strings`: array elements of one string form each: double-quoted ASCII of 1, 8, 16,
    24, 32, 64, 256 and 4096 bytes (`dq-N`); at 64 bytes, double-quoted with an escape every
    eight bytes (`dq-escaped-64`), double-quoted non-ASCII (`dq-utf8-64`), single-quoted
    (`sq-64`, also `sq-4096`) and unquoted (`unquoted-64`); heredocs of 1024 bytes.
  - `parse/keys`: sections of eight entries with bare keys of 4, 22, 23 and 64 bytes (keys of up
    to 22 bytes are stored inline) and double-quoted keys of 23 bytes.
  - `parse/numbers`: arrays of 16 numbers to a line: integers of 1, 4, 8, 16 and 19 digits,
    negative 8-digit integers, floats with a few digits (`float-short`), with 15 and with 17
    significant digits, with exponents within ±22 and beyond, and numbers with multiplier and
    time suffixes.
  - `parse/containers`: arrays of empty objects and of empty arrays, sections whose values are
    empty containers, and arrays of one-key objects and of one-element arrays.
  - `parse/deep`: arrays and JSON-style objects nested 1000 deep (`arrays-1000`,
    `json-objects-1000`), one chain after another, and sections nested 16 deep
    (`repeated-16`).
  - `parse/comments`: sections whose entries come with comments, about two thirds of the bytes:
    `#` lines, `#` after values, block comments of prose, `*` banners, text full of `/`, quoted
    parts, text dense in `*`, `/` and `"`, and nested comments; `hash-after-values-saved` and
    `block-prose-saved` parse two of them with `save-comments`.
  - `parse/whitespace`: the same kind of sections without any whitespace (`compact`), aligned
    with long runs of spaces, indented with tabs, with CRLF line ends, and with blank lines.
  - `parse/objects`: sections of 8, 16, 17, 24 and 32 keys of the same length, on both sides of
    the 16 keys of a small object; 16 keys that share a 29-byte prefix; and 8 keys that each
    appear twice (implicit arrays).

  `tests/bench_documents.rs` pins each one by digest and checks that it holds what it is meant
  to (for example, only integers in `int-*`, every section's entries with their types in the
  comment and whitespace workloads).
- The typed forms (`common/typed.rs`): `json(1000)`, `twitter`, `citm_catalog` and `canada` as
  structs that cover every field (a field that is always `null` or an empty array is
  `IgnoredAny`). The irregular documents have none, as their keys are random, and neither have
  the rspamd documents.

The groups of the JSON documents skip a document that is missing or does not deserialize with
either crate, with a message, so the benchmarks build and run without the network. The two serde
groups for each JSON document use the same input and byte throughput. The corpus is committed and
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
`target/bench-documents/checks/`. The workloads parsed with `save-comments` are checked with
their saved comments compared too. The check cannot compare documents nested 1000 deep, whose
dumps are deeper than its JSON reader reads: for those it reports that libucl accepted them, and
compares the same documents nested 20 deep (`parse-deep-*-20`). A difference goes to `docs/clean-room/QUESTIONS.md`. After a
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

`alloc_counts` prints its table instead of timing:

```bash
cargo bench --bench alloc_counts               # every document
cargo bench --bench alloc_counts -- twitter    # the documents whose name contains `twitter`
```

Criterion writes HTML reports to `target/criterion/report/index.html`. Each group runs 30 samples
after one second of warm-up, over three seconds; results vary with the machine and its load.

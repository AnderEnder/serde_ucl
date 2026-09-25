# Benchmarks

Three [criterion](https://docs.rs/criterion) benchmarks measure the throughput of the crate's
public API. Every generated document is valid libucl (checked with the libucl oracle); the
generators are in `common/mod.rs`.

| Benchmark | Groups | Measures |
| --- | --- | --- |
| `parse_benchmarks` | `parse/config/{10,100,1000}`, `parse/config-100-flags/{save-comments,no-implicit-arrays,key-lowercase}`, `parse/json/{100,1000}`, `parse/nested/{10,500}`, `parse/variables/1000` | `parse::Parser::parse`, in input bytes per second |
| `emit_benchmarks` | `emit/config-1000/{config,json,json-compact,yaml}` | `emit::Emitter::emit` of a parsed value, in output bytes per second |
| `serde_benchmarks` | `serde/deserialize-1000/{from_str,from_value}`, `serde/serialize-1000/{to_string,to_json_string,to_json_string_compact,to_yaml_string}`, `serde/to_value-1000` | deserializing into a typed struct (input bytes per second) and serializing it (output bytes per second) |

The documents:

- `config(n)`: a configuration in the style UCL is written in, about 600 bytes per service:
  braced sections, nginx-style `key value` entries, a repeated key, arrays, multipliers, times,
  the three quoted string forms, a heredoc, and `#` and nested block comments.
- `json(n)`: a JSON document, an array of `n` records.
- `nested(n)`: objects nested `n` deep.
- `variables(n)`: `n` lines of strings with braced and unbraced variable references.

## Running

```bash
# All benchmarks
cargo bench

# One benchmark, or one group
cargo bench --bench parse_benchmarks
cargo bench --bench serde_benchmarks -- serde/deserialize
```

Criterion writes HTML reports to `target/criterion/report/index.html`. Each group runs 30 samples
after one second of warm-up, over three seconds; results vary with the machine and its load.

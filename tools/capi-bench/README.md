# C API comparison with libucl

Spec-team tooling for C15: the same C11 caller links separately against the
merged `serde-ucl-c` library and libucl at the released contract's pinned commit.
Implementation participants may receive the resulting measurements as work-item
goals; this tooling and its reference checkout are not implementation inputs.

Run from the repository root:

```sh
python3 tools/capi-bench/run.py --rounds 5 --samples 9 --sample-ms 25
```

Requires Cargo, Git, CMake, a C compiler and Python 3 on native Linux or macOS.
The first run fetches the pinned reference. Outputs stay under ignored
`target/c-bench/`: `report.md`, raw `results.json`, input documents, binaries,
reference build and sanitized conformance evidence. `--output` changes that path.

The runner builds the candidate with locked Cargo release settings (fat LTO,
one codegen unit), and uses the spec-owned conformance gate to build libucl with
CMake Release and validate the candidate. The reference disables URL includes,
URL signing, Lua and utility programs, matching existing C conformance tooling.
Both benchmark executables use `-O3`, the same released public header and the
same C source. Input files are read before timing. Both parser flag sets are zero.

Measurements:

- `parse`: parser allocation, one complete input, retained-object retrieval,
  parser free, and object unref/destruction. This includes the public C object
  representation delivered by the adapter.
- `emit`: compact JSON emission from a pre-parsed object, including allocating
  and freeing each result. Initial parsing and final object cleanup are excluded.
- `phases`: a separate public-call breakdown for the two record inputs, with
  clock reads at call boundaries. It is diagnostic; aggregate timing remains
  the primary result.

The corpus has seven deterministic generated JSON/UCL inputs and an unchanged
13,404-byte Rspamd configuration from `benches/corpus/`. That document needs no
includes; filesystem access, custom variables and deferred C APIs are excluded.
Every document must emit byte-identical compact JSON with both libraries before
any measurement is accepted.

Calibration warms both executables and selects repetition counts targeting at
least 25 ms per sample. Nine samples per tool/input/mode are collected in each of
five rounds; library order alternates between samples and rounds. Reported times
are medians of the 45 samples; round-median ranges show variability. Sample
durations may vary after calibration. There are no timing-based pass/fail tests.

Raw evidence records every sample and repetition count, commit IDs, code
cleanliness, input/output/header/driver/library hashes, compiler versions,
machine details, build commands and load averages. It measures local wall time
with static linkage; it does not measure peak memory or claim another platform's
results. Both APIs clean up their objects in the parsing measurement.

See [the recorded macOS arm64 run](reports/2026-10-03-macos-arm64.md) for the first
comparison of merged C15.

To compare the Rust value API with the two C callers on these exact same inputs,
first run the main benchmark above, then:

```sh
python3 tools/capi-bench/rust_baseline.py
```

This builds a Rust public-API caller with the same release profile and dependency
versions, requires identical output for all three callers, and rotates their
parse/drop timing order. Results go to `target/c-bench/rust-baseline.{json,md}`.
See [the measured Rust/C comparison](reports/2026-10-03-rust-versus-c.md).

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

To measure a separate clean implementation worktree without copying spec-owned
tooling into it:

```sh
python3 tools/capi-bench/run.py --candidate-root /path/to/implementation --output target/c-bench-after
python3 tools/capi-bench/rust_baseline.py --output target/c-bench-after
```

The native Rust comparator uses that same candidate core and verifies its
dependency versions against the candidate lockfile. Candidate code status,
commits and library hashes distinguish measured checkpoints from final commits.

On macOS with Xcode, capture a separate sampled CPU trace of the C caller:

```sh
python3 tools/capi-bench/cpu_profile.py --benchmark-dir target/c-bench-after --name optimized-cpu
```

Profiling is separate from timing runs. Time Profiler records a disposable
benchmark process for eight seconds; the process is killed at the time limit.
The script verifies the trace and exports app stacks inside submission/unref.
Self/inclusive sample counts are retained; inclusive percentages overlap.
Leaf categories based on symbol names are approximate CPU attribution, not
allocation counts. Exact allocation counts/bytes come from the clean
implementer's own counting allocator, with its scope stated in the report.

To check that construction savings do not move into common reads, run matched
C callers against the recorded before/after archives and reference:

```sh
python3 tools/capi-bench/read_bench.py --before target/c-bench --after target/c-bench-after
```

This rejects archives or inputs whose hashes changed after the main benchmark.
It measures safe iterator creation, first child or all immediate children, and
iterator cleanup on pre-parsed 10,000-element arrays and 1,000-key objects.
Indexed lookup is measured separately. Parsing/destruction are outside timing;
the full operation includes a volatile sink and traversal count check. Counts
must agree for all libraries. Five rotated rounds of nine samples are calibrated
to approximately 30 ms, with medians and every raw sample in `reads.{json,md}`.
These timings do not represent recursive whole-tree traversal or Rust iteration.

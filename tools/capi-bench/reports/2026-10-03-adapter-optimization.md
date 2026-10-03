# C adapter optimization and profiling — 2026-10-03

The adapter is substantially faster, but the near-zero-overhead target remains unmet.
On the original 691,127-byte record input, parsing/destruction fell from 42.59 ms
to 8.87 ms (4.80× faster). The same optimized Rust core takes 6.17 ms, leaving
about 44% overhead. Pinned libucl takes 10.96 ms, so the optimized C API is
about 19% faster than libucl on that input. Escaped-string parsing remains slower
than libucl; this is not a universal performance win.

Baseline: `36da56cf7c251df37154e7b19a640bed362e2ef1` (merged C15).
Measured implementation: `dfefd2cff0c11a3f4710c9fce1720b2d5b2d273d`; independent review/provenance: `af90a298e2d07316d27254400d3b6b2cb74574c3`.
Implementation branch: `perf/c-api-adapter`. Measurement/tooling branch: `perf/c-api-vs-libucl`.
Reference: `24c8b399062ae4691168c243e3b7345ef7f31956`; released contract: `spec-v22`.

## Same-input public API measurements

Native Apple M4 Max, macOS 15.8.1 arm64; Rust 1.98.1, release fat LTO and one
codegen unit; C caller `-O3`; libucl CMake Release. Five rotated rounds of nine
samples, calibrated to 25 ms. Median complete parse/drop lifecycle; file IO
and process startup excluded. Both C flag sets are zero. All eight documents
emit byte-identical compact JSON with all three callers. The Rust comparator
builds against the same candidate core and verifies dependency versions. The
before/after C driver, released header, input and output hashes are identical.

| Input | C before µs | C after µs | Rust after µs | libucl after µs | C speedup | C / Rust |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| tiny-ucl | 10.18 | 8.38 | 0.55 | 7.88 | 1.21× | 15.34× |
| tiny-json | 10.23 | 8.25 | 0.56 | 7.82 | 1.24× | 14.73× |
| flat-1000 | 666.51 | 171.44 | 127.33 | 201.13 | 3.89× | 1.35× |
| floats-1000 | 686.62 | 192.70 | 144.44 | 220.91 | 3.56× | 1.33× |
| strings-1000 | 1133.91 | 492.41 | 385.16 | 404.46 | 2.30× | 1.28× |
| records-1000 | 4038.57 | 851.86 | 607.24 | 1083.04 | 4.74× | 1.40× |
| records-10000 | 42589.00 | 8868.25 | 6168.79 | 10957.67 | 4.80× | 1.44× |
| rspamd-rbl | 309.42 | 82.82 | 54.44 | 88.35 | 3.74× | 1.52× |

The larger generated inputs retain 28–44% overhead relative to the normal
Rust value API. The small Rspamd document retains 52%; tiny default C parses
take about 8.3 µs versus Rust’s 0.55 µs because the reviewed C parser captures
creation-time cwd. That behavior was checked against the reference and preserved.

## Native sampled CPU profiles

Separate eight-second Time Profiler captures of the same original 10,000-record
C caller. Only stacks inside submission or object destruction are counted.
Leaf-symbol grouping is approximate CPU attribution, not allocation counting;
inclusive frames in the raw summaries overlap. Profiling does not supply the
throughput numbers above.

| Sample attribution | Before | After |
| --- | ---: | ---: |
| allocator/memory-release leaf | 47.73% | 16.02% |
| byte-copy/zero leaf | 8.78% | 4.36% |
| other leaf | 43.49% | 79.62% |
| Counted application samples | 7,972 | 7,985 |

The original hotspots were repeated owned-tree reconstruction and per-node
allocation/free traffic. The implementation removes the unused successful-parse
snapshot, redundant owned rebuilding, copied paths and per-node C allocations.
Stable arena storage, direct indexed lookup and final-release fast paths remove
most of that cost. The retained Rust model and immediately observable C headers,
links and terminated bytes still require additional linear memory traffic.

## Exact allocation instrumentation

The fresh implementer’s own counting harness uses a different deterministic
706,670-byte record input. Its times are arithmetic means of 30 instrumented
lifecycles after three warmups; they must not replace the original-input medians
above. It counts Rust global allocation/reallocation requests, cumulative requested
bytes and peak extra live bytes. Caller/input generation and direct libc allocations
(including emission malloc) are excluded. Every lifecycle returns to its original
live-allocation baseline.

| 10,000 own generated records | C before | C after | Rust after |
| --- | ---: | ---: | ---: |
| Allocation requests | 820,212 | 70,059 | 70,037 |
| Cumulative requested bytes | 74,637,312 | 11,887,104 | 6,365,892 |
| Peak extra live bytes | 30,189,168 | 9,919,872 | 4,661,796 |
| Remaining extra live bytes | 0 | 0 | 0 |

The diagnostic matched-services run measures normal Rust 6.144 ms,
Rust with creation-time cwd/facts/C-position services 6.202 ms,
and C 7.396 ms. It leaves about 1.19 ms of
additional ABI-path work on that fixture. This is a diagnostic comparison, not a
replacement for the normal public Rust baseline. The C path adds 22 allocation
requests on this shape, but roughly doubles peak live storage.

## Public read measurements

Same C caller and released header, linked separately against the before/after
and reference archives. Five rotated rounds of nine samples, calibrated around
30 ms. Parsing is excluded; safe handle creation/next/free are included. Full
means all immediate children, not recursive tree traversal. Counts agree across
all libraries.

| Input / operation | Before µs | After µs | libucl µs |
| --- | ---: | ---: | ---: |
| records-10000/first | 3.157 | 0.022 | 0.020 |
| records-10000/full | 35.191 | 26.839 | 19.412 |
| records-10000/lookup | 0.020 | 0.014 | 0.012 |
| flat-1000/first | 0.334 | 0.023 | 0.033 |
| flat-1000/full | 3.450 | 3.854 | 3.014 |
| flat-1000/lookup | 0.020 | 0.023 | 0.012 |

First-child array reads improve about 142× and approach the reference cost.
Full array traversal improves about 24%. Full flat-object traversal regresses
about 12% (0.4 µs per 1,000 entries); streaming trades eager materialization for
per-step state work. No tree construction is deferred to lookup or emission.
The instrumented own iterator fixture drops from three allocation requests and
160,064 requested bytes to one 32-byte handle, for both early and full reads.

## Validation and provenance

- Final pinned native oracle gate: 43 signatures/links, C/C++ headers, four linkage
  combinations and ten snapshots passed. Eight additional benchmark outputs match.
- Full `scripts/ci.sh`, maintained ownership regression, direct/installed static
  and shared C checks, boundary/facts/lifetimes and 1,023-depth checks passed.
- Nightly Rust ASan plus C ASan/UBSan passed; maintained Miri and independent
  expanded public-API checks passed under Stacked Borrows and Tree Borrows.
- Independent review found a Model pointer-provenance bug in the first optimization
  commit. Final construction publishes pointers only after mutable construction
  borrows finish; `dfefd2c` fixes it and includes the maintained Miri regression.
  There are no open correctness findings on the final implementation.
- Independent packaging tests: 13 passed. Linux execution was not performed locally.
  Native CI must validate Linux when these commits are submitted.
- The fresh implementer and reviewer attest to the clean-room protocol. Coordinator
  authored tooling/evidence only; plans remain local and excluded from commits.
  No merge, push, PR or publication was performed for this optimization.

The near-zero target remains unmet. Reducing it further requires eliminating more
of the dual Rust/C representation’s eager memory work while preserving observable
headers, ownership and stable terminated strings. No allocation-count result alone
establishes zero-cost adaptation.

## Reproduce and inspect evidence

Run `run.py --candidate-root /path/to/candidate --output target/c-bench-final`,
then `rust_baseline.py --output target/c-bench-final`, `read_bench.py --before
target/c-bench --after target/c-bench-final`, and `cpu_profile.py --benchmark-dir
target/c-bench-final --name final-cpu` from the benchmark worktree.
See the parent README for build requirements and precise scope.

Committed raw timing samples, compiler/build/library/input hashes, CPU summaries
and allocation evidence are in [the evidence directory](evidence/2026-10-03-adapter/).
Timing JSON is gzip-compressed; use `gzip -dc FILE.json.gz` to inspect it.
`SHA256.json` records compressed and uncompressed hashes. Full native traces/XML
remain under ignored `target/c-bench/baseline-cpu.*` and
`target/c-bench-final/final-cpu.*`; no unusable Allocations trace is claimed.

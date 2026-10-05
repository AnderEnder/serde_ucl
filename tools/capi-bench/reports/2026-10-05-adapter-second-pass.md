# C adapter second pass and Rust 1.99 — 2026-10-05

The adapter improves another 1.16× in throughput on the original 10,000-record
input: 8.953 → 7.717 ms, a 13.8% reduction in elapsed time. Default Rust takes
6.296 ms and pinned libucl 11.130 ms. The C/Rust gap shrinks from 42.4% to 22.6%,
but the near-zero-overhead target remains unmet. The escaped-string workload
remains slower than libucl. Small inputs retain creation-time cwd capture cost.

Baseline: merged `80b77854472e81be5354d33fd559ccc3ef1f0f6a` (PR #30).
Implementation: `db08f32ce601aa134b20ef045cc61732fd468dd6`.
Public acceptance documentation: `a77a6c02279d27fbde5f34932802c83d6488e397`.
Independent tests: `059dfa029faf0ddde86a941d82368e107918bfe4`; independent review
closeout: `6e2ed75c6fc8232169d95119549dce4f5cfa1f41`.
Contract: released `spec-v22`, unchanged 43-function Stage A ABI.
Reference libucl: `24c8b399062ae4691168c243e3b7345ef7f31956`.

The implementation uses compact C header storage and sparse metadata, and copies
memory input into one immutable owned buffer so parsed values can borrow text.
Default C callers can still overwrite/free their input immediately after
submission. Retained nodes keep the model and backing buffer alive. C headers,
terminated strings and container metadata are built at submission; no tree
construction is shifted into lookup, iteration or emission. File/include/load,
variable and partial/error paths retain the necessary ownership behavior.

## Interleaved public API measurements

Apple M4 Max, macOS 15.8.1 arm64; both builds use Rust 1.99.0 / LLVM 23.1.1,
release fat LTO and one codegen unit. Same optimized C driver, released header,
flags, inputs and pinned reference; all five callers match eight recorded output
hashes. Each complete C lifecycle includes parser creation, submission, root
retrieval, parser destruction and final object release. The Rust comparator uses
normal public `parse::parse` plus destruction. Compact JSON emission is measured
separately on already parsed roots.

Five rounds of nine samples calibrated to 25 ms, rotating **before C, after C,
before Rust, after Rust and reference within each sample**, control for changing
machine load. Raw samples, per-round medians, loads and hashes are archived.
Ordinary separate before/after sessions changed by over 50% under load even for
the same baseline binary; those sessions are retained as provenance, not used to
attribute the reported improvement. No wall-clock threshold is a correctness test.

| Input | C before µs | C after µs | Rust before µs | Rust after µs | libucl µs | C speedup | C / Rust after |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| tiny-ucl | 8.32 | 8.05 | 0.55 | 0.55 | 7.91 | 1.033× | 14.687× |
| tiny-json | 8.27 | 8.04 | 0.56 | 0.56 | 7.91 | 1.028× | 14.417× |
| flat-1000 | 180.87 | 151.74 | 131.69 | 131.28 | 199.53 | 1.192× | 1.156× |
| floats-1000 | 203.38 | 175.09 | 147.89 | 147.95 | 219.18 | 1.162× | 1.183× |
| strings-1000 | 501.38 | 469.74 | 384.32 | 384.18 | 408.47 | 1.067× | 1.223× |
| records-1000 | 858.28 | 743.56 | 614.15 | 614.16 | 1090.42 | 1.154× | 1.211× |
| records-10000 | 8953.25 | 7716.75 | 6287.70 | 6295.88 | 11130.33 | 1.160× | 1.226× |
| rspamd-rbl | 85.75 | 73.59 | 56.73 | 56.87 | 87.93 | 1.165× | 1.294× |

| Compact JSON input | C before µs | C after µs | libucl µs | C speedup |
| --- | ---: | ---: | ---: | ---: |
| tiny-ucl | 0.33 | 0.33 | 0.21 | 0.993× |
| tiny-json | 0.33 | 0.33 | 0.21 | 0.986× |
| flat-1000 | 61.44 | 61.38 | 67.92 | 1.001× |
| floats-1000 | 109.08 | 108.18 | 181.26 | 1.008× |
| strings-1000 | 236.47 | 239.80 | 182.67 | 0.986× |
| records-1000 | 262.39 | 261.24 | 297.11 | 1.004× |
| records-10000 | 2616.27 | 2603.36 | 2918.60 | 1.005× |
| rspamd-rbl | 33.43 | 33.42 | 25.95 | 1.000× |


The new C API is 1.154–1.192× faster on the flat/float/record/Rspamd workloads,
and 1.067× faster on escaped strings. Residual overhead relative to owned/default
Rust is 15.6–29.4% on these larger inputs. Compact JSON emission is broadly
unchanged (within about 1.5%); tiny default C parses remain about 8 µs.

## Allocation and sampled CPU profiles

The author's separate generated 10k-record fixture is 706,670 bytes, rather than
the 691,127-byte public comparison input. Its Rust global allocation counters
include reallocations and requested layout bytes, excluding libc allocations.
Lifecycle timings in those files are means after three warmups/100 iterations;
read timings are means over 20,000 traversals. They are diagnostic, not the paired
public timing evidence above.

| Generated 10k lifecycle counter | Merged C | New C | Owned/default Rust |
| --- | ---: | ---: | ---: |
| Allocation requests | 70,059 | 10,060 | 70,037 |
| Requested allocation bytes | 11,887,105 | 10,916,099 | 6,365,892 |
| Peak extra live bytes | 9,919,872 | 8,948,866 | 4,661,796 |
| Remaining extra live bytes | 0 | 0 | 0 |

C requests fall 85.6%; requested bytes fall 8.2% and peak storage falls 9.8%.
The counting harness reported C6.405ms versus Rust6.423ms on that fixture.
Independent public measurements retain a material gap; the diagnostic parity
must not be promoted to a general performance guarantee. The C memory path now
uses borrowed-reader economics, while the primary Rust comparator owns text.
Matching owned Rust on a particular shape would not establish zero ABI marshalling
against borrowed Rust. Retained small children also retain the complete backing
model; C remains more memory hungry than the owned Rust result in this fixture.

Independent native Time Profiler captures use the uninstrumented public C caller
on the original input. Within input-submission/destruction stacks, allocator and
memory-release leaf samples fall **16.08% → 7.27%**; copy/zero leaf samples fall
**4.39% → 2.73%**, over 7,966 and 7,976 samples. Symbol grouping is approximate;
inclusive frames overlap and these percentages are not allocation counts or
whole-program speedup estimates. Separate author counting-binary profiles show
15.03% → 5.01% allocator leaves, with different scope/instrumentation and a
five-sample cutoff; those are separately labeled in the evidence.

## Public read paths

Same optimized C caller against frozen archives; pre-parsed immediate children,
new/next/free iterator handle, or indexed lookup. Five rotated rounds of nine
samples; traversal counts match all variants.

| Input / operation | Before µs | After µs | libucl µs |
| --- | ---: | ---: | ---: |
| records-10000/first | 0.022 | 0.023 | 0.020 |
| records-10000/full | 26.839 | 23.112 | 19.349 |
| records-10000/lookup | 0.014 | 0.013 | 0.012 |
| flat-1000/first | 0.023 | 0.024 | 0.033 |
| flat-1000/full | 3.858 | 3.095 | 3.028 |
| flat-1000/lookup | 0.023 | 0.023 | 0.012 |

Full array traversal improves 13.9%; full flat traversal improves 19.8%, removing
the first pass's flat traversal regression. First-child and lookup timings remain
near their prior costs. Iteration still allocates one 32-byte handle; it does not
prebuild the traversal.

## Compiler-only comparison

Frozen first-pass binaries built with Rust1.98.1/LLVM22.1.8 and
Rust1.99.0/LLVM23.1.1 were remeasured in a separate five-round interleaved session.
The source, Rust caller and dependency locks are identical (`dfefd2c` versus
merged `80b7785`), and C caller/header/input hashes match. The commits differ by
integration/provenance only in the checked build inputs. New source-level APIs
are not part of this experiment.

| Unchanged adapter input | C Rust1.98 µs | C Rust1.99 µs | Elapsed change |
| --- | ---: | ---: | ---: |
| tiny-ucl | 8.20 | 8.29 | +1.0% |
| tiny-json | 8.24 | 8.27 | +0.4% |
| flat-1000 | 173.58 | 183.23 | +5.6% |
| floats-1000 | 194.16 | 203.40 | +4.8% |
| strings-1000 | 494.87 | 503.33 | +1.7% |
| records-1000 | 846.09 | 858.09 | +1.4% |
| records-10000 | 8841.75 | 8950.25 | +1.2% |
| rspamd-rbl | 83.82 | 86.26 | +2.9% |

The latest compiler alone gives no broad parsing win in this experiment.
Records10k parsing is 1.2% slower, flat parsing 5.6% slower; records10k compact
emission is 5.3% faster. This is one machine/build configuration, not a general
Rust regression claim. The adapter comparison above holds both builds on1.99,
so the reported optimization gain is independent of the version upgrade. Keep
latest stable as the owner requested; do not substitute speculative new language
features for measured improvements. Compiler samples/identity metadata are
archived separately.

## Rust version and independent validation

All package manifests (root, capi, fuzz and feature-test package) now require
Rust 1.99, matching the repository policy of latest stable at release. CI and C
release workflows already install stable. New `Box::into_non_null/from_non_null`
APIs make the retained buffer's allocation ownership transfer explicit. Their
use still requires audited lifetime containment; the APIs alone do not promise
a speedup. New C variadic definitions do not help the fixed released signatures,
and new raw layout/vector ownership APIs were not substituted speculatively.
See the [official Rust 1.99 announcement](https://blog.rust-lang.org/2026/10/01/Rust-1.99.0/).

- Full `scripts/ci.sh` passed: feature matrix, optimized/unoptimized deep stacks,
  examples, benches, docs, formatting and clippy with warnings denied.
- Direct and installed static/shared C linkage, C/C++ header checks, all 43
  signatures/symbols and ten pinned snapshots passed. Eight benchmark outputs
  match before/after Rust, C and pinned libucl.
- Nightly Rust ASan plus C ASan/UBSan passed both direct linkages; C ASan/UBSan
  also passed all four direct/installed static/shared variants. No Darwin
  LeakSanitizer claim is made.
- All six maintained ownership tests pass native Rust and nightly Miri. The
  independent tests cover overwritten/freed caller buffers, mixed include/load/
  variables, retained subtrees/scalars, partial errors and all four emitter facts.
- Independent clean-room review found no correctness blocker in the final source.
  Reviewed source blobs exactly match `db08f32`. Raw measurements preserve the
  pre-commit HEAD `059dfa0` and code-dirty flag: source was frozen for validation
  and timing before its implementation commit. `source-checkpoint.json` records
  the reviewed blob identities, final commit and this qualification.
- These measurements and execution are local Darwin arm64. Linux runtime/CI
  execution remains for submission; no new push, PR, merge, tag or publication.
  Original unrelated work is preserved and local plans remain excluded.

## Reproduction and evidence

From the measurement worktree, build each version with `run.py --candidate-root
/path/to/version --output target/c-bench-VERSION`, then `rust_baseline.py --output
that-directory`. Run `paired.py --before BEFORE --after AFTER --output PAIRED`,
`read_bench.py --before BEFORE --after AFTER` and `cpu_profile.py --benchmark-dir
AFTER --name after-cpu`. See the parent README for requirements.

[Raw evidence](evidence/2026-10-05-adapter-second-pass/) includes gzip-compressed
public samples, before/after CPU summaries, scoped allocation JSONL, source
identities and a SHA256 manifest. Inspect compressed JSON with `gzip -dc`.
Full native trace/XML files remain in ignored `target/c-bench-before/before-cpu.*`
and `target/c-bench-after/after-cpu.*`.

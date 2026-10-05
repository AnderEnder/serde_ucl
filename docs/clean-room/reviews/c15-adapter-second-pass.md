# C15 independent second-pass adapter review

2026-10-05, independent clean-room implementation reviewer. Latest released
contract: `spec-v22`. Base: merged `80b7785`; branch:
`perf/c-api-adapter-second-pass`. No implementation source edits were made by
this reviewer. The implementation checkpoint was frozen by its author for review.

## Result and scope

No correctness or lifetime finding in the frozen candidate. Review covers
compact public-node storage, container ranges and duplicate heads, the
adapter-owned immutable input and artificial-static borrow containment, parser
and model destruction, retained nodes, scalar/subtree facts, reference counting,
forced-string cache stability and iterator behavior. Released Stage A remains
43 functions, one complete submission and read-only objects.

The input owner transfers into the same model as the borrowed Rust root; no Rust
value escapes through the C API. The root and parsed views precede the backing
input in field destruction order. Default C caller storage is copied before
parsing. Included/loaded content and expanded variables use the existing owned
paths. File input and partial/error snapshots remain owned. ZEROCOPY callers
must still obey the released unchanged-storage lifetime requirement, even though
this implementation makes its own copy.

Arena pre-counts match its node/container/head/string construction. Public
addresses are published from final owner provenance; range traversal creates
raw addresses rather than shared references to headers changed by release.
Retained children keep the complete model alive. Final release destroys it
directly; release with survivors propagates ownership iteratively. Container
metadata preserves cursor and entry count. Scalar string cursor sharing uses
only the two facts that scalar string emission consumes; nonstring scalar
emission has no descendant facts. Forced cache inner buffers survive metadata
growth. Safe/old/full iterator changes retain the released shallow traversal,
mode selection, exhaustion and reset behavior. Existing C matrix and deep-stack
gates are necessary validation in addition to these focused tests.

All public headers, terminated strings and child ranges are constructed during
submission. Lookup/iteration/emission do not defer graph construction. Lower
allocation counts do not imply equal memory usage with Rust: retained children
retain the complete arena, model, facts and input; terminated C strings and
public headers remain extra representation. Small inputs retain cwd capture
cost, and escaped/expanded/included/error text can still allocate.

## Independent validation

Added `capi/tests/review_ownership.rs`, three native tests on Rust 1.99.0,
LLVM 23.1.1, aarch64-apple-darwin. All pass; capi formatting and focused clippy
with warnings denied also pass. All three tests pass nightly Miri with
`MIRIFLAGS=-Zmiri-disable-isolation` (12.90 seconds for the final test version).
The ordinary nightly manifest warning about redundant homepage is unrelated.

The tests destroy overwritten default caller input before getting its object;
compare memory parsing against the ordinary owned file path for mixed includes,
loads, escaped and embedded-NUL strings, registered variables, duplicate chains
and nested arrays; destroy parsers and parents while retaining a subtree and
then a scalar; compare all four emitter selectors throughout; and validate an
independently retained partial/error graph. The ZEROCOPY variant uses the named
flag constant and keeps caller storage unchanged until final object release.
Scalar and container outputs also independently match ordinary Rust emission
using each original per-key facts path, including different strings sharing
single-quote, heredoc and plain fact classes.

An additional temporary stress replaced a registered variable after submission
and then freed its parser; native and Miri runs preserved the published values.
This is outside Stage A's supported registration timing and is deliberately not
a maintained behavioral requirement. Registration-buffer destruction before
submission and retained expanded values remain maintained coverage.

Source checkpoint blob identities, in path order:

| Path | Git blob |
| --- | --- |
| `capi/src/object.rs` | `37305b2b720179bd9b233512e49ecb50dd378006` |
| `capi/src/parser.rs` | `965ce4f89c820647bf82f1276dab66a6631c8494` |
| `capi/src/iteration.rs` | `1af465996c2559cfc7d5f0f56271dc4c77e048e4` |
| `src/parse/mod.rs` | `bb9c661468d2c004acd677090aae93790469b864` |
| `src/parse/facts.rs` | `93906ed8348606064bcbafbf5b1b71ad3489a08e` |

## Documentation and remaining validation

Reviewed README, changelog, manifests and CI toolchain descriptions. Requested
correction of stale capi Rust 1.98 minimum to 1.99 and explicit historical
first-pass labeling. Current manifests consistently use 1.99; existing historical
measurement toolchains should retain their original labels. New non-null Box
APIs make allocation ownership transfer explicit; their use does not eliminate
the unsafe lifetime-containment obligation reviewed above.

The candidate's own counting-profile documentation reports 70,059 to 10,060
allocation requests and 9.920 to 8.949 MB peak extra live storage for generated
10,000 records. These are author-supplied diagnostic measurements, not an
independent timing reproduction. Paired public C/Rust performance and broader
oracle comparisons remain the coordinator's acceptance evidence.

## Final checkpoint verification

Reviewed implementation commit:
`db08f32ce601aa134b20ef045cc61732fd468dd6`. All five source blob identities
above match this commit exactly. The source was unchanged between focused review,
the author's broad gates and this final verification. Reviewer test/provenance
commit is `059dfa0`; final closeout changes only this report and LOG.

Independently reran all capi native tests at the final implementation commit:
three arena tests and three independent review tests pass, as do all-target
clippy with warnings denied and capi fmt. Independently reran the documented
combined nightly Miri command at this final commit: all six tests pass
(arena 1.07 seconds, review 12.29 seconds).

Read only implementation-owned gate output under `target/adapter-second`:
`ci.log`, `asan.log` and `c-sanitize.log`. The author confirms each producing
command exited zero. The complete `scripts/ci.sh` output confirms Rust feature
checks, optimized/unoptimized depth, tests/examples/benches/docs and distribution
validation. The C gate confirms shipping/installed header, exactly 43 symbols,
C11/C++11 signatures/linkage, ten snapshots and boundary/lifetime/depth for all
four direct/installed static/shared configurations. `check.py --rust-asan` with
Homebrew LLVM21 clang/clang++ confirms Rust nightly ASan plus C ASan/UBSan for
direct static/shared; separate `check.py --sanitize` confirms C ASan/UBSan for
all four linkage configurations. Darwin leak detection is unavailable; no
LeakSanitizer claim is made. These broad gates were run by the author; focused
native/clippy/fmt and all-six Miri verification were rerun by the reviewer.

Final documentation inspection confirms the Rust 1.99 source-build requirement,
explicit historical first-pass labels, combined Miri command and arithmetic-mean
measurement qualifications. It explicitly distinguishes parity against the
owned/default Rust parser from overhead against the borrowed representation.
No correctness blocker or outstanding requested documentation correction remains.

Inputs consulted: current CLAUDE.md and clean-room protocol/provenance, released
spec-v22 C contract, current own adapter/parser/emitter/value sources, own tests,
profile example, distribution checks and public docs/manifests, plus the named
implementation-owned validation logs for final closeout. No coordinator
benchmark sources/artifacts, tools, plans/research or upstream implementation
were consulted.

I did not read libucl source code or any forbidden input listed in
docs/clean-room/PROTOCOL.md.

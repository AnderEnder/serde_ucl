# C15 adapter optimization: independent implementation review

Date: 2026-10-03. Role: fresh clean-room implementation reviewer.
Reviewed implementation: `dfefd2cff0c11a3f4710c9fce1720b2d5b2d273d`, following
the initial checkpoint `5c8df1a2e4a3f5b66610fd1fcacb39d3fd42d3d5`.
Contract: released `spec-v22`, particularly C API Stage A §§2–7.

There are no open correctness findings in the final reviewed implementation.
One confirmed unsafe pointer-provenance defect in the initial checkpoint was
repaired before the final checkpoint. The adapter has substantially less work,
but the requested zero or near-zero total overhead has not been achieved.

## Confirmed finding and repair

The initial arena saved a Model pointer derived inside `allocate(&mut self)`.
A subsequent exclusive borrow invalidated that pointer's borrow tag. This is
distinct from allocation-address stability: the Box never moving did not make
the saved pointer valid under Rust's aliasing rules.

An independent minimal Rust reproduction failed Miri. An actual public-API
reproduction also failed against a local copy of the initial checkpoint's own
C adapter sources: parser creation, parsing
`child={s='hello';x=42};a=1;a=2`, then `ucl_parser_get_object`. Miri diagnosed
the invalid Model-pointer access at the increment of `external` in the initial
`capi/src/object.rs:289`.

The final implementation initializes the reserved Model pointer only after
construction, from `Box::into_raw`, and uses raw arena-node access while
constructing links. The same public-API reproduction and additional lifetime
cases pass Miri after the repair. A separate suspicion about Vec mutable
indexing was not reproduced by the minimal Vec test; it is not reported as a
second confirmed defect.

## Ownership, lifetime and behavior assessment

Exact pre-counts cover every node, child link, optional separate object-head
link and terminated byte. Construction therefore does not grow those buffers
beyond their reserved capacities. Public node addresses, sibling links, keys
and string pointers remain stable. Empty slices use aligned non-NULL Vec
buffer pointers with length zero.

The arena's external-ownership count tracks the parser's root ownership and
references explicitly granted to callers. Public per-node counts continue to
track owning container edges. When a parent dies while another external
reference remains, the intrusive work stack only overwrites reserved storage
in dead nodes; separately retained containers and their descendants retain
their Model pointers and observable headers. The last external release drops
the complete arena without an otherwise unobservable public-refcount walk.
The immutable Rust value tree and facts remain owned by the arena, so retained
subtrees remain readable and emittable. Retaining one child also retains the
full original arena; this is a memory-retention tradeoff permitted by Stage A.

String keys and payloads have stable terminated storage. Forced scalar strings
are created on demand and cached in independently allocated inner byte
buffers. Growing the outer cache vector does not relocate those buffers.
The retained grandchild test preserves a forced string after releasing both
its original root and its retained child container.

Lookup uses the original Rust object's index and the already constructed head
table; it does not defer tree construction into lookup. Array access and old
iteration use existing link slices. Final safe/full iteration stores a small
cursor and reads existing slices and sibling links, without building a copied
sequence. Review covered mode freezing on first use, reset, duplicate entry
heads, scalar prefixes, first-container expansion, EXPLICIT object restart,
empty containers and IMPLICIT/BOTH exhaustion. Iterator handles grant no object
references; their caller must keep the target tree alive as required by §6.

Successful parse observation avoids the former unconditional root snapshot.
Errors and pending values still obtain a snapshot when necessary. Released
partial-result, silent-stop and diagnostic cases pass. Construction, retained
release, ordinary Rust-value destruction and shallow iteration remain
iterative; the public gate's deepest accepted object/array checks pass on a
2 MiB thread stack.

## Independent validation

The following commands were run against the reviewed live source. The final
commit contains identical adapter and maintained regression-test content;
source hashes below were checked against the committed blobs. CPU-heavy checks
were not repeated during the coordinator's final timing run.

```sh
python3 capi/tests/check.py
python3 capi/tests/distribution_test.py
MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test --manifest-path capi/Cargo.toml --test arena_lifetimes
MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri run --manifest-path target/capi-review-miri/Cargo.toml
MIRIFLAGS='-Zmiri-disable-isolation -Zmiri-tree-borrows' cargo +nightly miri run --manifest-path target/capi-review-miri/Cargo.toml
```

The public gate passed exact 43 exports, the released header, C11/C++11
signatures/linkage, ten golden snapshots, parser boundaries, retained lifetime
and emission facts, iterator cleanup, and 1023-container depth for direct and
installed static/shared variants. The distribution test suite passed all
13 tests. The maintained Rust ownership regression passed Miri, including
growth of the forced-string cache beyond its initial capacity. The independent
scratch public-API checks passed default Stacked Borrows and Tree Borrows and
covered retained child/grandchild destruction, borrowed and forced strings,
iteration after parent release, reset/full traversal, subtree emission/free,
empty input, failed partial objects/arrays and duplicate-value trees.

Final source SHA-256:

| Path | SHA-256 |
| --- | --- |
| `capi/src/object.rs` | `b9ce847d7415df712f18bfcbab0a590f6a154e64f79e7992bd9420db1504c8bc` |
| `capi/src/iteration.rs` | `6472318f1dd79cea362ea51a7ef257a818322e5bd358afd770389fd6879342a1` |
| `capi/tests/arena_lifetimes.rs` | `ffc46f9e49204b196c3c7d20faaffa27e1e9e5a5f7c63c9826694b1002cff7e5` |

## Remaining performance cost and packaging

The implementation's recorded own-input profile reports C 7.396 ms versus
default Rust 6.144 ms and the equivalent observed Rust parser 6.202 ms. That is
about 20% and 19% additional lifecycle time respectively, so it does not support
a near-zero claim. The coordinator reported a final same-input original 10,000-record result of
C 8.868 ms, Rust 6.169 ms and libucl 10.958 ms: approximately 44% above Rust.
Those results, rather than the implementation-side generated-input profile,
determine the externally reported gap. The reviewer did not inspect coordinator
benchmark tooling or artifacts. Zero or near-zero overhead remains unmet.

The remaining material work is linear arena construction: an exact count walk,
a header/link/terminated-byte construction walk, and final Model-pointer
publication. An LP64 Node is 88 bytes, including its required 64-byte public
header; links add eight bytes each and terminated keys/payloads are copied. The
original Rust tree remains allocated for lookup and emission. This adds memory
traffic and traversal to the equivalent Rust parse. Some public header and
terminated-storage work is necessary; the current layout's entire residual is
not proven irreducible. Potential local opportunities communicated to the
implementer included sharing duplicate-key storage, reducing redundant object
slot counting, private-node/link storage, and the publication pass. None is
required to resolve an outstanding correctness issue, and adopting one should
require profiling and ownership/provenance validation.

The coordinator also reported improved large-array first-read/full-traversal
and lookup times, with a modest flat-object full-traversal regression
(3.45 to 3.85 microseconds). Streaming removes the material up-front sequence
copy, but individual traversal workloads can still differ in throughput.

The profiling example's README claim is expressly limited to repository
checkouts. `capi/distribution.py` excludes `capi/examples` from source archives;
the manifest has no explicit example target, so its absence does not break the
source build. The example does not alter the required source/SDK checks.

Inputs consulted: current `CLAUDE.md`; `docs/clean-room/PROTOCOL.md`, work-item
goals and provenance; released Stage A specification; the crate's current own
C adapter, parser/value/emitter implementation, implementation-side tests,
profiling example and distribution code; allowed conformance snapshots via the
public gate. No coordinator benchmark tooling or reference source was read.
No implementation code was edited by this reviewer. No push, merge or
publication was performed.

I did not read libucl source code or any forbidden input listed in docs/clean-room/PROTOCOL.md.

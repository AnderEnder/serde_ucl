# JSON implementation optimization specification

This document specifies optimization experiments for a Rust JSON implementation, based on source and Git history research of sonic-rs, simd-json, json-rust, and the Rust crate jsonic. It covers parsing, value representation, extraction, lookup, serialization, and compilation. It is independent of this repository's lexer implementation and does not change its language or compatibility specification.

The objective is to reduce total processing time and memory cost while preserving the selected API's validation, numeric, decoding, ownership, and mutation semantics. Implementers should first remove unnecessary materialization and allocations, then optimize common scalar cases and bulk scanning, and finally tune measured machine-level bottlenecks.

Status: proposed optimization specification. Research date: 2026-10-01. No throughput measurements were performed for this document. Upstream performance reports are identified as such; proposed acceptance criteria are engineering choices rather than findings from the source libraries.

## 1 Scope and evidence

### 1.1 Source snapshots

| Library | Repository | Inspected commit | Manifest version |
| --- | --- | --- | --- |
| sonic-rs | cloudwego/sonic-rs | `7eca25c81884264043eb5434c1fa3fcba49600b1` | 0.5.10 |
| simd-json | simd-lite/simd-json | `61d649d13fae83ac6d9f587863696be2741f5b8b` | 0.18.1 |
| json-rust, crate `json` | maciejhirsz/json-rust | `0775592d339002ab148185264970c2a6e30b5d37` | 0.12.4 |
| jsonic | g1mv/jsonic | `b04980393d3617538672cd04d7872c30d5f5a920` | 0.2.14 |

These are repository snapshots, not a claim that each snapshot is the latest published crate. Jsonic here means the Rust crate from g1mv, not the similarly named JavaScript, Go, or C libraries.

Source files, benchmark definitions, and historical diffs were inspected. Small executable probes checked selected jsonic and json-rust behaviors. A compile-only probe checked whether jsonic's result lifetime is tied to its input; the dangling result was never dereferenced.

### 1.2 Evidence vocabulary

- **Observed:** present in inspected source or confirmed by a targeted probe.
- **Reported:** an upstream commit message or documentation reports a measurement; it was not independently reproduced.
- **Proposed:** a design or acceptance requirement derived from the research.
- **Conditional:** evaluate only when the API contract or workload makes the technique useful.
- **Rejected upstream:** a historical experiment did not justify its cost or was reverted. It is recorded to guide experiments, not prescribed for adoption.

Every optimization below is a candidate with an independent adoption decision. Competing representation and parsing strategies must not be enabled together merely to satisfy the inventory.

## 2 Semantic and ownership requirements

Before optimizing, define a reference contract for each API. Record accepted syntax, duplicate-key handling, numeric types and overflow, string decoding, UTF-8 handling, trailing-input handling, error positions, nesting limits, ownership, mutation, and retained-memory behavior.

1. Optimized checked APIs must preserve the reference contract on valid and invalid inputs.
2. Checked parsing must enforce the selected grammar even when conversion or materialization is deferred. For strict JSON, validate number syntax, escapes, control characters, and the complete document boundary.
3. Borrowed values must carry an input lifetime, or their storage must own the backing bytes. Raw pointers must not permit a safe result to outlive its backing storage.
4. A raw-text API may retain escape spelling and numeric spelling. A decoded-value API must return decoded strings and apply the promised numeric conversion.
5. Unchecked scanning must have a separate, explicit contract. Readable padding guarantees memory access bounds; it does not establish valid JSON syntax or UTF-8.
6. Duplicate-key behavior must be independent of object size and representation changes. Switching from a vector to a map must not silently change which duplicate wins.
7. Numeric fast paths must preserve signed boundaries, negative zero where promised, rounding, exponent handling, and overflow policy. Fast digit accumulation alone does not implement decimal-to-binary conversion.
8. Architecture fallback paths must preserve the same semantics and validation as accelerated paths.

These requirements prevent benchmarks from treating less work as an equivalent optimization. They are especially relevant to jsonic's raw representation and unchecked SIMD paths. See [J1], [J2], [J3], and [H12].

## 3 Parsing architecture

### A01 Parse directly into the requested result

**Evidence:** sonic-rs parses into visitors and serde targets without a mandatory structural-index and tape pipeline. Its DOM still uses staging storage. [S1], [S2].

**Proposal:** provide a direct path for typed deserialization or a visitor when the caller does not need a reusable document. Avoid building a DOM only to immediately traverse and discard it. Dispatch should remain statically optimizable where practical.

**Acceptance:** compare total time, allocations, and bytes copied for direct typed parsing against DOM-to-type conversion and tape-to-type conversion. Include unknown fields and escaped strings; skipped fields must receive the validation promised by the API.

### A02 Evaluate a structural index and contiguous tape

**Evidence:** simd-json first discovers structural offsets, then builds a contiguous tape from them. [M1], [M2].

**Proposal:** implement a tape candidate for full-document parsing, traversal, or repeated consumers. Encode container lengths and subtree spans so traversal can skip a subtree without visiting every descendant. Treat this as an alternative to mandatory direct parsing, not a universal prerequisite.

**Acceptance:** measure structural discovery, tape construction, tape consumption, and any subsequent DOM construction separately and together. Include tiny documents, punctuation-dense arrays, and large documents. Report structural-index and tape memory.

### A03 Delay DOM materialization until mutation needs it

**Evidence:** simd-json's lazy value starts from an already-parsed tape and upgrades to a borrowed DOM for mutation. It does not defer initial JSON parsing. [M3].

**Proposal:** preserve a read-only tape view until a mutating operation requires an owned or borrowed mutable representation. Specify the scope of each upgrade and the ownership of strings after upgrade.

**Acceptance:** measure read-only traversal, first mutation, subsequent mutation, and total mixed-workload cost. Verify that mutation preserves values and duplicate-key semantics. Report memory before and after upgrade.

### A04 Retain raw spans and defer conversions

**Evidence:** jsonic stores source slices and parses numeric text when accessors request a numeric type. Sonic-rs provides lazy values and raw numbers. [J1], [S9].

**Proposal:** expose validated raw spans when consumers need forwarding or selective conversion. Keep grammar validation separate from conversion. If repeated access is common, compare conversion on each access with an optional cache; caching is a proposed extension, not a measured finding.

**Acceptance:** measure parse-only, parse plus one conversion, conversion of every value, repeated conversions, and raw forwarding. Enforce backing-storage lifetimes. Distinguish raw strings from decoded strings in the API.

### A05 Use predictive byte parsing and mutate the active stack frame

**Evidence:** json-rust parses bytes predictively. Commit `87ed119` replaces repeated stack pop/push around each child with updates through the active frame; it reports up to 20% faster parsing. [R1], [H8].

**Proposal:** avoid backtracking when token prefixes determine the production. Keep the active container frame in place until that container closes. Use an explicit stack when it helps depth control and eliminates recursive call overhead.

**Acceptance:** measure large flat arrays, objects with many fields, and deep nesting. Count stack operations where possible. Preserve nesting limits and failure behavior; benchmark destruction of deep values separately.

## 4 Byte scanning and parser dispatch

### B01 Use scalar paths for short tokens

**Evidence:** sonic-rs adds a scalar object-key path scanning up to 24 bytes and falls back for escapes, control bytes, or longer keys. [H2].

**Proposal:** use a short scalar path for common keys and small tokens before constructing SIMD masks. Thresholds must be measured per workload and architecture; 24 bytes is an upstream choice, not a requirement. Reset or preserve cursor state correctly when falling back.

**Acceptance:** sweep key lengths around candidate thresholds, including multibyte UTF-8, escapes near the threshold, and truncated inputs. Measure the combined short-path and fallback cost, including rescanning.

### B02 Match frequent adjacent punctuation with a wide peek

**Evidence:** sonic-rs uses two-byte peeks for common compact-JSON patterns, later replacing slice matching with `u16` comparisons. [H1], [H2].

**Proposal:** recognize comma followed by quote, quote followed by colon, and other grammar-approved adjacent patterns with a bounded wide read. Fall back to general whitespace and syntax handling when the pattern does not match. Define byte order explicitly.

**Acceptance:** measure compact and formatted JSON, not just compact input. Test every truncated prefix and punctuation boundary. Keep gains attributable to fewer reads and branches rather than changed acceptance.

### B03 Combine scalar whitespace paths with cached masks

**Evidence:** sonic-rs tries zero or one intervening whitespace byte, then reuses a cached non-whitespace mask or scans a new 64-byte block. [S1].

**Proposal:** retain a scalar common path, cache block classification with its start offset, and mask away positions preceding the current cursor. Use bulk scanning for longer runs and a bounded tail. Define mask validity across cursor changes and input changes.

**Acceptance:** cover no whitespace, one space, short indentation, long runs, repeated skips within one block, and boundary positions. Recognize exactly the whitespace characters required by the contract.

### B04 Scan ordinary string bytes in blocks

**Evidence:** sonic-rs finds quotes, backslashes, and control bytes using SIMD. Json-rust uses a 256-entry byte table with an unescaped-string path and separate complex decoding. [S4], [R1].

**Proposal:** scan ordinary runs quickly and enter escape decoding only when needed. Compare scalar tables and SIMD at multiple lengths. Borrow unescaped source text when allowed; allocate or decode only when representation requires it.

**Acceptance:** measure long ordinary strings, short strings, high escape density, ASCII and multibyte UTF-8. Verify quote ordering, controls, surrogate handling, and all escape sequences. Measure output construction alongside scanning.

### B05 Classify multiple character classes with nibble tables

**Evidence:** simd-json's AVX2 stage uses low- and high-nibble shuffle tables to classify JSON structural characters and whitespace. [M2].

**Proposal:** evaluate SIMD table lookup against separate vector comparisons when several character classes are needed from the same load. Keep tables specific to the grammar and prove their classification for all 256 byte values.

**Acceptance:** exhaustively verify byte classification. Measure instructions, classification throughput, and mask extraction on x86_64 and aarch64. A SIMD operation count is not a substitute for total parser measurements.

### B06 Compute string interiors with quote and escape masks

**Evidence:** both sonic-rs's container skip and simd-json's structural scan compute masks of unescaped quotes and string interiors, carrying state across blocks. [S1], [M1], [M2].

**Proposal:** compute escaped positions using backslash-run parity, then use prefix XOR over unescaped quotes to mark string interiors. Carry both string state and relevant escape state across blocks. Evaluate carry-less multiplication where supported and shift/XOR alternatives elsewhere.

**Acceptance:** test odd and even backslash runs, quotes at every block offset, and runs spanning blocks. Brackets inside strings must never become container events. Unclosed strings and invalid escapes must receive the checked API's promised validation.

### B07 Enumerate masks efficiently and pipeline blocks

**Evidence:** simd-json enumerates structural bits using trailing-zero counts and lowest-set-bit clearing; its AVX2 path batches offset writes. The stage loop flattens the preceding block while processing the current one. [M1], [M2].

**Proposal:** compare scalar bit enumeration with batched offset writes, and pipeline independent work between neighboring blocks. Reserve enough space before unchecked writes and ensure speculative extra lanes are never exposed as valid offsets.

**Acceptance:** test empty, sparse, and dense masks, including the highest bit. Measure cycles per input byte and per structural event. Validate offsets exactly against a scalar reference.

### B08 Integrate UTF-8 validation and keep detailed diagnostics cold

**Evidence:** simd-json updates a chunked UTF-8 validator in the structural loop. Sonic-rs uses fast basic validation and falls back to diagnostic validation on failure. [M1], [S8].

**Proposal:** evaluate shared block traversal and fast validation paths. Compute detailed failure positions on error rather than adding equivalent bookkeeping to each successful byte. An input typed as `&str` already guarantees UTF-8, but byte-input APIs need validation.

**Acceptance:** test invalid sequences and partial code points at all block and input boundaries, including fallback architectures. Compare valid-input throughput and error-path behavior independently. Required error positions must remain correct.

### B09 Remove checks redundant with established parser state

**Evidence:** simd-json's `684fe91` uses the closing byte to choose a container node instead of reloading and matching its existing tag. [H5].

**Proposal:** use already-established grammar facts to remove redundant tag reloads and branches. Keep assertions in debug builds and document the state invariant that makes the replacement valid.

**Acceptance:** test matching and mismatched closure, nested containers, and malformed documents. Inspect generated code and measure total parsing; fewer source branches alone do not establish a gain.

## 5 Numeric processing

### N01 Specialize small integers and correct digit counting

**Evidence:** sonic-rs has a single-digit return path. Simd-json fixes digit counting so the minus sign does not push fitting 18-digit negative integers onto a cold large-integer path. [H1], [H4].

**Proposal:** return early for common integer forms after confirming that no fraction or exponent follows. Count digits independently of sign and punctuation. Keep separate fitting signed, unsigned, and overflow paths where the contract requires them.

**Acceptance:** cover one through twenty digits, leading zero rules, signs, integer limits, and following delimiters. Include negative zero and transitions to fractions or exponents.

### N02 Parse digit batches using SWAR

**Evidence:** sonic-rs loads eight digit bytes into a `u64` and combines pairs, groups of four, and groups of eight. Commit `0211f7c` reports 32–54% faster number microbenchmarks as part of its number changes. [S3], [H1].

**Proposal:** evaluate eight-digit batches with scalar tails. Choose dispatch based on measured token-length distribution. Define byte order and readable length explicitly, and preserve overflow handling when appending another batch.

**Acceptance:** benchmark digit counts around 8 and 16, long identifiers, short integer parts of floats, and short end-of-input tails. Compare the full number routine, including failed batch checks, with the scalar baseline.

### N03 Use tolerant SWAR tails and avoid execution-port contention

**Evidence:** sonic-rs's `15def0b` replaces SSE fraction accumulation with SWAR and a tolerant tail, and retains a scalar fraction path for short integer parts. The commit rationale is to avoid SIMD setup and competition for floating-point execution resources. [H3], [S3].

**Proposal:** evaluate a partial digit batch that finds the first non-digit with a bit mask and combines the valid prefix. Keep scalar fraction handling as a candidate. Adopt the execution-port rationale only when measurements on the target support it.

**Acceptance:** cover all prefix lengths from zero through eight, available padding, decimal and exponent delimiters, and truncated mantissas. Compare scalar, SIMD, and SWAR candidates on float-heavy input with hardware counters when available.

### N04 Preserve layered correctly rounded float conversion

**Evidence:** sonic-rs uses small-number fast paths, additional normal-number conversion, Eisel-Lemire conversion, and a slower long-mantissa fallback when the result is ambiguous. Json-rust instead stores a decimal mantissa and exponent, illustrating a different numeric contract. [S5], [R4].

**Proposal:** use fast decimal-to-binary conversion with a correct fallback rather than discarding significant digits without rounding analysis. Evaluate cached powers of ten and wider cached power tables where they replace repeated exponent computation; include table access and cache footprint in the measurement. A decimal representation is a separate API choice; evaluate it where delayed binary conversion benefits the application. Approximate parsing must remain an explicitly different contract.

**Acceptance:** compare promised binary results with a trusted correctly rounded reference, including halfway cases, subnormals, long mantissas, large exponents, underflow, and overflow. Benchmark common fast-path values and fallback-heavy values separately.

### N05 Parse directly to the requested floating type

**Evidence:** sonic-rs adds single-pass `f32` parsing instead of routing all requested floats through its `f64` path. [H11].

**Proposal:** parse to the requested precision when this avoids rescanning or unnecessary conversion. Preserve correct rounding for that precision; `f64` parsing followed by a cast is not automatically an equivalent substitute.

**Acceptance:** compare `f32` and `f64` workloads, type-specific boundaries, and cases that distinguish direct rounding from double rounding. Measure conversion and any rescanning together.

## 6 Memory and value representation

### V01 Pack metadata and keep children contiguous

**Evidence:** sonic-rs's value consists of compact metadata and payload, yielding 16-byte nodes on 64-bit targets. Simd-json uses a contiguous tape. [S2], [M1].

**Proposal:** reduce common-node size and store child sequences contiguously. Compare enum layouts, compact tags, and indexes before adopting raw pointer packing. Record alignment, maximum lengths, index width, and overflow behavior. Preserve pointer provenance where pointers are stored.

**Acceptance:** report `size_of`, alignment, document memory, bytes per value, traversal time, and mutation cost on each supported target. Test representation limits rather than silently truncating metadata.

### V02 Allocate document storage in an arena

**Evidence:** sonic-rs uses bump allocation for document nodes and owns backing bytes used by parsed strings. [S2].

**Proposal:** allocate related nodes in a document arena when their lifetimes align. Consider copying input once and decoding in that owned buffer instead of separately allocating every string. Describe the distinction between no per-string copy and no input copy.

**Acceptance:** measure allocation count, allocated bytes, peak memory, parsing, destruction, and traversal. Test retaining one child after dropping the root and report backing-document retention. Define how mutation and clones own their storage.

### V03 Reuse scratch buffers and output capacity

**Evidence:** simd-json exposes reusable buffers and `fill_tape`; sonic-rs reuses thread-local node staging. [M1], [S6], [H6].

**Proposal:** allow repeated parses to reuse structural indexes, string scratch, stacks, and tape or staging capacity. Prefer explicit ownership when it simplifies reentrancy. If thread-local storage is used, define nested-parse and callback behavior.

**Acceptance:** measure cold first parse and steady-state parsing separately. Use mixed-size sequences and concurrent threads. Report retained capacity, allocation counts, and behavior after errors.

### V04 Bound retention and initialize only required padding

**Evidence:** simd-json changed initialization from retained capacity to current input length plus padding. Sonic-rs has a size policy for retained TLS staging. [H7], [S6], [H10].

**Proposal:** limit retained scratch after unusually large requests. Initialize only the tail bytes that actual wide reads require, using a fixed-size bulk write where profitable. Do not make per-request work proportional to historical peak capacity.

**Acceptance:** alternate large and tiny documents, parse tiny documents after a very large input, and inspect bytes initialized per operation. Test shrinking and retention policies without oscillatory allocation behavior.

### V05 Reserve capacity from proven bounds or measured hints

**Evidence:** sonic-rs reserves node staging from a grammar-derived upper bound of input length divided by two plus two. Simd-json uses structural-density heuristics. Json-rust removed a depth-based allocation predictor after negligible gains. [S2], [M1], [H9].

**Proposal:** compare a proven bound, conservative growth, and workload hints. Use a proven bound only for the grammar for which it is established. Do not prescan just to count children unless the avoided growth outweighs the extra pass.

**Acceptance:** include punctuation-dense inputs and documents dominated by large strings. Measure reallocations and peak memory together. Record whether a hint is a guarantee or a heuristic.

### V06 Advance an output pointer and specialize node copies

**Evidence:** sonic-rs replaces staging length updates with base/end/capacity-end pointers and adds inline AVX2 node copying. Its bundled commit reports fewer forwarding stalls and substantial total parsing gains. [H1], [S6].

**Proposal:** evaluate pointer advancement only after profiling identifies bookkeeping dependencies. Compare ordinary vector push and compiler-generated copies with specialized small or medium copies. Reserve capacity first and keep fallback handling bounded and correct.

**Acceptance:** inspect generated code and measure representative container-size distributions. Track cycles, instructions, stalls, and total time where supported. Exercise empty ranges, capacity boundaries, error cleanup, and ownership of partially initialized nodes.

### V07 Inline short strings and keys conditionally

**Evidence:** json-rust stores strings up to 30 bytes inline and object keys up to 32 bytes inline. [R2], [R3].

**Proposal:** compare inline storage against borrowed slices and compact heap-backed handles. Tune capacities with the resulting whole-node size in mind. Avoid enlarging every numeric or null node merely to optimize rare strings without measuring that cost.

**Acceptance:** sweep lengths around capacities and measure allocation reduction, node size, traversal, copying, and arrays of non-string values. If keys cache internal pointers, verify pointer repair after relocation; indexes or offsets are an alternative to evaluate.

### V08 Allocate empty containers only when necessary

**Evidence:** jsonic does not allocate a container vector until its first element; json-rust also has allocation-free empty objects. [J1], [R3], [H14].

**Proposal:** represent empty containers without backing allocation and allocate on first insertion. Consider separate true and false tags where they reduce payload work without enlarging nodes.

**Acceptance:** count allocations for empty arrays, empty objects, and documents with many empty children. Test the first insertion and transitions between empty and non-empty states.

## 7 Objects and repeated lookup

### O01 Use vectors for small objects and indexes when justified

**Evidence:** jsonic keeps a vector through 64 entries and converts larger objects to a `BTreeMap`. Sonic-rs uses parsed pairs and a hash map in its owned mutable representation. [J1], [H13], [H15].

**Proposal:** compare contiguous linear lookup with hash or tree indexing. Choose promotion by measured size, lookup frequency, or mutation needs; the threshold is a tuning parameter. Preserve order and duplicates according to the contract.

**Acceptance:** test sizes around candidate thresholds, lookup misses, first and last keys, repeated access, and mutation. Measure construction plus application access, not construction alone. Test duplicate keys across promotion boundaries.

### O02 Cache key fingerprints and known-key hashes

**Evidence:** json-rust uses cached FNV-1a hashes in its tree; jsonic caches a selected-byte fingerprint; simd-json's `KnownKey` caches a hash using shared process-initialized AHash state. Its README's fxhash description is stale for the inspected source. [R3], [J4], [M4].

**Proposal:** cache hashes when repeated lookup saves meaningful work. Any precomputed hash must use the same state as the destination map. Fingerprint equality must still compare full keys. Choose collision behavior appropriate to the input trust model.

**Acceptance:** compare ordinary lookup with reused known-key handles, accounting for handle construction. Include equal fingerprints with unequal text, long keys, and adversarial key families. Verify that representation changes do not invalidate hash assumptions.

### O03 Evaluate a vector of tree nodes

**Evidence:** json-rust stores object nodes in one vector with left and right child indexes ordered by key hash. The tree is not balanced. [R3].

**Proposal:** evaluate this representation when stable iteration and contiguous storage are useful. Do not assume logarithmic lookup; compare with vectors, balanced trees, and hash maps under unfavorable insertion order and collision patterns.

**Acceptance:** measure build, iteration, random lookup, worst-case lookup, relocation, and memory. Treat this as a competing object implementation rather than combining it with every indexing candidate.

### O04 Append during construction when semantics allow

**Evidence:** sonic-rs has a commit replacing insertion with pair append. [H16].

**Proposal:** avoid repeated lookup or deduplication during object construction when the contract permits preserving entries or resolving duplicates later. If deduplication is required, specify when it occurs and include that work in the total cost.

**Acceptance:** benchmark unique and duplicate-heavy objects. Compare complete semantic results and total build-plus-resolution time. No candidate may silently weaken duplicate handling to improve parse time.

## 8 Selective extraction

### Q01 Skip unused subtrees without materializing them

**Evidence:** sonic-rs computes string-interior masks and counts brackets outside strings to skip containers. Checked and unchecked extraction expose different validation contracts. [S1], [S9].

**Proposal:** use bulk skipping when consumers need few fields. Checked skipping must enforce required syntax, types of matching delimiters, string rules, and document completion; bracket counting by itself is not a complete validator. Use unchecked skipping only under its explicit precondition.

**Acceptance:** vary selected-field position and selected fraction of the document. Include brackets inside strings, mismatched containers, invalid skipped values, and malformed trailing content. Compare with a full parse producing the same requested outputs.

### Q02 Share traversal of multiple requested paths

**Evidence:** sonic-rs's `PointerTree` groups paths so `get_many` can share traversal instead of repeating `get`. [S7].

**Proposal:** compile requested paths into a reusable prefix tree. Preserve request order, duplicate requests, missing-path behavior, and overlap between a selected container and selected descendants.

**Acceptance:** benchmark one, several, and many paths with shared and disjoint prefixes. Report path-plan construction separately and amortized over repeated documents. Validate every requested result against independent reference lookup.

## 9 Serialization

### E01 Copy ordinary runs and handle escapes separately

**Evidence:** sonic-rs combines vector copying and escape discovery. Json-rust scans for the first escape and writes ordinary spans in bulk, with complex escaping outside the common path. [S4], [R5].

**Proposal:** reserve output capacity, write ordinary spans in bulk, and enter scalar escape handling only for special bytes. Compare fused copy-and-find with scan-then-copy. Any speculative vector store must remain within writable capacity, and only initialized valid output may be exposed.

**Acceptance:** measure short and long strings, high and low escape density, control bytes, UTF-8, and output growth. Verify exact output bytes, escaping, and reported output length. Include allocation and growth costs.

### E02 Reuse output buffers and avoid temporary formatted strings

**Evidence:** json-rust's generator writes directly into a byte vector or writer and bulk-writes spans. Sonic-rs uses specialized number formatting dependencies. [R5], [S10].

**Proposal:** write integers and floats into reusable buffers without allocating a temporary formatted string for each value. Buffer small writer operations where the consumer benefits. Preallocate from defensible size estimates, and separately measure formatted output's additional work.

**Acceptance:** count writes, allocations, and growth; measure vector output and external-writer output separately. Check short writes and errors. Output capacity reuse must have a documented retention policy.

### E03 Evaluate float formatting independently

**Evidence:** sonic-rs switched float formatting from ryu to zmij. Json-rust stores decimal mantissa and exponent and prints that representation, so it performs a different conversion task. [H17], [R4], [R5].

**Proposal:** compare specialized formatters under identical round-trip and formatting requirements. If raw numbers are retained, evaluate forwarding their validated spelling instead of binary conversion and reformatting as a separate API operation.

**Acceptance:** verify round trips, negative zero, exponent spelling, and handling of non-finite values. Include common numbers and difficult values. Pin dependency versions and record formatter changes separately from string serialization changes.

## 10 Machine and compiler tuning

### C01 Use architecture-specific mask extraction

**Evidence:** sonic-rs has NEON-specific string masks. Simd-json combines deinterleaved NEON loads with shifts and narrowing to produce a mask instead of the previous pairwise-add approach. [S4], [M5], [H18].

**Proposal:** tune loads, comparisons, mask extraction, and first-match selection per architecture. Do not mechanically reproduce an x86 movemask design on ARM. Evaluate vector width rather than assuming wider is faster.

**Acceptance:** compare generated assembly and complete workloads on actual x86_64 and aarch64 hardware. Include tails and sparse masks. Verify every mask bit against the scalar reference.

### C02 Select CPU features outside hot per-byte work

**Evidence:** simd-json caches runtime-selected implementations; sonic-rs recommends target-specific compilation and has architecture fallbacks. [M1], [S10].

**Proposal:** support a portable runtime-dispatched build and, where deployment permits, a fixed-target build. Resolve available instruction sets once or at an appropriately coarse boundary. The optimized path must never execute unsupported instructions.

**Acceptance:** record CPU, enabled features, and dispatch configuration for each benchmark. Measure tiny-input dispatch overhead. Run fallback correctness independently; native-target results do not establish portable performance.

### C03 Inline measured hot helpers and separate cold paths

**Evidence:** sonic-rs history includes reader/writer inlining; simd-json adds number-parser inlining, cold error helpers, and stable branch-hint macros using a cold helper. Json-rust's macros reflect older compiler limitations. [H4], [H19], [H20], [M1], [R1].

**Proposal:** compare ordinary functions and selective inlining before converting logic to macros. Move expensive diagnostic work into cold paths. Evaluate branch hints only where profiles establish a stable bias. Track code growth and instruction-cache effects.

**Acceptance:** inspect generated code and measure tiny and large workloads on the pinned compiler. Preserve error content and positions. Reevaluate compiler-specific workarounds after compiler upgrades.

### C04 Evaluate pointer-based interfaces only for measured compiler issues

**Evidence:** simd-json changed an internal slice interface to a pointer after an optimizer issue in a nightly compiler. [H21].

**Proposal:** establish a reproducible compiler/code-generation issue before replacing safe interfaces. Confine raw-pointer operations behind documented bounds, ownership, and aliasing invariants. Revisit the workaround when toolchains change.

**Acceptance:** show the problematic code generation and before/after workload measurements. Exercise aliasing, relocation, and boundary cases. A historical compiler workaround is not sufficient evidence for adoption today.

### C05 Evaluate allocator and release compilation choices

**Evidence:** simd-json recommends benchmarking alternative allocators. Its manifest enables optimization, LTO, and one codegen unit for repository benchmarks; dependency profiles do not automatically configure a consuming application's root profile. Jsonic also defines repository-level release options. [M6], [J5].

**Proposal:** compare the default allocator with suitable alternatives when allocation remains material. Evaluate LTO, codegen units, and target features at the application root. Keep sanitizer and debug runs for correctness separate from performance runs.

**Acceptance:** record complete build profiles and allocator versions. Compare runtime, peak memory, binary size, and build time. Do not attribute a library win to an unmatched compilation profile or allocator.

## 11 Failed experiments and correctness findings

### 11.1 Integer SWAR can regress short-number workloads

Simd-json's `ebc6743` records an attempted SWAR integer-part loop: float-heavy `canada.json` regressed about 3.8%, while `twitter` improved about 0.7%. The extra failed eight-digit check on common short integer parts outweighed the gain. Keep N02 conditional on measured token distribution. [H4].

### 11.2 Allocation prediction can add complexity without gains

Json-rust removed depth-based predictions of sibling container allocation sizes after finding barely any benchmark difference. V05 must compare the heuristic's bookkeeping with the actual reallocations avoided. [H9].

### 11.3 Wide reads require actual readable storage

Simd-json reverted an early string optimization after reads beyond input could cross page boundaries and cause segmentation faults. Padding and tail handling are correctness requirements, not optional details. A byte-address being in a mapped page is not sufficient permission to read beyond the Rust allocation. [H22].

### 11.4 Faster validation cannot mean missing validation

Simd-json history contains a fix for missing UTF-8 validation in a fallback configuration. Every accelerated and fallback path must satisfy section 2, including byte-input APIs. [H12].

### 11.5 Jsonic results are not equivalent to decoded checked DOM parsing

Targeted probes of the inspected jsonic snapshot accepted trailing garbage, a leading plus, a leading-zero number, an incomplete exponent, and an invalid escape. A valid string whose closing quote follows an even backslash run was rejected. Escaped string text remained raw, and a key spelled as an escape was not found through its decoded spelling. Source inspection explains these outcomes: number scanning recognizes a character set rather than a complete grammar, and quote detection checks the previous byte rather than complete backslash parity. [J2].

Jsonic's safe parse result contains raw source pointers without an input lifetime. A function returning that result from a local string compiled in the probe. No dangling-pointer read was performed. Adopt the raw-span idea only with a sound ownership model. [J1], [J3].

Measured type sizes on the probe target were 56 bytes for jsonic's `JsonItem` and 32 bytes for json-rust's `JsonValue`. These are node sizes, not total document memory. The phrase “small footprint” is insufficient evidence of lower memory use.

Jsonic's benchmark parses sonic-rs and simd-json into `serde_json::Value`, bypassing their native DOM and tape representations. Jsonic also defers work the other targets perform. Those timings must not be used to rank equivalent checked native-DOM implementations. [J6], [J7].

### 11.6 Upstream reports provide hypotheses rather than performance budgets

Sonic-rs's `0211f7c` reports bundled improvements from SWAR integers, punctuation peeks, pointer staging, and specialized copies. Reported native-DOM times were 511 to 314 microseconds for `citm_catalog`, 1416 to 1099 for `golang_source`, and 495 to 351 for `lottie`. These results do not isolate each change and do not establish expected gains on another CPU or implementation. [H1].

## 12 Benchmark and correctness acceptance

### 12.1 Equivalent operations

Maintain separate benchmark groups for validation, typed parsing, native owned DOM, native borrowed DOM, tape construction, selective extraction, repeated lookup, mutation, and serialization. Compare libraries only within an explicitly matched operation and semantic contract.

For each group, report whether timing includes input cloning, padding, UTF-8 validation, scratch initialization, result construction, conversion, and destruction. Provide complete-operation timing in addition to any isolated kernel measurement. Raw forwarding and parse-only lazy values are separate groups from full decoding and conversion.

### 12.2 Workload matrix

| Dimension | Required examples |
| --- | --- |
| Document size | Tens of bytes, hundreds of bytes, several KiB, hundreds of KiB, multiple MiB |
| Formatting | Compact, single spaces, short indentation, long whitespace runs |
| Strings and keys | Length sweeps across scalar/SIMD and inline thresholds; ASCII, UTF-8, low/high escape density |
| Numbers | Short integers, long identifiers, short/long fractions, exponents, correctly rounded fallback cases |
| Structure | Flat arrays, many small objects, large objects, deep nesting, empty containers |
| Object access | Lookup hits and misses, repeated known keys, duplicate keys, unfavorable key families |
| Extraction | Early/late selected fields, sparse/dense selection, shared/disjoint path prefixes |
| Lifetime | Parse/drop, retained root, retained child, first mutation, repeated mutation |
| Reuse | Cold start, steady state, alternating large/small inputs, errors between valid inputs |
| Architecture | Actual x86_64 and aarch64 targets, supported SIMD variants, scalar fallback |

Realistic corpora should supplement these controlled workloads. No single corpus is sufficient to select a universal threshold.

### 12.3 Correctness validation

Differentially compare candidates with the reference contract. Add generated valid documents, destructive mutations, and targeted edge cases for each changed kernel. Specifically exercise every byte alignment near wide-read boundaries, short tails, odd/even backslash runs, UTF-8 boundaries, escapes, integer limits, float rounding, duplicate keys, representation promotion, error cleanup, and depth limits.

Use Miri or other appropriate memory tools for unsafe storage and lifetime logic, and sanitizers for optimized paths where supported. If Miri uses a different non-SIMD implementation, a passing Miri run validates that implementation; it does not validate every production intrinsic path. A scalar fallback must receive its own tests.

### 12.4 Measurements and experiment decisions

Record source revision, toolchain, dependencies, CPU, OS, allocator, build flags, enabled features, workload checksum, and harness configuration. Avoid contention, interleave baseline and candidate measurements, and pin execution where practical. Observe values so work cannot be optimized away.

Record elapsed time, throughput, allocation count, allocated bytes, peak/retained memory, and destruction time. Add instruction counts, branch misses, cache behavior, and execution stalls when investigating a specific machine-level hypothesis.

Adopt an experiment only when its intended workload improves beyond measured noise and affected workloads have understood tradeoffs. There is no universal percentage gate. A regression needs an explicit documented scope decision; it must not be concealed by an aggregate score. Reject complexity that produces no repeatable benefit.

Each experiment record must contain the optimization identifier, baseline, hypothesis, changed mechanism, preserved contract, measurements, affected workloads, correctness evidence, and an adopt/reject/defer decision. Report time reduction and speedup with their correct denominators.

## 13 Evaluation order and deliverables

The proposed evaluation order is:

1. Establish contracts, equivalent benchmark groups, and the scalar reference.
2. Compare direct parsing, tape, and raw-span APIs: A01–A05 and Q01–Q02.
3. Measure layout, allocation, ownership, reuse, and empty containers: V01–V05, V07–V08.
4. Optimize common scalar dispatch and string/whitespace scanning: B01–B04 and N01.
5. Evaluate block classification, masks, numeric batches, and float conversion: B05–B08 and N02–N05.
6. Evaluate object construction and repeated lookup: O01–O04.
7. Optimize serialization independently: E01–E03.
8. Tune profile-confirmed remaining bottlenecks: B09, V06, C01–C05.

Required deliverables are an experiment inventory covering every identifier, baseline and candidate measurements, retained correctness checks, documented ownership and representation invariants, selected architecture and thresholds, and a record of rejected candidates. An identifier may be completed by a justified rejection or a scope-based deferral; completion does not require adding every optimization to one implementation.

## 14 Source references

References use immutable source snapshots or commit links. Existing upstream descriptions and reported measurements may be outdated relative to a later release; implementation evidence in this document is anchored to section 1.

- [S1] [sonic-rs parser](https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/src/parser.rs).
- [S2] [sonic-rs value layout and DOM construction](https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/src/value/node.rs).
- [S3] [sonic-rs SWAR routines](https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/sonic-number/src/swar.rs).
- [S4] [sonic-rs string parsing and serialization](https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/src/util/string.rs).
- [S5] [sonic-rs number parsing and float conversion](https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/sonic-number/src/lib.rs).
- [S6] [sonic-rs reusable node buffer](https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/src/value/tls_buffer.rs).
- [S7] [sonic-rs pointer tree](https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/src/pointer/tree.rs).
- [S8] [sonic-rs UTF-8 validation](https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/src/util/utf8.rs).
- [S9] [sonic-rs selective extraction](https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/src/lazyvalue/get.rs).
- [S10] [sonic-rs manifest](https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/Cargo.toml) and [README](https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/README.md).
- [M1] [simd-json buffers, dispatch, and stage orchestration](https://github.com/simd-lite/simd-json/blob/61d649d13fae83ac6d9f587863696be2741f5b8b/src/lib.rs).
- [M2] [simd-json AVX2 classification and bit enumeration](https://github.com/simd-lite/simd-json/blob/61d649d13fae83ac6d9f587863696be2741f5b8b/src/impls/avx2/stage1.rs).
- [M3] [simd-json lazy value](https://github.com/simd-lite/simd-json/blob/61d649d13fae83ac6d9f587863696be2741f5b8b/src/value/lazy.rs).
- [M4] [simd-json known-key implementation](https://github.com/simd-lite/simd-json/blob/61d649d13fae83ac6d9f587863696be2741f5b8b/src/known_key.rs).
- [M5] [simd-json NEON stage](https://github.com/simd-lite/simd-json/blob/61d649d13fae83ac6d9f587863696be2741f5b8b/src/impls/neon/stage1.rs).
- [M6] [simd-json manifest](https://github.com/simd-lite/simd-json/blob/61d649d13fae83ac6d9f587863696be2741f5b8b/Cargo.toml) and [README](https://github.com/simd-lite/simd-json/blob/61d649d13fae83ac6d9f587863696be2741f5b8b/README.md).
- [R1] [json-rust parser](https://github.com/maciejhirsz/json-rust/blob/0775592d339002ab148185264970c2a6e30b5d37/src/parser.rs).
- [R2] [json-rust inline strings](https://github.com/maciejhirsz/json-rust/blob/0775592d339002ab148185264970c2a6e30b5d37/src/short.rs).
- [R3] [json-rust object representation](https://github.com/maciejhirsz/json-rust/blob/0775592d339002ab148185264970c2a6e30b5d37/src/object.rs).
- [R4] [json-rust decimal number representation](https://github.com/maciejhirsz/json-rust/blob/0775592d339002ab148185264970c2a6e30b5d37/src/number.rs).
- [R5] [json-rust serializer](https://github.com/maciejhirsz/json-rust/blob/0775592d339002ab148185264970c2a6e30b5d37/src/codegen.rs).
- [J1] [jsonic value and object representation](https://github.com/g1mv/jsonic/blob/b04980393d3617538672cd04d7872c30d5f5a920/src/json_item.rs).
- [J2] [jsonic parser](https://github.com/g1mv/jsonic/blob/b04980393d3617538672cd04d7872c30d5f5a920/src/lib.rs).
- [J3] [jsonic raw slices](https://github.com/g1mv/jsonic/blob/b04980393d3617538672cd04d7872c30d5f5a920/src/slice.rs).
- [J4] [jsonic key fingerprint](https://github.com/g1mv/jsonic/blob/b04980393d3617538672cd04d7872c30d5f5a920/src/key.rs).
- [J5] [jsonic manifest](https://github.com/g1mv/jsonic/blob/b04980393d3617538672cd04d7872c30d5f5a920/Cargo.toml).
- [J6] [jsonic sonic-rs benchmark](https://github.com/g1mv/jsonic/blob/b04980393d3617538672cd04d7872c30d5f5a920/benches/sonic-rs.rs).
- [J7] [jsonic simd-json benchmark](https://github.com/g1mv/jsonic/blob/b04980393d3617538672cd04d7872c30d5f5a920/benches/simd-json.rs).
- [H1] [sonic-rs SWAR integers, punctuation, pointer staging, and copies](https://github.com/cloudwego/sonic-rs/commit/0211f7c00f9b51ccbe6dee1cb9789d10ab9b2e3e).
- [H2] [sonic-rs wide punctuation peeks and short scalar keys](https://github.com/cloudwego/sonic-rs/commit/06a2a63898d3338dc6443e435ab05b730ada677a).
- [H3] [sonic-rs padded number parsing and SWAR fractions](https://github.com/cloudwego/sonic-rs/commit/15def0bab685602f3adfe47cf43b387834b9a011).
- [H4] [simd-json number inlining, signed digit count, and rejected integer SWAR](https://github.com/simd-lite/simd-json/commit/ebc6743b96428b6c5e6482a963ad45ee23b34985).
- [H5] [simd-json container closure branching](https://github.com/simd-lite/simd-json/commit/684fe91872556688fba1bfb7aa77c4f158c65c52).
- [H6] [simd-json reusable tape parsing](https://github.com/simd-lite/simd-json/commit/4e8cd75a7894e9c93c805f211a6a2a6567d32d9b).
- [H7] [simd-json buffer initialization extent](https://github.com/simd-lite/simd-json/commit/cf73db550f408ee415d12df052afe9d950039f8f) and [fixed padding copy](https://github.com/simd-lite/simd-json/commit/e05a9bd2b0a5a5b401bbcc8ff6a9d4fcca6dce15).
- [H8] [json-rust active stack frame improvement](https://github.com/maciejhirsz/json-rust/commit/87ed1194379c3b1d2085c5ce0b1ded5e1b663206).
- [H9] [json-rust removed allocation prediction](https://github.com/maciejhirsz/json-rust/commit/bf0e27ca3abf66a2aedbab91512f2a61026e8283).
- [H10] [sonic-rs TLS capacity limit](https://github.com/cloudwego/sonic-rs/commit/145a8830685a7ee3dceeaf20b60fb6ea890e7df7).
- [H11] [sonic-rs single-pass f32 parsing](https://github.com/cloudwego/sonic-rs/commit/03545a9530346fe279b674dd496e037d94204bc5).
- [H12] [simd-json fallback UTF-8 validation fix](https://github.com/simd-lite/simd-json/commit/acabe0c13e98424ace8a11ae9b1248ccb6e84025).
- [H13] [jsonic hybrid objects](https://github.com/g1mv/jsonic/commit/c1b74f9459d52e513385a215ee6020c9b1b5fee3).
- [H14] [jsonic deferred empty-container construction](https://github.com/g1mv/jsonic/commit/e676afe61007ce6225a836dbbf7d16f0ac183538).
- [H15] [sonic-rs owned object hash map](https://github.com/cloudwego/sonic-rs/commit/272ef60b10f13026325771e7cf6b8a2b4b847358).
- [H16] [sonic-rs pair append](https://github.com/cloudwego/sonic-rs/commit/4cbb9780f4848e3a58f216b64d669d2121b3707f).
- [H17] [sonic-rs float formatter change](https://github.com/cloudwego/sonic-rs/commit/ae08e4e4f5d6e629630b5895ebb676142e593b74).
- [H18] [simd-json NEON mask extraction](https://github.com/simd-lite/simd-json/commit/af5bd05dd9cf5603cbdef87150908bba33a67922).
- [H19] [sonic-rs reader and writer inlining](https://github.com/cloudwego/sonic-rs/commit/88314fe42001104251d0cf220586485eb52f3909).
- [H20] [simd-json stable branch hints](https://github.com/simd-lite/simd-json/commit/c83335adf4d5b580203ed0f3c84ccf7655ebf21a).
- [H21] [simd-json pointer interface compiler workaround](https://github.com/simd-lite/simd-json/commit/0eee8cb50c1e22357b9ed72c8a1395b812c6a137).
- [H22] [simd-json string optimization reverted after invalid reads](https://github.com/simd-lite/simd-json/commit/32fcd779cf20f14902b3dd11ced83d9611fb821f).

[S1]: https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/src/parser.rs
[S2]: https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/src/value/node.rs
[S3]: https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/sonic-number/src/swar.rs
[S4]: https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/src/util/string.rs
[S5]: https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/sonic-number/src/lib.rs
[S6]: https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/src/value/tls_buffer.rs
[S7]: https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/src/pointer/tree.rs
[S8]: https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/src/util/utf8.rs
[S9]: https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/src/lazyvalue/get.rs
[S10]: https://github.com/cloudwego/sonic-rs/blob/7eca25c81884264043eb5434c1fa3fcba49600b1/Cargo.toml
[M1]: https://github.com/simd-lite/simd-json/blob/61d649d13fae83ac6d9f587863696be2741f5b8b/src/lib.rs
[M2]: https://github.com/simd-lite/simd-json/blob/61d649d13fae83ac6d9f587863696be2741f5b8b/src/impls/avx2/stage1.rs
[M3]: https://github.com/simd-lite/simd-json/blob/61d649d13fae83ac6d9f587863696be2741f5b8b/src/value/lazy.rs
[M4]: https://github.com/simd-lite/simd-json/blob/61d649d13fae83ac6d9f587863696be2741f5b8b/src/known_key.rs
[M5]: https://github.com/simd-lite/simd-json/blob/61d649d13fae83ac6d9f587863696be2741f5b8b/src/impls/neon/stage1.rs
[M6]: https://github.com/simd-lite/simd-json/blob/61d649d13fae83ac6d9f587863696be2741f5b8b/Cargo.toml
[R1]: https://github.com/maciejhirsz/json-rust/blob/0775592d339002ab148185264970c2a6e30b5d37/src/parser.rs
[R2]: https://github.com/maciejhirsz/json-rust/blob/0775592d339002ab148185264970c2a6e30b5d37/src/short.rs
[R3]: https://github.com/maciejhirsz/json-rust/blob/0775592d339002ab148185264970c2a6e30b5d37/src/object.rs
[R4]: https://github.com/maciejhirsz/json-rust/blob/0775592d339002ab148185264970c2a6e30b5d37/src/number.rs
[R5]: https://github.com/maciejhirsz/json-rust/blob/0775592d339002ab148185264970c2a6e30b5d37/src/codegen.rs
[J1]: https://github.com/g1mv/jsonic/blob/b04980393d3617538672cd04d7872c30d5f5a920/src/json_item.rs
[J2]: https://github.com/g1mv/jsonic/blob/b04980393d3617538672cd04d7872c30d5f5a920/src/lib.rs
[J3]: https://github.com/g1mv/jsonic/blob/b04980393d3617538672cd04d7872c30d5f5a920/src/slice.rs
[J4]: https://github.com/g1mv/jsonic/blob/b04980393d3617538672cd04d7872c30d5f5a920/src/key.rs
[J5]: https://github.com/g1mv/jsonic/blob/b04980393d3617538672cd04d7872c30d5f5a920/Cargo.toml
[J6]: https://github.com/g1mv/jsonic/blob/b04980393d3617538672cd04d7872c30d5f5a920/benches/sonic-rs.rs
[J7]: https://github.com/g1mv/jsonic/blob/b04980393d3617538672cd04d7872c30d5f5a920/benches/simd-json.rs
[H1]: https://github.com/cloudwego/sonic-rs/commit/0211f7c00f9b51ccbe6dee1cb9789d10ab9b2e3e
[H2]: https://github.com/cloudwego/sonic-rs/commit/06a2a63898d3338dc6443e435ab05b730ada677a
[H3]: https://github.com/cloudwego/sonic-rs/commit/15def0bab685602f3adfe47cf43b387834b9a011
[H4]: https://github.com/simd-lite/simd-json/commit/ebc6743b96428b6c5e6482a963ad45ee23b34985
[H5]: https://github.com/simd-lite/simd-json/commit/684fe91872556688fba1bfb7aa77c4f158c65c52
[H6]: https://github.com/simd-lite/simd-json/commit/4e8cd75a7894e9c93c805f211a6a2a6567d32d9b
[H7]: https://github.com/simd-lite/simd-json/commit/cf73db550f408ee415d12df052afe9d950039f8f
[H8]: https://github.com/maciejhirsz/json-rust/commit/87ed1194379c3b1d2085c5ce0b1ded5e1b663206
[H9]: https://github.com/maciejhirsz/json-rust/commit/bf0e27ca3abf66a2aedbab91512f2a61026e8283
[H10]: https://github.com/cloudwego/sonic-rs/commit/145a8830685a7ee3dceeaf20b60fb6ea890e7df7
[H11]: https://github.com/cloudwego/sonic-rs/commit/03545a9530346fe279b674dd496e037d94204bc5
[H12]: https://github.com/simd-lite/simd-json/commit/acabe0c13e98424ace8a11ae9b1248ccb6e84025
[H13]: https://github.com/g1mv/jsonic/commit/c1b74f9459d52e513385a215ee6020c9b1b5fee3
[H14]: https://github.com/g1mv/jsonic/commit/e676afe61007ce6225a836dbbf7d16f0ac183538
[H15]: https://github.com/cloudwego/sonic-rs/commit/272ef60b10f13026325771e7cf6b8a2b4b847358
[H16]: https://github.com/cloudwego/sonic-rs/commit/4cbb9780f4848e3a58f216b64d669d2121b3707f
[H17]: https://github.com/cloudwego/sonic-rs/commit/ae08e4e4f5d6e629630b5895ebb676142e593b74
[H18]: https://github.com/simd-lite/simd-json/commit/af5bd05dd9cf5603cbdef87150908bba33a67922
[H19]: https://github.com/cloudwego/sonic-rs/commit/88314fe42001104251d0cf220586485eb52f3909
[H20]: https://github.com/simd-lite/simd-json/commit/c83335adf4d5b580203ed0f3c84ccf7655ebf21a
[H21]: https://github.com/simd-lite/simd-json/commit/0eee8cb50c1e22357b9ed72c8a1395b812c6a137
[H22]: https://github.com/simd-lite/simd-json/commit/32fcd779cf20f14902b3dd11ced83d9611fb821f

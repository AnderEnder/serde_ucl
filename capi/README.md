# serde_ucl C API — Stage A

This separate package builds `libucl.a` and `libucl.dylib` (Darwin), or `libucl.so`
(on Unix platforms using that suffix), with an installable `ucl.h`. It provides
exactly the 43 read-only functions and aliases in released spec-v22.
The exact released contract is bundled in the matching
`serde-ucl-c-VERSION-source.tar.gz` archive at `docs/spec/c-api/stage-a/README.md`,
with the declaration artifact at `docs/spec/c-api/stage-a/include/ucl.h` and its
function/case inventories alongside them. Extract that archive to read the contract
offline; the SDK does not depend on an online spec tag for its documentation.

Local compatibility evidence is Darwin arm64, LP64. Release automation additionally
runs native Linux amd64 and Linux arm64 reference gates; those SDKs are attached
only after their actual target ABI/behavior checks pass. Linux execution has not
been performed locally on the macOS development host.

A parser accepts **one complete input submission** through add_chunk, add_string
or add_file, including an unsuccessful submission. A second submission returns
false, records ESTATE and preserves the first result. Constructors, mutation,
callbacks, comment access, binary emission and multiple-input parsing are deferred.
The public Rust crate retains its existing defaults and APIs.

## Distribution identity and isolation

The opt-in C package/distribution is **serde-ucl-c**; its pkg-config module is
**serde-ucl**. It exports library `ucl` and header `ucl.h` to preserve the released
C interface. The Rust crate `serde_ucl` does not build/install this library by
default, and its crates.io package contents remain unchanged.

This is a 43-function read-only subset, not a full upstream libucl replacement.
Use a dedicated installation prefix and explicit `PKG_CONFIG_PATH` to select it
alongside upstream libucl. Do not install both libraries/headers into the same
prefix, link both into one process, or exchange their handles. Symbol and header
names overlap; distinct package identity does not provide symbol namespaces.
Other operating systems, Intel macOS and musl targets remain deferred.

## Build and install from source

Source builds require Rust 1.99 or newer, a native C linker/toolchain, and pinned
[cargo-c 0.10.25](https://github.com/lu-zero/cargo-c/blob/v0.10.25/docs/configuration.md).
The source archive includes both `capi/` and its Rust path dependency; keep them
together. Its root manifest omits benchmark targets because those development
harnesses are excluded; Rust/C implementation sources are unchanged. Registry
dependencies are fetched by Cargo (the archive is not vendored or an offline build). From the extracted source directory or repository root:

```sh
cargo install cargo-c --version 0.10.25+cargo-0.99.0 --locked
cargo cinstall --locked --release --manifest-path capi/Cargo.toml \
  --prefix "$HOME/.local/serde-ucl-c" --libdir "$HOME/.local/serde-ucl-c/lib"
```

On GNU/Linux install your distribution's native C/C++ compiler, pkg-config and
OpenSSL development files before building cargo-c (Debian/Ubuntu packages:
`build-essential pkg-config libssl-dev`). On macOS use Xcode command-line tools
and pkg-config; cargo-c may require the host's OpenSSL development package.
These source build commands use the native host toolchain on all three targets.

`make -C capi install PREFIX=... DESTDIR=...` wraps cargo-c and explicitly uses
`LIBDIR=$(PREFIX)/lib` on every host, matching the SDK layout. Override `LIBDIR=...`
for a different source installation layout. `DESTDIR` stages the installation
while retaining the final prefix in the library/pkg-config metadata.
The default prefix is `/usr/local`; choose an isolated prefix explicitly. cargo-c
copies the exact reviewed `include/ucl.h`; declaration generation and version
constants are disabled. Package version follows the Rust release version; update
`capi/Cargo.toml` and `capi/Cargo.lock` together with the root release manifests.
The release check rejects a C/Rust version mismatch.

The shared ABI version is independently set to **1.0.0** in
`package.metadata.capi.library`. Its Darwin install identity is
`<prefix>/lib/libucl.1.dylib` for an ordinary source installation, with compatibility
version 1.0.0 and current version 1.0.0. The SDK instead uses
`@rpath/libucl.1.dylib`. Linux ELF SONAME is `libucl.so.1`, with actual file
`libucl.so.1.0.0` and development symlink `libucl.so`. ABI-breaking
changes require a new ABI major and reviewed contract; compatible additions may
raise the ABI minor, and compatible fixes may raise the patch. Rust semver alone
does not change the C ABI major. Rebuild static consumers when upgrading.

## Native SDKs: no Rust needed

Normal future GitHub releases produce one `serde-ucl-c-VERSION-source.tar.gz`
and three native SDK archives, collected under one outer `SHA256SUMS`:

| SDK archive suffix | Native release runner | Deployment evidence |
| --- | --- | --- |
| `linux-amd64` | `ubuntu-24.04` (x86_64) | ELF64 x86-64, SONAME 1, measured GLIBC requirements |
| `linux-arm64` | `ubuntu-24.04-arm` (aarch64) | ELF64 AArch64, SONAME 1, measured GLIBC requirements |
| `macos-arm64` | `macos-15` (arm64) | Mach-O arm64, ABI 1, macOS 11.0 deployment floor |

Runner OS and architecture are asserted before candidate compilation; no SDK is
cross-built or mislabeled. Each candidate must pass the pinned native reference
comparison before packaging. Linux uses newly generated Linux snapshots, never
Darwin output as Linux evidence. A failure on any target prevents Rust publishing
and C artifact attachment. Existing tag/changelog checks and Trusted Publishing
remain in force. Reruns replace uniquely named input artifacts; collection rejects
missing, extra, corrupt or mixed-version/commit inputs before consolidated output.

Each archive contains file checksums, project/dependency/Rust runtime notices
and `BUILD-INFO.json` recording the commit, spec, toolchain and ABI. SDKs contain
`include/ucl.h`, static/shared libraries, `lib/pkgconfig/serde-ucl.pc`, this README,
and `conformance/` with their ten target snapshots and reference evidence.
Extract anywhere; pkg-config uses a relative prefix. Darwin uses an rpath install
identity; Linux retains a path-independent SONAME and no build-directory RPATH.
A C/C++ compiler and pkg-config suffice for consumption; Rust/cargo-c are build tools.

The macOS deployment floor remains **11.0 arm64 (LP64)**. Local execution used
macOS 15.8.1; the floor is checked in Mach-O metadata, with no macOS 11 runtime
execution claim. Linux SDKs target GNU glibc, not musl. Their metadata records the
runner's actual glibc, kernel and OS, plus `deployment.shared_glibc_symbol_floor`
and `deployment.static_consumer_glibc_symbol_floor`. `deployment.glibc_symbol_floor`
is the maximum version required by the shared object and tested static C/C++
consumers, measured numerically from ELF version requirements. No fixed older
Linux floor is guessed, and unknown GLIBC ABI requirement names fail packaging.
Consult your downloaded SDK's actual metadata; these measured symbol requirements
do not establish execution on older glibc, distributions or kernels. Execution is
tested on the named native runner only. Static consumers can introduce additional
system requirements depending on their code and linker/toolchain.

Verify downloaded outer checksums with `sha256sum -c SHA256SUMS` on Linux or
`shasum -a 256 -c SHA256SUMS` on macOS, then extract your matching SDK. For example:

```sh
tar -xzf serde-ucl-c-VERSION-linux-amd64.tar.gz
cd serde-ucl-c-VERSION-linux-amd64
sha256sum -c SHA256SUMS
export SDK="$PWD"
export PKG_CONFIG_PATH="$SDK/lib/pkgconfig"
pkg-config --modversion serde-ucl
cc app.c $(pkg-config --cflags --libs serde-ucl) \
  -Wl,-rpath,"$SDK/lib" -o app
```

The compiler/pkg-config commands work for each SDK; macOS uses its archive suffix
and `shasum` instead. For static linkage, select the archive explicitly (otherwise
linkers normally prefer the shared library). Retain native dependencies from `--static`:

```sh
cc app.c $(pkg-config --cflags serde-ucl) "$SDK/lib/libucl.a" \
  $(pkg-config --static --libs-only-other serde-ucl) \
  $(pkg-config --static --libs-only-l serde-ucl | sed 's/-lucl//g') -o app
```

macOS native static dependencies here are `iconv`, `System`, `c`, `m`; Linux
compiler/runtime/system link dependencies are recorded by the actual native build
in `Libs.private`. Use the supplied pkg-config flags rather than a copied list.
To inspect your source toolchain's exact native dependencies:

```sh
cargo rustc --release --manifest-path capi/Cargo.toml --lib -- --print native-static-libs
```

Ship the full versioned shared library and its ABI alias with your application:
`libucl.so.1.0.0` + `libucl.so.1` on Linux, or `libucl.1.0.0.dylib` +
`libucl.1.dylib` on Darwin. Linux applications can use an `$ORIGIN/../lib` rpath;
Darwin applications can use `@executable_path/../lib`. Preserve library identity.
The macOS SDK is unsigned/not notarized; application signing belongs to its owner.

## Repository archive and CI validation

Python 3.12+, pkg-config, C/C++ compilers, pinned cargo-c, Rust LLVM tools and
runtime license documentation are required. Linux additionally uses native binutils
(`readelf`) and CMake for the black-box reference gate; Darwin uses Xcode Mach-O
tools and CMake. The full release pipeline runs on native hosts in GitHub CI.
There is no local Linux VM/emulator setup and no claim of local Linux execution.

For a local Darwin regression in a repository checkout:

```sh
rustup component add llvm-tools rust-docs
python3 capi/distribution.py --target macos-arm64 --verify --output target/c-local
python3 capi/tests/distribution_test.py
python3 capi/tests/check.py
```

The archive builder reads released documentation from `spec-v22`, which must exist
in the checkout and be available to release CI on the remote. Source/SDK release
jobs require that tag to be fetched; ordinary Rust/C pull-request checks use the
checked-in contract and do not require it. The builder never pushes tags.
`--kind source` emits only the source archive; `--kind sdk` emits only the native SDK;
default `all` is convenient locally. `--assert-native --target TARGET` checks the
actual host and refuses cross-target requests. `--require-clean` checks the release
checkout/commit. Per-job checksum names are unique; the collector creates the outer
release `SHA256SUMS` only after all four archives pass identity/digest checks.

The native SDK release sequence is candidate build, black-box reference comparison,
then SDK construction using the same staged libraries and generated target evidence:

```sh
# TARGET is linux-amd64, linux-arm64 or macos-arm64 on its corresponding native host.
python3 capi/distribution.py --build-only --target "$TARGET" \
  --candidate-prefix target/c-candidate --require-clean
# Release CI invokes the spec-owned reference comparator as a black box here.
python3 capi/distribution.py --kind sdk --target "$TARGET" \
  --candidate-prefix target/c-candidate --golden-dir target/c-oracle/golden \
  --verify --require-clean --output target/c-sdk
```

`--golden-dir` requires all ten snapshots and the successful comparator's
`reference-evidence.json`. The builder checks the native platform, reference pin,
contract/probe/snapshot hashes, 43-function/four-combination counts, and hashes of
the staged candidate libraries. Linux refuses missing native evidence. Oracle
source/build tooling stays out of all distributions; only public snapshots/evidence
are bundled. External evidence lives under ignored `target/`, preserving clean checks.

`--verify` extracts archives and relocates the SDK into a path with a space,
checks contents/checksums/header/ABI/architecture/deployment, tests all 43 symbols
and C11/C++11 signatures, and runs ten native snapshots plus boundary/lifetime/depth
checks through static/shared pkg-config linkage. It also builds and installs clean
extracted source with locked dependencies and repeats the native checks. Metadata
fixtures test Linux parsing and collection gates on macOS; they are synthetic
packaging tests, not Linux execution or reference conformance results.

The existing conformance driver also validates direct and installed libraries.
Its `--sanitize` mode instruments C callers with ASan/UBSan; `--rust-asan`
instruments Rust with nightly ASan plus C ASan/UBSan. Darwin has no LeakSanitizer.
Full Rust instrumentation requires a compatible non-Apple LLVM compiler/runtime;
the validated command is:

```sh
CC=/opt/homebrew/opt/llvm@21/bin/clang \
CXX=/opt/homebrew/opt/llvm@21/bin/clang++ \
python3 capi/tests/check.py --rust-asan
```

The prebuilt Rust standard library remains uninstrumented. The conformance program
in `tests/conformance/capi/stage-a/probe.c` is also a complete C link example.
That program and the validation scripts are in the matching source archive/repository,
not in the binary SDK. Use the source archive for the build and validation commands
above; SDK consumption needs only the compiler and pkg-config commands.

## Ownership and caller domain

```c
#include <ucl.h>
#include <assert.h>
#include <stdlib.h>

int main(void) {
    struct ucl_parser *parser = ucl_parser_new(0);
    assert(ucl_parser_add_string(parser, "child={answer=42}", 0));
    ucl_object_t *root = ucl_parser_get_object(parser); /* owned reference */
    const ucl_object_t *child = ucl_object_lookup(root, "child"); /* borrowed */
    assert(ucl_object_toint(ucl_object_lookup(child, "answer")) == 42);
    unsigned char *json = ucl_object_emit(root, UCL_EMIT_JSON_COMPACT);
    free(json); /* every emission is independently allocated and C-free-compatible */
    ucl_object_unref(root);
    ucl_parser_free(parser);
}
```

Every successful get_object or object_ref grants an owned reference that must be
balanced with object_unref. The parser owns its result independently. Lookup,
conversions and iteration borrow their objects and strings. Retaining a child
without implicit siblings lets it and its descendants survive the parent and
parser. For duplicate chains, keep the parent tree alive until separate chain
references have been released; retaining a head does not retain its siblings.

Strings and keys are UTF-8, NUL terminated, and can contain embedded NUL bytes
when a length is supplied. Chunk lengths are exact; add_string's zero length
selects strlen; lookup_len's zero length compares the empty key. ZEROCOPY callers
must preserve input storage unchanged until all derived objects are released.
Objects are read-only: never write public fields, construct them yourself, mix
libraries' handles or dereference opaque container/storage pointers. Synchronize
concurrent access to reference state, parsers and iterators. Safe conversion
failure preserves caller outputs. NULL support follows the released contract;
invalid pointers, unsupported enum values and allocation exhaustion are outside
its verified domain. No Rust panic unwinds across the ABI boundary.

Iterators do not retain targets. Old iteration handles are specific to one
object/mode: initialize them to NULL, and use iterate_end only for expanded
objects. Scalar/array old handles require no cleanup. Safe iterator handles must
be freed, can be reset after exhaustion and require their tree to remain alive.
The historical chk_excpn declaration accepts the **handle cast** to
`ucl_object_iter_t *`, rather than the address of the handle.

C parsing supports the filesystem and built-in include/load macros. Relative
includes resolve against process cwd for both memory and file inputs; FILENAME
and CURDIR for file inputs are the canonical filename and its parent. Error
strings belong to the parser; exact wording is unspecified. get_object can
return a partial tree after failure and does not imply parse success. Text
emitter selectors 0–3 are supported. See the released contract for all flags,
conversion and iterator details.


## Adapter overhead and profiling

Public objects use stable arena storage with 64-byte public headers. Only containers
have private child-range, entry-head and fact metadata. Array elements and object
values occupy contiguous arena ranges; only objects with duplicate entries need a
separate head-pointer table. Lookup reuses the Rust object's key index and iteration
uses the cached head count. All headers, terminated key/value bytes and child ranges
are built during submission.

For memory submissions, the Rust emission tree borrows unchanged keys and strings
from one adapter-owned immutable copy of the input. The caller's buffer can be reused
immediately after ordinary submission; no caller buffer is retained. File input,
decoded/expanded strings and error snapshots keep their existing owned paths. A
retained child keeps the arena, Rust tree, output facts and backing input alive until
the last owned reference is released. Final release drops the Rust tree before its
backing input; parent release with retained children still updates public reference
counts iteratively without allocating a traversal.

Forced conversions use a sparse arena-owned cache. Its inner terminated buffers
keep their addresses when cache metadata grows. Container emissions retain their
subtree fact cursor; scalar string emissions select the recorded single-quote/heredoc
facts already represented by their public flags. No tree construction is deferred to
lookups, iteration or emission. Creation-time cwd capture and all released public
observations remain supported.

Latest stable Rust 1.99 is used throughout. Its new `Box::into_non_null` and
`Box::from_non_null` APIs make the backing-input ownership transfer explicit: the
allocation is not represented by an exclusive `Box` while parsed strings borrow it.
This follows the standard library's [Box ownership guidance](https://doc.rust-lang.org/stable/std/boxed/struct.Box.html#method.into_non_null).
The [Rust 1.99 release](https://blog.rust-lang.org/2026/10/01/Rust-1.99.0/) was also
reviewed for performance opportunities: C variadic definitions, unsized raw layout,
boxed-array iteration, lossy UTF-8 conversion and filesystem timestamp APIs do not
apply to these fixed-signature, valid-UTF-8, sized arena paths. Compiler and adapter
changes are measured separately, with baseline and candidate both built on 1.99.

An independent implementation-side counting profile is available in repository
checkouts:

```sh
cargo run --release --manifest-path capi/Cargo.toml --example adapter_profile -- 10000
cargo run --release --manifest-path capi/Cargo.toml --example adapter_profile -- \
  benches/corpus/rspamd/scores.d/rbl_group.conf
```

The numeric argument generates deterministic records; a filename selects a
standalone input document. JSON lines report arithmetic means after three warmups and 30 measured lifecycles,
setup/submission/destruction times, Rust allocator requests (reallocations count
as requests), cumulative requested bytes, peak extra live bytes and remaining live
bytes. Input generation and caller buffers are outside the counted scope; libc
allocations, such as emission buffers, are not counted. No timing thresholds are
asserted. The counting allocator adds measurement overhead; use ordinary public
C/Rust benchmarks for precise timings. `PROFILE_ROUNDS` changes the sample count
and `PROFILE_MODE=C` selects C-only runs for native CPU sampling.

`Rust` remains the default public memory-parser comparison. The separately labeled
`RustObserved` diagnostic uses a Rust parser with creation-time cwd, filesystem
loader and the C observation API (facts, partial results and byte cursor), without
constructing C headers. It separates differing services from ABI marshalling;
it does not replace the default Rust baseline. One 10,000-record profile measured
6.14 ms for default Rust, 6.20 ms for RustObserved and 7.40 ms for C, with about
1.17 ms of the remaining difference in submission/arena construction.

`PROFILE_READS=1` measures safe-iterator creation, first child or all immediate
children, and iterator cleanup against an already parsed live tree. Parse/input
allocation is excluded from these read measurements. Iterators now stream borrowed
children directly, with one 32-byte handle allocation and no allocation in next or
reset. For the same 10,000-record input, first-child traversal fell from 4.26 µs
and three allocations requesting 160,064 bytes to 43 ns and one allocation requesting
32 bytes; full immediate-child traversal fell from 23.8 µs to 15.8 µs in this Rust
calling harness. Ordinary separately compiled C callers may have different timings.

Historical first-pass evidence (2026-10-03): on local Apple M4 Max Darwin arm64,
Rust 1.98 release with fat LTO and one codegen unit, the counting harness compared
the original merged Stage A adapter with the first arena optimization:

| Input | C before, total | C after, total | Rust after, total | C allocation requests, before → after |
| --- | ---: | ---: | ---: | ---: |
| 1,000 generated records, 67,670 bytes | 3.39 ms | 0.713 ms | 0.604 ms | 82,159 → 7,045 |
| 10,000 generated records, 706,670 bytes | 37.42 ms | 7.395 ms | 6.071 ms | 820,212 → 70,059 |
| rbl_group.conf, 13,404 bytes | 293 µs | 75.3 µs | 55.6 µs | 6,644 → 658 |

For 10,000 records, requested allocation bytes fell from 74.64 MB to 11.89 MB;
peak extra live storage fell from 30.19 MB to 9.92 MB. Rust used 70,037 allocation
requests, so adaptation adds 22 requests in that case; every measured lifecycle
returned to its original live-allocation baseline. Native CPU samples identified
allocator/free traffic and owned-tree reconstruction as the original hotspots;
those reconstructions and per-node allocation graphs have been removed.

These measurements do **not** establish zero total overhead. The remaining large
input cost includes building immediately observable C headers, terminated bytes,
child links, and retaining the immutable Rust emission model. Very small default
C parses are dominated by creation-time cwd capture, which the default Rust memory
parser does not perform. An empty-input lifecycle measured about 11.31 µs through
C and 0.250 µs through Rust here. Retaining the reviewed C environment preserves
that fixed cost; timing and ratios depend on shape, machine and measurement method.


The arena owner pointer is published only after construction from its final
`Box::into_raw` provenance. A maintained Rust ownership regression covers retained
nodes, duplicate headers, forced-string cache growth and emission; ordinary CI
runs it. To also check Rust aliasing/provenance rules in a repository checkout:

```sh
MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test \
  --manifest-path capi/Cargo.toml --test arena_lifetimes --test review_ownership
```

Filesystem isolation is disabled for the existing creation-time cwd service.


The second pass on Rust 1.99 reduces 10,000 generated-record C allocator requests
from 70,059 to 10,060 by borrowing unchanged parsed text from one owned input.
Requested bytes fall from 11.887 MB to 10.916 MB and peak extra live storage from
9.920 MB to 8.949 MB. The 1,000-record request count falls from 7,045 to 1,046;
rbl_group.conf falls from 658 to 214. All measured lifecycles return to their original
live-allocation baseline. These counters are scoped as described above and compare
frozen merged and candidate binaries using the same Rust 1.99 toolchain.

One local second-pass counting run measured 6.405 ms for C, 6.423 ms for default
Rust and 6.546 ms for RustObserved on the 706,670-byte generated document. Full
immediate-object traversal measured 15.6 µs against about 17.4 µs for the frozen
merged adapter, with one handle allocation in each. These means are diagnostic,
not timing guarantees: machine load varied during this session and independent
interleaved public-call measurements are the acceptance evidence. The small rbl
counting run retained about 18% total overhead and tiny parses still pay creation-time
cwd capture. Escapes, lowercase keys, included/file text and partial error snapshots
can require owned text, while immediately observable C headers and terminated
strings remain additional representation cost. Near-zero total cost on one generated
shape does not establish zero overhead for every input or equal memory usage with Rust.


Implementation-owned native CPU sampling of the frozen Rust 1.99 counting binaries
collected 8,315 baseline and 8,296 candidate samples. Allocator-library leaf samples
shown in `sample`'s flat table fell from 15.03% to 5.01%; copy/zero leaf samples fell
from 4.71% to 4.38%. The flat table omits symbols below five samples, and the counting
allocator itself affects this diagnostic; these percentages are not whole-program
speedup claims. The measured remaining arena initialization/key-quoting/copy work
and backing-input retention still explain why adaptation is not universally free.


Second-pass profile values above are arithmetic means after three warmups and
100 measured lifecycles; the iterator mean uses 20,000 measured traversals. The
backing `Box<[u8]>` requests exactly the memory-input length once (no allocation
for empty text). Reported allocation bytes are requested layout sizes, including
reallocations, rather than allocator usable capacity.

The near-parity comparison is with the public **owned/default** Rust parser.
The C memory path now uses the internal borrowed reader, so the two paths have
different text-storage costs. It still adds eagerly constructed C headers,
container metadata, terminated bytes and backing ownership over a borrowed Rust
tree; equal total time against owned Rust does not demonstrate zero ABI marshalling
against that borrowed representation. `RustObserved` remains an owned Rust
observation diagnostic, not a borrowed-parser baseline.

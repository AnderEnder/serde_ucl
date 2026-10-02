# serde_ucl C API — Stage A

This separate package builds `libucl.a` and `libucl.dylib` (Darwin), or `libucl.so`
(on Unix platforms using that suffix), with an installable `ucl.h`. It provides
exactly the 43 read-only functions and aliases in [released spec-v22](../docs/spec/c-api/stage-a/README.md).
The verified compatibility target is Darwin arm64, LP64. Other targets require
reference ABI and behavior validation before compatibility is claimed.

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
Linux and other binary targets are deferred until reference ABI/behavior validation.

## Build and install from source

Source builds require Rust 1.98 or newer, a native C linker/toolchain, and pinned
[cargo-c 0.10.25](https://github.com/lu-zero/cargo-c/blob/v0.10.25/docs/configuration.md).
The source archive includes both `capi/` and its Rust path dependency; keep them
together. Its root manifest omits benchmark targets because those development
harnesses are excluded; Rust/C implementation sources are unchanged. Registry dependencies are fetched by Cargo (the archive is not vendored
or an offline build). From the extracted source directory or repository root:

```sh
cargo install cargo-c --version 0.10.25+cargo-0.99.0 --locked
cargo cinstall --locked --release --manifest-path capi/Cargo.toml \
  --prefix "$HOME/.local/serde-ucl-c"
```

`make -C capi install PREFIX=... DESTDIR=...` wraps cargo-c; `DESTDIR` stages the
installation while retaining the final prefix in the library/pkg-config metadata.
The default prefix is `/usr/local`; choose an isolated prefix explicitly. cargo-c
copies the exact reviewed `include/ucl.h`; declaration generation and version
constants are disabled. Package version follows the Rust release version; update
`capi/Cargo.toml` and `capi/Cargo.lock` together with the root release manifests.
The release check rejects a C/Rust version mismatch.

The shared ABI version is independently set to **1.0.0** in
`package.metadata.capi.library`. Its Darwin install identity is
`<prefix>/lib/libucl.1.dylib` for an ordinary source installation, with compatibility
version 1.0.0 and current version 1.0.0. The SDK instead uses
`@rpath/libucl.1.dylib`. `libucl.dylib` is the development symlink. ABI-breaking
changes require a new ABI major and reviewed contract; compatible additions may
raise the ABI minor, and compatible fixes may raise the patch. Rust semver alone
does not change the C ABI major. Rebuild static consumers when upgrading.

## macOS arm64 SDK: no Rust needed

Future normal GitHub releases attach `serde-ucl-c-VERSION-source.tar.gz`,
`serde-ucl-c-VERSION-macos-arm64.tar.gz`, and outer `SHA256SUMS`. Each archive
also contains file checksums, project/dependency/Rust runtime license notices and `BUILD-INFO.json` recording its source
commit, released spec, toolchain and ABI. The SDK contains `include/ucl.h`, static
and shared libraries, `lib/pkgconfig/serde-ucl.pc`, and this README. Extract it
wherever desired; its pkg-config prefix and shared-library identity are relocatable.
A C/C++ compiler and pkg-config suffice for consumption; Rust and cargo-c are
build tools only.

The binary deployment minimum is **macOS 11.0 on arm64 (LP64)**. Local runtime
validation used macOS 15.8.1; release CI validates on its macOS arm64 runner.
The deployment minimum is checked in Mach-O metadata; older OS runtime testing
is not claimed. No Intel/universal, Linux or other SDK is released in this stage.

Verify the downloaded archive checksums before extracting, then the extracted files:

```sh
shasum -a 256 -c SHA256SUMS
tar -xzf serde-ucl-c-VERSION-macos-arm64.tar.gz
cd serde-ucl-c-VERSION-macos-arm64
shasum -a 256 -c SHA256SUMS
export SDK="$PWD"
export PKG_CONFIG_PATH="$SDK/lib/pkgconfig"
pkg-config --modversion serde-ucl
cc app.c $(pkg-config --cflags --libs serde-ucl) \
  -Wl,-rpath,"$SDK/lib" -o app
```

For static linkage, select the archive explicitly because macOS otherwise prefers
the shared library when both are present. Retain the native dependencies reported
by `--static`:

```sh
cc app.c $(pkg-config --cflags serde-ucl) "$SDK/lib/libucl.a" \
  $(pkg-config --static --libs-only-other serde-ucl) \
  $(pkg-config --static --libs-only-l serde-ucl | sed 's/-lucl//g') -o app
```

For this macOS build, native static dependencies are `iconv`, `System`, `c` and
`m`; cargo-c records them in `Libs.private`.
For source builds on other targets the actual toolchain dependencies are authoritative:

```sh
cargo rustc --release --manifest-path capi/Cargo.toml --lib -- --print native-static-libs
```

Ship `libucl.1.0.0.dylib` and its `libucl.1.dylib` alias with a shared application and set an appropriate rpath
(for example `@executable_path/../lib`); keep its install identity intact. The SDK
is unsigned and not notarized; application signing/distribution belongs to its owner.

## Local archives and validation

In a repository checkout, Python 3.12+, pkg-config, C/C++ compilers, Xcode command-line Mach-O tools and
Rust LLVM tools are required for archive validation on macOS arm64:

```sh
rustup component add llvm-tools rust-docs
python3 capi/distribution.py --verify
python3 capi/tests/check.py
```

Artifacts go to `target/c15-dist` (`--output DIR` changes the destination).
`--verify` extracts both archives, moves the SDK to a path containing a space,
checks contents/checksums/header/ABI/deployment metadata and all 43 symbols and
C11/C++11 signatures, runs all ten released snapshots plus boundary/lifetime/depth
checks using static/shared pkg-config linkage, and builds/installs the extracted
source with its locked dependencies before repeating the checks. `BUILD-INFO.json`
marks local dirty-tree builds; release CI builds from the tested tagged commit.

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

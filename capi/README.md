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

## Build, install and link

Rust 1.98 or newer, C/C++ compilers, make, Python 3 and matching LLVM symbol tools are needed for the complete check.
From the repository root:

```sh
cargo build --release --manifest-path capi/Cargo.toml
make -C capi install PREFIX="$PWD/capi/target/install"
rustup component add llvm-tools
python3 capi/tests/check.py
```

The installed header is an exact copy of the released declaration artifact,
including its license notice. `DESTDIR` supports staged installation; `PREFIX`
defaults to `/usr/local`. Installation of the Darwin shared library sets its
install name to `@rpath/libucl.dylib`.

On Darwin, compile a C application against the installed static library with:

```sh
cc -std=c11 -Icapi/target/install/include app.c \
  capi/target/install/lib/libucl.a -framework Security -framework CoreFoundation \
  -liconv -lSystem -lc -lm -o app
```

For the shared library:

```sh
cc -std=c11 -Icapi/target/install/include app.c -Lcapi/target/install/lib -lucl \
  -Wl,-rpath,"$PWD/capi/target/install/lib" -o app
```

Linux static links use `-ldl -lpthread -lm`; shared links similarly use `-L` and
an installation-directory rpath. To obtain the actual native static dependencies
for your Rust toolchain and target:

```sh
cargo rustc --release --manifest-path capi/Cargo.toml --lib -- --print native-static-libs
```

The public conformance program is a working link example: replace `app.c` above
with `tests/conformance/capi/stage-a/probe.c`, then run `./app lifetime`. The check
script builds and executes that program with static and shared linkage, checks
all ten snapshots and compiles typed function pointers independently generated
from the released function inventory. It also checks the C11 and C++11 header,
installed artifacts, caller lifetime/boundary tests and deep trees. Its sanitizer
modes are `--sanitize` (C ASan/UBSan) and `--rust-asan` (nightly Rust ASan plus C ASan/UBSan). Darwin does not support LeakSanitizer; the scope of each run is printed. These modes require sanitizer runtimes, and the Rust mode requires nightly with LLVM tools installed.

On Darwin, full Rust instrumentation needs a non-Apple LLVM clang with a
compatible upstream ASan runtime; the system Apple clang runtime does not
satisfy Rust nightly's runtime version check. The validated command was:

```sh
CC=/opt/homebrew/opt/llvm@21/bin/clang \
CXX=/opt/homebrew/opt/llvm@21/bin/clang++ \
python3 capi/tests/check.py --rust-asan
```

The driver uses nightly `llvm-nm` to inspect nightly archives, external clang
runtime linkage and, on Darwin, preloads that compiler's ASan runtime for shared
library interception. Install the `llvm-tools` component for the selected
Rust toolchain if its `llvm-nm` is unavailable. `--sanitize` also checks direct
and installed libraries with C ASan/UBSan; `--rust-asan` checks fully instrumented
direct static/shared libraries. Both passed on Darwin arm64. The Rust library and its dependencies are instrumented in the nightly run; the prebuilt Rust standard library is uninstrumented. LeakSanitizer is unavailable on Darwin.

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

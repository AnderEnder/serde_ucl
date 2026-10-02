# Changelog

All notable changes to this crate are recorded here.

## Unreleased

### C distribution

C15 adds cargo-c installation using the unchanged reviewed header, distinct
`serde-ucl-c` package and `serde-ucl` pkg-config identities, and independent shared
ABI 1.0.0. Locally verified source and relocatable macOS arm64 SDK archives include
licenses, provenance and checksums. Release CI validates them before Rust Trusted
Publishing and attaches them to the same GitHub release. Native Linux amd64, Linux
arm64 and macOS arm64 release jobs now gate SDK attachment on actual target reference
comparisons, relocated C/C++ consumer checks and clean source builds. ELF SDKs record
SONAME, architecture, actual host glibc and measured symbol requirements; Darwin
deployment metadata remains unchanged. One source archive and three SDKs are collected
with unique job manifests and consolidated checksums, rejecting mixed or missing inputs.
Linux native execution is delegated to CI, not claimed from the local macOS checks.
Other binary targets remain deferred. Source builds and static/shared C consumption are documented.

### Initial C API (spec-v22)

Clean-room work item C15. The separate `capi/` package builds static/shared
libraries named `ucl` and installs the exact released public `ucl.h`. Stage A
provides 43 functions and aliases for one-input parsing, diagnostics, read-only
object metadata, references, conversions, lookup, iteration and four text
emitters. Retained child trees preserve their values and output facts after
parser/root release; emitted buffers can be released with C `free`.

Compatibility is verified initially on Darwin arm64, LP64. Multiple submissions,
constructors, mutation, callbacks, comment access and binary emission are
outside this milestone. Existing Rust parser defaults and APIs are preserved;
additive hidden adapter APIs expose the C cursor/partial result and subtree
output facts. See `capi/README.md` for build, installation and linking.

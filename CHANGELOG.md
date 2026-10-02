# Changelog

All notable changes to this crate are recorded here.

## Unreleased

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

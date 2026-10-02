# C15 Stage A Implementation Plan

**Goal:** Deliver the 43-function, one-submission C API from released spec-v22.

**Architecture:** A separate Rust staticlib/cdylib adapts the existing parser and emitters. Stable C object allocations form a read-only tree with independent reference counts; a shared Rust model keeps values and output facts alive. Parser observations are additive hidden Rust APIs and do not change existing entry points.

**Tech Stack:** Rust, libc allocation, C11/C++11, Python validation driver.

- [x] Add hidden parser observation and emitter fact-subtree APIs in `src/parse/` and `src/emit/mod.rs`; preserve partial containers, read cursor and fact paths.
- [x] Create `capi/Cargo.toml`, `capi/src/object.rs`, `parser.rs`, `iteration.rs`, `lib.rs`; export all 43 signatures, use C-free-compatible emission buffers, implement iterative construction/release.
- [x] Validate the released `probe.c` against all ten Darwin arm64 snapshots. Generate independent typed function-pointer checks from released `api.json`; compile C and C++ headers, static and shared links.
- [x] Add boundary, retained-tree, iterator and maximum-depth tests; run sanitizers over the probe and lifetime tests.
- [x] Add installation commands, document supported domain/subset, update README/changelog/provenance, run `scripts/ci.sh checks` and commit with `Work item: C15`.

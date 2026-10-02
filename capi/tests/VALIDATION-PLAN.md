# C15 Validation Implementation Plan

> **For agentic workers:** Execute this scoped validation work inline; the parent implementer owns library changes and commits.

**Goal:** Validate the released Stage A C ABI independently of its implementation.

**Architecture:** A Python driver derives signature checks from the released function inventory, builds static/shared libraries, and compares public probe output with ten snapshots. Independent C assertions cover the project boundary, ownership, and accepted maximum depth.

**Tech Stack:** Python 3, Cargo, C11, C++11, nm, AddressSanitizer/UndefinedBehaviorSanitizer.

- [x] Create `capi/tests/check.py`: load `api.json`/`cases.json`; require 43 provided functions and exact shipping header; generate pointers by replacing function names in each declaration with `(*signature_N)`; compile/link generated C11 and C++11 sources against static/shared libraries; require all provided exported symbols; run all ten probe cases in fresh directories with C locale/UTC and byte-compare stdout.
- [x] Create `capi/tests/extra.c`: assert second string/chunk/file submissions return ESTATE without accessing inaccessible arguments; cover failed and silent-stop first submissions, root identity preservation, independent retained child containers, stable borrowed conversion strings, old iterator early cleanup, safe iterator reset, deep parse/emit/free at 1023 arrays and objects.
- [x] Run `python3 capi/tests/check.py`; run its sanitizer mode against both C callers; report exact failures to the parent for library fixes. Sanitizers instrument the C callers and observe allocator use across the ABI; they do not instrument Rust.
- [x] Append the clean-room session log with allowed inputs, no commits, and the required attestation. Leave commits to the parent.

Validation outcome: normal and C ASan/UBSan modes passed direct and make-installed static/shared linkage, C11/C++11 signature checks, ten snapshots, and all independent C assertions. Nightly Rust ASan mode passed direct static/shared checks with Homebrew LLVM21 clang/clang++; Darwin LeakSanitizer is unavailable. The driver skips oracle comparisons explicitly on targets without released snapshots.

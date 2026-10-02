# C15 Distribution Implementation Plan

> **For agentic workers:** Execute this plan inline under the existing owner authorization and clean-room protocol.

**Goal:** Ship opt-in serde-ucl-c source and verified macOS arm64 SDK archives with exact reviewed declarations and release automation.

**Architecture:** Keep the separate capi crate. cargo-c installs the existing header, ABI-versioned libraries, and distinct pkg-config metadata. A focused Python distribution driver selects source inputs, builds a relocatable SDK, creates checksums, and tests extracted archives. Release jobs build and validate artifacts before Trusted Publishing and attach them to the same tag.

**Tech Stack:** Rust 1.98, pinned cargo-c, Python 3, C/C++, pkg-config, macOS linker tools, GitHub Actions.

### Task 1: cargo-c identity and installation
- [x] Modify capi/Cargo.toml: name serde-ucl-c, capi feature, disabled header generation, copied include/ucl.h, library ucl ABI 1.0.0, pkg-config serde-ucl.
- [x] Pin and install cargo-c, using its official README/source to check metadata behavior.
- [x] Run cargo cinstall with --locked --release and an isolated prefix; compare installed header and inspect dylib identity and pkg-config flags.
- [x] Keep make installation as a cargo-c wrapper and update capi/Cargo.lock.

### Task 2: archives and verification
- [x] Create capi/distribution.py with explicit source allowlist, source and SDK archive modes and SHA256SUMS generation. Include root Rust sources, manifests/lockfiles/licenses, capi code/checks, released C contract and cases, protocol/provenance. Exclude oracle tooling and build outputs.
- [x] Use MACOSX_DEPLOYMENT_TARGET=11.0 and aarch64-apple-darwin only for SDKs; install libraries with @rpath ABI identity and relative pkg-config prefix.
- [x] Verify SDK contents, exact header, checksums, ABI versions/minimum OS and relocation, 43 symbol signatures and ten golden cases for static/shared pkg-config links.
- [x] Extract source archive, run locked clean cargo-c installation and verify it with the same packaged-library checks.

### Task 3: docs and release integration
- [x] Document isolated opt-in subset, ABI policy, minimum OS, native dependencies, source and binary commands in capi/README.md; add root README link and changelog entry.
- [x] Add tested macOS arm64 distribution job after checks and before publish. Upload artifacts and download/attach them in github-release, including reruns, without changing Trusted Publishing controls.
- [x] Run capi checks, archive verification, required repository checks; append clean-room LOG with inputs, checks, commit and exact attestation.
- [x] Commit using conventional message and Work item: C15 footer. No push/tag/publish/merge.

Validation: source/SDK archive checks and existing C checks passed; scripts/ci.sh passed; actionlint passed. Final artifacts are regenerated from the completed clean commit.

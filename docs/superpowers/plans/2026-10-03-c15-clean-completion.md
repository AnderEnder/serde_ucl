# C15 Native Release Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Complete native Linux amd64/arm64 and macOS arm64 release automation with four consistent verified archives.

**Architecture:** Each native runner stages and verifies its own candidate against the authorized black-box oracle. Packaging verifies relocated static/shared consumers and a clean extracted-source install. A collector validates all four artifacts before existing publishing jobs.

**Tech Stack:** Python standard library, cargo-c, native C/C++ linkers, GitHub Actions.

---

This plan is independently derived by the fresh replacement participant from released spec-v22 and owner work goals. Existing predecessor plans were not read. Inline implementation is already authorized; no further delegation or scope approval is requested.

### Task 1: Review existing pre-exposure implementation

Files: `.github/workflows/release.yml`, `capi/distribution.py`, `capi/distribution_platform.py`, `capi/collect_distribution.py`, `capi/tests/distribution_test.py`.

- [x] Read current protocol before any other repository material; compare current released spec with `git diff spec-v22 -- docs/spec` (expected empty).
- [x] Review native assertions, ELF metadata, staged candidate hashes, target snapshots, relocation, static/shared runtime checks, and upload dependencies.
- [x] Run `python3 capi/tests/distribution_test.py` and `actionlint .github/workflows/release.yml` (expected pass).

### Task 2: Strengthen artifact consistency where review finds gaps

Files: `capi/collect_distribution.py`, `capi/tests/distribution_test.py`.

- [x] Add native machine/system evidence checks to collector expected evidence using `"machine": target.machine, "system": target.system`.
- [x] Require ELF/Mach-O format and machine metadata on Darwin too using `deployment.get("format") != "Mach-O" or deployment.get("machine") != "arm64"` as rejection conditions.
- [x] Extend synthetic collection fixtures with those fields and reject tampered evidence identity.
- [x] Run `python3 capi/tests/distribution_test.py` (expected pass).

### Task 3: Validate fresh native Darwin behavior and required repository checks

- [x] Run `scripts/ci.sh checks` (expected full Rust/C success).
- [x] Stage `python3 capi/distribution.py --build-only --target macos-arm64 --candidate-prefix target/c15-clean-candidate`.
- [x] Run authorized black-box CLI with that prefix and fresh `target/c15-clean-oracle` output; only sanitized public results may be read.
- [x] Run `python3 capi/distribution.py --kind all --target macos-arm64 --candidate-prefix target/c15-clean-candidate --golden-dir target/c15-clean-oracle/golden --verify --output target/c15-clean-dist` (expected source and relocated SDK pass).
- [x] Check release tag gates with `scripts/ci.sh release v0.6.0` (existing version section should pass) and mismatched version tag (should reject); test matching release fixture without modifying checkout docs.

### Task 4: Final provenance and commit

Files: `capi/README.md`, `CHANGELOG.md`, `docs/clean-room/LOG.md` and reviewed implementation above.

- [x] Record actual checks and limitations; native Linux runtime execution remains future GitHub work.
- [x] Commit reviewed implementation using `ci(capi): verify native SDK release matrix` and footer `Work item: C15`.
- [ ] Validate source-only and SDK-only `--require-clean` archive modes after implementation commit. No merge, push, release tag, publishing, or uploading.

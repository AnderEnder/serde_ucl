# C15 PR Rebase and CI Coordination Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebase PR #29 on current main and obtain passing GitHub CI.

**Architecture:** The spec coordinator resolves metadata/documentation integration and maintains provenance. A clean implementation participant derives its own repair plan, fixes native linkage validation, and validates the rebased implementation without oracle-source inputs.

**Tech Stack:** Git rebase, GitHub Actions/gh, Rust Cargo, C/C++, Python validation.

---

### Task 1: Preserve current main and released contract

**Files:** `CHANGELOG.md`, `docs/clean-room/LOG.md`, `docs/spec/README.md`.

- [x] Fetch main and inspect PR failure metadata.
- [x] Rebase C15 commits with `git rebase --onto origin/main spec-v21 c15/initial-c-api`; preserve the original local spec-v22 tag.
- [x] Resolve documentation by retaining current main and adding C15 entries. Verify `git diff --check` and check that released Stage A files remain byte-identical.

### Task 2: Clean implementation repair

**Files:** implementation-owned C validation tooling, its tests, and its provenance.

- [ ] Clean participant derives its own exact repair plan from the observed ANSI-contaminated linker flag failure; no coordinator implementation hints or oracle code are supplied.
- [ ] Participant fixes the native linker-output boundary and adds meaningful color-output regression coverage.
- [ ] Participant runs `CARGO_TERM_COLOR=always scripts/ci.sh`, workflow linting, and needed integration checks, then commits the repair and attestation.

### Task 3: Update PR and verify GitHub CI

**Files:** PR description and `docs/clean-room/LOG.md`.

- [ ] Push only the PR branch with an explicit expected remote SHA using `--force-with-lease`.
- [ ] Update the PR description to remove resolved integration blockers and report actual validation.
- [ ] Monitor current-head GitHub checks; delegate any implementation failures until checks pass.
- [ ] Report final commits and checks without merging or publishing tags/releases.

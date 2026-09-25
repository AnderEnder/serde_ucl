#!/bin/sh
# Runs locally what CI runs. The workflows in .github/workflows/ call this script and nothing
# else, so a green run here means a green run there, for the same toolchain.
#
#   scripts/ci.sh          the checks run on every push and pull request
#   scripts/ci.sh golden   the nightly drift check: builds libucl with scripts/regen-golden.sh,
#                          regenerates every golden file and fails if any of them changed
#
# The active toolchain is used. CI runs the checks with stable and with the crate's
# rust-version (Cargo.toml); to do the same locally:
#
#   RUSTUP_TOOLCHAIN=1.88 scripts/ci.sh
#
# The golden check needs git, cmake and a C compiler, and network access on its first run
# (scripts/regen-golden.sh clones libucl under target/).
set -eu

cd "$(dirname "$0")/.."

step() {
	printf '\n==> %s\n' "$*" >&2
}

run() {
	step "$*"
	"$@"
}

checks() {
	run cargo fmt --all --check

	run cargo clippy --all-targets --all-features -- -D warnings
	# The crate's dev-dependency on itself turns the features `fs` and `load` back on for every
	# target except the library, so builds with other feature sets are checked on the library.
	run cargo clippy --lib --no-default-features -- -D warnings
	# The remaining feature sets: `fs` alone (the default) and `load` alone.
	run cargo clippy --lib -- -D warnings
	run cargo clippy --lib --no-default-features --features load -- -D warnings

	run cargo test
	run cargo build --examples --benches
	# Every example checks its results with assertions.
	for example in examples/*.rs; do
		name=$(basename "$example" .rs)
		run cargo run --quiet --example "$name" >/dev/null
	done

	step "RUSTDOCFLAGS='-D warnings' cargo doc --no-deps"
	RUSTDOCFLAGS='-D warnings' cargo doc --no-deps
}

# The golden files: libucl's results in tests/conformance/ (scripts/regen-golden.sh) and
# libucl's readings of the serde corpus, tests/serde_corpus/*.golden.json (tests/serde_roundtrip.rs).
GOLDEN_DIRS="tests/conformance tests/serde_corpus"

golden_changes() {
	# shellcheck disable=SC2086
	git status --porcelain --untracked-files=all -- $GOLDEN_DIRS
}

golden() {
	dirty=$(golden_changes)
	if [ -n "$dirty" ]; then
		echo "error: commit or discard these changes first, so that every change the check" >&2
		echo "reports comes from the regeneration:" >&2
		echo "$dirty" >&2
		exit 1
	fi

	run scripts/regen-golden.sh
	# With the oracle binary built, this also has libucl read back generated serde output.
	step "UCL_SERDE_REGEN=1 cargo test --test serde_roundtrip"
	UCL_SERDE_REGEN=1 cargo test --test serde_roundtrip

	changed=$(golden_changes)
	if [ -n "$changed" ]; then
		echo "error: golden files differ from libucl's current results:" >&2
		echo "$changed" >&2
		# shellcheck disable=SC2086
		git --no-pager diff --stat -- $GOLDEN_DIRS >&2
		# shellcheck disable=SC2086
		git --no-pager diff -- $GOLDEN_DIRS >&2
		exit 1
	fi
	echo "golden files are current" >&2
}

case "${1:-checks}" in
checks) checks ;;
golden) golden ;;
*)
	echo "usage: scripts/ci.sh [checks|golden]" >&2
	exit 2
	;;
esac

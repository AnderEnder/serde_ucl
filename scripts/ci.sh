#!/bin/sh
# Runs locally what CI runs. The workflows in .github/workflows/ call this script and nothing
# else, so a green run here means a green run there, for the same toolchain.
#
#   scripts/ci.sh              the checks run on every push and pull request
#   scripts/ci.sh golden       the nightly drift check: builds libucl with scripts/regen-golden.sh,
#                              regenerates every golden file and fails if any of them changed
#   scripts/ci.sh pin COMMIT   what moving the libucl pin to COMMIT would change: regenerates
#                              every golden file with libucl at COMMIT and writes the changes
#                              for review to target/pin-move/ (see pin below); commits nothing
#   scripts/ci.sh fuzz [SECONDS [SEED]]
#                              the differential fuzzer (fuzz/README.md) for SECONDS (default
#                              600); fails if it finds a difference from libucl
#
# The active toolchain is used. The crate targets the latest stable Rust (1.98 at this release,
# rust-version in Cargo.toml), and CI runs the checks with stable only.
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

# The package in tests/features/ tests the crate with feature sets that the crate's own test
# builds cannot have: its dev-dependency on itself turns `load` on in every one of them. It is a
# workspace of its own, built in a target directory of its own.
FEATURES_MANIFEST=tests/features/Cargo.toml
FEATURES_TARGET=target/features
# The differential fuzzer in fuzz/ is a package and workspace of its own too. The checks build
# it and run its unit tests, which need no oracle; `fuzz` runs it.
FUZZ_MANIFEST=fuzz/Cargo.toml
FUZZ_TARGET=target/fuzz

checks() {
	run cargo fmt --all --check
	run cargo fmt --manifest-path "$FEATURES_MANIFEST" --all --check
	run cargo fmt --manifest-path "$FUZZ_MANIFEST" --all --check

	run cargo clippy --all-targets --all-features -- -D warnings
	# The crate's dev-dependency on itself turns the features `fs` and `load` back on for every
	# target except the library, so builds with other feature sets are checked on the library.
	run cargo clippy --lib --no-default-features -- -D warnings
	# The remaining feature sets: `fs` alone (the default) and `load` alone.
	run cargo clippy --lib -- -D warnings
	run cargo clippy --lib --no-default-features --features load -- -D warnings
	run cargo clippy --manifest-path "$FEATURES_MANIFEST" --target-dir "$FEATURES_TARGET" \
		--all-targets -- -D warnings
	run cargo clippy --manifest-path "$FUZZ_MANIFEST" --target-dir "$FUZZ_TARGET" \
		--all-targets -- -D warnings

	run cargo test
	# The tests of stack depth again, with the crate unoptimised as a debug build of an
	# application builds it: the test profile optimises it (Cargo.toml).
	run cargo test --lib --test stack_depth \
		--config 'profile.test.package.ucl-rust-lexer.opt-level=0'
	# The crate without `load`: with its default features, and with none.
	run cargo test --manifest-path "$FEATURES_MANIFEST" --target-dir "$FEATURES_TARGET"
	run cargo test --manifest-path "$FEATURES_MANIFEST" --target-dir "$FEATURES_TARGET" \
		--no-default-features
	run cargo test --manifest-path "$FUZZ_MANIFEST" --target-dir "$FUZZ_TARGET"

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

require_clean_golden() {
	dirty=$(golden_changes)
	if [ -n "$dirty" ]; then
		echo "error: commit or discard these changes first, so that every change the check" >&2
		echo "reports comes from the regeneration:" >&2
		echo "$dirty" >&2
		exit 1
	fi
}

golden() {
	require_clean_golden

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

# The pin-move check, `scripts/ci.sh pin COMMIT`: builds libucl at COMMIT with
# scripts/regen-golden.sh, regenerates every golden file as `golden` does, runs the conformance
# tests against the new files, and writes to $PIN_OUT:
#
#   summary.md        what changed and how the tests went (the workflow's job summary)
#   golden.patch      every change to the golden files, new and removed files included, as a
#                     binary patch for `git apply`
#   files.txt         the changed files, with A (added), M (modified) or D (deleted)
#   serde.log, conformance.log   the output of the two test runs
#
# Nothing is committed, and the working tree keeps the regenerated files, as after a `golden`
# run that found drift. To go back, discard them (git checkout and git clean of GOLDEN_DIRS) and
# run scripts/regen-golden.sh, which rebuilds the oracle at the pin.
PIN_OUT=target/pin-move
# The part of the diff that summary.md shows, in lines, and the bytes kept of each line.
PIN_SUMMARY_LINES=400
PIN_SUMMARY_WIDTH=300

pin() {
	commit=${1:-}
	if [ ${#commit} -ne 40 ] || [ -n "$(printf '%s' "$commit" | tr -d 0-9a-f)" ]; then
		echo "usage: scripts/ci.sh pin COMMIT, where COMMIT is a full 40-character libucl SHA" >&2
		exit 2
	fi
	# shellcheck disable=SC2016 # the `$` and braces are the script's text, not expansions
	pinned=$(sed -n 's/^LIBUCL_COMMIT=${LIBUCL_COMMIT:-\([0-9a-f]*\)}$/\1/p' scripts/regen-golden.sh)
	if [ ${#pinned} -ne 40 ]; then
		echo "error: cannot read the pinned commit from scripts/regen-golden.sh" >&2
		exit 1
	fi
	require_clean_golden
	rm -rf "$PIN_OUT"
	mkdir -p "$PIN_OUT"

	step "LIBUCL_COMMIT=$commit scripts/regen-golden.sh"
	LIBUCL_COMMIT=$commit scripts/regen-golden.sh

	# The script writes output golden files only for cases that parse, so a case that no longer
	# parses would keep those of the earlier commit, which the emitter runner does not allow.
	step "remove the output golden files of cases that libucl now rejects"
	find tests/conformance -type f -name '*.golden.json' | while IFS= read -r dump; do
		if [ "$(cat "$dump")" = '{"error":true}' ]; then
			stem=${dump%.golden.json}
			for format in config json json-compact yaml config-comments; do
				rm -f "$stem.$format.golden"
			done
		fi
	done

	serde=passed
	step "UCL_SERDE_REGEN=1 cargo test --test serde_roundtrip > $PIN_OUT/serde.log"
	UCL_SERDE_REGEN=1 cargo test --test serde_roundtrip >"$PIN_OUT/serde.log" 2>&1 || serde=failed
	conformance=passed
	step "cargo test --test conformance > $PIN_OUT/conformance.log"
	cargo test --test conformance -- --nocapture >"$PIN_OUT/conformance.log" 2>&1 ||
		conformance=failed

	# The changes, new files included, through an index of its own, so that the repository's
	# index stays as it is.
	step "write $PIN_OUT/golden.patch"
	index="$PWD/$PIN_OUT/index"
	GIT_INDEX_FILE=$index git read-tree HEAD
	# shellcheck disable=SC2086
	GIT_INDEX_FILE=$index git add -A -- $GOLDEN_DIRS
	for what in binary name-status stat text; do
		case $what in
		binary) options="--binary" out=golden.patch ;;
		name-status) options="--name-status" out=files.txt ;;
		stat) options="--stat=200,160" out=stat.txt ;;
		text) options="" out=text.diff ;;
		esac
		# shellcheck disable=SC2086
		GIT_INDEX_FILE=$index git --no-pager diff --cached --no-renames --no-ext-diff \
			--no-textconv $options -- $GOLDEN_DIRS >"$PIN_OUT/$out"
	done
	rm -f "$index"

	pin_summary "$pinned" "$commit" "$serde" "$conformance" >"$PIN_OUT/summary.md"
	rm -f "$PIN_OUT/stat.txt" "$PIN_OUT/text.diff"
	echo "golden files changed: $(wc -l <"$PIN_OUT/files.txt" | tr -d ' ');" \
		"serde corpus regeneration $serde; conformance tests $conformance" >&2
	echo "wrote $PIN_OUT/summary.md, $PIN_OUT/golden.patch and $PIN_OUT/files.txt" >&2
}

# Writes the Markdown summary of a pin run: pin_summary PINNED COMMIT SERDE CONFORMANCE.
pin_summary() {
	files=$(wc -l <"$PIN_OUT/files.txt" | tr -d ' ')
	added=$(grep -c '^A' "$PIN_OUT/files.txt" || true)
	modified=$(grep -c '^M' "$PIN_OUT/files.txt" || true)
	deleted=$(grep -c '^D' "$PIN_OUT/files.txt" || true)
	# A case's golden files, and a corpus file's reading, share the name without the suffix.
	cases=$(cut -f2 "$PIN_OUT/files.txt" |
		sed -e 's/\.golden\.json$//' -e 's/\.[a-z-]*\.golden$//' | sort -u | wc -l | tr -d ' ')
	fence='```'

	echo "# Golden files at libucl $2"
	echo
	echo "\`scripts/ci.sh pin\` built libucl at \`$2\` with \`scripts/regen-golden.sh\` and"
	if [ "$1" = "$2" ]; then
		echo "regenerated every golden file. That is the pinned commit. Nothing was committed."
	else
		echo "regenerated every golden file. The pinned commit is \`$1\`. Nothing was committed."
	fi
	echo
	echo "| | |"
	echo "| --- | --- |"
	echo "| Golden files changed | $files ($added added, $modified modified, $deleted deleted) |"
	echo "| Cases and serde corpus files affected | $cases |"
	echo "| \`cargo test --test conformance\` with the new files | $4 |"
	echo "| \`UCL_SERDE_REGEN=1 cargo test --test serde_roundtrip\` | $3 |"
	echo
	echo "The artifact holds \`golden.patch\` (every change, binary files included; apply it with"
	echo "\`git apply\`), \`files.txt\`, \`serde.log\` and \`conformance.log\`. Moving the pin means"
	echo "setting \`LIBUCL_COMMIT\` in \`scripts/regen-golden.sh\` to \`$2\` and applying the patch."
	echo
	echo "## Conformance tests"
	echo
	echo "$fence"
	grep -E '^conformance \(|^test .* FAILED$' "$PIN_OUT/conformance.log" ||
		echo "(no report line; see conformance.log)"
	# Each failure's message: from the line that says where a test panicked to the next line
	# that is empty, a test result or the runtime's note.
	awk '/ panicked at / { show = 1 } /^(note:|stack backtrace:|test |$)/ { show = 0 } show' \
		"$PIN_OUT/conformance.log" | head -n 100 | LC_ALL=C cut -c "1-$PIN_SUMMARY_WIDTH"
	echo "$fence"
	if [ "$files" -eq 0 ]; then
		echo
		echo "No golden file changed."
		return
	fi
	echo
	echo "## Changed files"
	echo
	echo "$fence"
	head -n 300 "$PIN_OUT/stat.txt"
	echo "$fence"
	echo
	echo "## Diff"
	echo
	echo "The first $PIN_SUMMARY_LINES lines, each cut to $PIN_SUMMARY_WIDTH bytes; binary files"
	echo "are in \`golden.patch\` only."
	echo
	echo "${fence}diff"
	head -n "$PIN_SUMMARY_LINES" "$PIN_OUT/text.diff" | LC_ALL=C cut -c "1-$PIN_SUMMARY_WIDTH"
	echo "$fence"
}

# The differential fuzzer, `scripts/ci.sh fuzz [SECONDS [SEED]]`: builds it, builds the oracle
# with scripts/regen-golden.sh if target/libucl-oracle/ucl-dump is missing, and runs it for
# SECONDS (default 600), with SEED if given. Its findings and summary.txt go to
# target/fuzz-differential/; it fails if it saved any finding.
fuzz() {
	seconds=${1:-600}
	if [ -n "${2:-}" ]; then
		set -- --seed "$2"
	else
		set --
	fi
	if [ ! -x target/libucl-oracle/ucl-dump ]; then
		run scripts/regen-golden.sh
	fi
	run cargo build --release --manifest-path "$FUZZ_MANIFEST" --target-dir "$FUZZ_TARGET"
	rm -rf target/fuzz-differential
	run "$FUZZ_TARGET/release/ucl-differential" --seconds "$seconds" "$@"
}

case "${1:-checks}" in
checks) checks ;;
golden) golden ;;
pin)
	shift
	pin "$@"
	;;
fuzz)
	shift
	fuzz "$@"
	;;
*)
	echo "usage: scripts/ci.sh [checks|golden|pin COMMIT|fuzz [SECONDS [SEED]]]" >&2
	exit 2
	;;
esac

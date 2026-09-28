#!/bin/sh
# Compares serde_ucl with libucl and serde_json on the benchmarks' documents and the rspamd corpus,
# for the README's section *Against libucl and serde_json* (tools/bench-compare/README.md).
#
# Builds libucl at the oracle's pinned commit as a CMake Release build (-O3), tools/bench-compare/
# with the crate's release settings, and the C program tools/bench-compare/lucl.c; writes the
# generated documents; runs the two programs in turns for ROUNDS rounds, the order rotated each
# round; prints the medians over the rounds as tables. Every time includes freeing the result,
# except in the table that says it does not. Run it on a quiet machine.
#
# Usage: scripts/bench-compare.sh [ROUNDS]   (default 3)
#
# Environment:
#   LIBUCL_COMMIT  libucl commit (default: the pin of scripts/regen-golden.sh)
#   LIBUCL_DIR     existing libucl checkout at that commit (default: target/libucl-oracle/libucl,
#                  cloned if missing)
#   CC             C compiler (default: cc)
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
ROUNDS=${1:-3}
PIN=$(sed -n 's/^LIBUCL_COMMIT=\${LIBUCL_COMMIT:-\([0-9a-f]\{40\}\)}$/\1/p' "$ROOT/scripts/regen-golden.sh")
LIBUCL_COMMIT=${LIBUCL_COMMIT:-$PIN}
LIBUCL_URL=https://github.com/vstakhov/libucl.git
LIBUCL_DIR=${LIBUCL_DIR:-"$ROOT/target/libucl-oracle/libucl"}
CC=${CC:-cc}
WORK="$ROOT/target/bench-compare"
DOCS="$WORK/documents"
CORPUS="$ROOT/benches/corpus/rspamd"

case "$ROUNDS" in
'' | *[!0-9]* | 0) echo "usage: scripts/bench-compare.sh [ROUNDS]" >&2; exit 2 ;;
esac
if [ ${#LIBUCL_COMMIT} -ne 40 ]; then
	echo "error: LIBUCL_COMMIT must be a full 40-character commit SHA" >&2
	exit 1
fi
mkdir -p "$WORK"

if [ ! -d "$LIBUCL_DIR/.git" ]; then
	echo "cloning libucl into $LIBUCL_DIR" >&2
	git clone --quiet "$LIBUCL_URL" "$LIBUCL_DIR"
	git -C "$LIBUCL_DIR" checkout --quiet "$LIBUCL_COMMIT"
fi
actual=$(git -C "$LIBUCL_DIR" rev-parse HEAD)
if [ "$actual" != "$LIBUCL_COMMIT" ]; then
	echo "error: $LIBUCL_DIR is at $actual, expected $LIBUCL_COMMIT" >&2
	exit 1
fi

# A build configured from another checkout cannot be reused: CMake refuses a second source.
cache="$WORK/libucl-build/CMakeCache.txt"
if [ -f "$cache" ] &&
	[ "$(sed -n 's/^CMAKE_HOME_DIRECTORY:INTERNAL=//p' "$cache")" != "$(cd "$LIBUCL_DIR" && pwd -P)" ]; then
	rm -rf "$WORK/libucl-build"
fi
echo "building libucl (Release) in $WORK/libucl-build" >&2
if ! {
	cmake -S "$LIBUCL_DIR" -B "$WORK/libucl-build" -DCMAKE_BUILD_TYPE=Release \
		-DBUILD_SHARED_LIBS=OFF -DENABLE_URL_INCLUDE=OFF -DENABLE_URL_SIGN=OFF \
		-DENABLE_LUA=OFF -DENABLE_UTILS=OFF &&
		cmake --build "$WORK/libucl-build" --target ucl -j 8
} >"$WORK/libucl-build.log" 2>&1; then
	cat "$WORK/libucl-build.log" >&2
	echo "error: libucl build failed" >&2
	exit 1
fi
"$CC" -std=c99 -O3 -Wall -I"$LIBUCL_DIR/include" "$ROOT/tools/bench-compare/lucl.c" \
	"$WORK/libucl-build/libucl.a" -lm -o "$WORK/lucl"

echo "building tools/bench-compare" >&2
cargo build --quiet --release --manifest-path "$ROOT/tools/bench-compare/Cargo.toml" \
	--target-dir "$WORK/cargo"
TOOL="$WORK/cargo/release/bench-compare"

"$TOOL" write "$DOCS"
RESULTS="$WORK/results.txt"
: >"$RESULTS"

run_rust() {
	"$TOOL" run "$DOCS" "$CORPUS"
}
run_libucl() {
	"$WORK/lucl" "$DOCS/json-10000.json" "$DOCS/json-1000.json" "$DOCS/config-1000.ucl" \
		"$DOCS/small.ucl" "$DOCS/small.json"
	"$WORK/lucl" -v "CONFDIR=$(cd "$CORPUS" && pwd)" "$CORPUS/groups.conf" \
		"$CORPUS/composites.conf" "$CORPUS/scores.d/rbl_group.conf"
}

# The one-minute load average, printed with each run: other work on the machine makes rounds
# disagree, so a high load means the results should be run again.
load() {
	if [ -r /proc/loadavg ]; then cut -d' ' -f1 /proc/loadavg; else sysctl -n vm.loadavg | awk '{print $2}'; fi
}

round=1
while [ "$round" -le "$ROUNDS" ]; do
	if [ $((round % 2)) -eq 1 ]; then order="rust libucl"; else order="libucl rust"; fi
	for tool in $order; do
		echo "round $round of $ROUNDS: $tool (load $(load))" >&2
		"run_$tool" | sed "s/^/$round|/" >>"$RESULTS"
	done
	round=$((round + 1))
done

echo "results: $RESULTS" >&2
"$TOOL" summarize "$RESULTS"

#!/bin/sh
# Regenerates every golden file under tests/conformance/ from libucl: the typed dump
# <case>.golden.json and, for every case that parses, the output files <case>.<format>.golden.
#
# The expected results of the conformance suite come only from here. The script
# clones libucl at the pinned commit (or reuses LIBUCL_DIR), builds the static
# library and tools/ucl-dump, then dumps every case. The golden files are
# committed, so running the tests needs no C toolchain.
#
# Environment:
#   LIBUCL_DIR        existing libucl checkout to use (must be at LIBUCL_COMMIT)
#   LIBUCL_BUILD_DIR  where to build libucl (default: target/libucl-oracle/build)
#   CC                C compiler (default: cc)
set -eu

LIBUCL_COMMIT=24c8b399062ae4691168c243e3b7345ef7f31956
LIBUCL_URL=https://github.com/vstakhov/libucl.git

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CONF="$ROOT/tests/conformance"
WORK="$ROOT/target/libucl-oracle"
LIBUCL_DIR=${LIBUCL_DIR:-"$WORK/libucl"}
LIBUCL_BUILD_DIR=${LIBUCL_BUILD_DIR:-"$WORK/build"}
CC=${CC:-cc}
DUMP="$WORK/ucl-dump"

mkdir -p "$WORK"

if [ ! -d "$LIBUCL_DIR/.git" ]; then
	echo "cloning libucl into $LIBUCL_DIR" >&2
	git clone --quiet "$LIBUCL_URL" "$LIBUCL_DIR"
fi
actual=$(git -C "$LIBUCL_DIR" rev-parse HEAD)
if [ "$actual" != "$LIBUCL_COMMIT" ]; then
	if [ "$LIBUCL_DIR" = "$WORK/libucl" ]; then
		git -C "$LIBUCL_DIR" fetch --quiet origin
		git -C "$LIBUCL_DIR" checkout --quiet "$LIBUCL_COMMIT"
	else
		echo "error: $LIBUCL_DIR is at $actual, expected $LIBUCL_COMMIT" >&2
		exit 1
	fi
fi

echo "building libucl in $LIBUCL_BUILD_DIR" >&2
LOG="$WORK/build.log"
if ! {
	cmake -S "$LIBUCL_DIR" -B "$LIBUCL_BUILD_DIR" -DCMAKE_BUILD_TYPE=Release \
		-DBUILD_SHARED_LIBS=OFF -DENABLE_URL_INCLUDE=OFF -DENABLE_URL_SIGN=OFF \
		-DENABLE_LUA=OFF -DENABLE_UTILS=OFF &&
		cmake --build "$LIBUCL_BUILD_DIR" --target ucl -j 8
} >"$LOG" 2>&1; then
	cat "$LOG" >&2
	echo "error: libucl build failed (log: $LOG)" >&2
	exit 1
fi
"$CC" -std=c99 -O1 -Wall -I"$LIBUCL_DIR/include" "$ROOT/tools/ucl-dump/ucl_dump.c" \
	"$LIBUCL_BUILD_DIR/libucl.a" -lm -o "$DUMP"

# Maps a <case>.flags file (one flag name per line, '#' comments) to dumper options.
flag_opts() {
	opts=""
	if [ -f "$1" ]; then
		while IFS= read -r line || [ -n "$line" ]; do
			line=$(printf '%s' "$line" | sed 's/#.*//; s/[[:space:]]//g')
			case "$line" in
			"") ;;
			key-lowercase) opts="$opts -l" ;;
			zerocopy) opts="$opts -z" ;;
			no-time) opts="$opts -T" ;;
			no-implicit-arrays) opts="$opts -I" ;;
			save-comments) opts="$opts -C" ;;
			dump-comments) opts="$opts -c" ;;
			disable-macro) opts="$opts -M" ;;
			no-filevars) opts="$opts -F" ;;
			variable-handler) opts="$opts -H" ;;
			string-input) opts="$opts -S" ;;
			var:*=*) opts="$opts -v ${line#var:}" ;;
			priority:*) opts="$opts -p ${line#priority:}" ;;
			strategy:*) opts="$opts -s ${line#strategy:}" ;;
			*) echo "error: unknown flag '$line' in $1" >&2; exit 1 ;;
			esac
		done < "$1"
	fi
	printf '%s' "$opts"
}

count=0
list=$(
	{
		find "$CONF/libucl/basic" -maxdepth 1 -type f -name '*.in'
		if [ -d "$CONF/cases" ]; then
			find "$CONF/cases" -type f -name '*.ucl'
		fi
	} | LC_ALL=C sort
)
# Case paths contain no whitespace (checked below), so word splitting is safe here.
for case in $list; do
	case "$case" in
	*[[:space:]]*) echo "error: whitespace in case path: $case" >&2; exit 1 ;;
	esac
	dir=$(dirname "$case")
	base=$(basename "$case")
	stem=${base%.*}
	opts=$(flag_opts "$dir/$stem.flags")
	# Run from the case's directory, so relative include paths resolve the same on every machine.
	# shellcheck disable=SC2086
	(cd "$dir" && "$DUMP" $opts "$base" 2>/dev/null) > "$dir/$stem.golden.json"
	count=$((count + 1))
	# Every case that parses also gets libucl's own output in every text format, and cases that
	# save comments get the config output with those comments (spec section 10).
	if [ "$(cat "$dir/$stem.golden.json")" != '{"error":true}' ]; then
		fmts="config json json-compact yaml"
		case " $opts " in
		*" -C "* | *" -c "*) fmts="$fmts config-comments" ;;
		esac
		for fmt in $fmts; do
			# shellcheck disable=SC2086
			(cd "$dir" && "$DUMP" $opts -e "$fmt" "$base" 2>/dev/null) > "$dir/$stem.$fmt.golden"
			count=$((count + 1))
		done
	fi
done

# Golden files must not depend on where the repository is checked out.
if grep -rl --include='*.golden.json' --include='*.golden' -F "$ROOT" "$CONF" >/dev/null 2>&1; then
	echo "error: golden files contain the checkout path $ROOT:" >&2
	grep -rl --include='*.golden.json' --include='*.golden' -F "$ROOT" "$CONF" >&2
	exit 1
fi

echo "wrote $count golden files" >&2

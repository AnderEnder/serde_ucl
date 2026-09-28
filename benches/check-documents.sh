#!/bin/sh
# Checks every benchmark document against libucl the way the differential fuzzer checks one
# (`ucl-differential --check`, fuzz/README.md): libucl parses it, and the crate gives the same
# result. The documents are the irregular configurations of benches/common/irregular.rs, the
# JSON documents in target/bench-corpus/ (benches/fetch-documents.sh), and the configurations in
# benches/corpus/, or in the directory UCL_BENCH_CORPUS names.
#
#   benches/check-documents.sh [EXTRA_SEEDS]
#
# EXTRA_SEEDS (default 0) also checks that many more generated documents of 60 000 bytes, to
# test the generator beyond the documents the benchmarks use. A document passes when the
# verdict is `agree` and libucl accepted it; `skipped` and a rejection by both fail too. Each
# result is in target/bench-documents/checks/. Needs the oracle,
# target/libucl-oracle/ucl-dump (scripts/regen-golden.sh builds it).
set -eu

cd "$(dirname "$0")/.."

ORACLE=target/libucl-oracle/ucl-dump
OUT=target/bench-documents
FUZZER=target/fuzz/release/ucl-differential
EXTRA_SEEDS=${1:-0}
# The corpus directory, as in the benchmarks.
CORPUS=${UCL_BENCH_CORPUS:-benches/corpus}

if [ ! -x "$ORACLE" ]; then
	echo "error: $ORACLE is missing; scripts/regen-golden.sh builds it" >&2
	exit 2
fi

cargo build --quiet --release --manifest-path fuzz/Cargo.toml --target-dir target/fuzz
rm -rf "$OUT"
mkdir -p "$OUT/generated" "$OUT/checks"
UCL_BENCH_DOCUMENTS_OUT="$PWD/$OUT/generated" UCL_BENCH_EXTRA_SEEDS="$EXTRA_SEEDS" \
	cargo test --quiet --test bench_documents -- --exact write_documents >/dev/null

checked=0
failed=0

# check FILE DIR: runs FILE through both, with DIR as the working directory.
check() {
	report="$OUT/checks/$(basename "$1").txt"
	"$FUZZER" --check "$1" --dir "$2" --oracle "$ORACLE" --out "$OUT/fuzz" >"$report" 2>&1 || true
	verdict=$(tail -n 1 "$report" | cut -c 1-200)
	oracle=$(head -n 1 "$report" | cut -c 1-13)
	checked=$((checked + 1))
	if [ "$verdict" = agree ] && [ "$oracle" != "oracle: error" ]; then
		echo "agree: $1"
	else
		echo "FAILED: $1: $oracle ... $verdict (see $report)"
		failed=$((failed + 1))
	fi
}

for file in "$OUT"/generated/*.ucl; do
	check "$file" "$OUT/generated"
done

set -- target/bench-corpus/*.json
if [ -e "$1" ]; then
	for file in "$@"; do
		check "$file" target/bench-corpus
	done
else
	echo "no JSON documents in target/bench-corpus/; benches/fetch-documents.sh fetches them"
fi

# The corpus, without the metadata files that benches/common/files.rs leaves out.
for file in "$CORPUS"/*; do
	[ -f "$file" ] || continue
	name=$(basename "$file")
	case $(printf '%s' "$name" | tr '[:lower:]' '[:upper:]') in
	.* | *.MD | *.TXT | LICENSE* | LICENCE* | NOTICE* | COPYING*) continue ;;
	esac
	check "$file" "$CORPUS"
done

echo "$checked documents checked, $failed failed"
[ "$failed" -eq 0 ]

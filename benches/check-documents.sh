#!/bin/sh
# Checks every benchmark document against libucl the way the differential fuzzer checks one
# (`ucl-differential --check`, fuzz/README.md): libucl parses it, and the crate gives the same
# result. The documents are the irregular configurations of benches/common/irregular.rs, the
# workloads of benches/common/workloads.rs (those parsed with save-comments with their saved
# comments compared too, the flag dump-comments; those parsed with key-lowercase with that
# flag), the JSON documents in target/bench-corpus/
# (benches/fetch-documents.sh) and their compact forms, and the rspamd configurations of
# benches/corpus/ that the benchmarks use, with the settings they use (benches/common/files.rs,
# CORPUS).
#
#   benches/check-documents.sh [EXTRA_SEEDS]
#
# EXTRA_SEEDS (default 0) also checks that many more generated documents of 60 000 bytes, to
# test the generator beyond the documents the benchmarks use. A document passes when the
# verdict is `agree` and libucl accepted it; `skipped` and a rejection by both fail too, with
# one exception: a document nested deeper than the check's JSON reader reads, whose dump libucl
# wrote but the check cannot compare, passes as `accepted`. That verdict does not look at the
# crate's result: tests/bench_documents.rs checks that the crate parses those documents to the
# depth they have. The deep workloads are such documents, so the same documents nested 20 deep
# are compared in their place. Each result is
# in target/bench-documents/checks/. Needs the oracle, target/libucl-oracle/ucl-dump
# (scripts/regen-golden.sh builds it).
set -eu

cd "$(dirname "$0")/.."

ORACLE=target/libucl-oracle/ucl-dump
OUT=target/bench-documents
FUZZER=target/fuzz/release/ucl-differential
EXTRA_SEEDS=${1:-0}

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

# check FILE DIR [FLAG...]: runs FILE through both, with DIR as the working directory and the
# fuzzer's flags FLAG (such as var:NAME=VALUE).
check() {
	file=$1
	dir=$2
	shift 2
	report="$OUT/checks/$(basename "$file").txt"
	for flag in "$@"; do
		set -- "$@" --flag "$flag"
		shift
	done
	"$FUZZER" --check "$file" --dir "$dir" "$@" --oracle "$ORACLE" --out "$OUT/fuzz" \
		>"$report" 2>&1 || true
	verdict=$(tail -n 1 "$report" | cut -c 1-200)
	oracle=$(head -n 1 "$report" | cut -c 1-13)
	checked=$((checked + 1))
	if [ "$verdict" = agree ] && [ "$oracle" != "oracle: error" ]; then
		echo "agree: $file"
	elif [ "$verdict" = "skipped: dump deeper than serde_json reads" ]; then
		echo "accepted (too deep to compare): $file"
	else
		echo "FAILED: $file: $oracle ... $verdict (see $report)"
		failed=$((failed + 1))
	fi
}

for file in "$OUT"/generated/*.ucl; do
	check "$file" "$OUT/generated"
done
# Workloads parsed with a flag, in a directory named after the check's flag.
for flag in dump-comments key-lowercase; do
	for file in "$OUT/generated/$flag"/*.ucl; do
		check "$file" "$OUT/generated" "$flag"
	done
done

set -- target/bench-corpus/*.json
if [ -e "$1" ]; then
	for file in "$@"; do
		check "$file" target/bench-corpus
	done
	# Their compact forms, which write_documents writes when the documents are there.
	for file in "$OUT"/generated/*.json; do
		check "$file" "$OUT/generated"
	done
else
	echo "no JSON documents in target/bench-corpus/; benches/fetch-documents.sh fetches them"
fi

# The corpus documents of benches/common/files.rs (CORPUS), with its directories and variables
# (benches/corpus/README.md).
check benches/corpus/rspamd/groups.conf benches/corpus/rspamd var:CONFDIR=.
check benches/corpus/rspamd/composites.conf benches/corpus/rspamd
check benches/corpus/rspamd/scores.d/rbl_group.conf benches/corpus/rspamd

echo "$checked documents checked, $failed failed"
[ "$failed" -eq 0 ]

#!/bin/sh
# Fetches the JSON documents that serde_json and simd-json publish figures for (twitter.json,
# citm_catalog.json, canada.json) into target/bench-corpus/, for the benchmark groups over them
# (benches/README.md). They come from serde-rs/json-benchmark at a pinned commit and are checked
# against their SHA-256 digests. They are not committed, because their licences are unclear.
#
#   benches/fetch-documents.sh
#
# A file that is already there with the right digest is kept. A download goes to a .part file
# first and replaces the document only when its digest matches, so the benchmarks never read a
# partial or different file. Needs curl, and sha256sum or shasum.
set -eu

cd "$(dirname "$0")/.."

COMMIT=17b13dd2d7a5e5fdd5594e847077932f955b5e2b
BASE=https://raw.githubusercontent.com/serde-rs/json-benchmark/$COMMIT/data
DEST=target/bench-corpus

sha256() {
	if command -v sha256sum >/dev/null 2>&1; then
		sha256sum "$1" | cut -d ' ' -f 1
	else
		shasum -a 256 "$1" | cut -d ' ' -f 1
	fi
}

failed=0

fetch() {
	name=$1
	digest=$2
	file=$DEST/$name
	if [ -f "$file" ] && [ "$(sha256 "$file")" = "$digest" ]; then
		echo "$file: present"
		return
	fi
	echo "$file: fetching $BASE/$name"
	if ! curl --fail --silent --show-error --location --proto '=https' --retry 2 \
		--output "$file.part" "$BASE/$name"; then
		rm -f "$file.part"
		echo "error: cannot fetch $BASE/$name" >&2
		failed=1
		return
	fi
	actual=$(sha256 "$file.part")
	if [ "$actual" != "$digest" ]; then
		rm -f "$file.part"
		echo "error: $name has SHA-256 $actual, expected $digest" >&2
		failed=1
		return
	fi
	mv "$file.part" "$file"
}

mkdir -p "$DEST"
fetch twitter.json a08b769f32b95f426cbc3abafcec65c1a19d3eb544d4ddf320eae142c99efc5d
fetch citm_catalog.json a73e7a883f6ea8de113dff59702975e60119b4b58d451d518a929f31c92e2059
fetch canada.json f83b3b354030d5dd58740c68ac4fecef64cb730a0d12a90362a7f23077f50d78
exit "$failed"

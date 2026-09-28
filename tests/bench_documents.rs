//! The irregular documents of the benchmarks (`benches/common/irregular.rs`, clean-room work
//! item C14): the same seed gives the same document, each one parses, and each one has the
//! variety it is meant to have.
//!
//! Each document the benchmarks use was checked against libucl with
//! `benches/check-documents.sh`, which writes them out through `write_documents` below. The
//! digests in `DIGESTS` pin those documents: a change to the generator changes them, and then
//! the documents are checked again and the digests updated.

#[path = "../benches/common/mod.rs"]
mod common;

use common::{IRREGULAR, irregular};
use serde_ucl::value::Value;

/// FNV-1a digests of the documents of `IRREGULAR`, in its order.
const DIGESTS: [u64; 2] = [8066268227795855388, 5386605887859985361];

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[test]
fn the_same_seed_gives_the_same_document() {
    for (name, seed, size) in IRREGULAR {
        let document = irregular(seed, size);
        assert_eq!(document, irregular(seed, size), "{name}");
        assert_ne!(document, irregular(seed + 1, size), "{name}");
    }
}

#[test]
fn the_documents_are_the_checked_ones() {
    let digests: Vec<u64> = IRREGULAR
        .iter()
        .map(|&(_, seed, size)| fnv1a(irregular(seed, size).as_bytes()))
        .collect();
    assert_eq!(
        digests, DIGESTS,
        "the generated documents changed: check them with benches/check-documents.sh, then \
         update DIGESTS"
    );
}

/// What a document holds, to show that it varies.
#[derive(Debug, Default)]
struct Stats {
    objects: usize,
    /// Objects by number of keys: 0–3, 4–15, 16–40, more.
    object_sizes: [usize; 4],
    widest_object: usize,
    arrays: usize,
    implicit_arrays: usize,
    depth: usize,
    strings: usize,
    non_ascii_strings: usize,
    strings_of_1k: usize,
    longest_string: usize,
    integers: usize,
    floats: usize,
    times: usize,
    booleans: usize,
    nulls: usize,
}

impl Stats {
    fn of(value: &Value<'_>) -> Self {
        let mut stats = Stats::default();
        let mut stack = vec![(value, 1)];
        while let Some((value, depth)) = stack.pop() {
            stats.depth = stats.depth.max(depth);
            match value {
                Value::Object(object) => {
                    stats.objects += 1;
                    let n = object.len();
                    stats.object_sizes[match n {
                        0..=3 => 0,
                        4..=15 => 1,
                        16..=40 => 2,
                        _ => 3,
                    }] += 1;
                    stats.widest_object = stats.widest_object.max(n);
                    for (_, entry) in object {
                        if entry.is_multi() {
                            stats.implicit_arrays += 1;
                        }
                        stack.extend(entry.values().map(|v| (v, depth + 1)));
                    }
                }
                Value::Array(array) => {
                    stats.arrays += 1;
                    stack.extend(array.into_iter().map(|v| (v, depth + 1)));
                }
                Value::String(s) => {
                    let s: &str = s;
                    stats.strings += 1;
                    stats.non_ascii_strings += usize::from(!s.is_ascii());
                    stats.strings_of_1k += usize::from(s.len() >= 1000);
                    stats.longest_string = stats.longest_string.max(s.len());
                }
                Value::Integer(_) => stats.integers += 1,
                Value::Float(_) => stats.floats += 1,
                Value::Time(_) => stats.times += 1,
                Value::Boolean(_) => stats.booleans += 1,
                Value::Null => stats.nulls += 1,
            }
        }
        stats
    }
}

#[test]
fn each_document_parses_and_varies() {
    for (name, seed, size) in IRREGULAR {
        let document = irregular(seed, size);
        let value = serde_ucl::parse::parse(document.as_bytes())
            .unwrap_or_else(|e| panic!("irregular {name}: {e}"));
        let stats = Stats::of(&value);
        println!(
            "irregular {name} (seed {seed}): {} bytes, {stats:?}",
            document.len()
        );
        assert!(document.len() >= size, "{name}");
        assert!(stats.depth >= 6, "{name}: {stats:?}");
        assert!(
            stats.object_sizes.iter().all(|&n| n > 0),
            "{name}: {stats:?}"
        );
        assert!(stats.implicit_arrays > 0, "{name}: {stats:?}");
        assert!(stats.non_ascii_strings > 0, "{name}: {stats:?}");
        assert!(stats.strings_of_1k > 0, "{name}: {stats:?}");
        assert!(stats.longest_string >= 2000, "{name}: {stats:?}");
        for count in [
            stats.integers,
            stats.floats,
            stats.times,
            stats.booleans,
            stats.nulls,
            stats.arrays,
        ] {
            assert!(count > 0, "{name}: {stats:?}");
        }
        // The syntax forms that the values do not show.
        for form in ["<<", "'", "\\u", "\\n", "/*", "# ", ": ", "\": {"] {
            assert!(document.contains(form), "{name}: no {form:?}");
        }
    }
}

/// Writes the documents to the directory in `UCL_BENCH_DOCUMENTS_OUT`, for
/// `benches/check-documents.sh`; does nothing without it. With `UCL_BENCH_EXTRA_SEEDS=N`, it
/// also writes documents of 60 000 bytes for the seeds 1000 to 1000 + N - 1, which test the
/// generator beyond the documents the benchmarks use.
#[test]
fn write_documents() {
    let Some(dir) = std::env::var_os("UCL_BENCH_DOCUMENTS_OUT") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (name, seed, size) in IRREGULAR {
        std::fs::write(
            dir.join(format!("irregular-{name}.ucl")),
            irregular(seed, size),
        )
        .unwrap();
    }
    let extra: u64 = std::env::var("UCL_BENCH_EXTRA_SEEDS")
        .map(|n| n.parse().expect("UCL_BENCH_EXTRA_SEEDS is a number"))
        .unwrap_or(0);
    for seed in 1000..1000 + extra {
        std::fs::write(
            dir.join(format!("irregular-seed-{seed}.ucl")),
            irregular(seed, 60_000),
        )
        .unwrap();
    }
}

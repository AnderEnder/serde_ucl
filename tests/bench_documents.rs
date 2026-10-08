//! The generated documents of the benchmarks: the irregular documents
//! (`benches/common/irregular.rs`, clean-room work item C14), where the same seed gives the same
//! document and each one has the variety it is meant to have, and the controlled workloads
//! (`benches/common/workloads.rs`, C16), each of which holds the values it is meant to hold.
//! Also the compact forms of the JSON documents, and their typed forms.
//!
//! Each document the benchmarks use was checked against libucl with
//! `benches/check-documents.sh`, which writes them out through `write_documents` below. The
//! digests in `DIGESTS` and `WORKLOAD_DIGESTS` pin those documents: a change to a generator
//! changes them, and then the documents are checked again and the digests updated.

#[path = "../benches/common/mod.rs"]
mod common;

use common::{IRREGULAR, irregular};
use serde::de::DeserializeOwned;
use serde_ucl::ParserFlags;
use serde_ucl::parse::Parser;
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

/// The corpus documents parse with the settings of the benchmarks, which are those of their
/// check, and `groups.conf` finds its 14 includes: 21 groups with 253 symbols, as
/// `benches/corpus/README.md` says, from 55 215 bytes.
#[test]
fn the_corpus_documents_parse_as_checked() {
    let documents = common::corpus_documents("test");
    assert_eq!(documents.len(), common::CORPUS.len());
    for document in &documents {
        let value = document
            .parser()
            .parse(document.text.as_bytes())
            .unwrap_or_else(|e| panic!("{}: {e}", document.path.display()));
        if document.document.name != "rspamd-groups" {
            continue;
        }
        assert_eq!(document.bytes_read, 55_215);
        let groups = value.as_object().unwrap().entry("group").unwrap();
        let mut symbols = 0;
        for group in groups.values() {
            for (_, entry) in group.as_object().unwrap() {
                let group = entry.first().as_object().unwrap();
                symbols += group
                    .get("symbols")
                    .map_or(0, |s| s.as_object().unwrap().len());
            }
        }
        assert_eq!((groups.len(), symbols), (21, 253));
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
    // The workloads, once for each text; those parsed with a flag again in a directory named
    // after the check's flag, which `benches/check-documents.sh` passes: `dump-comments` (saved
    // comments compared too) for `save-comments`, and `key-lowercase`.
    let mut written = std::collections::HashSet::new();
    for workload in common::workloads() {
        let name = format!("{}.ucl", workload.id().replace('/', "-"));
        let flag = if workload.flags.is_empty() {
            None
        } else if workload.flags == ParserFlags::SAVE_COMMENTS {
            Some("dump-comments")
        } else if workload.flags == ParserFlags::KEY_LOWERCASE {
            Some("key-lowercase")
        } else {
            panic!("{name}: no check flag for {:?}", workload.flags)
        };
        if let Some(flag) = flag {
            std::fs::create_dir_all(dir.join(flag)).unwrap();
            std::fs::write(dir.join(flag).join(&name), &workload.text).unwrap();
        } else if written.insert(fnv1a(workload.text.as_bytes())) {
            std::fs::write(dir.join(&name), &workload.text).unwrap();
        }
    }
    // The deep chains at a depth whose dump the check can read, in place of those of the
    // benchmarks, which it can only see libucl accept.
    for workload in common::deep_chains(20) {
        let name = format!("{}.ucl", workload.id().replace('/', "-"));
        std::fs::write(dir.join(&name), &workload.text).unwrap();
    }
    std::fs::write(
        dir.join("parse-json-compact-json-1000.ucl"),
        common::compact_json(&common::json(1000)),
    )
    .unwrap();
    for document in common::json_documents("write_documents") {
        std::fs::write(
            dir.join(format!("parse-json-compact-{}.json", document.name)),
            common::compact_json(&document.text),
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

/// FNV-1a digests of the workloads of `common::workloads()`, by id.
const WORKLOAD_DIGESTS: &[(&str, u64)] = &[
    ("parse/strings/dq-1", 3397109732585326882),
    ("parse/strings/dq-8", 14795648265023298215),
    ("parse/strings/dq-16", 17328732671575075726),
    ("parse/strings/dq-24", 9625584532266748356),
    ("parse/strings/dq-32", 8522622951650077243),
    ("parse/strings/dq-64", 13512022318852155491),
    ("parse/strings/dq-256", 2981947958648486740),
    ("parse/strings/dq-4096", 8548395501389726538),
    ("parse/strings/dq-escaped-64", 9714396491685004314),
    ("parse/strings/dq-utf8-64", 9497408090071248287),
    ("parse/strings/sq-64", 9505414775039453099),
    ("parse/strings/sq-4096", 14635762553845053700),
    ("parse/strings/unquoted-64", 15870103482988444947),
    ("parse/strings/heredoc-1024", 10912057044031521945),
    ("parse/strings/dq-escaped-dense-64", 14151293141455840887),
    ("parse/keys/bare-4", 7304717182505186565),
    ("parse/keys/bare-22", 1132878005403401589),
    ("parse/keys/bare-23", 15831160380990138591),
    ("parse/keys/bare-64", 4686489160442266688),
    ("parse/keys/quoted-23", 4286967142565416537),
    ("parse/numbers/int-1", 2260337187800645672),
    ("parse/numbers/int-4", 6589930021543519760),
    ("parse/numbers/int-8", 11068696499466389475),
    ("parse/numbers/int-16", 1750737263612667202),
    ("parse/numbers/int-19", 3910800166464211048),
    ("parse/numbers/int-neg-8", 17615209137545929279),
    ("parse/numbers/float-short", 6729337136903190782),
    ("parse/numbers/float-15", 4391942830057404455),
    ("parse/numbers/float-17", 16061665255779085473),
    ("parse/numbers/float-exp", 6586078307175918106),
    ("parse/numbers/float-exp-large", 3962080271503951498),
    ("parse/numbers/suffixed", 10317600044218248446),
    ("parse/numbers/hex", 3691205948734428090),
    ("parse/numbers/float-16-exp", 11474609157145130322),
    ("parse/numbers/int-then-space-or-hash", 1743485584541154030),
    ("parse/containers/empty-objects", 14657274066200101971),
    ("parse/containers/empty-arrays", 360974758664151123),
    ("parse/containers/empty-values", 11868669492084191569),
    ("parse/containers/one-key-objects", 11698831450520512560),
    ("parse/containers/one-element-arrays", 10325554082204040652),
    ("parse/deep/arrays-1000", 1920244486229475431),
    ("parse/deep/json-objects-1000", 2361213540968275732),
    ("parse/deep/repeated-16", 16365922035629414403),
    ("parse/comments/hash-lines", 3380647026386443269),
    ("parse/comments/hash-after-values", 17074606882882375081),
    ("parse/comments/block-prose", 7204024078580389032),
    ("parse/comments/block-stars", 207054414155158634),
    ("parse/comments/block-slashes", 18237631582305121159),
    ("parse/comments/block-quotes", 11170023285804756558),
    ("parse/comments/block-dense", 14469326073034848378),
    ("parse/comments/block-nested", 6917579551416904135),
    (
        "parse/comments/hash-after-values-saved",
        17074606882882375081,
    ),
    ("parse/comments/block-prose-saved", 7204024078580389032),
    (
        "parse/comments/hash-after-values-no-separator",
        12011899222933193410,
    ),
    (
        "parse/comments/block-after-values-no-separator",
        15390601272990565304,
    ),
    (
        "parse/comments/hash-after-values-no-separator-saved",
        12011899222933193410,
    ),
    (
        "parse/comments/block-after-values-no-separator-saved",
        15390601272990565304,
    ),
    ("parse/comments/hash-long", 716720701233483892),
    ("parse/comments/block-long", 13245718204718329788),
    ("parse/whitespace/compact", 6865812900897743385),
    ("parse/whitespace/aligned", 9024777566714177723),
    ("parse/whitespace/tabs", 9292580417843937133),
    ("parse/whitespace/crlf", 18389909841634668026),
    ("parse/whitespace/blank-lines", 15727568771364184782),
    ("parse/whitespace/newline-ends", 5002583763124634519),
    ("parse/whitespace/space-separated", 16409844811476594847),
    ("parse/whitespace/colon-separated", 10291780271762621525),
    ("parse/objects/keys-8", 10286495065896882552),
    ("parse/objects/keys-16", 1591597068586436841),
    ("parse/objects/keys-17", 11533955280063579829),
    ("parse/objects/keys-24", 271010278840800081),
    ("parse/objects/keys-32", 9220386584439908044),
    ("parse/objects/keys-16-long-prefix", 1676048990998468813),
    ("parse/objects/keys-16-repeated", 17686712008780962455),
    ("parse/objects/keys-16-mixed-length", 302214490445225134),
    ("parse/objects/keys-16-lowercase", 9247650815093825118),
];

#[test]
fn the_workloads_are_the_checked_ones() {
    let digests: Vec<(String, u64)> = common::workloads()
        .iter()
        .map(|w| (w.id(), fnv1a(w.text.as_bytes())))
        .collect();
    let expected: Vec<(String, u64)> = WORKLOAD_DIGESTS
        .iter()
        .map(|&(id, digest)| (id.to_string(), digest))
        .collect();
    if digests != expected {
        for (id, digest) in &digests {
            println!("    (\"{id}\", {digest}),");
        }
    }
    assert_eq!(
        digests, expected,
        "the workloads changed: check them with benches/check-documents.sh, then update \
         WORKLOAD_DIGESTS (printed above)"
    );
}

/// The type an entry of a section workload holds, by its key: `name…` a string, `port…` an
/// integer, `enabled…` a boolean, `ratio…` a float, `configuration_parameter_name_…` an
/// integer; other keys (`field_NN`, `kNN…`, `field_name_NN`) by the entry's number `NN`, as
/// `name`, `port`, `enabled` and `ratio` are.
fn expected_type(key: &str) -> usize {
    let number = |key: &str| -> usize {
        let digits: String = key
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().unwrap()
    };
    if key.starts_with("configuration_parameter_name_") {
        return 1;
    }
    for (i, prefix) in ["name", "port", "enabled", "ratio"].iter().enumerate() {
        if key.starts_with(prefix) {
            return i;
        }
    }
    number(key) % 4
}

/// What a workload is meant to hold, by its id.
fn check_workload(workload: &common::Workload, value: &Value<'_>, stats: &Stats) {
    let id = &workload.id();
    let scalars = [
        stats.strings,
        stats.integers,
        stats.floats,
        stats.times,
        stats.booleans,
        stats.nulls,
    ];
    let only = |index: usize| {
        assert!(scalars[index] > 0, "{id}: {stats:?}");
        for (i, &count) in scalars.iter().enumerate() {
            assert!(i == index || count == 0, "{id}: {stats:?}");
        }
    };
    let group = id.rsplit_once('/').unwrap().0;
    match group {
        "parse/strings" => only(0),
        "parse/keys" => only(1),
        "parse/numbers" if id.contains("/int-") || id.ends_with("/hex") => only(1),
        "parse/numbers" if id.contains("/float-") => only(2),
        "parse/numbers" => {
            // Multipliers keep ints and floats; time suffixes make times (spec §5.5).
            assert!(
                stats.strings == 0 && stats.integers > 0 && stats.times > 0,
                "{id}"
            );
        }
        "parse/containers" => {
            assert!(
                stats.strings == 0 && stats.objects + stats.arrays > 1000,
                "{id}"
            );
        }
        "parse/deep" => assert!(stats.depth > 16, "{id}: {stats:?}"),
        _ => {
            // Sections of entries `name0`, `port1`, `enabled2`, `ratio3`, … or `field_00`, …:
            // every section has all its entries, and every entry its value, of its type,
            // whatever comments or whitespace surround it.
            let entries = match id.rsplit_once("/keys-") {
                Some((_, "16-repeated")) => 8,
                Some((_, keys)) => keys.split('-').next().unwrap().parse().unwrap(),
                None => 8,
            };
            let root = value.as_object().unwrap();
            assert!(root.len() >= 50, "{id}");
            for (_, section) in root {
                let section = section.first().as_object().unwrap();
                assert_eq!(section.len(), entries, "{id}");
                for (key, entry) in section {
                    let key: &str = key;
                    if workload.flags.contains(ParserFlags::KEY_LOWERCASE) {
                        assert_eq!(key, key.to_lowercase(), "{id}");
                    }
                    let value = entry.first();
                    let ok = match expected_type(key) {
                        0 => matches!(value, Value::String(_)),
                        1 => matches!(value, Value::Integer(_)),
                        2 => matches!(value, Value::Boolean(_)),
                        _ => matches!(value, Value::Float(_)),
                    };
                    assert!(ok, "{id}: {key} = {value:?}");
                }
            }
        }
    }
}

#[test]
fn each_workload_holds_what_it_is_meant_to() {
    for workload in common::workloads() {
        let id = workload.id();
        let mut parser = Parser::with_flags(workload.flags);
        let value = parser
            .parse(workload.text.as_bytes())
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        let stats = Stats::of(&value);
        println!("{id}: {} bytes, {stats:?}", workload.text.len());
        assert!(workload.text.len() >= common::SIZE, "{id}");
        check_workload(&workload, &value, &stats);
        if id.ends_with("-saved") {
            // One comment for each entry: no comment swallowed an entry or another comment.
            let entries: usize = value
                .as_object()
                .unwrap()
                .into_iter()
                .map(|(_, section)| section.first().as_object().unwrap().len())
                .sum();
            assert_eq!(parser.comments().len(), entries, "{id}");
        }
        if id.starts_with("parse/deep/") && id.ends_with("-1000") {
            assert!(stats.depth >= 1000, "{id}: {stats:?}");
        }
    }
}

/// The compact form of a JSON document parses to the same value as the document.
#[test]
fn compact_json_keeps_the_value() {
    let mut documents = vec![("json-1000".to_string(), common::json(1000))];
    documents.extend(
        common::json_documents("compact_json_keeps_the_value")
            .into_iter()
            .map(|d| (d.name, d.text)),
    );
    for (name, text) in documents {
        let compact = common::compact_json(&text);
        assert!(compact.len() < text.len(), "{name}");
        assert!(!compact.contains("\n"), "{name}");
        let value = serde_ucl::parse::parse(text.as_bytes()).unwrap();
        let compact_value = serde_ucl::parse::parse(compact.as_bytes()).unwrap();
        assert!(value == compact_value, "{name}");
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let compact_json: serde_json::Value = serde_json::from_str(&compact).unwrap();
        assert_eq!(json, compact_json, "{name}");
    }
}

fn both<T: DeserializeOwned>(name: &str, text: &str) {
    if let Err(e) = serde_ucl::from_str::<T>(text) {
        panic!("{name}: serde_ucl: {e}");
    }
    if let Err(e) = serde_json::from_str::<T>(text) {
        panic!("{name}: serde_json: {e}");
    }
}

/// The typed forms of the JSON documents deserialize with both crates. The fetched documents
/// are checked when they are present (`benches/fetch-documents.sh`).
#[test]
fn the_typed_forms_deserialize() {
    use common::typed::{Canada, CitmCatalog, JsonItems, Twitter};
    let json = common::json(1000);
    both::<JsonItems>("json-1000", &json);
    both::<JsonItems>("json-1000 compact", &common::compact_json(&json));
    for document in common::json_documents("the_typed_forms_deserialize") {
        let name = document.name.as_str();
        match name {
            "twitter" => both::<Twitter>(name, &document.text),
            "citm_catalog" => both::<CitmCatalog>(name, &document.text),
            "canada" => both::<Canada>(name, &document.text),
            _ => panic!("no typed form for {name}"),
        }
    }
}

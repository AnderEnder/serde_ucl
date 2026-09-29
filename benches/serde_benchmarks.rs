//! serde throughput: deserializing UCL into a typed struct, and serializing it in each format.
//! Deserialization is measured in input bytes per second, serialization in output bytes.
//!
//! `serde/deserialize-error-1000` measures a document whose last value does not fit the type:
//! `from_str` then parses and deserializes a second time to find the value's path and position,
//! which `from_value` does not.

mod common;

use common::Config;
use criterion::measurement::WallTime;
use criterion::{
    BatchSize, BenchmarkGroup, Criterion, Throughput, criterion_group, criterion_main,
};
use serde::Deserialize;
use serde::de::IgnoredAny;
use serde_ucl::{UclDeserializer, UclValue};
use std::hint::black_box;
use std::time::Duration;

fn bench_deserialize(c: &mut Criterion) {
    let input = common::config(1000);
    let mut group = c.benchmark_group("serde/deserialize-1000");
    group.throughput(Throughput::Bytes(input.len() as u64));
    // Parse and deserialize.
    group.bench_function("from_str", |b| {
        b.iter(|| serde_ucl::from_str::<Config>(black_box(&input)).unwrap())
    });
    // The same into a target that borrows its keys and strings from the input.
    group.bench_function("from_str-borrowed", |b| {
        b.iter(|| serde_ucl::from_str::<common::ConfigBorrowed>(black_box(&input)).unwrap())
    });
    // The document-level deserializer, which records the paths of errors as it goes.
    group.bench_function("UclDeserializer", |b| {
        b.iter(|| Config::deserialize(serde_ucl::UclDeserializer::new(black_box(&input))).unwrap())
    });
    // Deserialize a parsed value only.
    let value = serde_ucl::parse::parse(input.as_bytes()).unwrap();
    group.bench_function("from_value", |b| {
        b.iter_batched(
            || value.clone(),
            |value| serde_ucl::from_value::<Config>(value).unwrap(),
            BatchSize::LargeInput,
        )
    });
    group.finish();
}

fn bench_deserialize_small(c: &mut Criterion) {
    let input = common::SMALL;
    let mut group = c.benchmark_group("serde/deserialize-small");
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_function("from_str", |b| {
        b.iter(|| serde_ucl::from_str::<common::Small>(black_box(input)).unwrap())
    });
    group.bench_function("from_str-borrowed", |b| {
        b.iter(|| serde_ucl::from_str::<common::SmallBorrowed>(black_box(input)).unwrap())
    });
    group.finish();
}

fn bench_deserialize_error(c: &mut Criterion) {
    let input = common::config(1000);
    // The last service's `ratio` is a string.
    let at = input.rfind("ratio = ").unwrap();
    let end = at + input[at..].find('\n').unwrap();
    let input = format!("{}ratio = zero{}", &input[..at], &input[end..]);
    let err = serde_ucl::from_str::<Config>(&input).unwrap_err();
    assert!(err.position().is_some(), "{err}");
    let mut group = c.benchmark_group("serde/deserialize-error-1000");
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_function("from_str", |b| {
        b.iter(|| serde_ucl::from_str::<Config>(black_box(&input)).unwrap_err())
    });
    let value = serde_ucl::parse::parse(input.as_bytes()).unwrap();
    group.bench_function("from_value", |b| {
        b.iter_batched(
            || value.clone(),
            |value| serde_ucl::from_value::<Config>(value).unwrap_err(),
            BatchSize::LargeInput,
        )
    });
    group.finish();
}

fn bench_serialize(c: &mut Criterion) {
    let config: Config = serde_ucl::from_str(&common::config(1000)).unwrap();
    let mut group = c.benchmark_group("serde/serialize-1000");
    type Writer = fn(&Config) -> Result<String, serde_ucl::UclError>;
    for (name, write) in [
        ("to_string", serde_ucl::to_string::<Config> as Writer),
        ("to_json_string", serde_ucl::to_json_string::<Config>),
        (
            "to_json_string_compact",
            serde_ucl::to_json_string_compact::<Config>,
        ),
        ("to_yaml_string", serde_ucl::to_yaml_string::<Config>),
    ] {
        let size = write(&config).unwrap().len();
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_function(name, |b| b.iter(|| write(black_box(&config)).unwrap()));
    }
    group.finish();
    c.bench_function("serde/to_value-1000", |b| {
        b.iter(|| serde_ucl::to_value(black_box(&config)).unwrap())
    });
}

fn bench_nested(c: &mut Criterion) {
    let input = common::nested_mixed(1000);
    let value: UclValue = serde_ucl::from_str(&input).unwrap();
    let mut group = c.benchmark_group("serde/nested-mixed-1000");
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_function("from_str", |b| {
        b.iter(|| serde_ucl::from_str::<UclValue>(black_box(&input)).unwrap())
    });
    type Writer = fn(&UclValue) -> Result<String, serde_ucl::UclError>;
    for (name, write) in [
        ("to_string", serde_ucl::to_string::<UclValue> as Writer),
        (
            "to_json_string_compact",
            serde_ucl::to_json_string_compact::<UclValue>,
        ),
    ] {
        let size = write(&value).unwrap().len();
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_function(name, |b| b.iter(|| write(black_box(&value)).unwrap()));
    }
    group.finish();
}

/// Deserializes `input` into a `UclValue`, which takes an owned parse, and into `IgnoredAny`,
/// which takes a borrowed one (clean-room work item C13).
fn untyped(group: &mut BenchmarkGroup<'_, WallTime>, input: &str) {
    group.bench_function("UclValue", |b| {
        b.iter(|| serde_ucl::from_str::<UclValue>(black_box(input)).unwrap())
    });
    group.bench_function("IgnoredAny", |b| {
        b.iter(|| serde_ucl::from_str::<IgnoredAny>(black_box(input)).unwrap())
    });
}

/// The irregular configurations of `common::IRREGULAR`.
fn bench_irregular(c: &mut Criterion) {
    for (name, seed, size) in common::IRREGULAR {
        let input = common::irregular(seed, size);
        let mut group = c.benchmark_group(format!("serde/irregular-{name}"));
        group.throughput(Throughput::Bytes(input.len() as u64));
        untyped(&mut group, &input);
        group.finish();
    }
}

/// One serde_ucl and one serde_json group per JSON document. A document that does not
/// deserialize with either crate is skipped with a message.
fn bench_documents(c: &mut Criterion, prefix: &str, documents: Vec<common::Document>) {
    for document in documents {
        if let Err(e) = serde_ucl::from_str::<UclValue>(&document.text) {
            eprintln!("{prefix}: skipping {}: {e}", document.path.display());
            continue;
        }
        if let Err(e) = serde_json::from_str::<serde_json::Value>(&document.text) {
            eprintln!(
                "{prefix}: skipping {}: serde_json: {e}",
                document.path.display()
            );
            continue;
        }
        let mut group = c.benchmark_group(format!("{prefix}-{}", document.name));
        group.throughput(Throughput::Bytes(document.text.len() as u64));
        untyped(&mut group, &document.text);
        group.finish();

        let mut group = c.benchmark_group(format!("serde_json/json-corpus-{}", document.name));
        group.throughput(Throughput::Bytes(document.text.len() as u64));
        group.bench_function("Value", |b| {
            b.iter(|| serde_json::from_str::<serde_json::Value>(black_box(&document.text)).unwrap())
        });
        group.bench_function("IgnoredAny", |b| {
            b.iter(|| serde_json::from_str::<IgnoredAny>(black_box(&document.text)).unwrap())
        });
        group.finish();
    }
}

/// The JSON documents in `target/bench-corpus/` (`benches/fetch-documents.sh`).
fn bench_json_corpus(c: &mut Criterion) {
    let prefix = "serde/json-corpus";
    bench_documents(c, prefix, common::json_documents(prefix));
}

/// The rspamd configurations in `benches/corpus/`, through `UclDeserializer` with a parser set
/// up as their check sets one up (`common::Corpus::parser`); making that parser is part of each
/// iteration. The throughput counts the included files too. One that does not deserialize is a
/// failure, not a skip.
fn bench_corpus(c: &mut Criterion) {
    let prefix = "serde/corpus";
    for document in common::corpus_documents(prefix) {
        let input = document.text.as_bytes();
        if let Err(e) =
            UclValue::deserialize(UclDeserializer::from_parser(document.parser(), input))
        {
            panic!("{prefix}: {}: {e}", document.path.display());
        }
        let mut group = c.benchmark_group(format!("{prefix}-{}", document.document.name));
        group.throughput(Throughput::Bytes(document.bytes_read));
        group.bench_function("UclValue", |b| {
            b.iter(|| {
                UclValue::deserialize(UclDeserializer::from_parser(
                    document.parser(),
                    black_box(input),
                ))
                .unwrap()
            })
        });
        group.bench_function("IgnoredAny", |b| {
            b.iter(|| {
                IgnoredAny::deserialize(UclDeserializer::from_parser(
                    document.parser(),
                    black_box(input),
                ))
                .unwrap()
            })
        });
        group.finish();
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = bench_deserialize, bench_deserialize_small, bench_deserialize_error, bench_serialize, bench_nested,
        bench_irregular, bench_json_corpus, bench_corpus
}
criterion_main!(benches);

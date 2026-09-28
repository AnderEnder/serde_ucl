//! Parse throughput of the parser core (`serde_ucl::parse`), in input bytes per second.

mod common;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use serde_ucl::ParserFlags;
use serde_ucl::parse::{Parser, ParserBuilder};
use std::hint::black_box;
use std::time::Duration;

fn bench_config(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse/config");
    for services in [10, 100, 1000] {
        let input = common::config(services);
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(services), &input, |b, input| {
            let mut parser = Parser::new();
            b.iter(|| parser.parse(black_box(input.as_bytes())).unwrap());
        });
    }
    group.finish();
}

fn bench_small(c: &mut Criterion) {
    let input = common::SMALL;
    let mut group = c.benchmark_group("parse/small");
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_function("reused-parser", |b| {
        let mut parser = Parser::new();
        b.iter(|| parser.parse(black_box(input.as_bytes())).unwrap());
    });
    group.bench_function("new-parser", |b| {
        b.iter(|| serde_ucl::parse::parse(black_box(input.as_bytes())).unwrap());
    });
    group.finish();
}

fn bench_flags(c: &mut Criterion) {
    let input = common::config(100);
    let mut group = c.benchmark_group("parse/config-100-flags");
    group.throughput(Throughput::Bytes(input.len() as u64));
    for (name, flags) in [
        ("save-comments", ParserFlags::SAVE_COMMENTS),
        ("no-implicit-arrays", ParserFlags::NO_IMPLICIT_ARRAYS),
        ("key-lowercase", ParserFlags::KEY_LOWERCASE),
    ] {
        group.bench_function(name, |b| {
            let mut parser = Parser::with_flags(flags);
            b.iter(|| parser.parse(black_box(input.as_bytes())).unwrap());
        });
    }
    group.finish();
}

fn bench_json(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse/json");
    for items in [100, 1000] {
        let input = common::json(items);
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(items), &input, |b, input| {
            let mut parser = Parser::new();
            b.iter(|| parser.parse(black_box(input.as_bytes())).unwrap());
        });
    }
    group.finish();
}

fn bench_nested(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse/nested");
    for depth in [10, 500, 1000] {
        let input = common::nested(depth);
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(depth), &input, |b, input| {
            let mut parser = Parser::new();
            b.iter(|| parser.parse(black_box(input.as_bytes())).unwrap());
        });
    }
    group.finish();
    let input = common::nested_mixed(1000);
    let mut group = c.benchmark_group("parse/nested-mixed-1000");
    group.throughput(Throughput::Bytes(input.len() as u64));
    for (name, flags) in [
        ("default", ParserFlags::empty()),
        ("save-comments", ParserFlags::SAVE_COMMENTS),
    ] {
        group.bench_function(name, |b| {
            let mut parser = Parser::with_flags(flags);
            b.iter(|| parser.parse(black_box(input.as_bytes())).unwrap());
        });
    }
    group.finish();
}

fn bench_variables(c: &mut Criterion) {
    let input = common::variables(1000);
    let mut group = c.benchmark_group("parse/variables");
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_function("1000", |b| {
        let mut parser = ParserBuilder::new()
            .with_variables(common::VARIABLES)
            .build();
        b.iter(|| parser.parse(black_box(input.as_bytes())).unwrap());
    });
    group.finish();
}

/// The irregular configurations of `common::IRREGULAR`.
fn bench_irregular(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse/irregular");
    for (name, seed, size) in common::IRREGULAR {
        let input = common::irregular(seed, size);
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), &input, |b, input| {
            let mut parser = Parser::new();
            b.iter(|| parser.parse(black_box(input.as_bytes())).unwrap());
        });
    }
    group.finish();
}

/// A group of one benchmark per document. A document that does not parse is skipped with a
/// message, and so is the group if no document is left.
fn bench_documents(c: &mut Criterion, name: &str, documents: Vec<common::Document>) {
    let documents: Vec<common::Document> = documents
        .into_iter()
        .filter(
            |document| match Parser::new().parse(document.text.as_bytes()) {
                Ok(_) => true,
                Err(e) => {
                    eprintln!("{name}: skipping {}: {e}", document.path.display());
                    false
                }
            },
        )
        .collect();
    if documents.is_empty() {
        return;
    }
    let mut group = c.benchmark_group(name);
    for document in &documents {
        group.throughput(Throughput::Bytes(document.text.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(&document.name),
            &document.text,
            |b, input| {
                let mut parser = Parser::new();
                b.iter(|| parser.parse(black_box(input.as_bytes())).unwrap());
            },
        );
    }
    group.finish();
}

/// The JSON documents in `target/bench-corpus/` (`benches/fetch-documents.sh`).
fn bench_json_corpus(c: &mut Criterion) {
    let name = "parse/json-corpus";
    bench_documents(c, name, common::json_documents(name));
}

/// The configurations in `benches/corpus/`.
fn bench_corpus(c: &mut Criterion) {
    let name = "parse/corpus";
    bench_documents(c, name, common::corpus_documents(name));
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = bench_config, bench_small, bench_flags, bench_json, bench_nested, bench_variables,
        bench_irregular, bench_json_corpus, bench_corpus
}
criterion_main!(benches);

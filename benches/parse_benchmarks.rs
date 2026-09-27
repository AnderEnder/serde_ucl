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

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = bench_config, bench_small, bench_flags, bench_json, bench_nested, bench_variables
}
criterion_main!(benches);

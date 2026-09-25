//! Output throughput of the emitters (`ucl_lexer::emit`), in output bytes per second.

mod common;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;
use ucl_lexer::emit::Format;
use ucl_lexer::parse::Parser;

fn bench_formats(c: &mut Criterion) {
    let input = common::config(1000);
    let mut parser = Parser::new();
    let value = parser.parse(input.as_bytes()).unwrap();
    let mut group = c.benchmark_group("emit/config-1000");
    for (name, format) in [
        ("config", Format::Config),
        ("json", Format::Json),
        ("json-compact", Format::JsonCompact),
        ("yaml", Format::Yaml),
    ] {
        // The emitter uses the output facts of the parse (spec §10.1), as libucl's output does.
        let emitter = parser.emitter(format);
        let size = emitter.emit(&value).len();
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_function(name, |b| b.iter(|| emitter.emit(black_box(&value))));
    }
    group.finish();
}

fn bench_nested(c: &mut Criterion) {
    let input = common::nested_mixed(1000);
    let mut parser = Parser::new();
    let value = parser.parse(input.as_bytes()).unwrap();
    let mut group = c.benchmark_group("emit/nested-mixed-1000");
    for (name, format) in [
        ("config", Format::Config),
        ("json", Format::Json),
        ("json-compact", Format::JsonCompact),
        ("yaml", Format::Yaml),
    ] {
        let emitter = parser.emitter(format);
        let size = emitter.emit(&value).len();
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_function(name, |b| b.iter(|| emitter.emit(black_box(&value))));
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = bench_formats, bench_nested
}
criterion_main!(benches);

//! Output throughput of the emitters (`serde_ucl::emit`), in output bytes per second.
//!
//! `emit/config-1000` and `emit/nested-mixed-1000` use the parser's output facts, as libucl's
//! output does; so do `emit/json-corpus-*` (the JSON documents in every format),
//! `emit/strings-*` (string workloads in every format) and `emit/numbers-*` (the number sweeps
//! in JSON and config output).

mod common;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use serde_ucl::emit::Format;
use serde_ucl::parse::Parser;
use std::hint::black_box;
use std::time::Duration;

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

/// The formats of the emit groups below.
const FORMATS: [(&str, Format); 4] = [
    ("config", Format::Config),
    ("json", Format::Json),
    ("json-compact", Format::JsonCompact),
    ("yaml", Format::Yaml),
];

/// The JSON documents in `target/bench-corpus/` (`benches/fetch-documents.sh`) in every format
/// (clean-room work item C16, task P1): `canada` is mostly floats, `twitter` and
/// `citm_catalog` mostly strings and keys. A missing document is skipped with a message.
fn bench_json_corpus(c: &mut Criterion) {
    for document in common::json_documents("emit/json-corpus") {
        let mut parser = Parser::new();
        let value = match parser.parse(document.text.as_bytes()) {
            Ok(value) => value,
            Err(e) => {
                eprintln!(
                    "emit/json-corpus: skipping {}: {e}",
                    document.path.display()
                );
                continue;
            }
        };
        let mut group = c.benchmark_group(format!("emit/json-corpus-{}", document.name));
        for (name, format) in FORMATS {
            let emitter = parser.emitter(format);
            let size = emitter.emit(&value).len();
            group.throughput(Throughput::Bytes(size as u64));
            group.bench_function(name, |b| b.iter(|| emitter.emit(black_box(&value))));
        }
        group.finish();
    }
}

/// Emits each workload of `group` named in `names` (`common::workloads()`) in each of `formats`,
/// in the groups `emit/<prefix>-<format>`.
fn bench_workloads(
    c: &mut Criterion,
    prefix: &str,
    group: &str,
    names: &[&str],
    formats: &[(&str, Format)],
) {
    let workloads: Vec<_> = common::workloads()
        .into_iter()
        .filter(|w| w.group == group && names.contains(&w.name.as_str()))
        .collect();
    assert_eq!(workloads.len(), names.len(), "{group}: {names:?}");
    for &(name, format) in formats {
        let mut group = c.benchmark_group(format!("emit/{prefix}-{name}"));
        for workload in &workloads {
            let mut parser = Parser::new();
            let value = parser.parse(workload.text.as_bytes()).unwrap();
            let emitter = parser.emitter(format);
            let size = emitter.emit(&value).len();
            group.throughput(Throughput::Bytes(size as u64));
            group.bench_function(&workload.name, |b| {
                b.iter(|| emitter.emit(black_box(&value)))
            });
        }
        group.finish();
    }
}

/// String workloads in every format, for the string writers (C16, P1): short, long, escaped,
/// densely escaped (control characters among the escapes) and non-ASCII strings.
fn bench_strings(c: &mut Criterion) {
    bench_workloads(
        c,
        "strings",
        "parse/strings",
        &[
            "dq-8",
            "dq-64",
            "dq-4096",
            "dq-escaped-64",
            "dq-utf8-64",
            "dq-escaped-dense-64",
        ],
        &FORMATS,
    );
}

/// The number sweeps in JSON and config output, for the number writers (C16, P1).
fn bench_numbers(c: &mut Criterion) {
    bench_workloads(
        c,
        "numbers",
        "parse/numbers",
        &[
            "int-1",
            "int-8",
            "int-19",
            "float-short",
            "float-15",
            "float-17",
            "float-exp-large",
            "suffixed",
        ],
        &[("json", Format::Json), ("config", Format::Config)],
    );
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = bench_formats, bench_nested, bench_json_corpus, bench_strings, bench_numbers
}
criterion_main!(benches);

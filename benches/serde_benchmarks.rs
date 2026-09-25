//! serde throughput: deserializing UCL into a typed struct, and serializing it in each format.
//! Deserialization is measured in input bytes per second, serialization in output bytes.

mod common;

use common::Config;
use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;
use ucl_lexer::UclValue;

fn bench_deserialize(c: &mut Criterion) {
    let input = common::config(1000);
    let mut group = c.benchmark_group("serde/deserialize-1000");
    group.throughput(Throughput::Bytes(input.len() as u64));
    // Parse and deserialize.
    group.bench_function("from_str", |b| {
        b.iter(|| ucl_lexer::from_str::<Config>(black_box(&input)).unwrap())
    });
    // Deserialize a parsed value only.
    let value = ucl_lexer::parse::parse(input.as_bytes()).unwrap();
    group.bench_function("from_value", |b| {
        b.iter_batched(
            || value.clone(),
            |value| ucl_lexer::from_value::<Config>(value).unwrap(),
            BatchSize::LargeInput,
        )
    });
    group.finish();
}

fn bench_serialize(c: &mut Criterion) {
    let config: Config = ucl_lexer::from_str(&common::config(1000)).unwrap();
    let mut group = c.benchmark_group("serde/serialize-1000");
    type Writer = fn(&Config) -> Result<String, ucl_lexer::UclError>;
    for (name, write) in [
        ("to_string", ucl_lexer::to_string::<Config> as Writer),
        ("to_json_string", ucl_lexer::to_json_string::<Config>),
        (
            "to_json_string_compact",
            ucl_lexer::to_json_string_compact::<Config>,
        ),
        ("to_yaml_string", ucl_lexer::to_yaml_string::<Config>),
    ] {
        let size = write(&config).unwrap().len();
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_function(name, |b| b.iter(|| write(black_box(&config)).unwrap()));
    }
    group.finish();
    c.bench_function("serde/to_value-1000", |b| {
        b.iter(|| ucl_lexer::to_value(black_box(&config)).unwrap())
    });
}

fn bench_nested(c: &mut Criterion) {
    let input = common::nested_mixed(1000);
    let value: UclValue = ucl_lexer::from_str(&input).unwrap();
    let mut group = c.benchmark_group("serde/nested-mixed-1000");
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_function("from_str", |b| {
        b.iter(|| ucl_lexer::from_str::<UclValue>(black_box(&input)).unwrap())
    });
    type Writer = fn(&UclValue) -> Result<String, ucl_lexer::UclError>;
    for (name, write) in [
        ("to_string", ucl_lexer::to_string::<UclValue> as Writer),
        (
            "to_json_string_compact",
            ucl_lexer::to_json_string_compact::<UclValue>,
        ),
    ] {
        let size = write(&value).unwrap().len();
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_function(name, |b| b.iter(|| write(black_box(&value)).unwrap()));
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = bench_deserialize, bench_serialize, bench_nested
}
criterion_main!(benches);

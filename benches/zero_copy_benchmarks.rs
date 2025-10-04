// Note: Zero-copy is now always enabled by default.
// This file benchmarks the effectiveness of automatic zero-copy optimization
// (borrowed strings vs owned strings with escapes).

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use std::borrow::Cow;
use ucl_lexer::{StringFormat, Token, UclLexer};

/// Benchmark the effectiveness of zero-copy (borrowed vs owned strings)
fn bench_zero_copy_effectiveness(c: &mut Criterion) {
    let mut group = c.benchmark_group("zero_copy_effectiveness");

    // Content that benefits from zero-copy (no escapes)
    let optimal_content = format!(
        r#"{{
            {}
        }}"#,
        (0..500)
            .map(|i| format!(r#"key{} = "value{}""#, i, i))
            .collect::<Vec<_>>()
            .join("\n            ")
    );

    // Content that requires owned strings (has escapes)
    let suboptimal_content = format!(
        r#"{{
            {}
        }}"#,
        (0..500)
            .map(|i| format!(r#"key{} = "value{}\nwith\tescapes""#, i, i))
            .collect::<Vec<_>>()
            .join("\n            ")
    );

    group.throughput(Throughput::Bytes(optimal_content.len() as u64));

    group.bench_function("optimal_zero_copy", |b| {
        b.iter(|| {
            let mut lexer = UclLexer::new(black_box(&optimal_content));
            let mut borrowed_count = 0;
            let mut owned_count = 0;

            while let Ok(token) = lexer.next_token() {
                if let Token::String { value, .. } = &token {
                    match value {
                        Cow::Borrowed(_) => borrowed_count += 1,
                        Cow::Owned(_) => owned_count += 1,
                    }
                }
                if matches!(token, Token::Eof) {
                    break;
                }
            }
            (borrowed_count, owned_count)
        });
    });

    group.bench_function("suboptimal_zero_copy", |b| {
        b.iter(|| {
            let mut lexer = UclLexer::new(black_box(&suboptimal_content));
            let mut borrowed_count = 0;
            let mut owned_count = 0;

            while let Ok(token) = lexer.next_token() {
                if let Token::String { value, .. } = &token {
                    match value {
                        Cow::Borrowed(_) => borrowed_count += 1,
                        Cow::Owned(_) => owned_count += 1,
                    }
                }
                if matches!(token, Token::Eof) {
                    break;
                }
            }
            (borrowed_count, owned_count)
        });
    });

    group.finish();
}

/// Benchmark string size impact on parsing performance
fn bench_string_size_impact(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_size_impact");

    let sizes = vec![10, 100, 1000, 10000];

    for size in sizes {
        let content = format!(r#""{}""#, "x".repeat(size));

        group.throughput(Throughput::Bytes(content.len() as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), &content, |b, content| {
            b.iter(|| {
                let mut lexer = UclLexer::new(black_box(content));
                lexer.next_token()
            });
        });
    }

    group.finish();
}

/// Benchmark Cow<str> usage patterns
fn bench_cow_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("cow_usage");

    let test_strings = vec![
        ("borrowed", "simple_string_without_escapes"),
        ("owned_escapes", "string_with\\nescapes\\tand\\rmore"),
        ("owned_variables", "string_with_${VARIABLE}_references"),
    ];

    for (name, string_content) in test_strings {
        group.bench_with_input(
            BenchmarkId::new("cow_borrowed", name),
            &string_content,
            |b, content| {
                b.iter(|| {
                    let cow: Cow<str> = Cow::Borrowed(black_box(content));
                    cow.len()
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("cow_owned", name),
            &string_content,
            |b, content| {
                b.iter(|| {
                    let cow: Cow<str> = Cow::Owned(black_box(content).to_string());
                    cow.len()
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("cow_clone", name),
            &string_content,
            |b, content| {
                b.iter(|| {
                    let cow: Cow<str> = Cow::Borrowed(black_box(content));
                    let _cloned = cow.clone();
                    cow.len()
                });
            },
        );
    }

    group.finish();
}

/// Benchmark string format detection
fn bench_string_format_optimization(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_format_optimization");

    let string_formats = vec![
        ("json_simple", r#""simple string""#, StringFormat::Json),
        (
            "json_escaped",
            r#""string with \"quotes\" and \n newlines""#,
            StringFormat::Json,
        ),
        ("single_simple", "'simple string'", StringFormat::Single),
        (
            "single_escaped",
            r"'string with \' quote'",
            StringFormat::Single,
        ),
        (
            "heredoc_simple",
            "<<EOF\nsimple content\nEOF",
            StringFormat::Heredoc,
        ),
        (
            "heredoc_multiline",
            "<<TERM\nline 1\nline 2\nline 3\nTERM",
            StringFormat::Heredoc,
        ),
    ];

    for (name, content, expected_format) in string_formats {
        group.bench_with_input(
            BenchmarkId::new("parse_and_classify", name),
            &(content, expected_format),
            |b, (content, _expected_format)| {
                b.iter(|| {
                    let mut lexer = UclLexer::new(black_box(content));
                    if let Ok(Token::String { format, .. }) = lexer.next_token() {
                        format
                    } else {
                        StringFormat::Json // fallback
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_zero_copy_effectiveness,
    bench_string_size_impact,
    bench_cow_usage,
    bench_string_format_optimization
);
criterion_main!(benches);

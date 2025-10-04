use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use std::io::Cursor;
use ucl_lexer::{LexerConfig, Token, UclLexer, streaming_lexer_from_reader};

/// Generate test UCL content of various sizes
fn generate_ucl_content(size_category: &str) -> String {
    match size_category {
        "small" => {
            // ~1KB of UCL content
            r#"{
    name = "test-app"
    version = "1.0.0"
    description = "A test application"
    author = "Test Author"
    license = "MIT"
    dependencies = {
        serde = "1.0"
        tokio = "1.0"
        reqwest = "0.11"
    }
    features = ["json", "yaml", "toml"]
    debug = true
    port = 8080
    timeout = 30s
    max_connections = 1000
}"#
            .to_string()
        }
        "medium" => {
            // ~10KB of UCL content
            let mut content = String::new();
            content.push_str("{\n");
            for i in 0..100 {
                content.push_str(&format!(
                    r#"
    service_{} = {{
        name = "service-{}"
        port = {}
        enabled = {}
        config = {{
            timeout = {}s
            retries = {}
            endpoints = ["/health", "/metrics", "/status"]
            features = ["logging", "monitoring", "tracing"]
        }}
        metadata = {{
            version = "1.{}.0"
            author = "Service Team {}"
            description = "Auto-generated service configuration"
        }}
    }}
"#,
                    i,
                    i,
                    8000 + i,
                    i % 2 == 0,
                    10 + (i % 20),
                    3 + (i % 5),
                    i % 10,
                    i % 5
                ));
            }
            content.push_str("}\n");
            content
        }
        "large" => {
            // ~100KB of UCL content
            let mut content = String::new();
            content.push_str("{\n");
            for i in 0..1000 {
                content.push_str(&format!(
                    r#"
    item_{} = {{
        id = {}
        name = "item-{}"
        active = {}
        priority = {}
        tags = ["tag-{}", "category-{}", "type-{}"]
        config = {{
            timeout = {}s
            retries = {}
            batch_size = {}
            parallel = {}
        }}
        metadata = {{
            created_at = "2024-01-{}T10:{}:{}Z"
            updated_at = "2024-01-{}T15:{}:{}Z"
            version = "1.{}.{}"
            checksum = "sha256:abcdef{:08x}"
        }}
        nested = {{
            level1 = {{
                level2 = {{
                    level3 = {{
                        value = "deep-value-{}"
                        number = {}
                        flag = {}
                    }}
                }}
            }}
        }}
    }}
"#,
                    i,
                    i,
                    i,
                    i % 2 == 0,
                    i % 10,
                    i % 20,
                    i % 15,
                    i % 8,
                    5 + (i % 25),
                    2 + (i % 7),
                    50 + (i % 100),
                    i % 2 == 0,
                    1 + (i % 28),
                    (i % 60),
                    (i % 60),
                    1 + (i % 28),
                    (i % 60),
                    (i % 60),
                    i % 100,
                    i % 50,
                    i,
                    i,
                    i * 42,
                    i % 2 == 0
                ));
            }
            content.push_str("}\n");
            content
        }
        _ => "{ test = true }".to_string(),
    }
}

/// Benchmark basic lexer tokenization
fn bench_lexer_tokenization(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer_tokenization");

    for size in ["small", "medium", "large"] {
        let content = generate_ucl_content(size);
        let content_size = content.len();

        group.throughput(Throughput::Bytes(content_size as u64));

        // Benchmark regular lexer
        group.bench_with_input(BenchmarkId::new("regular", size), &content, |b, content| {
            b.iter(|| {
                let mut lexer = UclLexer::new(black_box(content));
                let mut token_count = 0;
                while let Ok(token) = lexer.next_token() {
                    black_box(&token);
                    token_count += 1;
                    if matches!(token, Token::Eof) {
                        break;
                    }
                }
                token_count
            });
        });

        // Benchmark zero-copy lexer
        group.bench_with_input(
            BenchmarkId::new("zero_copy", size),
            &content,
            |b, content| {
                b.iter(|| {
                    let mut lexer = UclLexer::new(black_box(content));
                    let mut token_count = 0;
                    while let Ok(token) = lexer.next_token() {
                        black_box(&token);
                        token_count += 1;
                        if matches!(token, Token::Eof) {
                            break;
                        }
                    }
                    token_count
                });
            },
        );

        // Benchmark streaming lexer
        group.bench_with_input(
            BenchmarkId::new("streaming", size),
            &content,
            |b, content| {
                b.iter(|| {
                    let cursor = Cursor::new(black_box(content.as_bytes()));
                    let mut lexer = streaming_lexer_from_reader(cursor);
                    let mut token_count = 0;
                    while let Ok(token) = lexer.next_token() {
                        black_box(&token);
                        token_count += 1;
                        if matches!(token, Token::Eof) {
                            break;
                        }
                    }
                    token_count
                });
            },
        );
    }

    group.finish();
}

/// Benchmark string parsing specifically
fn bench_string_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_parsing");

    // JSON strings with various escape patterns
    let long_json_string = format!(r#""very long string {}" "#, "x".repeat(1000));
    let json_strings = vec![
        r#""simple string""#,
        r#""string with \"escapes\" and \n newlines""#,
        r#""string with unicode \u0041\u0042\u0043""#,
        r#""string with variables $VAR and ${ANOTHER_VAR}""#,
        &long_json_string,
    ];

    for (i, string_content) in json_strings.iter().enumerate() {
        group.throughput(Throughput::Bytes(string_content.len() as u64));

        group.bench_with_input(
            BenchmarkId::new("json_string", i),
            string_content,
            |b, content| {
                b.iter(|| {
                    let mut lexer = UclLexer::new(black_box(content));
                    lexer.next_token()
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("json_string_zero_copy", i),
            string_content,
            |b, content| {
                b.iter(|| {
                    let mut lexer = UclLexer::new(black_box(content));
                    lexer.next_token()
                });
            },
        );
    }

    // Single-quoted strings
    let long_single_string = format!("'very long string {}'", "y".repeat(1000));
    let single_strings = vec![
        "'simple string'",
        "'string with \\' escape'",
        "'string with line\\\ncontinuation'",
        &long_single_string,
    ];

    for (i, string_content) in single_strings.iter().enumerate() {
        group.throughput(Throughput::Bytes(string_content.len() as u64));

        group.bench_with_input(
            BenchmarkId::new("single_string", i),
            string_content,
            |b, content| {
                b.iter(|| {
                    let mut lexer = UclLexer::new(black_box(content));
                    lexer.next_token()
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("single_string_zero_copy", i),
            string_content,
            |b, content| {
                b.iter(|| {
                    let mut lexer = UclLexer::new(black_box(content));
                    lexer.next_token()
                });
            },
        );
    }

    group.finish();
}

/// Benchmark number parsing
fn bench_number_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("number_parsing");

    let numbers = vec![
        "42", "-123", "3.14159", "1.23e-4", "0x1a2b3c", "1000k", "512mb", "30s", "2.5h", "inf",
        "nan",
    ];

    for (i, number_str) in numbers.iter().enumerate() {
        group.bench_with_input(BenchmarkId::new("number", i), number_str, |b, content| {
            b.iter(|| {
                let mut lexer = UclLexer::new(black_box(content));
                lexer.next_token()
            });
        });
    }

    group.finish();
}

/// Benchmark comment handling
fn bench_comment_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("comment_parsing");

    let comment_content = format!(
        "# Single line comment\n{}\n/* Multi-line\n   comment\n   /* nested */\n   block */\n{}",
        "key = value".repeat(100),
        "another = setting".repeat(100)
    );

    group.throughput(Throughput::Bytes(comment_content.len() as u64));

    // Without saving comments
    group.bench_function("without_saving", |b| {
        b.iter(|| {
            let mut lexer = UclLexer::new(black_box(&comment_content));
            let mut token_count = 0;
            while let Ok(token) = lexer.next_token() {
                black_box(&token);
                token_count += 1;
                if matches!(token, Token::Eof) {
                    break;
                }
            }
            token_count
        });
    });

    // With saving comments
    group.bench_function("with_saving", |b| {
        b.iter(|| {
            let mut config = LexerConfig::default();
            config.save_comments = true;
            let mut lexer = UclLexer::with_config(black_box(&comment_content), config);
            let mut token_count = 0;
            while let Ok(token) = lexer.next_token() {
                black_box(&token);
                token_count += 1;
                if matches!(token, Token::Eof) {
                    break;
                }
            }
            (token_count, lexer.comment_count())
        });
    });

    group.finish();
}

/// Benchmark character table lookups
fn bench_character_classification(c: &mut Criterion) {
    let mut group = c.benchmark_group("character_classification");

    let test_chars: Vec<u8> = (0..=255).collect();

    group.bench_function("is_whitespace", |b| {
        b.iter(|| {
            let table = &ucl_lexer::lexer::CHARACTER_TABLE;
            for &ch in black_box(&test_chars) {
                black_box(table.is_whitespace(ch));
            }
        });
    });

    group.bench_function("is_key_start", |b| {
        b.iter(|| {
            let table = &ucl_lexer::lexer::CHARACTER_TABLE;
            for &ch in black_box(&test_chars) {
                black_box(table.is_key_start(ch));
            }
        });
    });

    group.bench_function("is_digit", |b| {
        b.iter(|| {
            let table = &ucl_lexer::lexer::CHARACTER_TABLE;
            for &ch in black_box(&test_chars) {
                black_box(table.is_digit(ch));
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_lexer_tokenization,
    bench_string_parsing,
    bench_number_parsing,
    bench_comment_parsing,
    bench_character_classification
);
criterion_main!(benches);

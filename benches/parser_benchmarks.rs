use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use serde::Deserialize;
use std::collections::HashMap;
use ucl_lexer::{
    EnvironmentVariableHandler, LexerConfig, MapVariableHandler, ParserConfig, UclDeserializer,
    UclParser, from_str,
};

/// Generate complex nested UCL structures for parsing benchmarks
fn generate_nested_ucl(depth: usize, breadth: usize) -> String {
    fn generate_object(current_depth: usize, max_depth: usize, breadth: usize) -> String {
        if current_depth >= max_depth {
            return "{ leaf = true }".to_string();
        }

        let mut content = String::from("{\n");
        for i in 0..breadth {
            content.push_str(&format!(
                "  key_{} = {}\n",
                i,
                generate_object(current_depth + 1, max_depth, breadth)
            ));
        }
        content.push_str("  simple_value = \"test\"\n");
        content.push_str("  number_value = 42\n");
        content.push_str("  array_value = [1, 2, 3, \"four\", true]\n");
        content.push_str("}");
        content
    }

    generate_object(0, depth, breadth)
}

/// Generate UCL with variable references
fn generate_variable_ucl() -> String {
    r#"{
    base_url = "https://api.example.com"
    api_key = "${API_KEY}"
    database_url = "${DB_PROTOCOL}://${DB_USER}:${DB_PASS}@${DB_HOST}:${DB_PORT}/${DB_NAME}"
    
    services = {
        web = {
            url = "${base_url}/web"
            port = 8080
            config = {
                timeout = "${WEB_TIMEOUT:-30s}"
                workers = "${WEB_WORKERS:-4}"
            }
        }
        api = {
            url = "${base_url}/api"
            port = 8081
            config = {
                timeout = "${API_TIMEOUT:-60s}"
                rate_limit = "${API_RATE_LIMIT:-1000}"
            }
        }
    }
    
    features = {
        logging = {
            level = "${LOG_LEVEL:-info}"
            output = "${LOG_OUTPUT:-stdout}"
        }
        monitoring = {
            enabled = "${MONITORING_ENABLED:-true}"
            endpoint = "${MONITORING_URL:-http://localhost:9090}"
        }
    }
}"#
    .to_string()
}

/// Test structures for deserialization benchmarks
#[derive(Deserialize)]
struct SimpleConfig {
    name: String,
    port: u16,
    enabled: bool,
}

#[derive(Deserialize)]
struct ComplexConfig {
    services: HashMap<String, ServiceConfig>,
    features: FeatureConfig,
    metadata: MetadataConfig,
}

#[derive(Deserialize)]
struct ServiceConfig {
    url: String,
    port: u16,
    config: ServiceSettings,
}

#[derive(Deserialize)]
struct ServiceSettings {
    timeout: String,
    workers: Option<u32>,
    rate_limit: Option<u32>,
}

#[derive(Deserialize)]
struct FeatureConfig {
    logging: LoggingConfig,
    monitoring: MonitoringConfig,
}

#[derive(Deserialize)]
struct LoggingConfig {
    level: String,
    output: String,
}

#[derive(Deserialize)]
struct MonitoringConfig {
    enabled: bool,
    endpoint: String,
}

#[derive(Deserialize)]
struct MetadataConfig {
    version: String,
    author: String,
    created_at: String,
}

/// Benchmark basic parsing operations
fn bench_parser_basic(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser_basic");

    let simple_ucl = r#"{
        name = "test-app"
        port = 8080
        enabled = true
        tags = ["web", "api", "service"]
        config = {
            timeout = 30s
            retries = 3
        }
    }"#;

    group.throughput(Throughput::Bytes(simple_ucl.len() as u64));

    group.bench_function("parse_document", |b| {
        b.iter(|| {
            let mut parser = UclParser::new(black_box(simple_ucl));
            parser.parse_document()
        });
    });

    group.bench_function("parse_object", |b| {
        b.iter(|| {
            let mut parser = UclParser::new(black_box(simple_ucl));
            parser.parse_object()
        });
    });

    group.finish();
}

/// Benchmark nested structure parsing
fn bench_parser_nested(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser_nested");

    for depth in [3, 5, 7] {
        for breadth in [2, 4, 6] {
            let ucl_content = generate_nested_ucl(depth, breadth);
            let content_size = ucl_content.len();

            group.throughput(Throughput::Bytes(content_size as u64));

            group.bench_with_input(
                BenchmarkId::new("nested", format!("d{}_b{}", depth, breadth)),
                &ucl_content,
                |b, content| {
                    b.iter(|| {
                        let mut parser = UclParser::new(black_box(content));
                        parser.parse_document()
                    });
                },
            );
        }
    }

    group.finish();
}

/// Benchmark variable expansion
fn bench_variable_expansion(c: &mut Criterion) {
    let mut group = c.benchmark_group("variable_expansion");

    let variable_ucl = generate_variable_ucl();
    group.throughput(Throughput::Bytes(variable_ucl.len() as u64));

    // Setup variable handlers
    let mut env_vars = HashMap::new();
    env_vars.insert("API_KEY".to_string(), "secret-key-123".to_string());
    env_vars.insert("DB_PROTOCOL".to_string(), "postgresql".to_string());
    env_vars.insert("DB_USER".to_string(), "admin".to_string());
    env_vars.insert("DB_PASS".to_string(), "password".to_string());
    env_vars.insert("DB_HOST".to_string(), "localhost".to_string());
    env_vars.insert("DB_PORT".to_string(), "5432".to_string());
    env_vars.insert("DB_NAME".to_string(), "myapp".to_string());
    env_vars.insert("WEB_TIMEOUT".to_string(), "45s".to_string());
    env_vars.insert("WEB_WORKERS".to_string(), "8".to_string());

    group.bench_function("without_variables", |b| {
        b.iter(|| {
            let mut parser = UclParser::new(black_box(&variable_ucl));
            parser.parse_document()
        });
    });

    group.bench_function("with_map_handler", |b| {
        b.iter(|| {
            let handler = Box::new(MapVariableHandler::from_map(env_vars.clone()));
            let mut parser = UclParser::with_variable_handler(black_box(&variable_ucl), handler);
            parser.parse_document()
        });
    });

    group.bench_function("with_env_handler", |b| {
        b.iter(|| {
            let handler = Box::new(EnvironmentVariableHandler);
            let mut parser = UclParser::with_variable_handler(black_box(&variable_ucl), handler);
            parser.parse_document()
        });
    });

    group.finish();
}

/// Benchmark serde deserialization
fn bench_serde_deserialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("serde_deserialization");

    let simple_ucl = r#"{
        name = "test-app"
        port = 8080
        enabled = true
    }"#;

    let complex_ucl = r#"{
        services = {
            web = {
                url = "https://api.example.com/web"
                port = 8080
                config = {
                    timeout = "30s"
                    workers = 4
                }
            }
            api = {
                url = "https://api.example.com/api"
                port = 8081
                config = {
                    timeout = "60s"
                    rate_limit = 1000
                }
            }
        }
        features = {
            logging = {
                level = "info"
                output = "stdout"
            }
            monitoring = {
                enabled = true
                endpoint = "http://localhost:9090"
            }
        }
        metadata = {
            version = "1.0.0"
            author = "Test Author"
            created_at = "2024-01-01T00:00:00Z"
        }
    }"#;

    group.throughput(Throughput::Bytes(simple_ucl.len() as u64));

    group.bench_function("simple_struct", |b| {
        b.iter(|| {
            let result: Result<SimpleConfig, _> = from_str(black_box(simple_ucl));
            result
        });
    });

    group.throughput(Throughput::Bytes(complex_ucl.len() as u64));

    group.bench_function("complex_struct", |b| {
        b.iter(|| {
            let result: Result<ComplexConfig, _> = from_str(black_box(complex_ucl));
            result
        });
    });

    group.bench_function("deserializer_creation", |b| {
        b.iter(|| UclDeserializer::new(black_box(simple_ucl)));
    });

    group.finish();
}

/// Benchmark parser configuration impact
fn bench_parser_config(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser_config");

    let test_ucl = r#"{
        # This is a comment
        duplicate_key = "first"
        duplicate_key = "second"
        duplicate_key = "third"
        
        nested = {
            /* Multi-line
               comment */
            deep = {
                deeper = {
                    deepest = "value"
                }
            }
        }
    }"#;

    group.throughput(Throughput::Bytes(test_ucl.len() as u64));

    group.bench_function("default_config", |b| {
        b.iter(|| {
            let mut parser = UclParser::new(black_box(test_ucl));
            parser.parse_document()
        });
    });

    group.bench_function("no_duplicate_keys", |b| {
        b.iter(|| {
            let mut config = ParserConfig::default();
            config.allow_duplicate_keys = false;
            let mut parser = UclParser::new(black_box(test_ucl)).with_config(config);
            parser.parse_document()
        });
    });

    group.bench_function("max_depth_limited", |b| {
        b.iter(|| {
            let mut config = ParserConfig::default();
            config.max_depth = 10;
            let mut parser = UclParser::new(black_box(test_ucl)).with_config(config);
            parser.parse_document()
        });
    });

    group.bench_function("with_comments", |b| {
        b.iter(|| {
            let mut lexer_config = LexerConfig::default();
            lexer_config.save_comments = true;
            let mut parser = UclParser::with_lexer_config(black_box(test_ucl), lexer_config);
            parser.parse_document()
        });
    });

    group.finish();
}

/// Benchmark error handling performance
fn bench_error_handling(c: &mut Criterion) {
    let mut group = c.benchmark_group("error_handling");

    let invalid_ucl_samples = vec![
        r#"{ unterminated_string = "missing quote }"#,
        r#"{ invalid_number = 123.45.67 }"#,
        r#"{ unterminated_object = { missing_brace = true"#,
        r#"{ invalid_escape = "bad \q escape" }"#,
        r#"{ /* unterminated comment"#,
    ];

    for (i, invalid_ucl) in invalid_ucl_samples.iter().enumerate() {
        group.bench_with_input(
            BenchmarkId::new("parse_error", i),
            invalid_ucl,
            |b, content| {
                b.iter(|| {
                    let mut parser = UclParser::new(black_box(content));
                    let _ = parser.parse_document(); // Expect error
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("serde_error", i),
            invalid_ucl,
            |b, content| {
                b.iter(|| {
                    let _: Result<HashMap<String, serde_json::Value>, _> =
                        from_str(black_box(content));
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_parser_basic,
    bench_parser_nested,
    bench_variable_expansion,
    bench_serde_deserialization,
    bench_parser_config,
    bench_error_handling
);
criterion_main!(benches);

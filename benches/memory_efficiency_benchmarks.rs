use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use serde_json::Value;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use ucl_lexer::{LexerConfig, UclLexer, from_str};

/// Custom allocator to track memory usage during benchmarks
struct TrackingAllocator;

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static DEALLOCATED: AtomicUsize = AtomicUsize::new(0);
static PEAK_ALLOCATED: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe {
            let ptr = System.alloc(layout);
            if !ptr.is_null() {
                let size = layout.size();
                let current = ALLOCATED.fetch_add(size, Ordering::Relaxed) + size;
                let peak = PEAK_ALLOCATED.load(Ordering::Relaxed);
                if current > peak {
                    PEAK_ALLOCATED.store(current, Ordering::Relaxed);
                }
            }
            ptr
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe {
            System.dealloc(ptr, layout);
            DEALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        }
    }
}

#[global_allocator]
static GLOBAL: TrackingAllocator = TrackingAllocator;

/// Reset memory tracking counters
fn reset_memory_tracking() {
    ALLOCATED.store(0, Ordering::Relaxed);
    DEALLOCATED.store(0, Ordering::Relaxed);
    PEAK_ALLOCATED.store(0, Ordering::Relaxed);
}

/// Get current memory statistics
fn get_memory_stats() -> (usize, usize, usize) {
    (
        ALLOCATED.load(Ordering::Relaxed),
        DEALLOCATED.load(Ordering::Relaxed),
        PEAK_ALLOCATED.load(Ordering::Relaxed),
    )
}

/// Generate large UCL content for memory testing
fn generate_large_ucl_content(entries: usize) -> String {
    let mut content = String::with_capacity(entries * 200); // Pre-allocate to avoid measuring string growth
    content.push_str("{\n");

    for i in 0..entries {
        content.push_str(&format!(
            r#"
    entry_{} = {{
        id = {}
        name = "item-{}"
        description = "This is a test item with id {} for memory efficiency testing"
        active = {}
        priority = {}
        tags = ["tag-{}", "category-{}", "type-{}"]
        metadata = {{
            created_at = "2024-01-{}T10:{}:{}Z"
            updated_at = "2024-01-{}T15:{}:{}Z"
            version = "1.{}.{}"
            checksum = "sha256:abcdef{:08x}"
        }}
        config = {{
            timeout = "{}s"
            retries = {}
            batch_size = {}
            parallel = {}
            options = [
                "option-{}-1",
                "option-{}-2", 
                "option-{}-3"
            ]
        }}
    }}
"#,
            i,
            i,
            i,
            i,
            i % 2 == 0,
            i % 10,
            i % 20,
            i % 15,
            i % 8,
            1 + (i % 28),
            (i % 60),
            (i % 60),
            1 + (i % 28),
            (i % 60),
            (i % 60),
            i % 100,
            i % 50,
            i,
            5 + (i % 25),
            2 + (i % 7),
            50 + (i % 100),
            i % 2 == 0,
            i,
            i,
            i
        ));
    }

    content.push_str("}\n");
    content
}

/// Generate NGINX-style content for memory testing
fn generate_nginx_memory_test(servers: usize) -> String {
    let mut content = String::with_capacity(servers * 300);

    for i in 0..servers {
        content.push_str(&format!(
            r#"
server_{} {{
    listen {}
    server_name app{}.example.com
    root /var/www/app{}

    location / {{
        proxy_pass http://backend_{}
        proxy_timeout {}s
        proxy_retries {}
        proxy_set_header Host $host
        proxy_set_header X-Real-IP $remote_addr
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for
    }}

    location /api {{
        proxy_pass http://api_backend_{}
        proxy_timeout {}s
        proxy_connect_timeout {}s
        proxy_read_timeout {}s
    }}
}}

upstream backend_{} {{
    server 127.0.0.1:{} weight={}
    server 127.0.0.1:{} weight={}
    keepalive {}
}}
"#,
            i,
            8000 + i,
            i,
            i,
            i,
            10 + (i % 20),
            2 + (i % 5),
            i,
            15 + (i % 30),
            5 + (i % 10),
            60 + (i % 120),
            i,
            3000 + i * 2,
            1 + (i % 10),
            3001 + i * 2,
            1 + (i % 8),
            16 + (i % 32)
        ));
    }

    content
}

/// Generate content with many Unicode escapes for memory testing
fn generate_unicode_memory_test(strings: usize) -> String {
    let mut content = String::with_capacity(strings * 150);
    content.push_str("{\n");

    for i in 0..strings {
        content.push_str(&format!(
            r#"
    string_{} = "Unicode test \u{{1F600}} \u{{1F680}} \u{{2728}} with id {}"
    message_{} = "Hello \u{{1F44B}} World \u{{1F30D}} from string {}"
    emoji_{} = "\u{{1F389}} \u{{1F4AF}} \u{{1F525}} \u{{1F680}} \u{{2728}}"
"#,
            i, i, i, i, i
        ));
    }

    content.push_str("}\n");
    content
}

/// Benchmark memory usage during parsing
fn bench_memory_usage_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage_parsing");

    for &entries in &[100, 500, 1000, 2000] {
        let content = generate_large_ucl_content(entries);
        let content_size = content.len();

        group.throughput(Throughput::Bytes(content_size as u64));

        group.bench_with_input(
            BenchmarkId::new("large_object", entries),
            &content,
            |b, content| {
                b.iter_custom(|iters| {
                    reset_memory_tracking();
                    let start = std::time::Instant::now();

                    for _ in 0..iters {
                        let result: Result<Value, _> = from_str(black_box(content));
                        black_box(result);
                    }

                    let elapsed = start.elapsed();
                    let (allocated, deallocated, peak) = get_memory_stats();

                    // Print memory stats for analysis
                    eprintln!("Entries: {}, Allocated: {} bytes, Deallocated: {} bytes, Peak: {} bytes, Net: {} bytes", 
                        entries, allocated, deallocated, peak, allocated.saturating_sub(deallocated));

                    elapsed
                });
            },
        );
    }

    group.finish();
}

/// Benchmark memory usage with NGINX-style syntax
fn bench_nginx_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("nginx_memory_usage");

    for &servers in &[50, 100, 200, 400] {
        let nginx_content = generate_nginx_memory_test(servers);
        let content_size = nginx_content.len();

        group.throughput(Throughput::Bytes(content_size as u64));

        group.bench_with_input(
            BenchmarkId::new("nginx_servers", servers),
            &nginx_content,
            |b, content| {
                b.iter_custom(|iters| {
                    reset_memory_tracking();
                    let start = std::time::Instant::now();

                    for _ in 0..iters {
                        let result: Result<Value, _> = from_str(black_box(content));
                        black_box(result);
                    }

                    let elapsed = start.elapsed();
                    let (allocated, deallocated, peak) = get_memory_stats();

                    eprintln!("NGINX Servers: {}, Allocated: {} bytes, Deallocated: {} bytes, Peak: {} bytes, Net: {} bytes", 
                        servers, allocated, deallocated, peak, allocated.saturating_sub(deallocated));

                    elapsed
                });
            },
        );
    }

    group.finish();
}

/// Benchmark memory usage with Unicode escapes
fn bench_unicode_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("unicode_memory_usage");

    for &strings in &[100, 300, 500, 1000] {
        let unicode_content = generate_unicode_memory_test(strings);
        let content_size = unicode_content.len();

        group.throughput(Throughput::Bytes(content_size as u64));

        group.bench_with_input(
            BenchmarkId::new("unicode_strings", strings),
            &unicode_content,
            |b, content| {
                b.iter_custom(|iters| {
                    reset_memory_tracking();
                    let start = std::time::Instant::now();

                    for _ in 0..iters {
                        let result: Result<Value, _> = from_str(black_box(content));
                        black_box(result);
                    }

                    let elapsed = start.elapsed();
                    let (allocated, deallocated, peak) = get_memory_stats();

                    eprintln!("Unicode Strings: {}, Allocated: {} bytes, Deallocated: {} bytes, Peak: {} bytes, Net: {} bytes", 
                        strings, allocated, deallocated, peak, allocated.saturating_sub(deallocated));

                    elapsed
                });
            },
        );
    }

    group.finish();
}

/// Benchmark lexer memory efficiency
fn bench_lexer_memory_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer_memory_efficiency");

    let test_content = generate_large_ucl_content(500);

    group.throughput(Throughput::Bytes(test_content.len() as u64));

    // Test regular lexer
    group.bench_function("regular_lexer", |b| {
        b.iter_custom(|iters| {
            reset_memory_tracking();
            let start = std::time::Instant::now();

            for _ in 0..iters {
                let mut lexer = UclLexer::new(black_box(&test_content));
                let mut token_count = 0;
                while let Ok(token) = lexer.next_token() {
                    black_box(&token);
                    token_count += 1;
                    if matches!(token, ucl_lexer::Token::Eof) {
                        break;
                    }
                }
                black_box(token_count);
            }

            let elapsed = start.elapsed();
            let (allocated, deallocated, peak) = get_memory_stats();

            eprintln!("Regular Lexer - Allocated: {} bytes, Deallocated: {} bytes, Peak: {} bytes, Net: {} bytes", 
                allocated, deallocated, peak, allocated.saturating_sub(deallocated));

            elapsed
        });
    });

    // Test zero-copy lexer
    group.bench_function("zero_copy_lexer", |b| {
        b.iter_custom(|iters| {
            reset_memory_tracking();
            let start = std::time::Instant::now();

            for _ in 0..iters {
                let mut lexer = UclLexer::new(black_box(&test_content));
                let mut token_count = 0;
                while let Ok(token) = lexer.next_token() {
                    black_box(&token);
                    token_count += 1;
                    if matches!(token, ucl_lexer::Token::Eof) {
                        break;
                    }
                }
                black_box(token_count);
            }

            let elapsed = start.elapsed();
            let (allocated, deallocated, peak) = get_memory_stats();

            eprintln!("Zero-Copy Lexer - Allocated: {} bytes, Deallocated: {} bytes, Peak: {} bytes, Net: {} bytes", 
                allocated, deallocated, peak, allocated.saturating_sub(deallocated));

            elapsed
        });
    });

    group.finish();
}

/// Benchmark comment handling memory efficiency
fn bench_comment_memory_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("comment_memory_efficiency");

    let comment_heavy_content = format!(
        "{}\n{}\n{}\n{}",
        "// C++ style comment line".repeat(100),
        "# Hash comment line".repeat(100),
        "/* Multi-line comment block */".repeat(50),
        "key = \"value\"".repeat(200)
    );

    group.throughput(Throughput::Bytes(comment_heavy_content.len() as u64));

    // Without saving comments
    group.bench_function("without_saving_comments", |b| {
        b.iter_custom(|iters| {
            reset_memory_tracking();
            let start = std::time::Instant::now();

            for _ in 0..iters {
                let config = LexerConfig::default(); // save_comments = false by default
                let mut lexer = UclLexer::with_config(black_box(&comment_heavy_content), config);
                let mut token_count = 0;
                while let Ok(token) = lexer.next_token() {
                    black_box(&token);
                    token_count += 1;
                    if matches!(token, ucl_lexer::Token::Eof) {
                        break;
                    }
                }
                black_box(token_count);
            }

            let elapsed = start.elapsed();
            let (allocated, deallocated, peak) = get_memory_stats();

            eprintln!("Without Comments - Allocated: {} bytes, Deallocated: {} bytes, Peak: {} bytes, Net: {} bytes", 
                allocated, deallocated, peak, allocated.saturating_sub(deallocated));

            elapsed
        });
    });

    // With saving comments
    group.bench_function("with_saving_comments", |b| {
        b.iter_custom(|iters| {
            reset_memory_tracking();
            let start = std::time::Instant::now();

            for _ in 0..iters {
                let mut config = LexerConfig::default();
                config.save_comments = true;
                let mut lexer = UclLexer::with_config(black_box(&comment_heavy_content), config);
                let mut token_count = 0;
                while let Ok(token) = lexer.next_token() {
                    black_box(&token);
                    token_count += 1;
                    if matches!(token, ucl_lexer::Token::Eof) {
                        break;
                    }
                }
                black_box((token_count, lexer.comment_count()));
            }

            let elapsed = start.elapsed();
            let (allocated, deallocated, peak) = get_memory_stats();

            eprintln!("With Comments - Allocated: {} bytes, Deallocated: {} bytes, Peak: {} bytes, Net: {} bytes", 
                allocated, deallocated, peak, allocated.saturating_sub(deallocated));

            elapsed
        });
    });

    group.finish();
}

/// Benchmark error handling memory efficiency
fn bench_error_handling_memory(c: &mut Criterion) {
    let mut group = c.benchmark_group("error_handling_memory");

    let invalid_configs = vec![
        r#"{ unterminated_string = "missing quote }"#.repeat(50),
        r#"{ invalid_number = 123.45.67 }"#.repeat(50),
        r#"{ unterminated_object = { missing_brace = true"#.repeat(50),
        r#"{ invalid_escape = "bad \q escape" }"#.repeat(50),
    ];

    for (i, invalid_config) in invalid_configs.iter().enumerate() {
        group.throughput(Throughput::Bytes(invalid_config.len() as u64));

        group.bench_with_input(
            BenchmarkId::new("error_case", i),
            invalid_config,
            |b, content| {
                b.iter_custom(|iters| {
                    reset_memory_tracking();
                    let start = std::time::Instant::now();

                    for _ in 0..iters {
                        let result: Result<Value, _> = from_str(black_box(content));
                        black_box(result); // Should be an error
                    }

                    let elapsed = start.elapsed();
                    let (allocated, deallocated, peak) = get_memory_stats();

                    eprintln!("Error Case {}: Allocated: {} bytes, Deallocated: {} bytes, Peak: {} bytes, Net: {} bytes", 
                        i, allocated, deallocated, peak, allocated.saturating_sub(deallocated));

                    elapsed
                });
            },
        );
    }

    group.finish();
}

/// Benchmark string allocation optimization
fn bench_string_allocation_optimization(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_allocation_optimization");

    // Test with many bare words (should minimize allocations)
    let bare_word_config = (0..200)
        .map(|i| format!("key_{} value_{}", i, i))
        .collect::<Vec<_>>()
        .join("\n");

    // Test with many quoted strings (more allocations expected)
    let quoted_string_config = (0..200)
        .map(|i| format!("key_{} = \"value_{}\"", i, i))
        .collect::<Vec<_>>()
        .join("\n");

    group.throughput(Throughput::Bytes(bare_word_config.len() as u64));

    group.bench_function("bare_words", |b| {
        b.iter_custom(|iters| {
            reset_memory_tracking();
            let start = std::time::Instant::now();

            for _ in 0..iters {
                let result: Result<Value, _> = from_str(black_box(&bare_word_config));
                black_box(result);
            }

            let elapsed = start.elapsed();
            let (allocated, deallocated, peak) = get_memory_stats();

            eprintln!("Bare Words - Allocated: {} bytes, Deallocated: {} bytes, Peak: {} bytes, Net: {} bytes", 
                allocated, deallocated, peak, allocated.saturating_sub(deallocated));

            elapsed
        });
    });

    group.bench_function("quoted_strings", |b| {
        b.iter_custom(|iters| {
            reset_memory_tracking();
            let start = std::time::Instant::now();

            for _ in 0..iters {
                let result: Result<Value, _> = from_str(black_box(&quoted_string_config));
                black_box(result);
            }

            let elapsed = start.elapsed();
            let (allocated, deallocated, peak) = get_memory_stats();

            eprintln!("Quoted Strings - Allocated: {} bytes, Deallocated: {} bytes, Peak: {} bytes, Net: {} bytes", 
                allocated, deallocated, peak, allocated.saturating_sub(deallocated));

            elapsed
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_memory_usage_parsing,
    bench_nginx_memory_usage,
    bench_unicode_memory_usage,
    bench_lexer_memory_efficiency,
    bench_comment_memory_efficiency,
    bench_error_handling_memory,
    bench_string_allocation_optimization
);
criterion_main!(benches);

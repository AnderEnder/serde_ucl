//! Performance tests for UCL parsing
//!
//! These tests verify that the UCL parser performs well with various
//! types of input and scales appropriately with input size.

use std::io::Cursor;
use std::time::Instant;
use ucl_lexer::{UclLexer, from_str, streaming_lexer_from_reader};

#[test]
fn test_small_config_performance() {
    let small_config = r#"
        app = "test"
        port = 8080
        debug = true
        
        server {
            host = "localhost"
            timeout = 30s
        }
        
        features = ["auth", "logging"]
    "#;

    // Warm up
    for _ in 0..10 {
        let _: serde_json::Value = from_str(small_config).unwrap();
    }

    // Measure performance
    let iterations = 1000;
    let start = Instant::now();

    for _ in 0..iterations {
        let _: serde_json::Value = from_str(small_config).unwrap();
    }

    let duration = start.elapsed();
    let per_iteration = duration / iterations;

    println!(
        "Small config performance: {:?} per parse ({} iterations)",
        per_iteration, iterations
    );

    // Should be very fast for small configs
    assert!(
        per_iteration.as_micros() < 1000,
        "Small config parsing should be under 1ms"
    );
}

#[test]
fn test_medium_config_performance() {
    let medium_config = generate_config(100);

    // Warm up
    for _ in 0..5 {
        let _: serde_json::Value = from_str(&medium_config).unwrap();
    }

    // Measure performance
    let iterations = 100;
    let start = Instant::now();

    for _ in 0..iterations {
        let _: serde_json::Value = from_str(&medium_config).unwrap();
    }

    let duration = start.elapsed();
    let per_iteration = duration / iterations;
    let throughput = (medium_config.len() as f64 / 1_000_000.0) / per_iteration.as_secs_f64();

    println!(
        "Medium config performance: {:?} per parse, {:.2} MB/s throughput",
        per_iteration, throughput
    );

    // Should maintain reasonable performance
    assert!(
        per_iteration.as_millis() < 100,
        "Medium config parsing should be under 100ms"
    );
    assert!(
        throughput > 1.0,
        "Should achieve at least 1 MB/s throughput"
    );
}

#[test]
fn test_large_config_performance() {
    let large_config = generate_config(1000);

    // Single parse test
    let start = Instant::now();
    let _: serde_json::Value = from_str(&large_config).unwrap();
    let duration = start.elapsed();

    let throughput = (large_config.len() as f64 / 1_000_000.0) / duration.as_secs_f64();

    println!(
        "Large config performance: {:?} for {} bytes, {:.2} MB/s throughput",
        duration,
        large_config.len(),
        throughput
    );

    // Should complete in reasonable time
    assert!(
        duration.as_secs() < 5,
        "Large config parsing should complete within 5 seconds"
    );
    assert!(
        throughput > 0.5,
        "Should achieve at least 0.5 MB/s throughput for large files"
    );
}

#[test]
fn test_lexer_string_heavy_performance() {
    let config = generate_string_heavy_config(2000);

    // Warm-up and average multiple runs to reduce noise
    let iterations = 100;

    let mut total_duration = std::time::Duration::ZERO;
    let mut total_tokens = 0;

    for _ in 0..iterations {
        let start = Instant::now();
        let mut lexer = UclLexer::new(&config);
        let mut tokens = 0;

        while let Ok(token) = lexer.next_token() {
            tokens += 1;
            if matches!(token, ucl_lexer::Token::Eof) {
                break;
            }
        }
        total_duration += start.elapsed();
        total_tokens = tokens;
    }
    let avg_duration = total_duration / iterations as u32;
    let throughput =
        (config.len() * iterations as usize) as f64 / 1_000_000.0 / total_duration.as_secs_f64();

    println!("Config size: {} bytes", config.len());
    println!(
        "String-heavy parsing (avg of {} runs): {:?} ({} tokens, {:.2} MB/s)",
        iterations, avg_duration, total_tokens, throughput
    );

    println!(
        "Note: Zero-copy is always enabled, using borrowed strings when possible \
        to avoid allocations and improve performance."
    );

    // Should achieve reasonable throughput
    assert!(
        throughput > 200.0,
        "Should achieve at least 200 MB/s for string-heavy content (got {:.2} MB/s)",
        throughput
    );
}

#[test]
fn test_streaming_vs_regular_performance() {
    let config = generate_config(500);

    // Regular parsing
    let start = Instant::now();
    let _: serde_json::Value = from_str(&config).unwrap();
    let regular_duration = start.elapsed();

    // Streaming parsing (tokenization only)
    let start = Instant::now();
    let cursor = Cursor::new(config.as_bytes());
    let mut streaming_lexer = streaming_lexer_from_reader(cursor);
    let mut streaming_tokens = 0;

    while let Ok(token) = streaming_lexer.next_token() {
        streaming_tokens += 1;
        if matches!(token, ucl_lexer::Token::Eof) {
            break;
        }
    }
    let streaming_duration = start.elapsed();

    println!("Regular parsing: {:?}", regular_duration);
    println!(
        "Streaming tokenization: {:?} ({} tokens)",
        streaming_duration, streaming_tokens
    );

    let regular_throughput = (config.len() as f64 / 1_000_000.0) / regular_duration.as_secs_f64();
    let streaming_throughput =
        (config.len() as f64 / 1_000_000.0) / streaming_duration.as_secs_f64();

    println!("Regular throughput: {:.2} MB/s", regular_throughput);
    println!("Streaming throughput: {:.2} MB/s", streaming_throughput);

    // Streaming should maintain good throughput
    assert!(
        streaming_throughput > 0.5,
        "Streaming should achieve at least 0.5 MB/s"
    );
}

#[test]
fn test_memory_usage_scaling() {
    // Test that memory usage scales reasonably with input size
    let sizes = vec![50, 100, 200, 500];

    for size in sizes {
        let config = generate_config(size);

        // Measure parsing time
        let start = Instant::now();
        let _: serde_json::Value = from_str(&config).unwrap();
        let duration = start.elapsed();

        let throughput = (config.len() as f64 / 1_000_000.0) / duration.as_secs_f64();

        println!(
            "Size {}: {} bytes, {:?}, {:.2} MB/s",
            size,
            config.len(),
            duration,
            throughput
        );

        // Performance should not degrade dramatically with size
        assert!(
            throughput > 0.1,
            "Should maintain at least 0.1 MB/s throughput"
        );
    }
}

#[test]
fn test_deeply_nested_performance() {
    let deeply_nested = generate_deeply_nested_config(10, 3);

    let start = Instant::now();
    let _: serde_json::Value = from_str(&deeply_nested).unwrap();
    let duration = start.elapsed();

    println!(
        "Deeply nested config: {} bytes, {:?}",
        deeply_nested.len(),
        duration
    );

    // Should handle deep nesting without stack overflow or excessive time
    assert!(
        duration.as_secs() < 5,
        "Deeply nested parsing should complete within 5 seconds"
    );
}

#[test]
fn test_string_heavy_performance() {
    let string_heavy = generate_string_heavy_config(1000);

    let start = Instant::now();
    let _: serde_json::Value = from_str(&string_heavy).unwrap();
    let duration = start.elapsed();

    let throughput = (string_heavy.len() as f64 / 1_000_000.0) / duration.as_secs_f64();

    println!(
        "String-heavy config: {} bytes, {:?}, {:.2} MB/s",
        string_heavy.len(),
        duration,
        throughput
    );

    // Should handle string-heavy content efficiently
    assert!(
        throughput > 0.5,
        "Should achieve at least 0.5 MB/s for string-heavy content"
    );
}

#[test]
fn test_number_heavy_performance() {
    let number_heavy = generate_number_heavy_config(500);

    let start = Instant::now();
    let _: serde_json::Value = from_str(&number_heavy).unwrap();
    let duration = start.elapsed();

    let throughput = (number_heavy.len() as f64 / 1_000_000.0) / duration.as_secs_f64();

    println!(
        "Number-heavy config: {} bytes, {:?}, {:.2} MB/s",
        number_heavy.len(),
        duration,
        throughput
    );

    // Should handle number parsing efficiently
    // Cargo.toml sets opt-level=2 for test profile to ensure adequate performance
    assert!(
        throughput > 0.05,
        "Should achieve at least 0.05 MB/s for number-heavy content"
    );
}

// Helper functions to generate test configurations

fn generate_config(items: usize) -> String {
    let mut config = String::new();

    for i in 0..items {
        config.push_str(&format!(
            r#"item_{} {{
    id = {}
    name = "Item {}"
    enabled = {}
    timeout = {}s
    memory = {}mb
    tags = ["tag-{}", "category-{}"]
    metadata {{
      created = {}
      version = "1.{}.0"
    }}
  }}
"#,
            i,
            i,
            i,
            if i % 2 == 0 { "true" } else { "false" },
            10 + (i % 50),
            64 + (i % 256),
            i % 10,
            i % 5,
            1600000000 + i,
            i % 100
        ));
    }

    config
}

fn generate_string_heavy_config(items: usize) -> String {
    let mut config = String::new();

    for i in 0..items {
        config.push_str(&format!(
            r#"string_{} {{
    short_string = "value_{}"
    medium_string = "This is a medium length string for item {} with some additional content to make it longer"
    long_string = "This is a very long string for item {} that contains a lot of text and should test the string parsing performance of the UCL lexer. It includes various characters and should be representative of real-world string content that might appear in configuration files."
    path_string = "/very/long/path/to/some/file/or/directory/structure/item_{}/config.json"
    url_string = "https://api.example.com/v1/items/{}/details?param1=value1&param2=value2&param3=value3"
  }}
"#,
            i, i, i, i, i, i
        ));
    }

    config
}

fn generate_number_heavy_config(items: usize) -> String {
    let mut config = String::new();

    for i in 0..items {
        let exp = (i % 10) as i32 - 5;
        config.push_str(&format!(
            r#"numbers_{} {{
    integer = {}
    float = {}.{}
    negative = -{}
    large = {}
    memory_mb = {}mb
    size_kb = {}kb
    duration_s = {}s
    hex = 0x{:X}
    scientific = {}e{}
  }}
"#,
            i,
            i,
            i,
            i % 1000,
            i * 2,
            i * 1000000,
            256 + (i % 768),
            64 + (i % 192),
            30 + (i % 120),
            i,
            i,
            exp
        ));
    }

    config
}

fn generate_deeply_nested_config(depth: usize, width: usize) -> String {
    fn generate_level(current_depth: usize, max_depth: usize, width: usize) -> String {
        if current_depth >= max_depth {
            return r#""leaf_value""#.to_string();
        }

        let mut level = String::from("{\n");

        for i in 0..width {
            level.push_str(&format!(
                r#"    level_{}_{} = {}
"#,
                current_depth,
                i,
                generate_level(current_depth + 1, max_depth, width)
            ));
        }

        level.push_str("  }");
        level
    }

    format!("root = {}\n", generate_level(0, depth, width))
}

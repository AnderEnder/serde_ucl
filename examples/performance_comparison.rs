use std::io::Cursor;
use std::time::Instant;
use ucl_lexer::{Token, UclLexer, streaming_lexer_from_reader};

fn main() {
    println!("UCL Rust Lexer Performance Comparison");
    println!("=====================================");

    // Generate test content
    let small_content = generate_test_content(100); // ~1KB
    let medium_content = generate_test_content(1000); // ~10KB
    let large_content = generate_test_content(10000); // ~100KB

    println!("\nTest Content Sizes:");
    println!("  Small:  {} bytes", small_content.len());
    println!("  Medium: {} bytes", medium_content.len());
    println!("  Large:  {} bytes", large_content.len());

    // Test different configurations
    run_comparison("Small Content", &small_content);
    run_comparison("Medium Content", &medium_content);
    run_comparison("Large Content", &large_content);

    // Test zero-copy effectiveness
    test_zero_copy_effectiveness();
}

fn generate_test_content(items: usize) -> String {
    let mut content = String::from("{\n");
    for i in 0..items {
        content.push_str(&format!(
            r#"  item_{} = {{
    name = "item-{}"
    value = {}
    enabled = {}
    tags = ["tag-{}", "category-{}"]
    config = {{
      timeout = {}s
      retries = {}
    }}
  }}
"#,
            i,
            i,
            i * 42,
            i % 2 == 0,
            i % 10,
            i % 5,
            10 + (i % 20),
            2 + (i % 5)
        ));
    }
    content.push_str("}\n");
    content
}

fn run_comparison(label: &str, content: &str) {
    println!("\n{}", label);
    println!("{}", "=".repeat(label.len()));

    // Regular lexer (zero-copy is always enabled)
    let start = Instant::now();
    let token_count = tokenize_regular(content);
    let regular_duration = start.elapsed();

    // Streaming lexer
    let start = Instant::now();
    let streaming_token_count = tokenize_streaming(content);
    let streaming_duration = start.elapsed();

    println!(
        "  Regular Lexer:    {:>8} tokens in {:>8.2?} ({:.2} MB/s)",
        token_count,
        regular_duration,
        (content.len() as f64 / 1_000_000.0) / regular_duration.as_secs_f64()
    );

    println!(
        "  Streaming Lexer:  {:>8} tokens in {:>8.2?} ({:.2} MB/s)",
        streaming_token_count,
        streaming_duration,
        (content.len() as f64 / 1_000_000.0) / streaming_duration.as_secs_f64()
    );

    // Calculate streaming overhead
    let streaming_overhead = streaming_duration.as_secs_f64() / regular_duration.as_secs_f64();

    println!("  Streaming Overhead: {:.2}x", streaming_overhead);
    println!("  Note: Zero-copy optimization is always enabled automatically");
}

fn tokenize_regular(content: &str) -> usize {
    let mut lexer = UclLexer::new(content);
    let mut count = 0;

    while let Ok(token) = lexer.next_token() {
        count += 1;
        if matches!(token, Token::Eof) {
            break;
        }
    }

    count
}

fn tokenize_streaming(content: &str) -> usize {
    let cursor = Cursor::new(content.as_bytes());
    let mut lexer = streaming_lexer_from_reader(cursor);
    let mut count = 0;

    while let Ok(token) = lexer.next_token() {
        count += 1;
        if matches!(token, Token::Eof) {
            break;
        }
    }

    count
}

fn test_zero_copy_effectiveness() {
    println!("\nZero-Copy Effectiveness Test");
    println!("============================");

    // Content that should benefit from zero-copy (no escapes, no variables)
    let optimal_content = r#"{
        simple_key = "simple_value"
        another_key = "another_value"
        third_key = "third_value"
    }"#;

    // Content that cannot use zero-copy (has escapes)
    let suboptimal_content = r#"{
        escaped_key = "value with\nescapes\tand\rmore"
        variable_key = "value with ${VARIABLE} reference"
        complex_key = "mixed\tcontent\nwith $VAR"
    }"#;

    println!("  Testing optimal content (should use zero-copy):");
    let (borrowed, owned) = count_cow_types(optimal_content);
    println!("    Borrowed strings: {}", borrowed);
    println!("    Owned strings:    {}", owned);
    println!(
        "    Zero-copy rate:   {:.1}%",
        (borrowed as f64 / (borrowed + owned) as f64) * 100.0
    );

    println!("  Testing suboptimal content (should allocate):");
    let (borrowed, owned) = count_cow_types(suboptimal_content);
    println!("    Borrowed strings: {}", borrowed);
    println!("    Owned strings:    {}", owned);
    println!(
        "    Zero-copy rate:   {:.1}%",
        (borrowed as f64 / (borrowed + owned) as f64) * 100.0
    );
}

fn count_cow_types(content: &str) -> (usize, usize) {
    use std::borrow::Cow;

    let mut lexer = UclLexer::new(content);
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
}

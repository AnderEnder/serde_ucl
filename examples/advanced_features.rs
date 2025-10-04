//! Advanced Features Demonstration
//!
//! This example showcases the advanced features of the UCL lexer including
//! variable expansion, custom handlers, streaming parsing, and error handling.

use serde::Deserialize;
use std::collections::HashMap;
use std::io::Cursor;
use ucl_lexer::{
    ChainedVariableHandler, EnvironmentVariableHandler, Position, Token, UclError, UclLexer,
    VariableContext, VariableHandler, from_str, from_str_with_variables, streaming_lexer_from_reader,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Advanced UCL Features Demonstration");
    println!("===================================\n");

    // Example 1: Advanced variable expansion
    demo_advanced_variables()?;

    // Example 2: Custom variable handlers
    demo_custom_variable_handlers()?;

    // Example 3: Streaming parsing for large files
    demo_streaming_parsing()?;

    // Example 4: Error handling and diagnostics
    demo_error_handling()?;

    // Example 5: Zero-copy parsing optimization
    demo_zero_copy_parsing()?;

    Ok(())
}

fn demo_advanced_variables() -> Result<(), Box<dyn std::error::Error>> {
    println!("1. Advanced Variable Expansion");
    println!("------------------------------");

    #[derive(Debug, Deserialize)]
    struct ServiceConfig {
        name: String,
        database_url: String,
        redis_url: String,
        api_keys: HashMap<String, String>,
        endpoints: Vec<String>,
        timeouts: TimeoutConfig,
    }

    #[derive(Debug, Deserialize)]
    struct TimeoutConfig {
        connect: f64,
        read: f64,
        write: f64,
    }

    let config_text = r#"
        # Service configuration with complex variable expansion
        name = "${SERVICE_NAME:-my-service}"
        
        # Database URL with multiple variable substitutions
        database_url = "postgresql://${DB_USER:-app}:${DB_PASS:-secret}@${DB_HOST:-localhost}:${DB_PORT:-5432}/${DB_NAME:-${SERVICE_NAME}}"
        
        # Redis URL with fallback
        redis_url = "${REDIS_URL:-redis://${REDIS_HOST:-localhost}:${REDIS_PORT:-6379}/${REDIS_DB:-0}}"
        
        # API keys from environment
        api_keys {
            stripe = "${STRIPE_API_KEY}"
            sendgrid = "${SENDGRID_API_KEY}"
            aws = "${AWS_ACCESS_KEY_ID}"
        }
        
        # Endpoints with variable expansion
        endpoints = [
            "${API_BASE_URL:-http://localhost:8080}/health",
            "${API_BASE_URL}/metrics",
            "${API_BASE_URL}/api/v1"
        ]
        
        # Timeouts with environment overrides
        timeouts {
            connect = ${CONNECT_TIMEOUT:-5}s
            read = ${READ_TIMEOUT:-30}s
            write = ${WRITE_TIMEOUT:-10}s
        }
    "#;

    // Set up environment variables for demonstration
    unsafe {
        std::env::set_var("SERVICE_NAME", "advanced-demo");
        std::env::set_var("DB_HOST", "prod-database.internal");
        std::env::set_var("REDIS_HOST", "cache.internal");
        std::env::set_var("API_BASE_URL", "https://api.myservice.com");
        std::env::set_var("STRIPE_API_KEY", "sk_test_...");
        std::env::set_var("SENDGRID_API_KEY", "SG...");
        std::env::set_var("AWS_ACCESS_KEY_ID", "AKIA...");
    }

    let config: ServiceConfig =
        from_str_with_variables(config_text, Box::new(EnvironmentVariableHandler))?;

    println!("Parsed configuration with variable expansion:");
    println!("  Service name: {}", config.name);
    println!("  Database URL: {}", config.database_url);
    println!("  Redis URL: {}", config.redis_url);
    println!("  API keys: {} configured", config.api_keys.len());
    println!("  Endpoints: {:?}", config.endpoints);
    println!(
        "  Timeouts: connect={}s, read={}s, write={}s",
        config.timeouts.connect, config.timeouts.read, config.timeouts.write
    );

    println!();
    Ok(())
}

fn demo_custom_variable_handlers() -> Result<(), Box<dyn std::error::Error>> {
    println!("2. Custom Variable Handlers");
    println!("---------------------------");

    // Custom variable handler that provides computed values
    struct ComputedVariableHandler {
        base_values: HashMap<String, String>,
    }

    impl ComputedVariableHandler {
        fn new() -> Self {
            let mut base_values = HashMap::new();
            base_values.insert(
                "TIMESTAMP".to_string(),
                chrono::Utc::now().timestamp().to_string(),
            );
            base_values.insert(
                "HOSTNAME".to_string(),
                gethostname::gethostname().to_string_lossy().to_string(),
            );
            base_values.insert("PID".to_string(), std::process::id().to_string());

            Self { base_values }
        }
    }

    impl VariableHandler for ComputedVariableHandler {
        fn resolve_variable(&self, name: &str) -> Option<String> {
            match name {
                "RANDOM_ID" => Some(format!("id_{}", rand::random::<u32>())),
                "CURRENT_DIR" => std::env::current_dir()
                    .ok()
                    .and_then(|p| p.to_str().map(|s| s.to_string())),
                "MEMORY_LIMIT" => {
                    // Simulate getting system memory info
                    Some("8gb".to_string())
                }
                "CPU_COUNT" => Some(num_cpus::get().to_string()),
                _ => self.base_values.get(name).cloned(),
            }
        }

        fn resolve_variable_with_context(
            &self,
            name: &str,
            context: &VariableContext,
        ) -> Option<String> {
            match name {
                "CONTEXT_PATH" => Some(context.current_object_path.join(".")),
                "LINE_NUMBER" => Some(context.position.line.to_string()),
                _ => self.resolve_variable(name),
            }
        }
    }

    // Configuration using custom variables
    let config_text = r#"
        # Configuration with computed variables
        app {
            name = "dynamic-app"
            instance_id = "${RANDOM_ID}"
            hostname = "${HOSTNAME}"
            pid = ${PID}
            started_at = ${TIMESTAMP}
        }
        
        system {
            cpu_count = ${CPU_COUNT}
            memory_limit = "${MEMORY_LIMIT}"
            working_directory = "${CURRENT_DIR}"
        }
        
        metadata {
            config_path = "${CONTEXT_PATH}"
            config_line = ${LINE_NUMBER}
        }
    "#;

    #[derive(Debug, Deserialize)]
    struct DynamicConfig {
        app: AppMetadata,
        system: SystemInfo,
        metadata: ConfigMetadata,
    }

    #[derive(Debug, Deserialize)]
    struct AppMetadata {
        name: String,
        instance_id: String,
        hostname: String,
        pid: u32,
        started_at: i64,
    }

    #[derive(Debug, Deserialize)]
    struct SystemInfo {
        cpu_count: u32,
        memory_limit: String,
        working_directory: String,
    }

    #[derive(Debug, Deserialize)]
    struct ConfigMetadata {
        config_path: String,
        config_line: u32,
    }

    // Chain custom handler with environment handler
    let handler = ChainedVariableHandler::from_handlers(vec![
        Box::new(ComputedVariableHandler::new()),
        Box::new(EnvironmentVariableHandler),
    ]);

    let config: DynamicConfig = from_str_with_variables(config_text, Box::new(handler))?;

    println!("Configuration with custom variables:");
    println!(
        "  App: {} (instance: {})",
        config.app.name, config.app.instance_id
    );
    println!("  Host: {} (PID: {})", config.app.hostname, config.app.pid);
    println!("  Started at: {}", config.app.started_at);
    assert!(config.app.started_at >= 0);
    println!(
        "  System: {} CPUs, {} memory",
        config.system.cpu_count, config.system.memory_limit
    );
    println!("  Working dir: {}", config.system.working_directory);
    println!(
        "  Metadata: {} at line {}",
        config.metadata.config_path, config.metadata.config_line
    );

    println!();
    Ok(())
}

fn demo_streaming_parsing() -> Result<(), Box<dyn std::error::Error>> {
    println!("3. Streaming Parsing for Large Files");
    println!("------------------------------------");

    // Generate a large configuration file in memory
    let large_config = generate_large_config(1000);
    println!("Generated config size: {} bytes", large_config.len());

    // Parse using streaming lexer
    let cursor = Cursor::new(large_config.as_bytes());
    let mut lexer = streaming_lexer_from_reader(cursor);

    let mut token_count = 0;
    let mut string_tokens = 0;
    let mut number_tokens = 0;
    let mut object_tokens = 0;

    let start_time = std::time::Instant::now();

    while let Ok(token) = lexer.next_token() {
        match &token {
            Token::String { .. } => string_tokens += 1,
            Token::Integer(_) | Token::Float(_) | Token::Time(_) => number_tokens += 1,
            Token::ObjectStart | Token::ObjectEnd => object_tokens += 1,
            Token::Eof => break,
            _ => {}
        }
        token_count += 1;
    }

    let parse_time = start_time.elapsed();

    println!("Streaming parse results:");
    println!("  Total tokens: {}", token_count);
    println!("  String tokens: {}", string_tokens);
    println!("  Number tokens: {}", number_tokens);
    println!("  Object tokens: {}", object_tokens);
    println!("  Parse time: {:?}", parse_time);
    println!(
        "  Throughput: {:.2} MB/s",
        (large_config.len() as f64 / 1_000_000.0) / parse_time.as_secs_f64()
    );

    println!();
    Ok(())
}

fn generate_large_config(items: usize) -> String {
    let mut config = String::from("{\n");

    for i in 0..items {
        config.push_str(&format!(
            r#"  "item_{}" = {{
    "id" = {}
    "name" = "Item Number {}"
    "enabled" = {}
    "weight" = {}kg
    "timeout" = {}s
    "tags" = ["tag-{}", "category-{}"]
    "metadata" = {{
      "created_at" = {}
      "version" = "1.{}.0"
      "description" = "This is item number {} with some longer description text"
    }}
  }}
"#,
            i,
            i,
            i,
            i % 2 == 0,
            (i % 100) + 1,
            (i % 60) + 1,
            i % 10,
            i % 5,
            1600000000 + i,
            i % 100,
            i
        ));
    }

    config.push_str("}\n");
    config
}

fn demo_error_handling() -> Result<(), Box<dyn std::error::Error>> {
    println!("4. Error Handling and Diagnostics");
    println!("---------------------------------");

    let test_cases = vec![
        ("Unterminated string", r#"key = "unterminated string"#),
        ("Invalid escape sequence", r#"key = "invalid \q escape""#),
        ("Invalid number format", r#"key = 123.456.789"#),
        (
            "Unterminated comment",
            r#"key = "value" /* unterminated comment"#,
        ),
        ("Invalid Unicode escape", r#"key = "invalid \uXYZ unicode""#),
    ];

    for (description, invalid_ucl) in test_cases {
        println!("Testing: {}", description);

        match from_str::<serde_json::Value>(invalid_ucl) {
            Ok(_) => println!("  Unexpected success!"),
            Err(error) => {
                println!("  Error: {}", error);

                // Extract position information if available
                match &error {
                    UclError::Lex(lex_error) => {
                        if let Some(pos) = extract_position_from_error(lex_error) {
                            println!("  Position: line {}, column {}", pos.line, pos.column);

                            // Show context around error
                            show_error_context(invalid_ucl, pos);
                        }
                    }
                    _ => {}
                }
            }
        }
        println!();
    }

    Ok(())
}

fn extract_position_from_error(error: &ucl_lexer::LexError) -> Option<Position> {
    // Touch the error so the parameter isn't unused
    debug_assert!(!error.to_string().is_empty() || error.to_string().is_empty());
    // This would extract position from the actual error type
    // For demonstration, we'll return a mock position
    Some(Position {
        line: 1,
        column: 10,
        offset: 10,
    })
}

fn show_error_context(input: &str, position: Position) {
    let lines: Vec<&str> = input.lines().collect();
    if position.line > 0 && position.line <= lines.len() {
        let line = lines[position.line - 1];
        println!("  Context: {}", line);
        println!(
            "           {}^",
            " ".repeat(position.column.saturating_sub(1))
        );
    }
}

fn demo_zero_copy_parsing() -> Result<(), Box<dyn std::error::Error>> {
    println!("5. Zero-Copy Parsing Optimization");
    println!("---------------------------------");

    let config_text = r#"
        # Configuration optimized for zero-copy parsing
        simple_key = "simple_value"
        another_key = "another_value"
        numeric_key = 42
        boolean_key = true
        
        nested {
            inner_key = "inner_value"
            list = ["item1", "item2", "item3"]
        }
    "#;

    // Regular parsing
    let start = std::time::Instant::now();
    let mut regular_lexer = UclLexer::new(config_text);
    let mut regular_tokens = Vec::new();

    while let Ok(token) = regular_lexer.next_token() {
        if matches!(token, Token::Eof) {
            break;
        }
        regular_tokens.push(token);
    }
    let regular_time = start.elapsed();

    println!(
        "Parsing time: {:?} ({} tokens)",
        regular_time,
        regular_tokens.len()
    );
    println!("Note: Zero-copy is always enabled automatically");

    // Count borrowed vs owned strings (zero-copy is automatic)
    let mut borrowed_count = 0;
    let mut owned_count = 0;

    for token in &regular_tokens {
        if let Token::String { value, .. } = token {
            match value {
                std::borrow::Cow::Borrowed(_) => borrowed_count += 1,
                std::borrow::Cow::Owned(_) => owned_count += 1,
            }
        }
    }

    println!("  String allocation analysis:");
    println!("    Borrowed (zero-copy): {}", borrowed_count);
    println!("    Owned (allocated): {}", owned_count);

    if borrowed_count + owned_count > 0 {
        let zero_copy_rate =
            (borrowed_count as f64 / (borrowed_count + owned_count) as f64) * 100.0;
        println!("    Zero-copy rate: {:.1}%", zero_copy_rate);
    }

    println!();
    Ok(())
}

// Helper modules for dependencies that might not be available
mod chrono {
    pub struct Utc;
    impl Utc {
        pub fn now() -> DateTime {
            DateTime
        }
    }

    pub struct DateTime;
    impl DateTime {
        pub fn timestamp(&self) -> i64 {
            1640995200 // 2022-01-01 00:00:00 UTC
        }
    }
}

mod gethostname {
    use std::ffi::OsString;

    pub fn gethostname() -> OsString {
        OsString::from("localhost")
    }
}

mod rand {
    pub fn random<T>() -> T
    where
        T: From<u32>,
    {
        T::from(42) // Fixed value for demonstration
    }
}

mod num_cpus {
    pub fn get() -> usize {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    }
}

# UCL Rust Lexer Examples

This directory contains comprehensive examples demonstrating the features and capabilities of the UCL Rust lexer. Each example focuses on different aspects of UCL parsing and integration patterns.

## Quick Start

To run any example:

```bash
cargo run --example <example_name>
```

For example:
```bash
cargo run --example basic_usage
cargo run --example web_server_config
```

## Examples Overview

### 1. Basic Usage (`basic_usage.rs`)

**Purpose**: Introduction to UCL parsing with serde integration

**Features Demonstrated**:
- Simple struct deserialization
- Basic UCL syntax (objects, arrays, primitives)
- Error handling basics
- Serde derive macro usage

**Use Case**: Getting started with UCL in your Rust application

```rust
#[derive(Deserialize)]
struct Config {
    server: ServerConfig,
    database: DatabaseConfig,
}

let config: Config = from_str(ucl_text)?;
```

### 2. Web Server Configuration (`web_server_config.rs`)

**Purpose**: Real-world web server configuration management

**Features Demonstrated**:
- Complex nested configuration structures
- Environment-specific configuration loading
- Variable expansion with environment variables
- Multiple configuration sources
- Production-ready configuration patterns

**Use Case**: Configuring web applications with different deployment environments

**Key Highlights**:
- SSL/TLS configuration
- Database connection pooling
- Middleware configuration (CORS, rate limiting, compression)
- Logging and monitoring setup
- Feature flags

### 3. Framework Integration (`framework_integration.rs`)

**Purpose**: Integration with popular Rust frameworks and libraries

**Features Demonstrated**:
- Axum web framework configuration
- Tokio runtime configuration
- API service configuration
- Configuration composition patterns
- Framework-specific best practices

**Use Case**: Integrating UCL with existing Rust web frameworks

**Frameworks Covered**:
- Axum (web framework)
- Tokio (async runtime)
- Serde JSON (API responses)
- Custom configuration composition

### 4. Advanced Features (`advanced_features.rs`)

**Purpose**: Showcase advanced UCL parsing capabilities

**Features Demonstrated**:
- Complex variable expansion patterns
- Custom variable handlers
- Streaming parsing for large files
- Comprehensive error handling
- Zero-copy parsing optimizations
- Performance benchmarking

**Use Case**: High-performance applications with complex configuration needs

**Advanced Topics**:
- Custom `VariableHandler` implementations
- Context-aware variable resolution
- Memory-efficient parsing strategies
- Error diagnostics and reporting

### 5. Configuration Management (`configuration_management.rs`)

**Purpose**: Enterprise-grade configuration management patterns

**Features Demonstrated**:
- Environment-specific configuration loading
- Configuration validation
- Configuration merging and overrides
- Hot configuration reloading simulation
- Multi-source configuration composition

**Use Case**: Large applications with complex deployment requirements

**Management Patterns**:
- Base + environment override pattern
- User configuration customization
- Command-line argument integration
- Runtime configuration updates

### 6. Number Parsing (`number_parsing.rs`)

**Purpose**: Demonstrate rich number format support

**Features Demonstrated**:
- Integer and floating-point parsing
- Hexadecimal number support
- Time suffixes (s, min, h, d, w, y)
- Size suffixes (k, m, g, kb, mb, gb)
- Special values (inf, -inf, nan)
- Binary vs decimal multiplier configuration

**Use Case**: Configuration files with human-readable quantities

### 7. Performance Comparison (`performance_comparison.rs`)

**Purpose**: Performance analysis and optimization

**Features Demonstrated**:
- Regular vs zero-copy parsing comparison
- Streaming parser for large files
- Memory allocation analysis
- Throughput benchmarking
- Zero-copy effectiveness measurement

**Use Case**: Performance-critical applications

### 8. Extensibility Demo (`extensibility_demo.rs`)

**Purpose**: Custom parsing extensions and plugins

**Features Demonstrated**:
- Custom parsing hooks
- Plugin system usage
- Built-in plugin examples
- Configuration validation plugins
- String post-processing

**Use Case**: Applications requiring domain-specific parsing logic

### 9. Complete UCL Syntax (`complete_ucl_syntax.rs`)

**Purpose**: Comprehensive demonstration of all UCL syntax features

**Features Demonstrated**:
- NGINX-style implicit syntax (key value, key { ... })
- C++ style comments (//) alongside hash (#) and multi-line (/* */)
- Extended Unicode escapes (\u{...}) with emoji and international text
- Bare word values without quotes
- Implicit arrays through key repetition
- Mixed syntax styles in same configuration
- Improved heredoc with whitespace handling
- Special values (null, inf, -inf, nan)
- Boolean keywords (true/false, yes/no, on/off)

**Use Case**: Understanding complete UCL specification compliance

### 10. NGINX-Style Framework Integration (`nginx_style_framework_integration.rs`)

**Purpose**: Framework integration using NGINX-style UCL syntax

**Features Demonstrated**:
- Axum web framework configuration with implicit syntax
- Tokio runtime configuration
- Database and Redis configuration
- Security and middleware setup
- Microservices architecture configuration
- API gateway routing with NGINX-style syntax

**Use Case**: Modern Rust web applications with natural configuration syntax

### 11. Real-World Configurations (`real_world_configurations.rs`)

**Purpose**: Production deployment scenarios with complete UCL syntax

**Features Demonstrated**:
- Kubernetes infrastructure configuration
- Monitoring and alerting setup (Prometheus, Grafana)
- CI/CD pipeline configuration
- Multi-environment deployment
- Load balancing and networking
- Storage and persistent volumes

**Use Case**: Production infrastructure and DevOps configurations

## Running Examples

### Prerequisites

Make sure you have Rust installed and the project dependencies:

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build the project
git clone <repository-url>
cd ucl-rust-lexer
cargo build --examples
```

### Running Individual Examples

```bash
# Basic usage example
cargo run --example basic_usage

# Web server configuration
cargo run --example web_server_config

# Framework integration
cargo run --example framework_integration

# Advanced features
cargo run --example advanced_features

# Configuration management
cargo run --example configuration_management

# Number parsing demonstration
cargo run --example number_parsing

# Performance comparison
cargo run --example performance_comparison

# Extensibility features
cargo run --example extensibility_demo

# Complete UCL syntax demonstration
cargo run --example complete_ucl_syntax

# NGINX-style framework integration
cargo run --example nginx_style_framework_integration

# Real-world production configurations
cargo run --example real_world_configurations
```

### Running with Environment Variables

Some examples use environment variables for demonstration:

```bash
# Set environment variables for variable expansion examples
export SERVICE_NAME="my-service"
export DB_HOST="database.example.com"
export JWT_SECRET="your-secret-key"

# Run examples that use environment variables
cargo run --example web_server_config
cargo run --example advanced_features
```

## Example Configuration Files

The examples include various UCL configuration formats:

### Basic Object Syntax
```ucl
server {
    host = "localhost"
    port = 8080
    debug = true
}
```

### Array Syntax
```ucl
features = ["auth", "logging", "metrics"]
endpoints = [
    "http://api.example.com/v1",
    "http://api.example.com/v2"
]
```

### Variable Expansion
```ucl
database_url = "postgresql://${DB_USER}:${DB_PASS}@${DB_HOST}:5432/${DB_NAME}"
app_name = "${SERVICE_NAME:-default-service}"
```

### Rich Number Formats
```ucl
max_memory = 512mb
timeout = 30s
cache_size = 1gb
retry_delay = 500ms
```

### Multiple String Formats
```ucl
json_string = "Hello\nWorld"
single_quoted = 'Raw string with\nliteral backslashes'
heredoc = <<EOF
This is a multiline
heredoc string
EOF
```

### Comments (All Three Styles Supported)
```ucl
# Hash-style comments (traditional)
server {
    host = "localhost"  # End-of-line comment
    
    // C++ style comments
    port = 8080  // Also end-of-line
    
    /*
     * Multi-line comment
     * with nesting support
     */
    max_connections = 100
}
```

### NGINX-Style Implicit Syntax
```ucl
// Web server configuration with implicit syntax
server {
    listen 80
    listen 443 ssl
    server_name example.com www.example.com
    
    root /var/www/html
    index index.html index.php
    
    location / {
        try_files $uri $uri/ =404
    }
}

// Load balancer configuration
upstream backend {
    server 127.0.0.1:3000 weight=3
    server 127.0.0.1:3001 weight=2
    keepalive 32
}
```

### Extended Unicode Escapes
```ucl
unicode_examples {
    // Variable-length Unicode escapes
    emoji_party "\u{1F389} \u{1F38A} \u{1F381}"
    
    // Mixed escape formats
    mixed_formats "\u0041\u{42}\u0043\u{1F600}"  // "ABC😀"
    
    // International characters
    chinese "你好 \u{4E16}\u{754C}"              // "你好 世界"
}
```

### Bare Word Values and Implicit Arrays
```ucl
# Bare word values (no quotes needed)
environment production
worker_processes auto
debug yes

# Implicit arrays through key repetition
server 127.0.0.1:3000
server 127.0.0.1:3001
server 127.0.0.1:3002

# Boolean keywords
ssl_enabled true
maintenance_mode false
compression on
cache_disabled off
```

## Integration Patterns

### Serde Integration
```rust
use serde::Deserialize;
use ucl_lexer::from_str;

#[derive(Deserialize)]
struct Config {
    // Your configuration fields
}

let config: Config = from_str(ucl_text)?;
```

### Variable Handlers
```rust
use ucl_lexer::{from_str_with_variables, EnvironmentVariableHandler};

let config = from_str_with_variables(
    ucl_text,
    Box::new(EnvironmentVariableHandler)
)?;
```

### Custom Variable Handlers
```rust
use ucl_lexer::{VariableHandler, MapVariableHandler, ChainedVariableHandler};

let custom_vars = HashMap::from([
    ("APP_NAME".to_string(), "my-app".to_string()),
]);

let handler = ChainedVariableHandler::new(vec![
    Box::new(MapVariableHandler::new(custom_vars)),
    Box::new(EnvironmentVariableHandler),
]);

let config = from_str_with_variables(ucl_text, Box::new(handler))?;
```

### Zero-Copy Parsing
```rust
use ucl_lexer::{UclLexer, LexerConfig};

let config = LexerConfig {
    zero_copy: true,
    ..Default::default()
};

let mut lexer = UclLexer::with_config(input, config);
```

### Streaming Parsing
```rust
use ucl_lexer::streaming_lexer_from_file;
use std::fs::File;

let file = File::open("large_config.ucl")?;
let mut lexer = streaming_lexer_from_file(file)?;

while let Ok(token) = lexer.next_token() {
    // Process tokens with constant memory usage
}
```

## Best Practices

### Configuration Structure
1. **Use nested objects** for logical grouping
2. **Leverage variable expansion** for environment-specific values
3. **Include validation** at the application level
4. **Document configuration options** with comments

### Performance Optimization
1. **Enable zero-copy mode** for large configurations
2. **Use streaming parsing** for very large files
3. **Cache parsed configurations** when possible
4. **Profile memory usage** in production

### Error Handling
1. **Provide clear error messages** with context
2. **Validate configuration** at startup
3. **Handle missing optional fields** gracefully
4. **Log configuration loading** for debugging

### Security Considerations
1. **Protect sensitive values** in environment variables
2. **Validate input sources** before parsing
3. **Sanitize file paths** in configuration
4. **Use secure defaults** for optional settings

## Troubleshooting

### Common Issues

1. **Parsing Errors**: Check UCL syntax, especially string quoting and object braces
2. **Variable Expansion**: Ensure environment variables are set or provide defaults
3. **Type Mismatches**: Verify serde Deserialize implementations match UCL structure
4. **Performance Issues**: Consider zero-copy mode or streaming for large files

### Debug Tips

1. **Enable debug logging** to see parsing steps
2. **Use smaller test configurations** to isolate issues
3. **Check error positions** for syntax problems
4. **Validate JSON equivalents** to verify structure

## Contributing

To add new examples:

1. Create a new `.rs` file in the `examples/` directory
2. Follow the existing naming convention
3. Include comprehensive documentation
4. Add the example to this README
5. Test with `cargo run --example <name>`

## Further Reading

- [UCL Specification](https://github.com/vstakhov/libucl)
- [Serde Documentation](https://serde.rs/)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Configuration Management Patterns](https://12factor.net/config)
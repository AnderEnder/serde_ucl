//! # UCL Rust Lexer
//!
//! A high-performance UCL (Universal Configuration Language) lexer and parser with seamless serde integration.
//!
//! ## Overview
//!
//! This crate provides a complete implementation of the UCL format parser that integrates
//! directly with Rust's serde ecosystem. UCL is a human-readable configuration format
//! that combines the best features of JSON, YAML, and NGINX configuration syntax.
//!
//! ## Key Features
//!
//! - **Serde Integration**: Use `#[derive(Deserialize)]` with UCL files
//! - **Multiple String Formats**: JSON-style, single-quoted, and heredoc strings
//! - **Rich Number Parsing**: Support for size suffixes (1kb, 2mb) and time suffixes (30s, 5min)
//! - **Variable Expansion**: Environment variables and custom variable handlers
//! - **Nested Comments**: Both single-line (#) and multi-line (/* */) with nesting support
//! - **Zero-Copy Parsing**: Efficient parsing with minimal allocations
//! - **Extensible**: Custom parsing hooks and plugin system
//! - **Streaming Support**: Parse large files with constant memory usage
//!
//! ## Quick Start
//!
//! Add this to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! ucl-lexer = "0.1"
//! serde = { version = "1.0", features = ["derive"] }
//! ```
//!
//! ## Basic Usage
//!
//! ```rust
//! use serde::Deserialize;
//! use ucl_lexer::from_str;
//!
//! #[derive(Debug, Deserialize)]
//! struct ServerConfig {
//!     name: String,
//!     port: u16,
//!     debug: bool,
//! }
//!
//! let ucl_text = r#"
//!     name = "my-server"
//!     port = 8080
//!     debug = true
//! "#;
//!
//! let config: ServerConfig = from_str(ucl_text)?;
//! println!("Server {} running on port {}", config.name, config.port);
//! # Ok::<(), ucl_lexer::UclError>(())
//! ```
//!
//! ## Advanced Features
//!
//! ### Variable Expansion
//!
//! ```rust
//! use ucl_lexer::{from_str_with_variables, EnvironmentVariableHandler};
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct Config {
//!     database_url: String,
//!     port: u16,
//! }
//!
//! let ucl_text = r#"
//!     database_url = "${DATABASE_URL}"
//!     port = 8080
//! "#;
//!
//! // Use environment variables
//! let config: Config = from_str_with_variables(
//!     ucl_text,
//!     Box::new(EnvironmentVariableHandler)
//! )?;
//! # Ok::<(), ucl_lexer::UclError>(())
//! ```
//!
//! ### Custom Variable Handlers
//!
//! ```rust
//! use ucl_lexer::{VariableHandler, MapVariableHandler, from_str_with_variables};
//! use std::collections::HashMap;
//!
//! let mut handler = MapVariableHandler::new();
//! handler.insert("APP_NAME".to_string(), "my-app".to_string());
//! handler.insert("VERSION".to_string(), "1.0.0".to_string());
//!
//! let ucl_text = r#"
//!     name = "${APP_NAME}"
//!     version = "${VERSION}"
//! "#;
//!
//! let result: serde_json::Value = from_str_with_variables(ucl_text, Box::new(handler))?;
//! # Ok::<(), ucl_lexer::UclError>(())
//! ```
//!
//! ### Rich Number Formats
//!
//! ```rust
//! use serde::Deserialize;
//! use ucl_lexer::from_str;
//!
//! #[derive(Deserialize)]
//! struct Config {
//!     max_memory: u64,    // Will parse "512mb" as bytes
//!     timeout: f64,       // Will parse "30s" as seconds
//!     cache_size: u64,    // Will parse "1gb" as bytes
//! }
//!
//! let ucl_text = r#"
//!     max_memory = 512mb
//!     timeout = 30s
//!     cache_size = 1gb
//! "#;
//!
//! let config: Config = from_str(ucl_text)?;
//! assert_eq!(config.max_memory, 512 * 1024 * 1024);
//! assert_eq!(config.timeout, 30.0);
//! # Ok::<(), ucl_lexer::UclError>(())
//! ```
//!
//! ### Multiple String Formats
//!
//! ```rust
//! use serde::Deserialize;
//! use ucl_lexer::from_str;
//!
//! #[derive(Deserialize)]
//! struct Config {
//!     json_style: String,
//!     single_quoted: String,
//!     heredoc: String,
//! }
//!
//! let ucl_text = r#"
//!     json_style = "Hello\nWorld"
//!     single_quoted = 'Raw string with\nliteral backslashes'
//!     heredoc = <<EOF
//! This is a multiline
//! heredoc string that preserves
//! formatting and whitespace.
//! EOF
//! "#;
//!
//! let config: Config = from_str(ucl_text)?;
//! # Ok::<(), ucl_lexer::UclError>(())
//! ```
//!
//! ## Performance Features
//!
//! ### Zero-Copy Parsing
//!
//! ```rust
//! use ucl_lexer::UclLexer;
//!
//! let mut lexer = UclLexer::new("key = 'value'");
//! // Zero-copy optimization is always enabled automatically
//! // Strings will reference the original input when possible (no escapes/variables)
//! ```
//!
//! ### Streaming for Large Files
//!
//! ```rust,no_run
//! use ucl_lexer::streaming_lexer_from_file;
//!
//! let mut lexer = streaming_lexer_from_file("large_config.ucl")?;
//!
//! // Process tokens with constant memory usage
//! while let Ok(token) = lexer.next_token() {
//!     // Process token...
//!     # break; // For example purposes
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ## Error Handling
//!
//! The crate provides detailed error information with source location:
//!
//! ```rust
//! use ucl_lexer::{from_str, UclError, LexError};
//!
//! let invalid_ucl = r#"
//!     key = "unterminated string
//! "#;
//!
//! match from_str::<serde_json::Value>(invalid_ucl) {
//!     Err(UclError::Lex(lex_error)) => {
//!         // Extract position from the specific error variant
//!         let pos = match &lex_error {
//!             LexError::UnterminatedString { position } => position,
//!             _ => panic!("Expected UnterminatedString error"),
//!         };
//!         println!("Lexical error at line {}, column {}: {}",
//!                  pos.line, pos.column, lex_error);
//!     }
//!     Err(other) => println!("Other error: {}", other),
//!     Ok(_) => unreachable!(),
//! }
//! ```
//!
//! ## Migration from Other Formats
//!
//! ### From JSON
//!
//! UCL is largely compatible with JSON syntax:
//!
//! ```json
//! {
//!   "name": "my-app",
//!   "port": 8080,
//!   "features": ["auth", "logging"]
//! }
//! ```
//!
//! Can be written in UCL as:
//!
//! ```ucl
//! name = "my-app"
//! port = 8080
//! features = ["auth", "logging"]
//! ```
//!
//! ### From YAML
//!
//! YAML configurations can be converted to UCL:
//!
//! ```yaml
//! server:
//!   host: localhost
//!   port: 8080
//! database:
//!   url: postgres://localhost/mydb
//! ```
//!
//! Becomes:
//!
//! ```ucl
//! server {
//!   host = "localhost"
//!   port = 8080
//! }
//! database {
//!   url = "postgres://localhost/mydb"
//! }
//! ```
//!
//! ## Feature Flags
//!
//! - `zero-copy`: Enable zero-copy string parsing optimizations
//! - `save-comments`: Preserve comments during parsing
//! - `strict-unicode`: Enforce strict Unicode validation
//!
//! ## Examples
//!
//! See the `examples/` directory for more comprehensive examples:
//!
//! - `basic_usage.rs`: Simple configuration parsing
//! - `extensibility_demo.rs`: Custom parsing hooks and plugins
//! - `performance_comparison.rs`: Performance benchmarking
//! - `number_parsing.rs`: Rich number format examples

pub mod c_libucl_compatibility;
pub mod deserializer;
pub mod error;
pub mod lexer;
pub mod parser;

#[cfg(test)]
mod error_tests;

// Re-export main types and functions
pub use deserializer::{UclDeserializer, from_str, from_str_with_variables};
pub use error::{LexError, ParseError, UclError};
pub use lexer::{
    LexerConfig, StreamingUclLexer, StringFormat, Token, UclLexer, streaming_lexer_from_file,
    streaming_lexer_from_reader,
};
pub use parser::{DuplicateKeyBehavior, ParserConfig, UclArray, UclObject, UclParser, UclValue};

// Re-export position types
pub use error::{Position, Span};

// Re-export variable handler types
pub use parser::{
    ChainedVariableHandler, EnvironmentVariableHandler, MapVariableHandler, VariableContext,
    VariableHandler,
};

// Re-export custom parsing hooks
pub use parser::{NumberSuffixHandler, ParsingHooks, StringPostProcessor, ValidationHook};

// Re-export plugin system
pub use parser::{PluginConfig, PluginRegistry, UclParserBuilder, UclPlugin};

// Re-export example plugins
pub use parser::{ConfigValidationPlugin, CssUnitsPlugin, PathProcessingPlugin};

// Re-export example implementations
pub use parser::{CustomUnitSuffixHandler, PathNormalizationProcessor, SchemaValidationHook};

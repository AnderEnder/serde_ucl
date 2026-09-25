//! # UCL for Rust
//!
//! A reader and writer for UCL (Universal Configuration Language), the configuration format of
//! libucl, with serde integration. The target is compatibility with libucl, defined by behaviour:
//! the conformance suite in `tests/conformance/` and the behaviour spec in `docs/spec/`.
//!
//! ## Reading configuration
//!
//! ```rust
//! use serde::Deserialize;
//!
//! #[derive(Debug, Deserialize)]
//! struct Config {
//!     name: String,
//!     port: u16,
//!     timeout: f64,
//!     max_body: u64,
//!     upstream: Vec<String>,
//! }
//!
//! let text = r#"
//!     name = "my-server"
//!     port = 8080
//!     # A time, read as seconds.
//!     timeout = 30s
//!     # A multiplier: 512 * 1024.
//!     max_body = 512kb
//!     # A repeated key holds several values.
//!     upstream = a.example
//!     upstream = b.example
//! "#;
//!
//! let config: Config = ucl_lexer::from_str(text)?;
//! assert_eq!(config.port, 8080);
//! assert_eq!(config.timeout, 30.0);
//! assert_eq!(config.max_body, 512 * 1024);
//! assert_eq!(config.upstream, ["a.example", "b.example"]);
//! # Ok::<(), ucl_lexer::UclError>(())
//! ```
//!
//! [`from_str`], [`from_slice`], [`from_reader`] and [`from_file`] parse with a default
//! [`parse::Parser`]: no parser flags, no registered variables, and the macros of the format
//! enabled. Text input has no file access: an `.include` in a string finds no file. Only
//! [`from_file`] reads the filesystem, for the file and for its includes, whose relative paths
//! resolve against the file's directory. [`de`] describes this and how UCL values map onto
//! serde.
//!
//! ## Parser settings
//!
//! Parser flags, duplicate-key strategies, priorities, variables, a variable handler and the
//! loader that include macros read files from are settings of [`parse::Parser`], set with its
//! setters or with [`parse::ParserBuilder`]. A parser reads files only through a loader that
//! holds them, such as [`parse::FsLoader`]. Parse to a [`UclValue`], then deserialize it with
//! [`from_value`]:
//!
//! ```rust
//! use serde::Deserialize;
//! use ucl_lexer::parse::ParserBuilder;
//! use ucl_lexer::{DuplicateStrategy, from_value};
//!
//! #[derive(Deserialize)]
//! struct Config {
//!     url: String,
//!     workers: u32,
//! }
//!
//! let mut parser = ParserBuilder::new()
//!     .with_strategy(DuplicateStrategy::Rewrite)
//!     .with_variable("HOST", "example.org")
//!     .build();
//! let value = parser.parse(b"url = \"https://$HOST/\"\nworkers = 2\nworkers = 8")?;
//! let config: Config = from_value(value)?;
//! assert_eq!(config.url, "https://example.org/");
//! assert_eq!(config.workers, 8);
//! # Ok::<(), ucl_lexer::UclError>(())
//! ```
//!
//! ## Errors
//!
//! Every function returns [`UclError`]. A document the parser rejects is
//! [`UclError::Syntax`], whose [`parse::Error`] has an [`parse::ErrorKind`] and the
//! [`Position`] where the error was found:
//!
//! ```rust
//! use ucl_lexer::UclError;
//! use ucl_lexer::parse::ErrorKind;
//!
//! let err = ucl_lexer::from_str::<serde_json::Value>("a = 1\nb = \"open").unwrap_err();
//! let UclError::Syntax(e) = err else { panic!("{err}") };
//! assert_eq!(e.kind(), &ErrorKind::UnterminatedString);
//! assert_eq!((e.position().line, e.position().column), (2, 5));
//! ```
//!
//! ## Writing
//!
//! [`to_string`] writes any `Serialize` value in the UCL config format; [`to_json_string`],
//! [`to_json_string_compact`] and [`to_yaml_string`] write the other output formats, and
//! [`to_writer`] writes the config format to an `io::Write`. The output reads back, in libucl and
//! in this crate, as exactly the value written, floats included; the JSON output is valid JSON, in
//! which a time is its number of seconds and reads back as a float. See [`ser`] for the forms used
//! and the values that have none. [`to_value`] and [`from_value`] convert between Rust values and
//! the [`UclValue`] tree. [`emit`] writes a parsed value as libucl writes it.
//!
//! ```rust
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, PartialEq, Serialize, Deserialize)]
//! struct Limits {
//!     name: String,
//!     ratio: f64,
//! }
//!
//! let limits = Limits { name: "$HOME/db".into(), ratio: 0.1 };
//! let text = ucl_lexer::to_string(&limits)?;
//! assert_eq!(text, "name = '$HOME/db';\nratio = 0.1;\n");
//! let back: Limits = ucl_lexer::from_str(&text)?;
//! assert_eq!(back, limits);
//! # Ok::<(), ucl_lexer::UclError>(())
//! ```
//!
//! ## Cargo features
//!
//! - `fs` (default): the filesystem loader [`parse::FsLoader`] and [`from_file`].
//! - `load` (off by default): the `.load` macro (spec §9.6).
//! - `std` (default), `save-comments` and `strict-unicode` have no effect.

pub mod de;
pub mod emit;
pub mod error;
pub mod parse;
pub mod ser;
pub mod time;
pub mod value;

#[cfg(test)]
mod error_tests;

pub use de::{
    UclDeserializer, from_reader, from_slice, from_str, from_str_with_env, from_str_with_map,
    from_str_with_variables, from_value,
};

#[cfg(feature = "fs")]
pub use de::from_file;
pub use error::{Position, UclError};
pub use ser::{
    to_json_string, to_json_string_compact, to_string, to_value, to_writer, to_yaml_string,
};
pub use value::{
    DuplicateKeyError, DuplicateStrategy, Entry, ParserFlags, Placement, Slot, UclArray, UclObject,
    UclValue, Values,
};

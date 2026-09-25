//! # UCL for Rust
//!
//! A reader and writer for UCL (Universal Configuration Language), the configuration format of
//! libucl, with serde integration. The crate reads and writes UCL as libucl does: the
//! repository's conformance suite compares its results, and its output in every format, with
//! libucl's own. [COMPATIBILITY.md] lists the places where the crate deliberately differs, and
//! the `README.md` of the repository describes the crate at more length.
//!
//! [COMPATIBILITY.md]: https://github.com/AnderEnder/ucl-rust-lexer/blob/HEAD/docs/COMPATIBILITY.md
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
//! Parser flags, duplicate-key strategies, priorities, variables, a variable handler, the
//! loader that include macros read files from, their search directories and an input size limit
//! are settings of [`parse::Parser`], set with its setters or with [`parse::ParserBuilder`]. A
//! parser reads files only through a loader that holds them, such as [`parse::FsLoader`]. There
//! is no input limit by default, as in libucl; see [`parse::Parser::set_max_input_bytes`]. Parse
//! to a [`UclValue`], then deserialize it with [`from_value`]:
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
//! ## Several inputs and custom macros
//!
//! A parser can read several inputs into one result, each with its own priority and duplicate
//! strategy ([`parse::Parser::inputs`], [`parse::Input`]; spec §13.1). Defaults can so be
//! layered under a user's file: at a higher priority the user's values win, and under `merge`
//! objects combine.
//!
//! ```rust
//! use ucl_lexer::parse::{Input, MemoryLoader, ParserBuilder};
//! use ucl_lexer::DuplicateStrategy;
//!
//! let mut files = MemoryLoader::new();
//! files.add_file("/usr/share/app/app.conf", "workers = 4\nlog { level = info }\n");
//! files.add_file("/etc/app/app.conf", "workers = 16\nlog { file = /var/log/app.log }\n");
//! let mut parser = ParserBuilder::new().with_loader(files).build();
//! let mut inputs = parser.inputs();
//! inputs.add(Input::file("/usr/share/app/app.conf"))?;
//! inputs.add(
//!     Input::file("/etc/app/app.conf")
//!         .with_priority(1)
//!         .with_strategy(DuplicateStrategy::Merge),
//! )?;
//! let value = inputs.finish()?;
//! let root = value.as_object().unwrap();
//! assert_eq!(root["workers"].as_integer(), Some(16));
//! let log = root["log"].as_object().unwrap();
//! assert_eq!(log["level"].as_str(), Some("info"));
//! assert_eq!(log["file"].as_str(), Some("/var/log/app.log"));
//! # Ok::<(), ucl_lexer::parse::Error>(())
//! ```
//!
//! An application can also register macros of its own ([`parse::ParserBuilder::with_macro`];
//! spec §13.2). A handler adds entries where the macro stands, has text parsed in place of the
//! macro, stops the parse silently or fails with a message ([`parse::MacroCall`],
//! [`parse::MacroError`]):
//!
//! ```rust
//! use ucl_lexer::parse::{MacroError, ParserBuilder};
//! use ucl_lexer::UclValue;
//!
//! let mut parser = ParserBuilder::new()
//!     // `.version`: adds the application's version where the macro stands.
//!     .with_macro("version", |call| {
//!         call.add("version", UclValue::String("1.4.2".into()))
//!     })
//!     // `.feature NAME`: the settings of a named feature, parsed in place.
//!     .with_macro("feature", |call| match call.value_str() {
//!         Some("tls") => call.parse("tls { enabled = true; port = 443 }"),
//!         _ => Err(MacroError::new("unknown feature")),
//!     })
//!     .build();
//! let value = parser.parse(b".version {}\nserver { .feature tls\n}")?;
//! let root = value.as_object().unwrap();
//! assert_eq!(root["version"].as_str(), Some("1.4.2"));
//! let tls = root["server"].as_object().unwrap()["tls"].as_object().unwrap();
//! assert_eq!(tls["port"].as_integer(), Some(443));
//! # Ok::<(), ucl_lexer::parse::Error>(())
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
//! A document that parses but does not fit the target type is [`UclError::Deserialize`]. Its
//! [`error::DeserializeError`] has serde's error, the path of the value it is about, and where
//! that value was written, in the document or in the included file it came from, also when the
//! duplicate rules merged or moved it or `.inherit` copied it. [`UclError::position`] and
//! [`UclError::file`] give the position of both kinds:
//!
//! ```rust
//! #[derive(Debug, serde::Deserialize)]
//! struct Config {
//!     #[allow(dead_code)]
//!     port: u16,
//! }
//!
//! let err = ucl_lexer::from_str::<Config>("# the port\nport = http").unwrap_err();
//! let position = err.position().unwrap();
//! assert_eq!((position.line, position.column), (2, 8));
//! assert_eq!(
//!     err.to_string(),
//!     "Deserialization error: invalid type: string \"http\", expected u16 at port (line 2, column 8)"
//! );
//! ```
//!
//! The text functions find the path and position only when deserialization fails, by parsing and
//! deserializing a second time, so they cost nothing while it succeeds. To use other parser
//! settings and keep positions, deserialize with [`UclDeserializer::from_parser`], which records
//! paths as it goes. [`from_value`] records nothing: its errors have neither a path nor a
//! position. [`de`] describes this in full.
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
//! - `load` (off by default): the `.load` macro (spec §9.6). Without it, `.load` fails with an
//!   "unsupported" error.

pub mod de;
pub mod emit;
pub mod error;
mod handoff;
pub mod parse;
pub mod ser;
pub mod time;
pub mod value;

#[cfg(test)]
mod error_tests;

// The code examples of README.md run as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

pub use de::{
    MAX_SERDE_NESTING, UclDeserializer, from_reader, from_slice, from_str, from_str_with_env,
    from_str_with_map, from_str_with_variables, from_value,
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

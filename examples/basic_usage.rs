//! Basic usage: UCL text into Rust structs with serde.
//!
//! Run with `cargo run --example basic_usage`.

use serde::Deserialize;
use ucl_lexer::parse::ErrorKind;
use ucl_lexer::{UclError, from_str};

#[derive(Debug, Deserialize)]
struct AppConfig {
    server: ServerConfig,
    database: DatabaseConfig,
}

#[derive(Debug, Deserialize)]
struct ServerConfig {
    name: String,
    port: u16,
    debug: bool,
    listen: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DatabaseConfig {
    host: String,
    port: u16,
    username: String,
    password: String,
}

const CONFIG: &str = r#"
# Application configuration
server {
    name = "my-app";
    port = 8080;
    debug = true;
    listen = ["127.0.0.1", "::1"];
}

# Separators and quotes are optional: `key value` and unquoted strings work too.
database {
    host localhost
    port 5432
    username = admin
    password = "s3cret"
}
"#;

fn main() -> Result<(), UclError> {
    let config: AppConfig = from_str(CONFIG)?;
    println!("{config:#?}");

    assert_eq!(config.server.name, "my-app");
    assert_eq!(config.server.port, 8080);
    assert!(config.server.debug);
    assert_eq!(config.server.listen, ["127.0.0.1", "::1"]);
    assert_eq!(config.database.host, "localhost");
    assert_eq!(config.database.port, 5432);
    assert_eq!(config.database.username, "admin");
    assert_eq!(config.database.password.len(), 6);

    // A document the parser rejects gives an error with the kind and position of the problem.
    let broken = "server {\n    name = \"my-app\"\n    port = 8080\n";
    let err = from_str::<AppConfig>(broken).unwrap_err();
    println!("error: {err}");
    let parse_error = err.parse_error().expect("a parse error");
    assert_eq!(parse_error.kind(), &ErrorKind::UnterminatedObject);
    let position = err.position().expect("parse errors have a position");
    println!("at line {}, column {}", position.line, position.column);

    // A document that parses but does not fit the struct is a deserialization error, with the
    // path and the position of the value it is about: here the object that lacks a field.
    let err = from_str::<AppConfig>("server { name = x }").unwrap_err();
    println!("error: {err}");
    assert!(matches!(err, UclError::Deserialize(_)));
    let position = err
        .position()
        .expect("deserialization errors have a position");
    assert_eq!((position.line, position.column), (1, 8));

    Ok(())
}

//! Configuration management: layered sources, per-environment variables, validation and
//! reloading.
//!
//! 1. Layers. Built-in defaults, a site file and command-line overrides are combined in one
//!    parse: `main.conf` includes each layer with a higher priority and `duplicate="merge"`, so
//!    sections merge and the value of the highest priority wins (spec §8.3, §8.4, §9.4). An
//!    optional layer is included with `try=true`. The files come from a `MemoryLoader`.
//! 2. Environments. Registered variables (`$ENVIRONMENT`, `$DB_HOST`) fill in per-environment
//!    values; a variable handler supplies secrets from the process environment, with a fallback
//!    for names it knows. UCL has no `${NAME:-default}` form; the handler is where defaults go.
//! 3. Validation. A document the parser rejects gives the error's kind and position; serde
//!    reports missing fields and values out of range; checks that span fields run on the struct.
//! 4. Reloading. A changed document is parsed again and compared with the running one, both as
//!    typed structs and as value trees.
//!
//! Run with `cargo run --example configuration_management`.

use indexmap::IndexMap;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;
use ucl_lexer::emit::{Emitter, Format};
use ucl_lexer::parse::{ErrorKind, MemoryLoader, ParserBuilder};
use ucl_lexer::{UclError, UclValue, from_str, from_value};

#[derive(Debug, Deserialize, PartialEq)]
struct ApplicationConfig {
    app: AppConfig,
    server: ServerConfig,
    database: DatabaseConfig,
    logging: LoggingConfig,
    #[serde(default)]
    feature_flags: IndexMap<String, bool>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct AppConfig {
    name: String,
    version: String,
    environment: String,
    debug: bool,
    /// Bytes: `10mb` is 10 * 2^20.
    max_request_size: u64,
}

#[derive(Debug, Deserialize, PartialEq)]
struct ServerConfig {
    host: String,
    port: u16,
    #[serde(default)]
    tls: Option<TlsConfig>,
    timeouts: TimeoutConfig,
    max_connections: u32,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TlsConfig {
    cert_file: String,
    key_file: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TimeoutConfig {
    #[serde(with = "ucl_lexer::time")]
    read: Duration,
    #[serde(with = "ucl_lexer::time")]
    write: Duration,
    #[serde(with = "ucl_lexer::time")]
    idle: Duration,
}

#[derive(Debug, Deserialize, PartialEq)]
struct DatabaseConfig {
    host: String,
    port: u16,
    name: String,
    user: String,
    password: String,
    min_connections: u32,
    max_connections: u32,
    /// A repeated `replica` key; none, one or several.
    #[serde(default, rename = "replica")]
    replicas: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct LoggingConfig {
    level: String,
    format: String,
    /// Arrays merge by appending, so each layer can add outputs.
    outputs: Vec<String>,
}

/// Built-in defaults: every field has a value here.
const DEFAULTS: &str = r#"
app {
    name = "my-application";
    version = "1.0.0";
    environment = "$ENVIRONMENT";
    debug = false;
    max_request_size = 10mb;
}
server {
    host = "127.0.0.1";
    port = 8080;
    timeouts { read = 30s; write = 30s; idle = 2min; }
    max_connections = 1000;
}
database {
    host = "$DB_HOST";
    port = 5432;
    name = "myapp";
    user = "app";
    # A secret: resolved by the variable handler, never written in the file.
    password = "${DB_PASSWORD}";
    min_connections = 5;
    max_connections = 20;
}
logging {
    level = "$LOG_LEVEL";
    format = json;
    outputs = [stdout];
}
"#;

/// The site's file, per environment.
fn site_file(environment: &str) -> &'static str {
    match environment {
        "development" => {
            r#"
            app { debug = true; }
            server { host = localhost; }
            logging { format = pretty; outputs = ["file:./logs/dev.log"]; }
            "#
        }
        "staging" => {
            r#"
            server { host = "0.0.0.0"; max_connections = 5000; }
            database { max_connections = 50; replica = "staging-replica.internal"; }
            "#
        }
        _ => {
            r#"
            server {
                host = "0.0.0.0";
                port = 443;
                max_connections = 10000;
                tls { cert_file = "/etc/ssl/certs/app.crt"; key_file = "/etc/ssl/private/app.key"; }
            }
            database {
                max_connections = 100;
                replica = "replica1.internal";
                replica = "replica2.internal";
            }
            logging { outputs = ["syslog"]; }
            feature_flags { new_checkout = false; enhanced_logging = true; }
            "#
        }
    }
}

/// The entry point: each layer at a higher priority than the one before (spec §9.4). The
/// optional `local.conf` does not exist here; `try=true` skips it.
const MAIN: &str = r#"
.include "defaults.conf"
.include(priority=1, duplicate="merge") "site.conf"
.include(priority=2, duplicate="merge", try=true) "local.conf"
.include(priority=3, duplicate="merge") "overrides.conf"
"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Configuration management");
    println!("========================\n");
    layered_environments()?;
    validation();
    reloading()?;
    Ok(())
}

/// Loads the configuration of `environment`, with `overrides` as the command-line layer.
fn load(environment: &str, overrides: &str) -> Result<ApplicationConfig, UclError> {
    let mut files = MemoryLoader::new();
    files
        .add_file("/etc/myapp/main.conf", MAIN)
        .add_file("/etc/myapp/defaults.conf", DEFAULTS)
        .add_file("/etc/myapp/site.conf", site_file(environment))
        .add_file("/etc/myapp/overrides.conf", overrides);

    let (db_host, log_level) = match environment {
        "development" => ("localhost", "debug"),
        "staging" => ("staging-db.internal", "info"),
        _ => ("prod-db.internal", "warn"),
    };
    // Secrets the handler falls back to when the process environment does not set them.
    let fallbacks: HashMap<&str, &str> = [("DB_PASSWORD", "dev-secret")].into();
    let mut parser = ParserBuilder::new()
        .with_loader(files)
        .with_variables([
            ("ENVIRONMENT", environment),
            ("DB_HOST", db_host),
            ("LOG_LEVEL", log_level),
        ])
        // Asked only for braced references to names that are not registered (spec §7.7).
        .with_variable_handler(move |name| {
            std::env::var(name)
                .ok()
                .or_else(|| fallbacks.get(name).map(|v| v.to_string()))
        })
        .build();
    // Relative include paths resolve against the directory of the file parsed.
    let value = parser.parse_file("/etc/myapp/main.conf")?;
    from_value(value)
}

fn layered_environments() -> Result<(), Box<dyn std::error::Error>> {
    println!("1. Layers and environments");
    println!("--------------------------");

    for environment in ["development", "staging", "production"] {
        // A command-line override, as `--set server.port=9000` might produce for staging.
        let overrides = if environment == "staging" {
            "server { port = 9000; }"
        } else {
            ""
        };
        let config = load(environment, overrides)?;
        print_summary(&config);

        // Values every environment shares come from the defaults.
        assert_eq!(config.app.name, "my-application");
        assert_eq!(config.app.environment, environment);
        assert_eq!(config.app.max_request_size, 10 * 1024 * 1024);
        assert_eq!(config.server.timeouts.idle, Duration::from_secs(120));
        assert_eq!(config.database.min_connections, 5);
        if std::env::var("DB_PASSWORD").is_err() {
            assert_eq!(config.database.password, "dev-secret");
        }
        match environment {
            "development" => {
                assert!(config.app.debug);
                assert_eq!(config.server.host, "localhost");
                assert_eq!(config.logging.format, "pretty");
                assert_eq!(config.logging.outputs, ["stdout", "file:./logs/dev.log"]);
            }
            "staging" => {
                // The command-line layer has the highest priority.
                assert_eq!(config.server.port, 9000);
                assert_eq!(config.server.max_connections, 5000);
                assert_eq!(config.database.replicas, ["staging-replica.internal"]);
            }
            _ => {
                assert_eq!(config.server.port, 443);
                assert!(config.server.tls.is_some());
                assert_eq!(config.database.host, "prod-db.internal");
                assert_eq!(config.database.replicas.len(), 2);
                assert_eq!(config.logging.level, "warn");
                assert!(config.feature_flags["enhanced_logging"]);
            }
        }
        println!();
    }
    Ok(())
}

fn print_summary(config: &ApplicationConfig) {
    println!(
        "  {} v{} ({}), debug {}",
        config.app.name, config.app.version, config.app.environment, config.app.debug
    );
    println!(
        "  server {}:{} (max {} connections, TLS {}), timeouts {:?}/{:?}/{:?}",
        config.server.host,
        config.server.port,
        config.server.max_connections,
        if config.server.tls.is_some() {
            "on"
        } else {
            "off"
        },
        config.server.timeouts.read,
        config.server.timeouts.write,
        config.server.timeouts.idle
    );
    println!(
        "  database {}@{}:{}/{} (pool {}..{}, password {} characters), replicas {:?}",
        config.database.user,
        config.database.host,
        config.database.port,
        config.database.name,
        config.database.min_connections,
        config.database.max_connections,
        config.database.password.len(),
        config.database.replicas
    );
    println!(
        "  logging {} as {} to {:?}; feature flags {:?}",
        config.logging.level, config.logging.format, config.logging.outputs, config.feature_flags
    );
}

#[derive(Debug, Deserialize)]
#[allow(
    dead_code,
    reason = "the example reads only some of the fields it deserializes"
)]
struct Server {
    host: String,
    port: u16,
    timeouts: TimeoutConfig,
    min_workers: u32,
    max_workers: u32,
}

/// Checks that serde cannot express: relations between fields.
fn check(server: &Server) -> Result<(), String> {
    if server.min_workers > server.max_workers {
        return Err(format!(
            "min_workers ({}) is greater than max_workers ({})",
            server.min_workers, server.max_workers
        ));
    }
    Ok(())
}

fn validation() {
    println!("2. Validation");
    println!("-------------");

    let valid = "host = localhost; port = 8080; min_workers = 2; max_workers = 8;\n\
                 timeouts { read = 30s; write = 30s; idle = 2min; }";
    let server: Server = from_str(valid).expect("the valid document");
    assert!(check(&server).is_ok());
    println!("  valid: {server:?}");

    // A syntax error: the parser's error has a kind and a position.
    let err = from_str::<Server>("host = localhost\ntimeouts {\n    read = 30s;\n").unwrap_err();
    println!("  unclosed section: {err}");
    let position = err.position().expect("a parse error has a position");
    assert_eq!(
        err.parse_error().map(|e| e.kind()),
        Some(&ErrorKind::UnterminatedObject)
    );
    assert_eq!(position.line, 4);

    // Documents that parse but do not fit the struct: serde errors.
    for (what, text) in [
        ("missing field", "host = localhost; port = 80;"),
        (
            "port out of range",
            "host = h; port = 99999; min_workers = 1; max_workers = 1;\n\
             timeouts { read = 1s; write = 1s; idle = 1s; }",
        ),
        (
            "negative time",
            "host = h; port = 80; min_workers = 1; max_workers = 1;\n\
             timeouts { read = -5s; write = 1s; idle = 1s; }",
        ),
    ] {
        let err = from_str::<Server>(text).unwrap_err();
        println!("  {what}: {err}");
        assert!(matches!(err, UclError::Serde(_)), "{what}");
    }

    // A document that fits the struct but breaks a rule across fields.
    let text = "host = h; port = 80; min_workers = 16; max_workers = 4;\n\
                timeouts { read = 1s; write = 1s; idle = 1s; }";
    let server: Server = from_str(text).expect("parses and deserializes");
    let problem = check(&server).unwrap_err();
    println!("  cross-field rule: {problem}");
    println!();
}

#[derive(Debug, Deserialize, PartialEq)]
struct ReloadConfig {
    server: ReloadServer,
    logging: ReloadLogging,
}

#[derive(Debug, Deserialize, PartialEq)]
struct ReloadServer {
    host: String,
    port: u16,
    debug: bool,
}

#[derive(Debug, Deserialize, PartialEq)]
struct ReloadLogging {
    level: String,
    format: String,
    outputs: Vec<String>,
}

/// A value as compact JSON, as libucl writes it.
fn json(value: &UclValue) -> String {
    Emitter::new(Format::JsonCompact).emit(value)
}

/// The paths at which two value trees differ, with the old and new values.
fn diff(path: &str, old: &UclValue, new: &UclValue, changes: &mut Vec<String>) {
    let child = |key: &str| {
        if path.is_empty() {
            key.to_string()
        } else {
            format!("{path}.{key}")
        }
    };
    match (old.as_object(), new.as_object()) {
        (Some(old), Some(new)) => {
            for (key, entry) in old.iter() {
                match new.get(key) {
                    Some(value) => diff(&child(key), entry.first(), value, changes),
                    None => changes.push(format!("{}: removed", child(key))),
                }
            }
            for (key, entry) in new.iter() {
                if !old.contains_key(key) {
                    changes.push(format!("{}: added {}", child(key), json(entry.first())));
                }
            }
        }
        _ if old != new => changes.push(format!("{path}: {} -> {}", json(old), json(new))),
        _ => {}
    }
}

fn reloading() -> Result<(), Box<dyn std::error::Error>> {
    println!("3. Reloading");
    println!("------------");

    let running = r#"
        server { host = localhost; port = 3000; debug = true; }
        logging { level = debug; format = pretty; outputs = [stdout]; }
    "#;
    let changed = r#"
        server { host = localhost; port = 8080; debug = false; }
        logging { level = info; format = json; outputs = [stdout, "file:./app.log"]; }
        metrics { enabled = true; }
    "#;

    let old = ucl_lexer::parse::parse(running.as_bytes())?;
    let new = ucl_lexer::parse::parse(changed.as_bytes())?;
    let mut changes = Vec::new();
    diff("", &old, &new, &mut changes);
    println!("  {} changes in the document:", changes.len());
    for change in &changes {
        println!("    {change}");
    }
    assert_eq!(changes.len(), 6);

    // The typed view decides what the application must restart.
    let old: ReloadConfig = from_value(old)?;
    let new: ReloadConfig = from_value(new)?;
    let restart = old.server != new.server;
    let reopen_logs = old.logging.outputs != new.logging.outputs;
    println!("  restart the server: {restart}; reopen log outputs: {reopen_logs}");
    assert!(restart && reopen_logs);
    println!();
    Ok(())
}

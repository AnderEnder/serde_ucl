//! Web server configuration.
//!
//! 1. A complete configuration read into typed structs with `from_str`: nested sections,
//!    multipliers (`1kb`, `100mb`), times (`30s`, `1h`) read as `Duration`, and a repeated key
//!    read as a list.
//! 2. Per-environment files layered over defaults with `.include(priority=1, duplicate="merge")`,
//!    served from memory by a `MemoryLoader`.
//! 3. Registered variables (`$NAME`) and a variable handler for `${NAME}` that reads the process
//!    environment, with fallbacks so that the example never depends on it.

use serde::Deserialize;
use serde_ucl::emit::Format;
use serde_ucl::parse::{MemoryLoader, ParserBuilder};
use serde_ucl::{from_str, from_value};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct WebServerConfig {
    server: ServerConfig,
    database: DatabaseConfig,
    #[serde(default)]
    middleware: Option<MiddlewareConfig>,
    logging: LoggingConfig,
    features: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
    #[serde(default = "default_workers")]
    workers: u32,
    max_connections: u32,
    #[serde(with = "serde_ucl::time")]
    timeout: Duration,
    ssl: Option<SslConfig>,
}

#[derive(Debug, Deserialize)]
struct SslConfig {
    cert_path: String,
    key_path: String,
    protocols: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DatabaseConfig {
    url: String,
    #[serde(default)]
    password: Option<String>,
    pool_size: u32,
    #[serde(with = "serde_ucl::time")]
    timeout: Duration,
    retry_attempts: u32,
    /// `read_replica` may be repeated; every value is kept, in order.
    #[serde(default, rename = "read_replica")]
    read_replicas: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MiddlewareConfig {
    cors: CorsConfig,
    rate_limiting: RateLimitConfig,
    compression: CompressionConfig,
}

#[derive(Debug, Deserialize)]
struct CorsConfig {
    enabled: bool,
    allowed_origins: Vec<String>,
    allowed_methods: Vec<String>,
    #[serde(with = "serde_ucl::time")]
    max_age: Duration,
}

#[derive(Debug, Deserialize)]
struct RateLimitConfig {
    enabled: bool,
    requests_per_minute: u32,
    burst_size: u32,
}

#[derive(Debug, Deserialize)]
struct CompressionConfig {
    enabled: bool,
    algorithms: Vec<String>,
    min_size: u64,
}

#[derive(Debug, Deserialize)]
struct LoggingConfig {
    level: String,
    format: String,
    output: String,
    max_file_size: u64,
    rotation_count: u32,
}

fn default_workers() -> u32 {
    4
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    basic_config()?;
    environment_configs()?;
    variables()?;
    Ok(())
}

fn basic_config() -> Result<(), Box<dyn std::error::Error>> {
    println!("1. Basic web server configuration");

    let text = r#"
        # Web server configuration
        server {
            host = "0.0.0.0";
            port = 8080;
            max_connections = 1000;
            timeout = 30s;

            ssl {
                cert_path = "/etc/ssl/certs/server.crt";
                key_path = "/etc/ssl/private/server.key";
                protocols = ["TLSv1.2", "TLSv1.3"];
            }
        }

        database {
            url = "postgresql://localhost:5432/myapp";
            pool_size = 10;
            timeout = 5s;
            retry_attempts = 3;
            # A repeated key holds several values.
            read_replica = "postgresql://replica1:5432/myapp";
            read_replica = "postgresql://replica2:5432/myapp";
        }

        middleware {
            cors {
                enabled = yes;
                allowed_origins = ["https://example.com", "https://app.example.com"];
                allowed_methods = [GET, POST, PUT, DELETE];
                max_age = 1h;
            }
            rate_limiting {
                enabled = on;
                requests_per_minute = 100;
                burst_size = 20;
            }
            compression {
                enabled = true;
                algorithms = [gzip, br];
                min_size = 1kb;
            }
        }

        logging {
            level = info;
            format = json;
            output = "/var/log/myapp.log";
            max_file_size = 100mb;
            rotation_count = 5;
        }

        features = [auth, metrics, health_check];
    "#;

    let config: WebServerConfig = from_str(text)?;
    print_summary(&config);

    assert_eq!(config.server.port, 8080);
    assert_eq!(config.server.workers, 4, "absent, so the serde default");
    assert_eq!(config.server.timeout, Duration::from_secs(30));
    assert_eq!(config.database.read_replicas.len(), 2);
    let middleware = config.middleware.as_ref().expect("middleware section");
    assert!(middleware.cors.enabled, "`yes` is a boolean");
    assert_eq!(middleware.cors.max_age, Duration::from_secs(3600));
    assert!(middleware.rate_limiting.enabled, "`on` is a boolean");
    assert_eq!(middleware.compression.min_size, 1024, "`kb` is 2^10");
    assert_eq!(config.logging.max_file_size, 100 * 1024 * 1024);
    assert_eq!(config.features, ["auth", "metrics", "health_check"]);
    println!();
    Ok(())
}

/// The defaults and the environment files, as they might be installed under `/etc/webapp`.
fn config_files() -> MemoryLoader {
    let mut loader = MemoryLoader::new();
    loader.add_file(
        "/etc/webapp/webapp.conf",
        r#"
# Defaults for every environment.
server {
    host = "127.0.0.1";
    port = 3000;
    max_connections = 100;
    timeout = 10s;
}
database {
    url = "postgresql://localhost:5432/myapp_dev";
    pool_size = 5;
    timeout = 2s;
    retry_attempts = 1;
}
logging {
    level = debug;
    format = pretty;
    output = stdout;
    max_file_size = 10mb;
    rotation_count = 3;
}
features = [auth];

# The environment's file: its values have priority 1, so they replace the defaults (priority
# 0), and duplicate="merge" merges its sections into the ones above instead of adding second
# values. Arrays are merged by appending. The path starts from ${CURDIR}, the directory of this
# file: libucl resolves relative paths against the process's working directory.
.include(priority=1, duplicate="merge") "${CURDIR}/env/${ENVIRONMENT}.conf"
"#,
    );
    loader.add_file(
        "/etc/webapp/env/development.conf",
        "features = [debug, hot_reload];\n",
    );
    loader.add_file(
        "/etc/webapp/env/staging.conf",
        r#"
server { port = 8080; }
database {
    url = "postgresql://staging-db:5432/myapp";
    pool_size = 20;
}
logging { level = info; }
features = [metrics];
"#,
    );
    loader.add_file(
        "/etc/webapp/env/production.conf",
        r#"
server {
    host = "0.0.0.0";
    port = 80;
    max_connections = 10000;
    ssl {
        cert_path = "/etc/ssl/certs/myapp.crt";
        key_path = "/etc/ssl/private/myapp.key";
        protocols = ["TLSv1.3"];
    }
}
database {
    url = "postgresql://prod-db:5432/myapp";
    pool_size = 50;
    read_replica = "postgresql://prod-replica:5432/myapp";
}
logging {
    level = warn;
    format = json;
    output = "/var/log/myapp.log";
    max_file_size = 100mb;
}
features = [metrics, health_check];
"#,
    );
    loader
}

fn environment_configs() -> Result<(), Box<dyn std::error::Error>> {
    println!("2. Environment-specific configuration");

    for environment in ["development", "staging", "production"] {
        let mut parser = ParserBuilder::new()
            .with_loader(config_files())
            .with_variable("ENVIRONMENT", environment)
            .build();
        let value = parser.parse_file("/etc/webapp/webapp.conf")?;
        let config: WebServerConfig = from_value(value.clone())?;
        println!("  {environment}:");
        println!(
            "    server {}:{} max {} connections, TLS {}",
            config.server.host,
            config.server.port,
            config.server.max_connections,
            if config.server.ssl.is_some() {
                "on"
            } else {
                "off"
            }
        );
        println!(
            "    database {} (pool {}, {} replicas)",
            config.database.url,
            config.database.pool_size,
            config.database.read_replicas.len()
        );
        println!(
            "    logging {} {} to {}; features {:?}",
            config.logging.level, config.logging.format, config.logging.output, config.features
        );

        match environment {
            "development" => {
                assert_eq!(config.server.port, 3000);
                assert_eq!(config.features, ["auth", "debug", "hot_reload"]);
            }
            "staging" => {
                assert_eq!(config.server.port, 8080);
                assert_eq!(config.server.host, "127.0.0.1", "kept from the defaults");
                assert_eq!(config.database.pool_size, 20);
                assert_eq!(config.logging.format, "pretty", "kept from the defaults");
            }
            _ => {
                assert_eq!(config.server.port, 80);
                assert_eq!(config.server.timeout, Duration::from_secs(10));
                assert_eq!(config.database.retry_attempts, 1);
                assert_eq!(config.database.read_replicas.len(), 1);
                assert_eq!(config.logging.rotation_count, 3);
                assert_eq!(config.features, ["auth", "metrics", "health_check"]);
                // The merged document, written as libucl writes it.
                println!("    effective configuration:");
                for line in parser.emitter(Format::Config).emit(&value).lines() {
                    println!("      {line}");
                }
            }
        }
    }
    println!();
    Ok(())
}

fn variables() -> Result<(), Box<dyn std::error::Error>> {
    println!("3. Variables");

    // `$NAME` and `${NAME}` expand to registered variables. A variable handler is asked only for
    // braced references to names that are not registered; here it reads the environment, with
    // defaults for the names it knows. A reference nobody resolves stays as written. libucl
    // substitutes a handler's value reliably only when the reference is the whole string.
    let defaults: HashMap<&str, &str> = [
        ("WEBAPP_LOG_LEVEL", "info"),
        ("WEBAPP_DB_PASSWORD", "change-me"),
    ]
    .into();
    let mut parser = ParserBuilder::new()
        .with_variables([
            ("APP_NAME", "my-web-app"),
            ("DB_HOST", "database.internal"),
            ("DB_USER", "app"),
            ("BIND_ADDRESS", "0.0.0.0"),
        ])
        .with_variable_handler(move |name| {
            std::env::var(name)
                .ok()
                .or_else(|| defaults.get(name).map(|value| value.to_string()))
        })
        .build();

    let text = r#"
        server {
            host = $BIND_ADDRESS;
            port = 8080;
            max_connections = 1000;
            timeout = 30s;
        }
        database {
            url = "postgresql://$DB_USER@$DB_HOST:5432/$APP_NAME";
            password = "${WEBAPP_DB_PASSWORD}";
            pool_size = 10;
            timeout = 5s;
            retry_attempts = 3;
        }
        logging {
            level = "${WEBAPP_LOG_LEVEL}";
            format = json;
            output = "/var/log/$APP_NAME.log";
            max_file_size = 100mb;
            rotation_count = 5;
        }
        features = [auth, metrics];
        motd = "${WEBAPP_MOTD}";
    "#;
    let value = parser.parse(text.as_bytes())?;
    let motd = value.as_object().and_then(|root| root.get("motd"));
    let config: WebServerConfig = from_value(value.clone())?;

    println!("  server host: {}", config.server.host);
    println!("  database url: {}", config.database.url);
    println!("  log output: {}", config.logging.output);
    println!("  log level: {}", config.logging.level);
    println!(
        "  database password: {} characters",
        config.database.password.as_deref().map_or(0, str::len)
    );
    println!(
        "  motd: {}",
        motd.and_then(|v| v.as_str()).unwrap_or_default()
    );

    assert_eq!(config.server.host, "0.0.0.0");
    assert_eq!(
        config.database.url,
        "postgresql://app@database.internal:5432/my-web-app"
    );
    assert_eq!(config.logging.output, "/var/log/my-web-app.log");
    let expected_level = std::env::var("WEBAPP_LOG_LEVEL").unwrap_or_else(|_| "info".into());
    assert_eq!(config.logging.level, expected_level);
    if std::env::var("WEBAPP_MOTD").is_err() {
        assert_eq!(motd.and_then(|v| v.as_str()), Some("${WEBAPP_MOTD}"));
    }
    println!();
    Ok(())
}

fn print_summary(config: &WebServerConfig) {
    println!(
        "  server {}:{} ({} workers, {} connections, timeout {:?})",
        config.server.host,
        config.server.port,
        config.server.workers,
        config.server.max_connections,
        config.server.timeout
    );
    if let Some(ssl) = &config.server.ssl {
        println!(
            "  TLS {:?}: cert {}, key {}",
            ssl.protocols, ssl.cert_path, ssl.key_path
        );
    }
    println!(
        "  database {} (pool {}, timeout {:?}, {} retries), replicas {:?}",
        config.database.url,
        config.database.pool_size,
        config.database.timeout,
        config.database.retry_attempts,
        config.database.read_replicas
    );
    if let Some(middleware) = &config.middleware {
        println!(
            "  CORS {:?} {:?}, max age {:?}",
            middleware.cors.allowed_origins,
            middleware.cors.allowed_methods,
            middleware.cors.max_age
        );
        println!(
            "  rate limit {} req/min (burst {}); compression {} {:?} from {} bytes",
            middleware.rate_limiting.requests_per_minute,
            middleware.rate_limiting.burst_size,
            if middleware.compression.enabled {
                "on"
            } else {
                "off"
            },
            middleware.compression.algorithms,
            middleware.compression.min_size
        );
    }
    println!(
        "  logging {} {} to {} (max {} bytes, {} files)",
        config.logging.level,
        config.logging.format,
        config.logging.output,
        config.logging.max_file_size,
        config.logging.rotation_count
    );
    println!("  features {:?}", config.features);
}

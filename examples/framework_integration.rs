//! Framework integration: configuration structs of the kind that web frameworks, async runtimes
//! and API services take, read from UCL.
//!
//! 1. An Axum-style server: nested sections and times read as `Duration` through
//!    `ucl_lexer::time`.
//! 2. A Tokio-style runtime: optional fields, multipliers (`2mb`, `1k`).
//! 3. An API service: a secret from the environment through a variable handler, and a repeated
//!    key read as a list.
//! 4. Defaults: `#[serde(default)]` fills what a partial document leaves out, and the serde
//!    serializer writes the effective configuration as UCL and as JSON.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use ucl_lexer::parse::ParserBuilder;
use ucl_lexer::{from_str, from_value, to_json_string, to_string};

#[derive(Debug, Deserialize)]
struct AxumConfig {
    server: ServerConfig,
    database: DatabaseConfig,
    middleware: MiddlewareConfig,
    tracing: TracingConfig,
}

#[derive(Debug, Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
    #[serde(with = "ucl_lexer::time")]
    graceful_shutdown_timeout: Duration,
}

#[derive(Debug, Deserialize)]
struct DatabaseConfig {
    url: String,
    max_connections: u32,
    min_connections: u32,
    #[serde(with = "ucl_lexer::time")]
    acquire_timeout: Duration,
    #[serde(with = "ucl_lexer::time")]
    idle_timeout: Duration,
}

#[derive(Debug, Deserialize)]
struct MiddlewareConfig {
    cors: CorsConfig,
    compression: bool,
    request_id: bool,
    #[serde(with = "ucl_lexer::time")]
    timeout: Duration,
}

#[derive(Debug, Deserialize)]
struct CorsConfig {
    allow_origins: Vec<String>,
    allow_methods: Vec<String>,
    allow_headers: Vec<String>,
    #[serde(with = "ucl_lexer::time")]
    max_age: Duration,
}

#[derive(Debug, Deserialize)]
struct TracingConfig {
    level: String,
    format: String,
    jaeger: Option<JaegerConfig>,
}

#[derive(Debug, Deserialize)]
struct JaegerConfig {
    endpoint: String,
    service_name: String,
    sample_rate: f64,
}

#[derive(Debug, Deserialize)]
struct TokioConfig {
    runtime: RuntimeConfig,
    tasks: TaskConfig,
    metrics: MetricsConfig,
}

#[derive(Debug, Deserialize)]
struct RuntimeConfig {
    worker_threads: Option<usize>,
    max_blocking_threads: Option<usize>,
    /// Bytes.
    thread_stack_size: Option<u64>,
    thread_name: String,
}

#[derive(Debug, Deserialize)]
struct TaskConfig {
    max_concurrent: usize,
    #[serde(with = "ucl_lexer::time")]
    timeout: Duration,
    retry_attempts: u32,
    backoff_multiplier: f64,
}

#[derive(Debug, Deserialize)]
struct MetricsConfig {
    enabled: bool,
    endpoint: String,
    #[serde(with = "ucl_lexer::time")]
    interval: Duration,
}

#[derive(Debug, Deserialize)]
struct ApiConfig {
    version: String,
    base_path: String,
    rate_limiting: ApiRateLimitConfig,
    authentication: AuthConfig,
    response_format: ResponseFormatConfig,
}

#[derive(Debug, Deserialize)]
struct ApiRateLimitConfig {
    requests_per_second: u32,
    burst_capacity: u32,
    #[serde(with = "ucl_lexer::time")]
    window_size: Duration,
}

#[derive(Debug, Deserialize)]
struct AuthConfig {
    jwt_secret: String,
    #[serde(with = "ucl_lexer::time")]
    token_expiry: Duration,
    #[serde(with = "ucl_lexer::time")]
    refresh_token_expiry: Duration,
    #[serde(rename = "allowed_issuer")]
    allowed_issuers: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ResponseFormatConfig {
    pretty_print: bool,
    include_metadata: bool,
    error_details: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    axum_integration()?;
    tokio_integration()?;
    api_service_integration()?;
    defaults_and_output()?;
    Ok(())
}

fn axum_integration() -> Result<(), Box<dyn std::error::Error>> {
    println!("1. Axum-style web server");

    let text = r#"
        # Axum web server configuration
        server {
            host = "0.0.0.0";
            port = 3000;
            graceful_shutdown_timeout = 30s;
        }
        database {
            url = "postgresql://localhost:5432/myapp";
            max_connections = 20;
            min_connections = 5;
            acquire_timeout = 10s;
            idle_timeout = 10min;
        }
        middleware {
            cors {
                allow_origins = ["http://localhost:3000", "https://myapp.com"];
                allow_methods = [GET, POST, PUT, DELETE, OPTIONS];
                allow_headers = ["Content-Type", "Authorization", "X-Requested-With"];
                max_age = 1h;
            }
            compression = true;
            request_id = true;
            timeout = 30s;
        }
        tracing {
            level = info;
            format = json;
            jaeger {
                endpoint = "http://localhost:14268/api/traces";
                service_name = "my-web-service";
                sample_rate = 0.1;
            }
        }
    "#;

    let config: AxumConfig = from_str(text)?;
    println!(
        "  listen on {}:{}, graceful shutdown {:?}",
        config.server.host, config.server.port, config.server.graceful_shutdown_timeout
    );
    println!(
        "  database {}: {}..{} connections, acquire {:?}, idle {:?}",
        config.database.url,
        config.database.min_connections,
        config.database.max_connections,
        config.database.acquire_timeout,
        config.database.idle_timeout
    );
    println!(
        "  CORS {:?} {:?} {:?}, max age {:?}",
        config.middleware.cors.allow_origins,
        config.middleware.cors.allow_methods,
        config.middleware.cors.allow_headers,
        config.middleware.cors.max_age
    );
    println!(
        "  compression {}, request id {}, request timeout {:?}",
        config.middleware.compression, config.middleware.request_id, config.middleware.timeout
    );
    println!(
        "  tracing {} as {}",
        config.tracing.level, config.tracing.format
    );
    if let Some(jaeger) = &config.tracing.jaeger {
        println!(
            "  jaeger {} at {} ({}% sampled)",
            jaeger.service_name,
            jaeger.endpoint,
            jaeger.sample_rate * 100.0
        );
    }

    assert_eq!(config.server.port, 3000);
    assert_eq!(config.database.idle_timeout, Duration::from_secs(600));
    assert_eq!(config.middleware.cors.max_age, Duration::from_secs(3600));
    assert_eq!(config.middleware.cors.allow_methods.len(), 5);
    assert_eq!(config.tracing.jaeger.map(|j| j.sample_rate), Some(0.1));
    println!();
    Ok(())
}

fn tokio_integration() -> Result<(), Box<dyn std::error::Error>> {
    println!("2. Tokio-style runtime");

    let text = r#"
        # Tokio runtime configuration
        runtime {
            # worker_threads is left out: the runtime then starts one per CPU.
            max_blocking_threads = 512;
            thread_stack_size = 2mb;
            thread_name = "my-app-worker";
        }
        tasks {
            max_concurrent = 1k;
            timeout = 1min;
            retry_attempts = 3;
            backoff_multiplier = 2.0;
        }
        metrics {
            enabled = true;
            endpoint = "/metrics";
            interval = 10s;
        }
    "#;

    let config: TokioConfig = from_str(text)?;
    let workers = config.runtime.worker_threads.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    });
    println!(
        "  {} worker threads named {:?}, up to {:?} blocking threads, stack {:?} bytes",
        workers,
        config.runtime.thread_name,
        config.runtime.max_blocking_threads,
        config.runtime.thread_stack_size
    );
    println!(
        "  tasks: {} at once, timeout {:?}, {} retries with backoff x{}",
        config.tasks.max_concurrent,
        config.tasks.timeout,
        config.tasks.retry_attempts,
        config.tasks.backoff_multiplier
    );
    println!(
        "  metrics {} at {} every {:?}",
        if config.metrics.enabled { "on" } else { "off" },
        config.metrics.endpoint,
        config.metrics.interval
    );

    assert_eq!(config.runtime.worker_threads, None);
    assert_eq!(config.runtime.thread_stack_size, Some(2 * 1024 * 1024));
    assert_eq!(config.tasks.max_concurrent, 1000, "`k` is 10^3");
    assert_eq!(config.tasks.timeout, Duration::from_secs(60));
    println!();
    Ok(())
}

fn api_service_integration() -> Result<(), Box<dyn std::error::Error>> {
    println!("3. API service");

    let text = r#"
        # API service configuration
        version = "v1";
        base_path = "/api/v1";
        rate_limiting {
            requests_per_second = 100;
            burst_capacity = 200;
            window_size = 1min;
        }
        authentication {
            jwt_secret = "${JWT_SECRET}";
            token_expiry = 1h;
            refresh_token_expiry = 1w;
            allowed_issuer = "https://auth.myapp.com";
            allowed_issuer = "https://myapp.com";
        }
        response_format {
            pretty_print = false;
            include_metadata = true;
            error_details = true;
        }
    "#;

    // The handler is asked for `${JWT_SECRET}` because no variable of that name is registered.
    // When the environment has no such variable either, the reference stays as written.
    let mut parser = ParserBuilder::new()
        .with_variable_handler(|name| std::env::var(name).ok())
        .build();
    let config: ApiConfig = from_value(parser.parse(text.as_bytes())?)?;
    let secret_set = config.authentication.jwt_secret != "${JWT_SECRET}";

    println!("  {} at {}", config.version, config.base_path);
    println!(
        "  rate limit {}/s, burst {}, window {:?}",
        config.rate_limiting.requests_per_second,
        config.rate_limiting.burst_capacity,
        config.rate_limiting.window_size
    );
    println!(
        "  JWT secret {}; tokens {:?}, refresh tokens {:?}, issuers {:?}",
        if secret_set {
            "from JWT_SECRET"
        } else {
            "not set (JWT_SECRET is not in the environment)"
        },
        config.authentication.token_expiry,
        config.authentication.refresh_token_expiry,
        config.authentication.allowed_issuers
    );
    println!(
        "  responses: pretty {}, metadata {}, error details {}",
        config.response_format.pretty_print,
        config.response_format.include_metadata,
        config.response_format.error_details
    );

    assert_eq!(
        config.authentication.refresh_token_expiry,
        Duration::from_secs(7 * 24 * 3600)
    );
    assert_eq!(config.authentication.allowed_issuers.len(), 2);
    if std::env::var("JWT_SECRET").is_err() {
        assert!(!secret_set);
    }
    println!();
    Ok(())
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct ServiceConfig {
    name: String,
    listen: String,
    workers: u32,
    #[serde(with = "ucl_lexer::time")]
    request_timeout: Duration,
    features: Features,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Features {
    auth: bool,
    metrics: bool,
    tracing: bool,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            name: "service".into(),
            listen: "127.0.0.1:8080".into(),
            workers: 4,
            request_timeout: Duration::from_millis(1500),
            features: Features::default(),
        }
    }
}

impl Default for Features {
    fn default() -> Self {
        Self {
            auth: true,
            metrics: true,
            tracing: false,
        }
    }
}

fn defaults_and_output() -> Result<(), Box<dyn std::error::Error>> {
    println!("4. Defaults and output");

    // The document sets two values; serde's defaults fill in the rest.
    let config: ServiceConfig = from_str("name = billing;\nfeatures { tracing = true; }\n")?;
    assert_eq!(config.name, "billing");
    assert_eq!(config.workers, 4);
    assert!(config.features.auth && config.features.tracing);

    // The effective configuration, as UCL; it reads back as the same value.
    let text = to_string(&config)?;
    println!("  effective configuration:");
    for line in text.lines() {
        println!("    {line}");
    }
    assert_eq!(from_str::<ServiceConfig>(&text)?, config);

    // The same as JSON, for an API endpoint that reports the configuration. A time is its number
    // of seconds there.
    let json = to_json_string(&config)?;
    let parsed: serde_json::Value = serde_json::from_str(&json)?;
    assert_eq!(parsed["request_timeout"], 1.5);
    println!("  as JSON:");
    for line in json.lines() {
        println!("    {line}");
    }
    println!();
    Ok(())
}

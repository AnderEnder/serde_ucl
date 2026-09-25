//! NGINX-style UCL mapped onto the configuration structs of a Rust web service.
//!
//! UCL reads the nginx configuration style directly: `key value;` without `=`, `name { ... }`
//! sections, `location "/api" { ... }` named sections and repeated keys for lists. This example
//! configures a web service (server, TLS, middleware, upstreams and routes, in the shape an
//! Axum or Tokio application would use) and shows how each construct lands in serde:
//!
//! - a repeated key is an entry with several values, which deserializes into a `Vec`
//!   (`#[serde(rename)]` maps the singular key onto a plural field);
//! - `location "/api" { ... }` is the object `location { "/api" { ... } }`; repeating it adds
//!   another value to `location`, unless the parser merges repeated objects
//!   (`DuplicateStrategy::Merge`), which gives one map of routes;
//! - `30s`, `1d` are times (seconds), read into `Duration` with `ucl_lexer::time`; `1kb`, `10mb`
//!   are byte multipliers (powers of 1024) and `10k` a decimal one;
//! - `yes`, `on` and `true` are booleans; comments are `#` and `/* */` (not `//`).

use indexmap::IndexMap;
use serde::Deserialize;
use std::time::Duration;
use ucl_lexer::parse::ParserBuilder;
use ucl_lexer::{DuplicateStrategy, UclValue, from_value};

/// The service configuration, in the nginx-like style.
const SERVICE_CONF: &str = r#"
# Web service configuration.
application {
    name rust-web-api;
    version "2.1.0";
    environment production;
    author "Alice Johnson";
    author "Bob Smith";
}

server {
    listen 0.0.0.0:8080;
    worker_threads 4;
    max_connections 10k;        # 10 * 1000
    request_timeout 30s;
    keepalive 75s;
    shutdown_timeout 10s;
    max_body 10mb;              # 10 * 1024 * 1024 bytes

    tls {
        enabled yes;
        certificate /etc/ssl/certs/api.crt;
        certificate_key /etc/ssl/private/api.key;
        protocol TLSv1.2;
        protocol TLSv1.3;
    }
}

middleware {
    cors {
        origin "https://app.example.com";
        origin "https://admin.example.com";
        method GET; method POST; method PUT; method DELETE;
        max_age 1d;             # a time: 86400 seconds
    }
    rate_limit {
        enabled on;
        requests_per_minute 1000;
        burst 100;
        key ip_address;
    }
    compression {
        algorithm gzip;
        algorithm br;
        min_size 1kb;
    }
}

/* Upstreams and routes: named sections. */
upstream api_backend {
    server 10.0.1.10:8080;
    server 10.0.1.11:8080;
    health_check /health;
}

location "/api" {
    proxy_pass http://api_backend;
    timeout 30s;
}
location "/health" {
    return 200;
}
location "/static" {
    root /var/www/static;
    cache_control "public, max-age=31536000";
}
"#;

#[derive(Debug, Deserialize)]
struct ServiceConfig {
    application: Application,
    server: Server,
    middleware: Middleware,
    upstream: IndexMap<String, Upstream>,
    location: IndexMap<String, Location>,
}

#[derive(Debug, Deserialize)]
struct Application {
    name: String,
    version: String,
    environment: String,
    #[serde(rename = "author")]
    authors: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Server {
    listen: String,
    worker_threads: Option<u32>,
    max_connections: u32,
    #[serde(with = "ucl_lexer::time")]
    request_timeout: Duration,
    #[serde(with = "ucl_lexer::time")]
    keepalive: Duration,
    #[serde(with = "ucl_lexer::time")]
    shutdown_timeout: Duration,
    max_body: u64,
    tls: Option<Tls>,
}

#[derive(Debug, Deserialize)]
struct Tls {
    enabled: bool,
    certificate: String,
    certificate_key: String,
    #[serde(rename = "protocol")]
    protocols: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Middleware {
    cors: Cors,
    rate_limit: RateLimit,
    compression: Compression,
}

#[derive(Debug, Deserialize)]
struct Cors {
    #[serde(rename = "origin")]
    origins: Vec<String>,
    #[serde(rename = "method")]
    methods: Vec<String>,
    /// A time; an integral number of seconds reads into an integer.
    max_age: u32,
}

#[derive(Debug, Deserialize)]
struct RateLimit {
    enabled: bool,
    requests_per_minute: u32,
    burst: u32,
    key: String,
}

#[derive(Debug, Deserialize)]
struct Compression {
    #[serde(rename = "algorithm")]
    algorithms: Vec<String>,
    min_size: u64,
}

#[derive(Debug, Deserialize)]
struct Upstream {
    /// One `server` line gives a one-element list, several give several.
    #[serde(rename = "server")]
    servers: Vec<String>,
    health_check: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Location {
    proxy_pass: Option<String>,
    #[serde(default, with = "option_seconds")]
    timeout: Option<Duration>,
    root: Option<String>,
    cache_control: Option<String>,
    #[serde(rename = "return")]
    status: Option<u16>,
}

/// `Option<Duration>` through `ucl_lexer::time`.
mod option_seconds {
    use serde::{Deserialize, Deserializer};
    use std::time::Duration;

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        #[derive(Deserialize)]
        struct Seconds(#[serde(with = "ucl_lexer::time")] Duration);
        Ok(Option::<Seconds>::deserialize(d)?.map(|s| s.0))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== NGINX-style framework configuration ===\n");

    // With the default strategy (`append`), each `location "..." { }` block is one more value of
    // the key `location`: three objects of one route each.
    let appended = ucl_lexer::parse::parse(SERVICE_CONF.as_bytes())?;
    let locations = appended.as_object().unwrap().entry("location").unwrap();
    assert_eq!(locations.len(), 3);
    println!(
        "append: `location` holds {} values; `upstream` holds {}",
        locations.len(),
        appended
            .as_object()
            .unwrap()
            .entry("upstream")
            .unwrap()
            .len()
    );

    // With `merge`, repeated objects merge into the first one (repeated scalars inside still
    // collect), so the routes become one map, in the order they were written.
    let mut parser = ParserBuilder::new()
        .with_strategy(DuplicateStrategy::Merge)
        .build();
    let value = parser.parse(SERVICE_CONF.as_bytes())?;
    let config: ServiceConfig = from_value(value.clone())?;

    let app = &config.application;
    println!(
        "application: {} {} ({}), authors {:?}",
        app.name, app.version, app.environment, app.authors
    );
    assert_eq!(app.authors, ["Alice Johnson", "Bob Smith"]);

    let server = &config.server;
    println!(
        "server: listen {}, {} workers, {} connections, body up to {} bytes",
        server.listen,
        server.worker_threads.unwrap_or(1),
        server.max_connections,
        server.max_body
    );
    println!(
        "  timeouts: request {:?}, keepalive {:?}, shutdown {:?}",
        server.request_timeout, server.keepalive, server.shutdown_timeout
    );
    assert_eq!(server.listen, "0.0.0.0:8080");
    assert_eq!(server.max_connections, 10_000);
    assert_eq!(server.max_body, 10 * 1024 * 1024);
    assert_eq!(server.request_timeout, Duration::from_secs(30));
    if let Some(tls) = &server.tls {
        println!(
            "  tls: enabled {}, {} / {}, protocols {:?}",
            tls.enabled, tls.certificate, tls.certificate_key, tls.protocols
        );
        assert!(tls.enabled);
        assert_eq!(tls.protocols, ["TLSv1.2", "TLSv1.3"]);
    }

    let mw = &config.middleware;
    println!(
        "cors: origins {:?}, methods {:?}, max age {} s",
        mw.cors.origins, mw.cors.methods, mw.cors.max_age
    );
    println!(
        "rate limit: enabled {}, {}/min, burst {}, by {}",
        mw.rate_limit.enabled,
        mw.rate_limit.requests_per_minute,
        mw.rate_limit.burst,
        mw.rate_limit.key
    );
    println!(
        "compression: {:?} from {} bytes",
        mw.compression.algorithms, mw.compression.min_size
    );
    assert_eq!(mw.cors.methods, ["GET", "POST", "PUT", "DELETE"]);
    assert_eq!(mw.cors.max_age, 86_400);
    assert!(mw.rate_limit.enabled);
    assert_eq!(mw.compression.min_size, 1024);

    for (name, upstream) in &config.upstream {
        println!(
            "upstream {name}: {:?}, health check {:?}",
            upstream.servers, upstream.health_check
        );
    }
    assert_eq!(config.upstream["api_backend"].servers.len(), 2);

    for (path, route) in &config.location {
        let target = match (&route.proxy_pass, &route.root, route.status) {
            (Some(upstream), _, _) => format!("proxy to {upstream}"),
            (_, Some(root), _) => format!("files from {root}"),
            (_, _, Some(status)) => format!("status {status}"),
            _ => "nothing".to_string(),
        };
        println!(
            "location {path}: {target} (timeout {:?}, cache {:?})",
            route.timeout, route.cache_control
        );
    }
    let paths: Vec<&str> = config.location.keys().map(String::as_str).collect();
    assert_eq!(paths, ["/api", "/health", "/static"]);
    assert_eq!(
        config.location["/api"].timeout,
        Some(Duration::from_secs(30))
    );
    assert_eq!(config.location["/health"].status, Some(200));

    // The parsed tree keeps UCL's types, which the structs above flatten: `max_age` is a time.
    let cors = value.as_object().unwrap()["middleware"]
        .as_object()
        .unwrap()["cors"]
        .as_object()
        .unwrap();
    assert_eq!(cors["max_age"], UclValue::Time(86_400.0));

    println!("\nThe configuration maps onto the service structs.");
    Ok(())
}

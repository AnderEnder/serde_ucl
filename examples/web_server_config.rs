//! Web Server Configuration Example
//!
//! This example demonstrates how to use UCL for configuring a web server
//! with multiple environments, middleware settings, and database connections.

use serde::Deserialize;
use std::collections::HashMap;
use ucl_lexer::{
    ChainedVariableHandler, EnvironmentVariableHandler, MapVariableHandler, from_str_with_variables,
};

#[derive(Debug, Deserialize)]
struct WebServerConfig {
    server: ServerConfig,
    database: DatabaseConfig,
    middleware: MiddlewareConfig,
    logging: LoggingConfig,
    features: Vec<String>,
    #[serde(default)]
    environment_overrides: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
    #[serde(default = "default_workers")]
    workers: u32,
    max_connections: u32,
    timeout: f64, // in seconds
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
    pool_size: u32,
    timeout: f64,
    retry_attempts: u32,
    #[serde(default)]
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
    max_age: f64,
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
    min_size: u64, // in bytes
}

#[derive(Debug, Deserialize)]
struct LoggingConfig {
    level: String,
    format: String,
    output: String,
    max_file_size: u64, // in bytes
    rotation_count: u32,
}

fn default_workers() -> u32 {
    4 // Default to 4 workers
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Web Server Configuration Example");
    println!("================================\n");

    // Example 1: Basic configuration
    demo_basic_config()?;

    // Example 2: Environment-specific configuration
    demo_environment_config()?;

    // Example 3: Configuration with variable expansion
    demo_variable_expansion()?;

    Ok(())
}

fn demo_basic_config() -> Result<(), Box<dyn std::error::Error>> {
    println!("1. Basic Web Server Configuration");
    println!("---------------------------------");

    let config_text = r#"
        # Web server configuration
        server {
            host = "0.0.0.0"
            port = 8080
            max_connections = 1000
            timeout = 30s
            
            ssl {
                cert_path = "/etc/ssl/certs/server.crt"
                key_path = "/etc/ssl/private/server.key"
                protocols = ["TLSv1.2", "TLSv1.3"]
            }
        }
        
        database {
            url = "postgresql://localhost:5432/myapp"
            pool_size = 10
            timeout = 5s
            retry_attempts = 3
            read_replicas = [
                "postgresql://replica1:5432/myapp",
                "postgresql://replica2:5432/myapp"
            ]
        }
        
        middleware {
            cors {
                enabled = true
                allowed_origins = ["https://example.com", "https://app.example.com"]
                allowed_methods = ["GET", "POST", "PUT", "DELETE"]
                max_age = 1h
            }
            
            rate_limiting {
                enabled = true
                requests_per_minute = 100
                burst_size = 20
            }
            
            compression {
                enabled = true
                algorithms = ["gzip", "br"]
                min_size = 1kb
            }
        }
        
        logging {
            level = "info"
            format = "json"
            output = "/var/log/myapp.log"
            max_file_size = 100mb
            rotation_count = 5
        }
        
        features = ["auth", "metrics", "health_check"]
    "#;

    let config: WebServerConfig =
        from_str_with_variables(config_text, Box::new(EnvironmentVariableHandler))?;

    println!("Parsed configuration:");
    println!("  Server: {}:{}", config.server.host, config.server.port);
    println!("  Workers: {}", config.server.workers);
    println!("  Database pool size: {}", config.database.pool_size);
    println!("  SSL enabled: {}", config.server.ssl.is_some());
    println!("  CORS enabled: {}", config.middleware.cors.enabled);
    println!(
        "  Rate limiting: {} req/min",
        config.middleware.rate_limiting.requests_per_minute
    );
    println!("  Features: {:?}", config.features);
    print_detailed_config(&config);
    println!();

    Ok(())
}

fn demo_environment_config() -> Result<(), Box<dyn std::error::Error>> {
    println!("2. Environment-Specific Configuration");
    println!("-------------------------------------");

    let config_text = r#"
        # Base configuration
        server {
            host = "localhost"
            port = 3000
            max_connections = 100
            timeout = 10s
        }
        
        database {
            url = "sqlite:///tmp/dev.db"
            pool_size = 5
            timeout = 2s
            retry_attempts = 1
        }
        
        middleware {
            cors {
                enabled = true
                allowed_origins = ["http://localhost:3000"]
                allowed_methods = ["GET", "POST"]
                max_age = 5min
            }
            
            rate_limiting {
                enabled = false
                requests_per_minute = 1000
                burst_size = 100
            }
            
            compression {
                enabled = false
                algorithms = ["gzip"]
                min_size = 0
            }
        }
        
        logging {
            level = "debug"
            format = "pretty"
            output = "stdout"
            max_file_size = 10mb
            rotation_count = 3
        }
        
        features = ["debug", "hot_reload"]
        
        # Environment-specific overrides
        environment_overrides {
            production {
                server.host = "0.0.0.0"
                server.port = 80
                server.max_connections = 10000
                database.url = "postgresql://prod-db:5432/myapp"
                database.pool_size = 50
                logging.level = "warn"
                logging.output = "/var/log/myapp.log"
                middleware.rate_limiting.enabled = true
                middleware.compression.enabled = true
            }
            
            staging {
                server.port = 8080
                database.url = "postgresql://staging-db:5432/myapp"
                database.pool_size = 20
                logging.level = "info"
            }
        }
    "#;

    let config: WebServerConfig =
        from_str_with_variables(config_text, Box::new(EnvironmentVariableHandler))?;

    println!("Development configuration:");
    println!("  Server: {}:{}", config.server.host, config.server.port);
    println!("  Database: {}", config.database.url);
    println!("  Log level: {}", config.logging.level);
    println!(
        "  Rate limiting: {}",
        config.middleware.rate_limiting.enabled
    );

    // In a real application, you would apply environment overrides here
    if let Some(prod_overrides) = config.environment_overrides.get("production") {
        println!("\nProduction overrides available:");
        println!("  {:?}", prod_overrides);
    }

    println!();
    Ok(())
}

fn demo_variable_expansion() -> Result<(), Box<dyn std::error::Error>> {
    println!("3. Configuration with Variable Expansion");
    println!("----------------------------------------");

    // Set up custom variables
    let mut custom_vars = std::collections::HashMap::new();
    custom_vars.insert("APP_NAME".to_string(), "my-web-app".to_string());
    custom_vars.insert("DB_HOST".to_string(), "database.internal".to_string());
    custom_vars.insert(
        "REDIS_URL".to_string(),
        "redis://cache.internal:6379".to_string(),
    );

    let variable_handler = ChainedVariableHandler::from_handlers(vec![
        Box::new(MapVariableHandler::from_map(custom_vars)),
        Box::new(EnvironmentVariableHandler),
    ]);

    let config_text = r#"
        # Configuration with variable expansion
        server {
            host = "${HOST:-0.0.0.0}"
            port = ${PORT:-8080}
            max_connections = ${MAX_CONNECTIONS:-1000}
            timeout = ${TIMEOUT:-30}s
        }
        
        database {
            url = "postgresql://${DB_USER:-app}:${DB_PASSWORD:-secret}@${DB_HOST}:${DB_PORT:-5432}/${DB_NAME:-${APP_NAME}}"
            pool_size = ${DB_POOL_SIZE:-10}
            timeout = ${DB_TIMEOUT:-5}s
            retry_attempts = 3
        }
        
        middleware {
            cors {
                enabled = ${CORS_ENABLED:-true}
                allowed_origins = ["${FRONTEND_URL:-http://localhost:3000}"]
                allowed_methods = ["GET", "POST", "PUT", "DELETE"]
                max_age = 1h
            }
            
            rate_limiting {
                enabled = ${RATE_LIMIT_ENABLED:-true}
                requests_per_minute = ${RATE_LIMIT_RPM:-100}
                burst_size = ${RATE_LIMIT_BURST:-20}
            }
            
            compression {
                enabled = ${COMPRESSION_ENABLED:-true}
                algorithms = ["gzip", "br"]
                min_size = 1kb
            }
        }
        
        logging {
            level = "${LOG_LEVEL:-info}"
            format = "${LOG_FORMAT:-json}"
            output = "${LOG_OUTPUT:-/var/log/${APP_NAME}.log}"
            max_file_size = ${LOG_MAX_SIZE:-100}mb
            rotation_count = ${LOG_ROTATION:-5}
        }
        
        features = ["auth", "metrics"]
        
        # Add Redis configuration using variables
        redis {
            url = "${REDIS_URL}"
            pool_size = ${REDIS_POOL_SIZE:-5}
            timeout = ${REDIS_TIMEOUT:-1}s
        }
    "#;

    // Note: This would fail in the current implementation since we don't have Redis in our struct
    // In a real application, you'd extend the config struct or use a more flexible approach

    println!("Configuration with variables:");
    println!("  APP_NAME: my-web-app");
    println!("  DB_HOST: database.internal");
    println!("  REDIS_URL: redis://cache.internal:6379");
    println!("  Other variables from environment (HOST, PORT, etc.)");
    println!("  Variable config template size: {}", config_text.len());

    // For demonstration, let's parse a simpler version
    let simple_config = r#"
        server {
            host = "${HOST:-0.0.0.0}"
            port = ${PORT:-8080}
            max_connections = 1000
            timeout = 30s
        }
        
        database {
            url = "postgresql://app:secret@${DB_HOST}:5432/${APP_NAME}"
            pool_size = 10
            timeout = 5s
            retry_attempts = 3
        }
        
        middleware {
            cors {
                enabled = true
                allowed_origins = ["http://localhost:3000"]
                allowed_methods = ["GET", "POST"]
                max_age = 1h
            }
            
            rate_limiting {
                enabled = true
                requests_per_minute = 100
                burst_size = 20
            }
            
            compression {
                enabled = true
                algorithms = ["gzip"]
                min_size = 1kb
            }
        }
        
        logging {
            level = "info"
            format = "json"
            output = "/var/log/${APP_NAME}.log"
            max_file_size = 100mb
            rotation_count = 5
        }
        
        features = ["auth", "metrics"]
    "#;

    let config: WebServerConfig =
        from_str_with_variables(simple_config, Box::new(variable_handler))?;

    println!("\nParsed configuration with variable expansion:");
    println!("  Server: {}:{}", config.server.host, config.server.port);
    println!("  Database URL: {}", config.database.url);
    println!("  Log output: {}", config.logging.output);
    println!();

    Ok(())
}

fn print_detailed_config(config: &WebServerConfig) {
    println!("Detailed configuration:");
    println!(
        "  Max connections: {} Timeout: {}s",
        config.server.max_connections, config.server.timeout
    );
    if let Some(ssl) = &config.server.ssl {
        println!(
            "  SSL cert: {} key: {}",
            ssl.cert_path, ssl.key_path
        );
        println!("  SSL protocols: {:?}", ssl.protocols);
    }
    println!(
        "  DB timeout: {}s retries: {}",
        config.database.timeout, config.database.retry_attempts
    );
    println!("  DB replicas: {:?}", config.database.read_replicas);
    println!(
        "  CORS methods: {:?} max age: {}s",
        config.middleware.cors.allowed_methods, config.middleware.cors.max_age
    );
    println!(
        "  CORS origins: {:?}",
        config.middleware.cors.allowed_origins
    );
    println!(
        "  Rate limit burst: {}",
        config.middleware.rate_limiting.burst_size
    );
    println!(
        "  Compression: {} {:?} min {}",
        config.middleware.compression.enabled,
        config.middleware.compression.algorithms,
        config.middleware.compression.min_size
    );
    println!(
        "  Log format: {} output: {}",
        config.logging.format, config.logging.output
    );
    println!(
        "  Log max size: {} rotation: {}",
        config.logging.max_file_size, config.logging.rotation_count
    );
}

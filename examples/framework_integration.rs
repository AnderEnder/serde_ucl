//! Framework Integration Examples
//!
//! This example demonstrates how to integrate UCL configuration parsing
//! with popular Rust web frameworks and libraries.

use serde::Deserialize;
use ucl_lexer::{EnvironmentVariableHandler, from_str, from_str_with_variables};

// Example integration with Axum web framework
#[derive(Debug, Deserialize, Clone)]
struct AxumConfig {
    server: ServerConfig,
    database: DatabaseConfig,
    middleware: AxumMiddlewareConfig,
    tracing: TracingConfig,
}

#[derive(Debug, Deserialize, Clone)]
struct ServerConfig {
    host: String,
    port: u16,
    graceful_shutdown_timeout: f64, // seconds
}

#[derive(Debug, Deserialize, Clone)]
struct DatabaseConfig {
    url: String,
    max_connections: u32,
    min_connections: u32,
    acquire_timeout: f64, // seconds
    idle_timeout: f64,    // seconds
}

#[derive(Debug, Deserialize, Clone)]
struct AxumMiddlewareConfig {
    cors: CorsConfig,
    compression: bool,
    request_id: bool,
    timeout: f64, // seconds
}

#[derive(Debug, Deserialize, Clone)]
struct CorsConfig {
    allow_origins: Vec<String>,
    allow_methods: Vec<String>,
    allow_headers: Vec<String>,
    max_age: f64, // seconds
}

#[derive(Debug, Deserialize, Clone)]
struct TracingConfig {
    level: String,
    format: String,
    jaeger: Option<JaegerConfig>,
}

#[derive(Debug, Deserialize, Clone)]
struct JaegerConfig {
    endpoint: String,
    service_name: String,
    sample_rate: f64,
}

// Example integration with Tokio runtime configuration
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
    thread_stack_size: Option<u64>, // bytes
    thread_name: String,
}

#[derive(Debug, Deserialize)]
struct TaskConfig {
    max_concurrent: usize,
    timeout: f64, // seconds
    retry_attempts: u32,
    backoff_multiplier: f64,
}

#[derive(Debug, Deserialize)]
struct MetricsConfig {
    enabled: bool,
    endpoint: String,
    interval: f64, // seconds
}

// Example integration with Serde JSON for API responses
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
    window_size: f64, // seconds
}

#[derive(Debug, Deserialize)]
struct AuthConfig {
    jwt_secret: String,
    token_expiry: f64,         // seconds
    refresh_token_expiry: f64, // seconds
    allowed_issuers: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ResponseFormatConfig {
    pretty_print: bool,
    include_metadata: bool,
    error_details: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Framework Integration Examples");
    println!("==============================\n");

    // Example 1: Axum web framework configuration
    demo_axum_integration()?;

    // Example 2: Tokio runtime configuration
    demo_tokio_integration()?;

    // Example 3: API service configuration
    demo_api_service_integration()?;

    // Example 4: Configuration composition
    demo_configuration_composition()?;

    Ok(())
}

fn demo_axum_integration() -> Result<(), Box<dyn std::error::Error>> {
    println!("1. Axum Web Framework Integration");
    println!("---------------------------------");

    let axum_config = r#"
        # Axum web server configuration
        server {
            host = "0.0.0.0"
            port = 3000
            graceful_shutdown_timeout = 30s
        }
        
        database {
            url = "postgresql://localhost:5432/myapp"
            max_connections = 20
            min_connections = 5
            acquire_timeout = 10s
            idle_timeout = 600s
        }
        
        middleware {
            cors {
                allow_origins = ["http://localhost:3000", "https://myapp.com"]
                allow_methods = ["GET", "POST", "PUT", "DELETE", "OPTIONS"]
                allow_headers = ["Content-Type", "Authorization", "X-Requested-With"]
                max_age = 3600s
            }
            compression = true
            request_id = true
            timeout = 30s
        }
        
        tracing {
            level = "info"
            format = "json"
            
            jaeger {
                endpoint = "http://localhost:14268/api/traces"
                service_name = "my-web-service"
                sample_rate = 0.1
            }
        }
    "#;

    let config: AxumConfig = from_str(axum_config)?;

    println!("Axum configuration loaded:");
    println!("  Server: {}:{}", config.server.host, config.server.port);
    println!(
        "  Graceful shutdown: {}s",
        config.server.graceful_shutdown_timeout
    );
    println!("  Database URL: {}", config.database.url);
    println!(
        "  Database: {} connections ({}-{})",
        config.database.max_connections,
        config.database.min_connections,
        config.database.max_connections
    );
    println!(
        "  DB timeouts: acquire={}s idle={}s",
        config.database.acquire_timeout, config.database.idle_timeout
    );
    println!("  CORS origins: {:?}", config.middleware.cors.allow_origins);
    println!("  CORS methods: {:?}", config.middleware.cors.allow_methods);
    println!("  CORS headers: {:?}", config.middleware.cors.allow_headers);
    println!("  CORS max age: {}s", config.middleware.cors.max_age);
    println!(
        "  Middleware: compression={} request_id={}",
        config.middleware.compression, config.middleware.request_id
    );
    println!("  Tracing: {} level", config.tracing.level);
    println!("  Tracing format: {}", config.tracing.format);

    if let Some(jaeger) = &config.tracing.jaeger {
        println!(
            "  Jaeger: {} ({}% sampling)",
            jaeger.service_name,
            jaeger.sample_rate * 100.0
        );
        println!("  Jaeger endpoint: {}", jaeger.endpoint);
    }

    // Simulate Axum app setup
    println!("\n  Setting up Axum application...");
    setup_axum_app(config)?;

    println!();
    Ok(())
}

fn setup_axum_app(config: AxumConfig) -> Result<(), Box<dyn std::error::Error>> {
    // This would be the actual Axum setup in a real application
    println!("    ✓ Database connection pool configured");
    println!("    ✓ CORS middleware enabled");
    println!(
        "    ✓ Request timeout set to {}s",
        config.middleware.timeout
    );
    println!(
        "    ✓ Tracing initialized with {} level",
        config.tracing.level
    );
    println!(
        "    ✓ Server ready on {}:{}",
        config.server.host, config.server.port
    );

    Ok(())
}

fn demo_tokio_integration() -> Result<(), Box<dyn std::error::Error>> {
    println!("2. Tokio Runtime Configuration");
    println!("------------------------------");

    let tokio_config = r#"
        # Tokio runtime configuration
        runtime {
            worker_threads = 4
            max_blocking_threads = 512
            thread_stack_size = 2mb
            thread_name = "my-app-worker"
        }
        
        tasks {
            max_concurrent = 1000
            timeout = 60s
            retry_attempts = 3
            backoff_multiplier = 2.0
        }
        
        metrics {
            enabled = true
            endpoint = "/metrics"
            interval = 10s
        }
    "#;

    let config: TokioConfig = from_str(tokio_config)?;

    println!("Tokio configuration loaded:");
    println!("  Worker threads: {:?}", config.runtime.worker_threads);
    println!(
        "  Max blocking threads: {:?}",
        config.runtime.max_blocking_threads
    );
    println!(
        "  Thread stack size: {:?} bytes",
        config.runtime.thread_stack_size
    );
    println!("  Thread name: {}", config.runtime.thread_name);
    println!("  Max concurrent tasks: {}", config.tasks.max_concurrent);
    println!("  Task timeout: {}s", config.tasks.timeout);
    println!(
        "  Task retries: {} (backoff x{})",
        config.tasks.retry_attempts, config.tasks.backoff_multiplier
    );
    println!("  Metrics enabled: {}", config.metrics.enabled);
    println!("  Metrics interval: {}s", config.metrics.interval);

    // Simulate Tokio runtime setup
    println!("\n  Configuring Tokio runtime...");
    setup_tokio_runtime(config)?;

    println!();
    Ok(())
}

fn setup_tokio_runtime(config: TokioConfig) -> Result<(), Box<dyn std::error::Error>> {
    // This would be the actual Tokio runtime setup
    println!(
        "    ✓ Runtime configured with {} worker threads",
        config
            .runtime
            .worker_threads
            .unwrap_or_else(|| num_cpus::get())
    );
    println!(
        "    ✓ Task limits set: {} concurrent, {}s timeout",
        config.tasks.max_concurrent, config.tasks.timeout
    );
    println!(
        "    ✓ Metrics collection enabled at {}",
        config.metrics.endpoint
    );

    Ok(())
}

fn demo_api_service_integration() -> Result<(), Box<dyn std::error::Error>> {
    println!("3. API Service Configuration");
    println!("----------------------------");

    let api_config = r#"
        # API service configuration
        version = "v1"
        base_path = "/api/v1"
        
        rate_limiting {
            requests_per_second = 100
            burst_capacity = 200
            window_size = 60s
        }
        
        authentication {
            jwt_secret = "${JWT_SECRET}"
            token_expiry = 3600s      # 1 hour
            refresh_token_expiry = 604800s  # 1 week
            allowed_issuers = ["https://auth.myapp.com", "https://myapp.com"]
        }
        
        response_format {
            pretty_print = false
            include_metadata = true
            error_details = true
        }
    "#;

    // Use environment variable handler for JWT secret
    let config: ApiConfig =
        from_str_with_variables(api_config, Box::new(EnvironmentVariableHandler))?;

    println!("API configuration loaded:");
    println!("  Version: {}", config.version);
    println!("  Base path: {}", config.base_path);
    println!(
        "  Rate limit: {}/s (burst: {})",
        config.rate_limiting.requests_per_second, config.rate_limiting.burst_capacity
    );
    println!("  Rate limit window: {}s", config.rate_limiting.window_size);
    println!("  Token expiry: {}s", config.authentication.token_expiry);
    println!(
        "  Refresh token expiry: {}s",
        config.authentication.refresh_token_expiry
    );
    println!(
        "  JWT secret length: {}",
        config.authentication.jwt_secret.len()
    );
    println!(
        "  Allowed issuers: {:?}",
        config.authentication.allowed_issuers
    );
    println!("  Pretty print: {}", config.response_format.pretty_print);
    println!(
        "  Response metadata: {} error details: {}",
        config.response_format.include_metadata, config.response_format.error_details
    );

    // Simulate API service setup
    println!("\n  Setting up API service...");
    setup_api_service(config)?;

    println!();
    Ok(())
}

fn setup_api_service(config: ApiConfig) -> Result<(), Box<dyn std::error::Error>> {
    // This would be the actual API service setup
    println!(
        "    ✓ Rate limiting configured: {}/s",
        config.rate_limiting.requests_per_second
    );
    println!("    ✓ JWT authentication enabled");
    println!("    ✓ Response formatting configured");
    println!("    ✓ API endpoints available at {}", config.base_path);

    Ok(())
}

fn demo_configuration_composition() -> Result<(), Box<dyn std::error::Error>> {
    println!("4. Configuration Composition");
    println!("----------------------------");

    // Example of composing multiple configuration sources
    let base_config = r#"
        # Base application configuration
        app {
            name = "my-service"
            version = "1.0.0"
            environment = "development"
        }
        
        server {
            host = "localhost"
            port = 8080
        }
        
        features {
            auth = true
            metrics = true
            tracing = false
        }
    "#;

    let environment_overrides = r#"
        # Production overrides
        server {
            host = "0.0.0.0"
            port = 80
        }
        
        features {
            tracing = true
        }
        
        app {
            environment = "production"
        }
    "#;

    #[derive(Debug, Deserialize)]
    struct ComposedConfig {
        app: AppInfo,
        server: ServerInfo,
        features: FeatureFlags,
    }

    #[derive(Debug, Deserialize)]
    struct AppInfo {
        name: String,
        version: String,
        environment: String,
    }

    #[derive(Debug, Deserialize)]
    struct ServerInfo {
        host: String,
        port: u16,
    }

    #[derive(Debug, Deserialize)]
    struct FeatureFlags {
        auth: bool,
        metrics: bool,
        tracing: bool,
    }

    // Parse base configuration
    let base: ComposedConfig = from_str(base_config)?;
    println!("Base configuration:");
    println!(
        "  App: {} v{} ({})",
        base.app.name, base.app.version, base.app.environment
    );
    println!("  Server: {}:{}", base.server.host, base.server.port);
    println!(
        "  Features: auth={}, metrics={}, tracing={}",
        base.features.auth, base.features.metrics, base.features.tracing
    );

    // Parse environment overrides
    let overrides: ComposedConfig = from_str(environment_overrides)?;
    println!("\nEnvironment overrides:");
    println!("  App environment: {}", overrides.app.environment);
    println!(
        "  Server: {}:{}",
        overrides.server.host, overrides.server.port
    );
    println!("  Tracing enabled: {}", overrides.features.tracing);

    // In a real application, you would merge these configurations
    println!("\n  ✓ Configuration composition demonstrated");
    println!("  ✓ Multiple sources can be parsed and merged");

    println!();
    Ok(())
}

// Helper function to get CPU count (would normally use num_cpus crate)
mod num_cpus {
    pub fn get() -> usize {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    }
}

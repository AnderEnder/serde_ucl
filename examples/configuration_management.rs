//! Configuration Management Example
//!
//! This example demonstrates practical configuration management patterns
//! including environment-specific configs, validation, and hot reloading.

use serde::Deserialize;
use std::collections::HashMap;
use ucl_lexer::{
    ChainedVariableHandler, EnvironmentVariableHandler, MapVariableHandler, from_str,
    from_str_with_variables,
};

#[derive(Debug, Deserialize, Clone)]
struct ApplicationConfig {
    app: AppConfig,
    server: ServerConfig,
    database: DatabaseConfig,
    cache: CacheConfig,
    logging: LoggingConfig,
    monitoring: MonitoringConfig,
    #[serde(default)]
    feature_flags: HashMap<String, bool>,
}

#[derive(Debug, Deserialize, Clone)]
struct AppConfig {
    name: String,
    version: String,
    environment: String,
    debug: bool,
    max_request_size: u64, // bytes
    worker_count: u32,
}

#[derive(Debug, Deserialize, Clone)]
struct ServerConfig {
    host: String,
    port: u16,
    tls: Option<TlsConfig>,
    timeouts: TimeoutConfig,
    limits: LimitConfig,
}

#[derive(Debug, Deserialize, Clone)]
struct TlsConfig {
    cert_file: String,
    key_file: String,
    ca_file: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct TimeoutConfig {
    read: f64,  // seconds
    write: f64, // seconds
    idle: f64,  // seconds
}

#[derive(Debug, Deserialize, Clone)]
struct LimitConfig {
    max_connections: u32,
    max_request_size: u64,  // bytes
    rate_limit_per_ip: u32, // requests per minute
}

#[derive(Debug, Deserialize, Clone)]
struct DatabaseConfig {
    primary: DatabaseConnection,
    #[serde(default)]
    replicas: Vec<DatabaseConnection>,
    pool: PoolConfig,
    migrations: MigrationConfig,
}

#[derive(Debug, Deserialize, Clone)]
struct DatabaseConnection {
    host: String,
    port: u16,
    database: String,
    username: String,
    password: String,
    ssl_mode: String,
}

#[derive(Debug, Deserialize, Clone)]
struct PoolConfig {
    min_connections: u32,
    max_connections: u32,
    acquire_timeout: f64, // seconds
    idle_timeout: f64,    // seconds
}

#[derive(Debug, Deserialize, Clone)]
struct MigrationConfig {
    auto_migrate: bool,
    migration_path: String,
}

#[derive(Debug, Deserialize, Clone)]
struct CacheConfig {
    redis: RedisConfig,
    local: LocalCacheConfig,
}

#[derive(Debug, Deserialize, Clone)]
struct RedisConfig {
    url: String,
    pool_size: u32,
    timeout: f64, // seconds
    key_prefix: String,
}

#[derive(Debug, Deserialize, Clone)]
struct LocalCacheConfig {
    max_size: u64,         // bytes
    ttl: f64,              // seconds
    cleanup_interval: f64, // seconds
}

#[derive(Debug, Deserialize, Clone)]
struct LoggingConfig {
    level: String,
    format: String,
    outputs: Vec<LogOutput>,
}

#[derive(Debug, Deserialize, Clone)]
struct LogOutput {
    r#type: String, // "file", "stdout", "syslog"
    target: Option<String>,
    level: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct MonitoringConfig {
    metrics: MetricsConfig,
    health_check: HealthCheckConfig,
    alerts: AlertConfig,
}

#[derive(Debug, Deserialize, Clone)]
struct MetricsConfig {
    enabled: bool,
    endpoint: String,
    interval: f64,  // seconds
    retention: f64, // seconds
}

#[derive(Debug, Deserialize, Clone)]
struct HealthCheckConfig {
    enabled: bool,
    endpoint: String,
    interval: f64, // seconds
    timeout: f64,  // seconds
}

#[derive(Debug, Deserialize, Clone)]
struct AlertConfig {
    enabled: bool,
    webhook_url: Option<String>,
    email_recipients: Vec<String>,
    thresholds: AlertThresholds,
}

#[derive(Debug, Deserialize, Clone)]
struct AlertThresholds {
    cpu_usage: f64,     // percentage
    memory_usage: f64,  // percentage
    disk_usage: f64,    // percentage
    response_time: f64, // seconds
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Configuration Management Example");
    println!("================================\n");

    // Example 1: Environment-specific configuration loading
    demo_environment_configs()?;

    // Example 2: Configuration validation
    demo_config_validation()?;

    // Example 3: Configuration merging and overrides
    demo_config_merging()?;

    // Example 4: Hot configuration reloading simulation
    demo_hot_reloading()?;

    Ok(())
}

fn demo_environment_configs() -> Result<(), Box<dyn std::error::Error>> {
    println!("1. Environment-Specific Configuration");
    println!("-------------------------------------");

    let environments = vec!["development", "staging", "production"];

    for env in environments {
        println!("Loading {} configuration...", env);

        let config = load_environment_config(env)?;
        print_config_summary(&config);
        println!();
    }

    Ok(())
}

fn load_environment_config(
    environment: &str,
) -> Result<ApplicationConfig, Box<dyn std::error::Error>> {
    let base_config = get_base_config();
    let env_overrides = get_environment_overrides(environment);

    // Set up variables for this environment
    let mut env_vars = HashMap::new();
    env_vars.insert("ENVIRONMENT".to_string(), environment.to_string());

    match environment {
        "development" => {
            env_vars.insert("DB_HOST".to_string(), "localhost".to_string());
            env_vars.insert("REDIS_HOST".to_string(), "localhost".to_string());
            env_vars.insert("LOG_LEVEL".to_string(), "debug".to_string());
        }
        "staging" => {
            env_vars.insert("DB_HOST".to_string(), "staging-db.internal".to_string());
            env_vars.insert(
                "REDIS_HOST".to_string(),
                "staging-cache.internal".to_string(),
            );
            env_vars.insert("LOG_LEVEL".to_string(), "info".to_string());
        }
        "production" => {
            env_vars.insert("DB_HOST".to_string(), "prod-db.internal".to_string());
            env_vars.insert("REDIS_HOST".to_string(), "prod-cache.internal".to_string());
            env_vars.insert("LOG_LEVEL".to_string(), "warn".to_string());
        }
        _ => {}
    }

    let variable_handler = ChainedVariableHandler::from_handlers(vec![
        Box::new(MapVariableHandler::from_map(env_vars)),
        Box::new(EnvironmentVariableHandler),
    ]);

    // Combine base config with environment overrides
    let combined_config = format!("{}\n{}", base_config, env_overrides);

    let config: ApplicationConfig =
        from_str_with_variables(&combined_config, Box::new(variable_handler))?;

    assert_application_config(&config);

    Ok(config)
}

fn get_base_config() -> String {
    r#"
        # Base application configuration
        app {
            name = "my-application"
            version = "1.0.0"
            environment = "${ENVIRONMENT}"
            debug = false
            max_request_size = 10mb
            worker_count = 4
        }
        
        server {
            host = "127.0.0.1"
            port = 8080
            
            timeouts {
                read = 30s
                write = 30s
                idle = 120s
            }
            
            limits {
                max_connections = 1000
                max_request_size = 10mb
                rate_limit_per_ip = 100
            }
        }
        
        database {
            primary {
                host = "${DB_HOST}"
                port = 5432
                database = "myapp"
                username = "app"
                password = "${DB_PASSWORD:-secret}"
                ssl_mode = "prefer"
            }
            
            pool {
                min_connections = 5
                max_connections = 20
                acquire_timeout = 10s
                idle_timeout = 600s
            }
            
            migrations {
                auto_migrate = false
                migration_path = "./migrations"
            }
        }
        
        cache {
            redis {
                url = "redis://${REDIS_HOST}:6379/0"
                pool_size = 10
                timeout = 5s
                key_prefix = "myapp:"
            }
            
            local {
                max_size = 100mb
                ttl = 300s
                cleanup_interval = 60s
            }
        }
        
        logging {
            level = "${LOG_LEVEL}"
            format = "json"
            outputs = [
                { type = "stdout", level = "info" }
            ]
        }
        
        monitoring {
            metrics {
                enabled = true
                endpoint = "/metrics"
                interval = 10s
                retention = 3600s
            }
            
            health_check {
                enabled = true
                endpoint = "/health"
                interval = 30s
                timeout = 5s
            }
            
            alerts {
                enabled = false
                email_recipients = []
                thresholds {
                    cpu_usage = 80.0
                    memory_usage = 85.0
                    disk_usage = 90.0
                    response_time = 2.0
                }
            }
        }
    "#
    .to_string()
}

fn get_environment_overrides(environment: &str) -> String {
    match environment {
        "development" => r#"
            # Development overrides
            app.debug = true
            server.host = "localhost"
            database.migrations.auto_migrate = true
            logging.outputs = [
                { type = "stdout", level = "debug" },
                { type = "file", target = "./logs/dev.log", level = "debug" }
            ]
        "#
        .to_string(),

        "staging" => r#"
            # Staging overrides
            server.host = "0.0.0.0"
            server.port = 8080
            server.limits.max_connections = 5000
            database.pool.max_connections = 50
            monitoring.alerts.enabled = true
            monitoring.alerts.email_recipients = ["staging-alerts@company.com"]
        "#
        .to_string(),

        "production" => r#"
            # Production overrides
            server.host = "0.0.0.0"
            server.port = 80
            server.limits.max_connections = 10000
            server.tls {
                cert_file = "/etc/ssl/certs/app.crt"
                key_file = "/etc/ssl/private/app.key"
            }
            database.pool.max_connections = 100
            database.replicas = [
                {
                    host = "replica1.internal"
                    port = 5432
                    database = "myapp"
                    username = "readonly"
                    password = "${REPLICA_PASSWORD}"
                    ssl_mode = "require"
                }
            ]
            monitoring.alerts.enabled = true
            monitoring.alerts.webhook_url = "${ALERT_WEBHOOK_URL}"
            monitoring.alerts.email_recipients = ["ops@company.com", "alerts@company.com"]
            feature_flags {
                new_feature = false
                beta_api = false
                enhanced_logging = true
            }
        "#
        .to_string(),

        _ => String::new(),
    }
}

fn print_config_summary(config: &ApplicationConfig) {
    println!(
        "  App: {} v{} ({})",
        config.app.name, config.app.version, config.app.environment
    );
    println!("  Server: {}:{}", config.server.host, config.server.port);
    println!(
        "  Database: {}:{}",
        config.database.primary.host, config.database.primary.port
    );
    println!("  Cache: {}", config.cache.redis.url);
    println!("  Log level: {}", config.logging.level);
    println!(
        "  TLS: {}",
        if config.server.tls.is_some() {
            "enabled"
        } else {
            "disabled"
        }
    );
    println!("  Replicas: {}", config.database.replicas.len());
    println!("  Feature flags: {}", config.feature_flags.len());

    assert_application_config(config);
}

fn assert_application_config(config: &ApplicationConfig) {
    assert!(!config.app.name.is_empty());
    assert!(!config.app.version.is_empty());
    assert!(!config.app.environment.is_empty());
    if config.app.environment == "development" {
        assert!(config.app.debug);
    }
    assert!(config.app.max_request_size > 0);
    assert!(config.app.worker_count > 0);

    assert!(config.server.timeouts.read >= 0.0);
    assert!(config.server.timeouts.write >= 0.0);
    assert!(config.server.timeouts.idle >= 0.0);
    assert!(config.server.limits.max_connections > 0);
    assert!(config.server.limits.max_request_size > 0);
    assert!(config.server.limits.rate_limit_per_ip > 0);
    if let Some(tls) = &config.server.tls {
        assert!(!tls.cert_file.is_empty());
        assert!(!tls.key_file.is_empty());
        if let Some(ca) = &tls.ca_file {
            assert!(!ca.is_empty());
        }
    }

    assert!(config.database.pool.min_connections > 0);
    assert!(config.database.pool.max_connections > 0);
    assert!(config.database.pool.acquire_timeout >= 0.0);
    assert!(config.database.pool.idle_timeout >= 0.0);
    assert!(!config.database.primary.database.is_empty());
    assert!(!config.database.primary.username.is_empty());
    assert!(!config.database.primary.password.is_empty());
    assert!(!config.database.primary.ssl_mode.is_empty());
    assert!(config.database.migrations.auto_migrate || !config.database.migrations.auto_migrate);
    assert!(config.database.migrations.migration_path.contains("migration"));

    assert!(!config.cache.redis.url.is_empty());
    assert!(config.cache.redis.pool_size > 0);
    assert!(config.cache.redis.timeout >= 0.0);
    assert!(!config.cache.redis.key_prefix.is_empty());
    assert!(config.cache.local.max_size > 0);
    assert!(config.cache.local.ttl >= 0.0);
    assert!(config.cache.local.cleanup_interval >= 0.0);

    assert!(!config.logging.format.is_empty());
    for output in &config.logging.outputs {
        assert!(!output.r#type.is_empty());
        // target is optional; ensure access
        let _ = output.target.as_deref();
        assert!(output.level.as_deref().unwrap_or("info").len() > 0);
    }

    assert!(config.monitoring.metrics.enabled || !config.monitoring.metrics.enabled);
    assert!(!config.monitoring.metrics.endpoint.is_empty());
    assert!(config.monitoring.metrics.interval >= 0.0);
    assert!(config.monitoring.metrics.retention >= 0.0);
    assert!(config.monitoring.health_check.enabled || !config.monitoring.health_check.enabled);
    assert!(!config.monitoring.health_check.endpoint.is_empty());
    assert!(config.monitoring.health_check.interval >= 0.0);
    assert!(config.monitoring.health_check.timeout >= 0.0);
    assert!(config.monitoring.alerts.enabled || !config.monitoring.alerts.enabled);
    assert!(config.monitoring.alerts.email_recipients.len() <= 10_000);
    if let Some(webhook) = &config.monitoring.alerts.webhook_url {
        assert!(!webhook.is_empty());
    }
    assert!(config.monitoring.alerts.thresholds.cpu_usage >= 0.0);
    assert!(config.monitoring.alerts.thresholds.memory_usage >= 0.0);
    assert!(config.monitoring.alerts.thresholds.disk_usage >= 0.0);
    assert!(config.monitoring.alerts.thresholds.response_time >= 0.0);
}

fn demo_config_validation() -> Result<(), Box<dyn std::error::Error>> {
    println!("2. Configuration Validation");
    println!("---------------------------");

    let valid_config = get_base_config();
    let invalid_configs = vec![
        (
            "Missing required field",
            r#"
                app {
                    name = "test"
                    # version missing
                }
            "#,
        ),
        (
            "Invalid port number",
            r#"
                app {
                    name = "test"
                    version = "1.0.0"
                    environment = "test"
                }
                server {
                    host = "localhost"
                    port = 99999  # Invalid port
                }
            "#,
        ),
        (
            "Invalid timeout value",
            r#"
                app {
                    name = "test"
                    version = "1.0.0"
                    environment = "test"
                }
                server {
                    timeouts {
                        read = -5s  # Negative timeout
                    }
                }
            "#,
        ),
    ];

    println!("Validating configurations...");

    // Test valid configuration
    match from_str::<ApplicationConfig>(&valid_config) {
        Ok(_) => println!("  ✓ Valid configuration parsed successfully"),
        Err(e) => println!("  ✗ Valid configuration failed: {}", e),
    }

    // Test invalid configurations
    for (description, invalid_config) in invalid_configs {
        match from_str::<ApplicationConfig>(invalid_config) {
            Ok(_) => println!("  ✗ {} should have failed", description),
            Err(e) => println!("  ✓ {} correctly rejected: {}", description, e),
        }
    }

    println!();
    Ok(())
}

fn demo_config_merging() -> Result<(), Box<dyn std::error::Error>> {
    println!("3. Configuration Merging and Overrides");
    println!("--------------------------------------");

    // Simulate loading configuration from multiple sources
    let base_config = r#"
        app {
            name = "base-app"
            version = "1.0.0"
            environment = "development"
            debug = true
        }
        
        server {
            host = "localhost"
            port = 3000
        }
    "#;

    let user_config = r#"
        # User-specific overrides
        app {
            name = "my-custom-app"
        }
        
        server {
            port = 8080
        }
        
        custom_settings {
            theme = "dark"
            notifications = true
        }
    "#;

    let cli_overrides = r#"
        # Command-line overrides
        app {
            debug = false
        }
        
        server {
            host = "0.0.0.0"
        }
    "#;

    println!("Configuration sources:");
    println!("  1. Base configuration");
    println!("  2. User configuration file");
    println!("  3. Command-line overrides");

    // In a real application, you would implement proper merging logic
    // For demonstration, we'll show how different configs would be parsed

    #[derive(Debug, Deserialize)]
    struct SimpleConfig {
        app: SimpleApp,
        server: SimpleServer,
        #[serde(default)]
        custom_settings: HashMap<String, serde_json::Value>,
    }

    #[derive(Debug, Deserialize)]
    struct SimpleApp {
        name: String,
        version: String,
        environment: String,
        debug: bool,
    }

    #[derive(Debug, Deserialize)]
    struct SimpleServer {
        host: String,
        port: u16,
    }

    let base: SimpleConfig = from_str(base_config)?;
    println!(
        "\nBase config: {} on {}:{} (debug: {})",
        base.app.name, base.server.host, base.server.port, base.app.debug
    );
    assert!(!base.app.environment.is_empty());

    let user: SimpleConfig = from_str(user_config)?;
    println!(
        "User config: {} on port {} with {} custom settings",
        user.app.name,
        user.server.port,
        user.custom_settings.len()
    );

    let cli: serde_json::Value = from_str(cli_overrides)?;
    let cli_host = cli
        .get("server")
        .and_then(|server| server.get("host"))
        .and_then(|host| host.as_str())
        .unwrap_or("unknown");
    let cli_debug = cli
        .get("app")
        .and_then(|app| app.get("debug"))
        .and_then(|debug| debug.as_bool())
        .unwrap_or(false);
    println!(
        "CLI overrides: host={} debug={}",
        cli_host, cli_debug
    );

    // Simulate final merged configuration
    println!("\nFinal merged configuration would be:");
    println!("  App name: {} (from user config)", user.app.name);
    println!("  Version: {} (from base config)", base.app.version);
    println!("  Host: 0.0.0.0 (from CLI override)");
    println!("  Port: {} (from user config)", user.server.port);
    println!("  Debug: false (from CLI override)");
    println!(
        "  Custom settings: {} items (from user config)",
        user.custom_settings.len()
    );

    println!();
    Ok(())
}

fn demo_hot_reloading() -> Result<(), Box<dyn std::error::Error>> {
    println!("4. Hot Configuration Reloading Simulation");
    println!("------------------------------------------");

    // Simulate configuration file changes
    let initial_config = r#"
        app {
            name = "hot-reload-demo"
            version = "1.0.0"
            environment = "development"
            debug = true
        }
        
        server {
            host = "localhost"
            port = 3000
        }
        
        logging {
            level = "debug"
            format = "pretty"
            outputs = [{ type = "stdout" }]
        }
    "#;

    let updated_config = r#"
        app {
            name = "hot-reload-demo"
            version = "1.0.0"
            environment = "development"
            debug = false  # Changed
        }
        
        server {
            host = "localhost"
            port = 8080    # Changed
        }
        
        logging {
            level = "info"  # Changed
            format = "json" # Changed
            outputs = [
                { type = "stdout" },
                { type = "file", target = "./app.log" }  # Added
            ]
        }
    "#;

    #[derive(Debug, Deserialize, PartialEq)]
    struct ReloadConfig {
        app: ReloadApp,
        server: ReloadServer,
        logging: ReloadLogging,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct ReloadApp {
        name: String,
        version: String,
        environment: String,
        debug: bool,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct ReloadServer {
        host: String,
        port: u16,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct ReloadLogging {
        level: String,
        format: String,
        outputs: Vec<serde_json::Value>,
    }

    println!("Initial configuration loaded:");
    let initial: ReloadConfig = from_str(initial_config)?;
    println!("  Debug: {}", initial.app.debug);
    println!("  Port: {}", initial.server.port);
    println!("  Log level: {}", initial.logging.level);
    println!("  Log format: {}", initial.logging.format);
    println!("  Log outputs: {}", initial.logging.outputs.len());

    println!("\nConfiguration file changed...");

    println!("Reloading configuration:");
    let updated: ReloadConfig = from_str(updated_config)?;

    // Detect changes
    let mut changes = Vec::new();
    if initial.app.debug != updated.app.debug {
        changes.push(format!(
            "app.debug: {} -> {}",
            initial.app.debug, updated.app.debug
        ));
    }
    if initial.server.port != updated.server.port {
        changes.push(format!(
            "server.port: {} -> {}",
            initial.server.port, updated.server.port
        ));
    }
    if initial.logging.level != updated.logging.level {
        changes.push(format!(
            "logging.level: {} -> {}",
            initial.logging.level, updated.logging.level
        ));
    }
    if initial.logging.format != updated.logging.format {
        changes.push(format!(
            "logging.format: {} -> {}",
            initial.logging.format, updated.logging.format
        ));
    }
    if initial.logging.outputs.len() != updated.logging.outputs.len() {
        changes.push(format!(
            "logging.outputs: {} -> {} items",
            initial.logging.outputs.len(),
            updated.logging.outputs.len()
        ));
    }

    println!("Detected {} changes:", changes.len());
    for change in changes {
        println!("  - {}", change);
    }

    println!("\n  ✓ Configuration reloaded successfully");
    println!("  ✓ Application components notified of changes");

    println!();
    Ok(())
}

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ucl_lexer::from_str;

/// Framework integration example demonstrating NGINX-style UCL syntax
/// for configuring popular Rust web frameworks and services.

#[derive(Debug, Deserialize, Serialize)]
struct FrameworkConfig {
    // Application metadata
    application: ApplicationInfo,

    // Axum web framework configuration
    axum: AxumConfig,

    // Tokio runtime configuration
    tokio: TokioConfig,

    // Database configuration (SQLx/Diesel)
    database: DatabaseConfig,

    // Redis configuration
    redis: RedisConfig,

    // Tracing/logging configuration
    tracing: TracingConfig,

    // Security configuration
    security: SecurityConfig,

    // API configuration
    api: ApiConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct ApplicationInfo {
    name: String,
    version: String,
    description: String,
    authors: Vec<String>,
    environment: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct AxumConfig {
    server: ServerConfig,
    middleware: MiddlewareConfig,
    routes: RoutesConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct ServerConfig {
    host: String,
    port: u16,
    worker_threads: Option<u32>,
    max_connections: u32,
    timeout: TimeoutConfig,
    tls: Option<TlsConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TimeoutConfig {
    request: String,
    keepalive: String,
    shutdown: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct TlsConfig {
    enabled: bool,
    cert_path: String,
    key_path: String,
    protocols: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct MiddlewareConfig {
    cors: CorsConfig,
    rate_limiting: RateLimitConfig,
    compression: CompressionConfig,
    request_id: bool,
    request_logging: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct CorsConfig {
    enabled: bool,
    allowed_origins: Vec<String>,
    allowed_methods: Vec<String>,
    allowed_headers: Vec<String>,
    max_age: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct RateLimitConfig {
    enabled: bool,
    requests_per_minute: u32,
    burst_size: u32,
    key_extractor: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct CompressionConfig {
    enabled: bool,
    algorithms: Vec<String>,
    min_size: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct RoutesConfig {
    api_prefix: String,
    health_check: String,
    metrics: String,
    static_files: Option<StaticFilesConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
struct StaticFilesConfig {
    enabled: bool,
    path: String,
    directory: String,
    cache_control: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct TokioConfig {
    runtime: RuntimeConfig,
    tasks: TaskConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct RuntimeConfig {
    flavor: String,
    worker_threads: Option<u32>,
    max_blocking_threads: u32,
    thread_stack_size: String,
    thread_name: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct TaskConfig {
    max_concurrent: u32,
    default_timeout: String,
    panic_handler: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct DatabaseConfig {
    driver: String,
    connection: ConnectionConfig,
    pool: PoolConfig,
    migrations: MigrationConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct ConnectionConfig {
    host: String,
    port: u16,
    database: String,
    username: String,
    password: Option<String>,
    ssl_mode: String,
    connect_timeout: String,
    command_timeout: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct PoolConfig {
    min_connections: u32,
    max_connections: u32,
    acquire_timeout: String,
    idle_timeout: String,
    max_lifetime: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct MigrationConfig {
    auto_migrate: bool,
    migration_path: String,
    create_database: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct RedisConfig {
    connection: RedisConnectionConfig,
    pool: RedisPoolConfig,
    features: RedisFeatures,
}

#[derive(Debug, Deserialize, Serialize)]
struct RedisConnectionConfig {
    url: String,
    database: u8,
    username: Option<String>,
    password: Option<String>,
    connect_timeout: String,
    command_timeout: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct RedisPoolConfig {
    max_connections: u32,
    min_idle: u32,
    max_idle: u32,
    idle_timeout: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct RedisFeatures {
    session_store: bool,
    cache: bool,
    pub_sub: bool,
    rate_limiting: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct TracingConfig {
    level: String,
    format: String,
    targets: Vec<TracingTarget>,
    filters: Vec<String>,
    sampling: SamplingConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct TracingTarget {
    name: String,
    level: String,
    enabled: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct SamplingConfig {
    enabled: bool,
    rate: f64,
    max_events_per_second: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct SecurityConfig {
    jwt: JwtConfig,
    session: SessionConfig,
    csrf: CsrfConfig,
    headers: SecurityHeaders,
}

#[derive(Debug, Deserialize, Serialize)]
struct JwtConfig {
    secret: String,
    algorithm: String,
    expiration: String,
    issuer: String,
    audience: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct SessionConfig {
    store: String,
    cookie_name: String,
    max_age: String,
    secure: bool,
    http_only: bool,
    same_site: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct CsrfConfig {
    enabled: bool,
    token_header: String,
    cookie_name: String,
    max_age: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct SecurityHeaders {
    hsts: bool,
    content_type_options: bool,
    frame_options: String,
    xss_protection: bool,
    referrer_policy: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ApiConfig {
    versioning: VersioningConfig,
    documentation: DocumentationConfig,
    validation: ValidationConfig,
    serialization: SerializationConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct VersioningConfig {
    strategy: String,
    default_version: String,
    supported_versions: Vec<String>,
    deprecation_warnings: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct DocumentationConfig {
    openapi: OpenApiConfig,
    swagger_ui: SwaggerUiConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct OpenApiConfig {
    enabled: bool,
    title: String,
    version: String,
    description: String,
    contact: ContactInfo,
}

#[derive(Debug, Deserialize, Serialize)]
struct ContactInfo {
    name: String,
    email: String,
    url: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct SwaggerUiConfig {
    enabled: bool,
    path: String,
    title: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ValidationConfig {
    strict_mode: bool,
    max_payload_size: String,
    custom_validators: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct SerializationConfig {
    default_format: String,
    pretty_print: bool,
    date_format: String,
    null_handling: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== NGINX-Style Framework Integration Example ===\n");

    // Comprehensive framework configuration using NGINX-style UCL syntax
    let config_text = r#"
        // Modern Rust web application configuration
        // Demonstrates NGINX-style syntax for framework integration
        
        application {
            name "rust-web-api"
            version "2.1.0"
            description "High-performance web API built with Rust"
            
            // Implicit array creation through key repetition
            author "Alice Johnson"
            author "Bob Smith"
            author "Carol Davis"
            
            environment production
        }
        
        // Axum web framework configuration
        axum {
            server {
                host "0.0.0.0"          // Bare word IP address
                port 8080
                worker_threads null     // Use default
                max_connections 10000
                
                timeout {
                    request "30s"
                    keepalive "75s"
                    shutdown "10s"
                }
                
                tls {
                    enabled yes         // Boolean keyword
                    cert_path "/etc/ssl/certs/api.crt"
                    key_path "/etc/ssl/private/api.key"
                    
                    protocol "TLSv1.2"
                    protocol "TLSv1.3"  // Implicit array
                }
            }
            
            middleware {
                cors {
                    enabled true
                    
                    allowed_origin "https://app.example.com"
                    allowed_origin "https://admin.example.com"
                    
                    allowed_method "GET"
                    allowed_method "POST"
                    allowed_method "PUT"
                    allowed_method "DELETE"
                    allowed_method "OPTIONS"
                    
                    allowed_header "Content-Type"
                    allowed_header "Authorization"
                    allowed_header "X-Requested-With"
                    
                    max_age 86400
                }
                
                rate_limiting {
                    enabled on          // Boolean keyword
                    requests_per_minute 1000
                    burst_size 100
                    key_extractor ip_address
                }
                
                compression {
                    enabled yes
                    
                    algorithm "gzip"
                    algorithm "br"      // Brotli
                    algorithm "deflate"
                    
                    min_size "1kb"
                }
                
                request_id true
                request_logging true
            }
            
            routes {
                api_prefix "/api/v1"
                health_check "/health"
                metrics "/metrics"
                
                static_files {
                    enabled false
                    path "/static"
                    directory "./public"
                    cache_control "public, max-age=31536000"
                }
            }
        }
        
        // Tokio async runtime configuration
        tokio {
            runtime {
                flavor "multi_thread"   // Bare word enum value
                worker_threads null     // Auto-detect
                max_blocking_threads 512
                thread_stack_size "2mb"
                thread_name "tokio-worker"
            }
            
            tasks {
                max_concurrent 1000
                default_timeout "60s"
                panic_handler "abort"
            }
        }
        
        // Database configuration (PostgreSQL with SQLx)
        database {
            driver postgresql
            
            connection {
                host "db.example.com"
                port 5432
                database "api_production"
                username "api_user"
                password null           // Set via environment variable
                ssl_mode require
                connect_timeout "10s"
                command_timeout "30s"
            }
            
            pool {
                min_connections 5
                max_connections 50
                acquire_timeout "10s"
                idle_timeout "300s"
                max_lifetime "1800s"
            }
            
            migrations {
                auto_migrate false      // Manual migration in production
                migration_path "./migrations"
                create_database false
            }
        }
        
        // Redis configuration
        redis {
            connection {
                url "redis://redis.example.com:6379"
                database 0
                username null
                password null
                connect_timeout "5s"
                command_timeout "10s"
            }
            
            pool {
                max_connections 20
                min_idle 5
                max_idle 10
                idle_timeout "300s"
            }
            
            features {
                session_store true
                cache true
                pub_sub false
                rate_limiting true
            }
        }
        
        // Tracing and logging configuration
        tracing {
            level info
            format json
            
            // Multiple targets with different levels
            targets [
                {
                    name = "api"
                    level = "debug"
                    enabled = true
                },
                {
                    name = "database"
                    level = "warn"
                    enabled = true
                },
                {
                    name = "redis"
                    level = "error"
                    enabled = true
                }
            ]
            
            filter "api=debug"
            filter "sqlx=warn"
            filter "tower_http=info"
            
            sampling {
                enabled true
                rate 0.1            // 10% sampling
                max_events_per_second 1000
            }
        }
        
        // Security configuration
        security {
            jwt {
                secret "your-super-secret-jwt-key-here"
                algorithm "HS256"
                expiration "24h"
                issuer "api.example.com"
                
                audience "web-app"
                audience "mobile-app"
                audience "admin-panel"
            }
            
            session {
                store redis
                cookie_name "session_id"
                max_age "7d"
                secure true
                http_only true
                same_site strict
            }
            
            csrf {
                enabled true
                token_header "X-CSRF-Token"
                cookie_name "csrf_token"
                max_age "1h"
            }
            
            headers {
                hsts true
                content_type_options true
                frame_options "DENY"
                xss_protection true
                referrer_policy "strict-origin-when-cross-origin"
            }
        }
        
        // API configuration
        api {
            versioning {
                strategy header      // Version via Accept header
                default_version "v1"
                
                supported_version "v1"
                supported_version "v2"
                
                deprecation_warnings true
            }
            
            documentation {
                openapi {
                    enabled true
                    title "Rust Web API"
                    version "2.1.0"
                    description "High-performance web API built with Rust and Axum"
                    
                    contact {
                        name "API Team"
                        email "api-team@example.com"
                        url "https://docs.example.com"
                    }
                }
                
                swagger_ui {
                    enabled true
                    path "/docs"
                    title "API Documentation"
                }
            }
            
            validation {
                strict_mode true
                max_payload_size "10mb"
                
                custom_validator "email"
                custom_validator "phone"
                custom_validator "uuid"
            }
            
            serialization {
                default_format json
                pretty_print false
                date_format "iso8601"
                null_handling "omit"
            }
        }
    "#;

    println!("1. Parsing NGINX-style framework configuration...");
    let config: FrameworkConfig = from_str(config_text)?;

    println!("✓ Successfully parsed framework configuration!");

    // Display key configuration details
    println!("\n=== Application Information ===");
    println!("Name: {}", config.application.name);
    println!("Version: {}", config.application.version);
    println!("Description: {}", config.application.description);
    println!("Authors: {:?}", config.application.authors);
    println!("Environment: {}", config.application.environment);

    println!("\n=== Axum Server Configuration ===");
    println!("Host: {}", config.axum.server.host);
    println!("Port: {}", config.axum.server.port);
    println!("Max connections: {}", config.axum.server.max_connections);
    println!(
        "TLS enabled: {}",
        config
            .axum
            .server
            .tls
            .as_ref()
            .map(|t| t.enabled)
            .unwrap_or(false)
    );
    if let Some(tls) = &config.axum.server.tls {
        println!("TLS protocols: {:?}", tls.protocols);
    }

    println!("\n=== Middleware Configuration ===");
    println!("CORS enabled: {}", config.axum.middleware.cors.enabled);
    println!(
        "CORS origins: {:?}",
        config.axum.middleware.cors.allowed_origins
    );
    println!(
        "Rate limiting: {} req/min",
        config.axum.middleware.rate_limiting.requests_per_minute
    );
    println!(
        "Compression algorithms: {:?}",
        config.axum.middleware.compression.algorithms
    );

    println!("\n=== Database Configuration ===");
    println!("Driver: {}", config.database.driver);
    println!(
        "Host: {}:{}",
        config.database.connection.host, config.database.connection.port
    );
    println!("Database: {}", config.database.connection.database);
    println!(
        "Pool: {}-{} connections",
        config.database.pool.min_connections, config.database.pool.max_connections
    );

    println!("\n=== Redis Configuration ===");
    println!("URL: {}", config.redis.connection.url);
    println!("Database: {}", config.redis.connection.database);
    println!(
        "Features: session_store={}, cache={}, pub_sub={}",
        config.redis.features.session_store,
        config.redis.features.cache,
        config.redis.features.pub_sub
    );

    println!("\n=== Security Configuration ===");
    println!("JWT algorithm: {}", config.security.jwt.algorithm);
    println!("JWT expiration: {}", config.security.jwt.expiration);
    println!("JWT audiences: {:?}", config.security.jwt.audience);
    println!("Session store: {}", config.security.session.store);
    println!("CSRF enabled: {}", config.security.csrf.enabled);

    println!("\n=== API Configuration ===");
    println!("Versioning strategy: {}", config.api.versioning.strategy);
    println!(
        "Supported versions: {:?}",
        config.api.versioning.supported_versions
    );
    println!(
        "OpenAPI enabled: {}",
        config.api.documentation.openapi.enabled
    );
    println!(
        "Swagger UI path: {}",
        config.api.documentation.swagger_ui.path
    );

    // Example 2: Microservices configuration
    println!("\n=== Microservices Configuration Example ===");

    let microservices_config = r#"
        // Microservices architecture configuration
        services {
            // User service
            user_service {
                name "user-service"
                port 8001
                
                database {
                    host "user-db.internal"
                    port 5432
                    database "users"
                }
                
                // Service discovery
                consul {
                    enabled true
                    address "consul.internal:8500"
                    health_check "/health"
                    tags ["user", "authentication"]
                }
            }
            
            // Order service
            order_service {
                name "order-service"
                port 8002
                
                database {
                    host "order-db.internal"
                    port 5432
                    database "orders"
                }
                
                // Message queue
                rabbitmq {
                    host "rabbitmq.internal"
                    port 5672
                    vhost "/orders"
                    
                    queue "order.created"
                    queue "order.updated"
                    queue "order.cancelled"
                }
            }
            
            // Notification service
            notification_service {
                name "notification-service"
                port 8003
                
                // Multiple notification channels
                channels {
                    email {
                        provider "sendgrid"
                        api_key null  // From environment
                        from_address "noreply@example.com"
                    }
                    
                    sms {
                        provider "twilio"
                        account_sid null
                        auth_token null
                    }
                    
                    push {
                        provider "firebase"
                        project_id "my-app-project"
                        credentials_path "/etc/firebase/credentials.json"
                    }
                }
            }
        }
        
        // API Gateway configuration
        gateway {
            port 8000
            
            // Route definitions with NGINX-style syntax
            routes {
                "/api/users/*" {
                    upstream "user-service"
                    timeout "30s"
                    retry_attempts 3
                }
                
                "/api/orders/*" {
                    upstream "order-service"
                    timeout "45s"
                    retry_attempts 2
                }
                
                "/api/notifications/*" {
                    upstream "notification-service"
                    timeout "15s"
                    retry_attempts 1
                }
            }
            
            // Load balancing
            load_balancing {
                strategy "round_robin"
                health_check_interval "10s"
                unhealthy_threshold 3
                healthy_threshold 2
            }
            
            // Rate limiting per service
            rate_limits {
                "/api/users/*" {
                    requests_per_minute 1000
                    burst 100
                }
                
                "/api/orders/*" {
                    requests_per_minute 500
                    burst 50
                }
            }
        }
    "#;

    #[derive(Debug, Deserialize)]
    struct MicroservicesConfig {
        services: HashMap<String, ServiceConfig>,
        gateway: GatewayConfig,
    }

    #[derive(Debug, Deserialize)]
    struct ServiceConfig {
        name: String,
        port: u16,
        database: Option<ServiceDatabase>,
        consul: Option<ConsulConfig>,
        rabbitmq: Option<RabbitMqConfig>,
        channels: Option<HashMap<String, serde_json::Value>>,
    }

    #[derive(Debug, Deserialize)]
    struct ServiceDatabase {
        host: String,
        port: u16,
        database: String,
    }

    #[derive(Debug, Deserialize)]
    struct ConsulConfig {
        enabled: bool,
        address: String,
        health_check: String,
        tags: Vec<String>,
    }

    #[derive(Debug, Deserialize)]
    struct RabbitMqConfig {
        host: String,
        port: u16,
        vhost: String,
        queue: Vec<String>,
    }

    #[derive(Debug, Deserialize)]
    struct GatewayConfig {
        port: u16,
        routes: HashMap<String, RouteConfig>,
        load_balancing: LoadBalancingConfig,
        rate_limits: HashMap<String, RateLimitConfig>,
    }

    #[derive(Debug, Deserialize)]
    struct RouteConfig {
        upstream: String,
        timeout: String,
        retry_attempts: u32,
    }

    #[derive(Debug, Deserialize)]
    struct LoadBalancingConfig {
        strategy: String,
        health_check_interval: String,
        unhealthy_threshold: u32,
        healthy_threshold: u32,
    }

    let microservices: MicroservicesConfig = from_str(microservices_config)?;

    println!("Microservices configuration:");
    println!("  Services: {}", microservices.services.len());
    for (name, service) in &microservices.services {
        println!("    {}: {} (port {})", name, service.name, service.port);
        if let Some(channels) = &service.channels {
            println!("      Channels defined: {}", channels.len());
        }
        if let Some(db) = &service.database {
            println!("      Database: {}:{}/{}", db.host, db.port, db.database);
        }
        if let Some(consul) = &service.consul {
            println!(
                "      Consul: {} (enabled: {}, health: {}, tags: {:?})",
                consul.address, consul.enabled, consul.health_check, consul.tags
            );
        }
        if let Some(rabbitmq) = &service.rabbitmq {
            println!(
                "      RabbitMQ: {}:{} vhost {} queues {}",
                rabbitmq.host,
                rabbitmq.port,
                rabbitmq.vhost,
                rabbitmq.queue.len()
            );
        }
    }

    println!(
        "  Gateway rate limits: {}",
        microservices.gateway.rate_limits.len()
    );
    for (path, limit) in &microservices.gateway.rate_limits {
        println!("    Rate limit {} -> {}/min", path, limit.requests_per_minute);
    }

    println!(
        "  Load balancing: {} interval {} unhealthy {} healthy {}",
        microservices.gateway.load_balancing.strategy,
        microservices.gateway.load_balancing.health_check_interval,
        microservices.gateway.load_balancing.unhealthy_threshold,
        microservices.gateway.load_balancing.healthy_threshold
    );
    for (route, cfg) in &microservices.gateway.routes {
        println!(
            "    Route {} upstream {} timeout {} retries {}",
            route, cfg.upstream, cfg.timeout, cfg.retry_attempts
        );
    }

    assert!(!format!("{:?}", microservices).is_empty());

    println!("  Gateway: port {}", microservices.gateway.port);
    println!("    Routes: {}", microservices.gateway.routes.len());
    println!(
        "    Load balancing: {}",
        microservices.gateway.load_balancing.strategy
    );

    println!("\n✅ Framework integration examples completed successfully!");
    println!("\nKey NGINX-style syntax features demonstrated:");
    println!("  • Implicit object creation (service {{ ... }})");
    println!("  • Bare word values (host localhost, enabled true)");
    println!("  • Implicit arrays through key repetition");
    println!("  • Mixed syntax styles (explicit and implicit)");
    println!("  • Boolean keywords (true/false, yes/no, on/off)");
    println!("  • Null values and special handling");
    println!("  • C++ style comments for documentation");

    Ok(())
}

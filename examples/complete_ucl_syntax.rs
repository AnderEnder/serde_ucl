use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use ucl_lexer::from_str;

/// Comprehensive example demonstrating all UCL syntax features
/// including NGINX-style configurations, extended Unicode, C++ comments,
/// bare word values, and implicit arrays.

#[derive(Debug, Deserialize, Serialize)]
struct CompleteConfig {
    // Basic configuration
    app_name: String,
    version: String,
    debug: bool,

    // NGINX-style server configuration
    server: ServerConfig,

    // Load balancer with implicit arrays
    upstream: UpstreamConfig,

    // Database configuration with bare words
    database: DatabaseConfig,

    // Features with implicit arrays
    features: Vec<String>,

    // Internationalization with Unicode
    i18n: I18nConfig,

    // Mixed syntax examples
    mixed_syntax: MixedSyntaxConfig,

    // Special values
    special_values: SpecialValuesConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct ServerConfig {
    listen: Vec<String>,
    server_name: Vec<String>,
    root: String,
    index: Vec<String>,
    locations: HashMap<String, LocationConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
struct LocationConfig {
    try_files: Option<String>,
    fastcgi_pass: Option<String>,
    fastcgi_index: Option<String>,
    proxy_pass: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct UpstreamConfig {
    backend: BackendConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct BackendConfig {
    server: Vec<String>,
    keepalive: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct DatabaseConfig {
    driver: String,
    host: String,
    port: u16,
    database: String,
    ssl_mode: String,
    pool_size: u32,
    timeout: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct I18nConfig {
    default_locale: String,
    welcome_messages: HashMap<String, String>,
    emoji_support: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct MixedSyntaxConfig {
    explicit_assignment: String,
    implicit_value: String,
    colon_assignment: String,
    nested_object: NestedObjectConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct NestedObjectConfig {
    property: String,
    value: i32,
}

#[derive(Debug, Deserialize, Serialize)]
struct SpecialValuesConfig {
    null_value: Option<String>,
    infinity: f64,
    negative_infinity: f64,
    not_a_number: f64,
    boolean_true: bool,
    boolean_false: bool,
    boolean_yes: bool,
    boolean_no: bool,
    boolean_on: bool,
    boolean_off: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Complete UCL Syntax Demonstration ===\n");

    // Example 1: NGINX-style web server configuration
    let nginx_config = r#"
        // NGINX-style web server configuration with C++ comments
        app_name "complete-ucl-demo"
        version "2.0.0"
        debug yes                    // Boolean keyword
        
        server {
            listen 80
            listen 443 ssl           // Implicit bare word values
            server_name example.com
            server_name www.example.com
            
            root /var/www/html       // Bare word path
            index index.html
            index index.php
            
            locations {
                "/" {
                    try_files "$uri $uri/ =404"
                }
                
                "~ \\.php$" {
                    fastcgi_pass "unix:/var/run/php/php7.4-fpm.sock"
                    fastcgi_index index.php
                }
                
                "/api/" {
                    proxy_pass "http://backend"
                }
            }
        }
        
        upstream {
            backend {
                server "127.0.0.1:3000"
                server "127.0.0.1:3001"
                server "127.0.0.1:3002"
                keepalive 32
            }
        }
        
        # Hash-style comments still work
        database {
            driver postgresql        // Bare word value
            host localhost
            port 5432
            database myapp
            ssl_mode require
            pool_size 10
            timeout null             // Special null value
        }
        
        /*
         * Multi-line comment demonstrating
         * implicit array creation through
         * key repetition
         */
        feature authentication
        feature logging
        feature metrics
        feature caching
        
        // Internationalization with extended Unicode escapes
        i18n {
            default_locale en-US
            
            welcome_messages {
                english "Welcome \u{1F44B} to our application!"
                japanese "いらっしゃいませ \u{1F1EF}\u{1F1F5}"
                emoji_demo "Rocket: \u{1F680}, Heart: \u{2764}\u{FE0F}, Star: \u{2B50}"
            }
            
            emoji_support true
        }
        
        // Mixed syntax styles in same configuration
        mixed_syntax {
            explicit_assignment = "uses equals sign"
            implicit_value bare_word_without_quotes
            colon_assignment: "uses colon separator"
            
            nested_object config {
                property "nested value"
                value 42
            }
        }
        
        // Special values demonstration
        special_values {
            null_value null
            infinity inf
            negative_infinity -inf
            not_a_number nan
            boolean_true true
            boolean_false false
            boolean_yes yes
            boolean_no no
            boolean_on on
            boolean_off off
        }
    "#;

    println!("1. Parsing NGINX-style configuration...");
    let config: CompleteConfig = from_str(nginx_config)?;

    println!("✓ Successfully parsed complete UCL configuration!");
    println!("  App: {} v{}", config.app_name, config.version);
    println!("  Debug mode: {}", config.debug);
    println!("  Server listening on: {:?}", config.server.listen);
    println!("  Server names: {:?}", config.server.server_name);
    println!("  Document root: {}", config.server.root);
    println!("  Index files: {:?}", config.server.index);
    println!("  Backend servers: {:?}", config.upstream.backend.server);
    println!(
        "  Database: {}@{}:{}",
        config.database.driver, config.database.host, config.database.port
    );
    println!("  Features: {:?}", config.features);
    println!("  Default locale: {}", config.i18n.default_locale);
    println!("  Welcome messages: {:#?}", config.i18n.welcome_messages);
    println!(
        "  Special values: infinity={}, null={:?}",
        config.special_values.infinity, config.special_values.null_value
    );

    println!("\n2. JSON representation:");
    let json = serde_json::to_string_pretty(&config)?;
    println!("{}", json);

    // Example 2: Extended Unicode demonstration
    println!("\n=== Extended Unicode Escape Sequences ===");

    let unicode_config = r#"
        unicode_examples {
            // Variable-length Unicode escapes
            emoji_party "\u{1F389} \u{1F38A} \u{1F381}"
            
            // Mixed escape formats
            mixed_formats "\u0041\u{42}\u0043\u{1F600}"  // "ABC😀"
            
            // International characters
            chinese "你好 \u{4E16}\u{754C}"              // "你好 世界"
            arabic "مرحبا \u{628}\u{627}\u{644}\u{639}\u{627}\u{644}\u{645}"
            
            // Mathematical symbols
            math_symbols "\u{2211} \u{222B} \u{221E}"    // "∑ ∫ ∞"
            
            // Emoji combinations
            flags "\u{1F1FA}\u{1F1F8} \u{1F1EF}\u{1F1F5} \u{1F1EC}\u{1F1E7}"  // "🇺🇸 🇯🇵 🇬🇧"
        }
    "#;

    #[derive(Debug, Deserialize)]
    struct UnicodeConfig {
        unicode_examples: HashMap<String, String>,
    }

    let unicode: UnicodeConfig = from_str(unicode_config)?;
    println!("Unicode examples:");
    for (key, value) in &unicode.unicode_examples {
        println!("  {}: {}", key, value);
    }

    // Example 3: Heredoc with improved terminator detection
    println!("\n=== Heredoc with Whitespace Handling ===");

    let heredoc_config = r#"
        scripts {
            nginx_conf = <<NGINX
                server {
                    listen 80;
                    server_name example.com;
                    
                    location / {
                        try_files $uri $uri/ =404;
                    }
                }
            NGINX
            
            shell_script = <<BASH
                #!/bin/bash
                echo "Starting application..."
                ./start_server.sh
                echo "Application started"
            BASH
            
            sql_query = <<SQL
                SELECT u.name, u.email, p.title
                FROM users u
                JOIN posts p ON u.id = p.user_id
                WHERE u.active = true
                ORDER BY p.created_at DESC;
            SQL
        }
    "#;

    #[derive(Debug, Deserialize)]
    struct HeredocConfig {
        scripts: HashMap<String, String>,
    }

    let heredoc: HeredocConfig = from_str(heredoc_config)?;
    println!("Heredoc examples:");
    for (key, value) in &heredoc.scripts {
        println!("  {}:\n{}", key, value);
        println!("  ---");
    }

    // Example 4: Real-world application configuration
    println!("\n=== Real-World Application Configuration ===");

    let app_config = r#"
        // Production web application configuration
        application {
            name "production-api"
            version "1.2.3"
            environment production
            
            // Server configuration with NGINX-style syntax
            server {
                bind_address "0.0.0.0"
                port 8080
                worker_processes auto
                worker_connections 1024
                
                ssl {
                    enabled yes
                    certificate "/etc/ssl/certs/app.crt"
                    private_key "/etc/ssl/private/app.key"
                    protocols "TLSv1.2 TLSv1.3"
                }
            }
            
            // Database cluster configuration
            database {
                primary {
                    host "db-primary.example.com"
                    port 5432
                    database "production_db"
                    ssl_mode require
                }
                
                replica {
                    host "db-replica-1.example.com"
                    port 5432
                }
                
                replica {
                    host "db-replica-2.example.com"
                    port 5432
                }
                
                pool_size 20
                max_connections 100
                timeout 30s
            }
            
            // Redis cache configuration
            cache {
                redis {
                    host "redis.example.com"
                    port 6379
                    database 0
                    password null
                    ssl_enabled false
                }
                
                ttl 3600s
                max_memory 512mb
            }
            
            // Logging configuration
            logging {
                level info
                format json
                
                output console
                output file
                
                file_config {
                    path "/var/log/app/application.log"
                    max_size 100mb
                    max_files 10
                    compress true
                }
            }
            
            // Feature flags
            features {
                authentication enabled
                rate_limiting enabled
                metrics_collection enabled
                debug_endpoints disabled
            }
        }
    "#;

    #[derive(Debug, Deserialize)]
    struct ApplicationConfig {
        application: AppDetails,
    }

    #[derive(Debug, Deserialize)]
    struct AppDetails {
        name: String,
        version: String,
        environment: String,
        server: AppServerConfig,
        database: AppDatabaseConfig,
        cache: AppCacheConfig,
        logging: AppLoggingConfig,
        features: HashMap<String, String>,
    }

    #[derive(Debug, Deserialize)]
    struct AppServerConfig {
        bind_address: String,
        port: u16,
        worker_processes: String,
        worker_connections: u32,
        ssl: SslConfig,
    }

    #[derive(Debug, Deserialize)]
    struct SslConfig {
        enabled: bool,
        certificate: String,
        private_key: String,
        protocols: String,
    }

    #[derive(Debug, Deserialize)]
    struct AppDatabaseConfig {
        primary: DatabaseNode,
        replica: Vec<DatabaseNode>,
        pool_size: u32,
        max_connections: u32,
        timeout: String,
    }

    #[derive(Debug, Deserialize)]
    struct DatabaseNode {
        host: String,
        port: u16,
        database: Option<String>,
        ssl_mode: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    struct AppCacheConfig {
        redis: RedisConfig,
        ttl: String,
        max_memory: String,
    }

    #[derive(Debug, Deserialize)]
    struct RedisConfig {
        host: String,
        port: u16,
        database: u8,
        password: Option<String>,
        ssl_enabled: bool,
    }

    #[derive(Debug, Deserialize)]
    struct AppLoggingConfig {
        level: String,
        format: String,
        output: Vec<String>,
        file_config: FileLoggingConfig,
    }

    #[derive(Debug, Deserialize)]
    struct FileLoggingConfig {
        path: String,
        max_size: String,
        max_files: u32,
        compress: bool,
    }

    let app: ApplicationConfig = from_str(app_config)?;
    println!("Real-world application configuration:");
    println!(
        "  App: {} v{} ({})",
        app.application.name, app.application.version, app.application.environment
    );
    println!(
        "  Server: {}:{}",
        app.application.server.bind_address, app.application.server.port
    );
    println!("  SSL enabled: {}", app.application.server.ssl.enabled);
    println!(
        "  Database primary: {}:{}",
        app.application.database.primary.host, app.application.database.primary.port
    );
    println!(
        "  Database replicas: {}",
        app.application.database.replica.len()
    );
    println!(
        "  Cache: {}:{}",
        app.application.cache.redis.host, app.application.cache.redis.port
    );
    println!("  Log level: {}", app.application.logging.level);
    println!(
        "  Workers: {} (connections: {})",
        app.application.server.worker_processes, app.application.server.worker_connections
    );
    println!(
        "  SSL cert: {} (protocols: {})",
        app.application.server.ssl.certificate, app.application.server.ssl.protocols
    );
    println!(
        "  SSL key length: {}",
        app.application.server.ssl.private_key.len()
    );
    println!(
        "  DB pool: {} max {} timeout {}",
        app.application.database.pool_size,
        app.application.database.max_connections,
        app.application.database.timeout
    );
    println!(
        "  Primary DB optional fields: db={} ssl={}",
        app.application
            .database
            .primary
            .database
            .as_deref()
            .unwrap_or("none"),
        app.application
            .database
            .primary
            .ssl_mode
            .as_deref()
            .unwrap_or("none")
    );
    if let Some(replica) = app.application.database.replica.first() {
        println!(
            "  Replica optional fields: db={} ssl={}",
            replica.database.as_deref().unwrap_or("none"),
            replica.ssl_mode.as_deref().unwrap_or("none")
        );
    }
    println!(
        "  Cache TTL: {} max memory: {}",
        app.application.cache.ttl, app.application.cache.max_memory
    );
    println!(
        "  Redis db: {} ssl: {}",
        app.application.cache.redis.database, app.application.cache.redis.ssl_enabled
    );
    println!(
        "  Redis password set: {}",
        app.application.cache.redis.password.is_some()
    );
    println!(
        "  Log format: {} outputs: {}",
        app.application.logging.format,
        app.application.logging.output.len()
    );
    println!(
        "  Log file: {} max size: {} max files: {} compress: {}",
        app.application.logging.file_config.path,
        app.application.logging.file_config.max_size,
        app.application.logging.file_config.max_files,
        app.application.logging.file_config.compress
    );
    println!("  Features: {:#?}", app.application.features);

    println!("\n✅ All UCL syntax features demonstrated successfully!");
    println!("\nKey features showcased:");
    println!("  • NGINX-style implicit syntax (key value, key {{ ... }})");
    println!("  • C++ style comments (//) alongside hash (#) and multi-line (/* */)");
    println!("  • Extended Unicode escapes (\\u{{...}}) with emoji and international text");
    println!("  • Bare word values without quotes");
    println!("  • Implicit arrays through key repetition");
    println!("  • Mixed syntax styles in same configuration");
    println!("  • Improved heredoc with whitespace handling");
    println!("  • Special values (null, inf, -inf, nan)");
    println!("  • Boolean keywords (true/false, yes/no, on/off)");

    Ok(())
}

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use serde_json::Value;
use std::collections::HashMap;
use ucl_lexer::{LexerConfig, ParserConfig, UclParser, from_str};

/// Generate NGINX-style configuration content for benchmarking
fn generate_nginx_style_config(size: &str) -> String {
    match size {
        "small" => r#"
server {
    listen 80
    server_name example.com
    root /var/www/html
    
    location / {
        try_files $uri $uri/ =404
        proxy_pass http://backend
    }
    
    location /api {
        proxy_pass http://api_backend
        proxy_timeout 30s
        proxy_retries 3
    }
}

upstream backend {
    server 127.0.0.1:3000 weight=5
    server 127.0.0.1:3001 weight=3
    keepalive 32
}

upstream api_backend {
    server 127.0.0.1:4000
    server 127.0.0.1:4001 backup
}
"#
        .to_string(),
        "medium" => {
            let mut config = String::new();
            for i in 0..50 {
                config.push_str(&format!(
                    r#"
server_{} {{
    listen {}
    server_name app{}.example.com
    root /var/www/app{}
    
    location / {{
        try_files $uri $uri/ =404
        proxy_pass http://backend_{}
        proxy_timeout {}s
        proxy_retries {}
    }}
    
    location /api {{
        proxy_pass http://api_backend_{}
        proxy_timeout {}s
        proxy_connect_timeout {}s
        proxy_read_timeout {}s
    }}
    
    location /static {{
        expires 1y
        add_header Cache-Control public
        gzip on
        gzip_types text/css application/javascript
    }}
}}

upstream backend_{} {{
    server 127.0.0.1:{} weight={}
    server 127.0.0.1:{} weight={}
    keepalive {}
    keepalive_requests {}
}}

upstream api_backend_{} {{
    server 127.0.0.1:{}
    server 127.0.0.1:{} backup
    health_check interval={}s
    health_check_timeout={}s
}}
"#,
                    i,
                    8000 + i,
                    i,
                    i,
                    i,
                    10 + (i % 20),
                    2 + (i % 5),
                    i,
                    15 + (i % 30),
                    5 + (i % 10),
                    60 + (i % 120),
                    i,
                    3000 + i * 2,
                    1 + (i % 10),
                    3001 + i * 2,
                    1 + (i % 8),
                    16 + (i % 32),
                    100 + (i % 500),
                    i,
                    4000 + i * 2,
                    4001 + i * 2,
                    5 + (i % 25),
                    3 + (i % 10)
                ));
            }
            config
        }
        "large" => {
            let mut config = String::new();
            for i in 0..200 {
                config.push_str(&format!(
                    r#"
server_{} {{
    listen {}
    server_name app{}.example.com www.app{}.example.com
    root /var/www/app{}
    index index.html index.htm
    
    // C++ style comment for server {}
    access_log /var/log/nginx/app{}.access.log
    error_log /var/log/nginx/app{}.error.log
    
    location / {{
        try_files $uri $uri/ @fallback_{}
        proxy_pass http://backend_{}
        proxy_timeout {}s
        proxy_retries {}
        proxy_next_upstream error timeout
        
        # Hash comment in location block
        proxy_set_header Host $host
        proxy_set_header X-Real-IP $remote_addr
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for
        proxy_set_header X-Forwarded-Proto $scheme
    }}
    
    location /api {{
        proxy_pass http://api_backend_{}
        proxy_timeout {}s
        proxy_connect_timeout {}s
        proxy_read_timeout {}s
        proxy_send_timeout {}s
        
        /* Multi-line comment
           for API configuration
           with nested /* comment */ inside */
        proxy_buffering off
        proxy_request_buffering off
    }}
    
    location /static {{
        expires 1y
        add_header Cache-Control "public, immutable"
        gzip on
        gzip_vary on
        gzip_types
            text/plain
            text/css
            text/xml
            text/javascript
            application/javascript
            application/xml+rss
            application/json
    }}
    
    location @fallback_{} {{
        proxy_pass http://fallback_backend_{}
        proxy_timeout 5s
    }}
}}

upstream backend_{} {{
    server 127.0.0.1:{} weight={} max_fails={} fail_timeout={}s
    server 127.0.0.1:{} weight={} max_fails={} fail_timeout={}s
    server 127.0.0.1:{} weight={} backup
    
    keepalive {}
    keepalive_requests {}
    keepalive_timeout {}s
}}

upstream api_backend_{} {{
    server 127.0.0.1:{}
    server 127.0.0.1:{} backup
    server 127.0.0.1:{} down
    
    health_check interval={}s
    health_check_timeout={}s
    health_check_passes={}
    health_check_fails={}
}}

upstream fallback_backend_{} {{
    server 127.0.0.1:{}
    server 127.0.0.1:{}
}}
"#,
                    i,
                    8000 + i,
                    i,
                    i,
                    i,
                    i,
                    i,
                    i,
                    i,
                    i,
                    10 + (i % 20),
                    2 + (i % 5),
                    i,
                    15 + (i % 30),
                    5 + (i % 10),
                    60 + (i % 120),
                    30 + (i % 60),
                    i,
                    i,
                    i,
                    3000 + i * 3,
                    1 + (i % 10),
                    1 + (i % 5),
                    10 + (i % 30),
                    3001 + i * 3,
                    1 + (i % 8),
                    1 + (i % 3),
                    15 + (i % 45),
                    3002 + i * 3,
                    1 + (i % 6),
                    16 + (i % 32),
                    100 + (i % 500),
                    60 + (i % 300),
                    i,
                    4000 + i * 3,
                    4001 + i * 3,
                    4002 + i * 3,
                    5 + (i % 25),
                    3 + (i % 10),
                    2 + (i % 5),
                    3 + (i % 8),
                    i,
                    5000 + i * 2,
                    5001 + i * 2
                ));
            }
            config
        }
        _ => "server { listen 80 }".to_string(),
    }
}

/// Generate mixed syntax UCL content (explicit + implicit)
fn generate_mixed_syntax_config(size: &str) -> String {
    match size {
        "small" => r#"
// Mixed syntax configuration
database = {
    host = "localhost"      // Explicit assignment
    port: 5432             // Explicit with colon
    name myapp             // Implicit bare word
    ssl_mode require       // Implicit bare word
    
    connection_pool {      // Implicit object
        min_size 5
        max_size: 20
        timeout = "30s"
    }
}

server {
    listen 8080
    workers auto
    
    routes api {
        prefix "/api/v1"
        timeout = 60s
        
        middleware {
            cors {
                origins ["*"]
                methods: ["GET", "POST", "PUT", "DELETE"]
                headers = ["Content-Type", "Authorization"]
            }
            
            rate_limit {
                requests 1000
                window: "1h"
                burst = 100
            }
        }
    }
}
"#
        .to_string(),
        "medium" => {
            let mut config = String::new();
            for i in 0..25 {
                config.push_str(&format!(
                    r#"
service_{} {{
    // Service configuration {}
    name = "service-{}"
    port {}
    enabled {}
    
    database_{} {{
        host: "db{}.example.com"
        port = {}
        name service_{}
        ssl_mode require
        
        pool {{
            min_size {}
            max_size: {}
            timeout = "{}s"
            idle_timeout "{}s"
        }}
    }}
    
    cache redis_{}{{
        host "redis{}.example.com"
        port: {}
        db = {}
        
        cluster {{
            nodes [
                "redis{}-1.example.com:6379",
                "redis{}-2.example.com:6379",
                "redis{}-3.example.com:6379"
            ]
            
            failover {{
                timeout: "{}s"
                retry_attempts = {}
                backoff_multiplier {}
            }}
        }}
    }}
    
    # Hash comment for monitoring
    monitoring {{
        enabled: true
        endpoint = "http://monitor{}.example.com"
        interval {}s
        timeout: "{}s"
        
        metrics {{
            cpu true
            memory: true
            disk = true
            network {}
        }}
    }}
}}
"#,
                    i,
                    i,
                    i,
                    8000 + i,
                    i % 2 == 0,
                    i,
                    i,
                    5432 + i,
                    i,
                    2 + (i % 8),
                    10 + (i % 20),
                    15 + (i % 45),
                    300 + (i % 600),
                    i,
                    i,
                    6379 + i,
                    i % 16,
                    i,
                    i,
                    i,
                    5 + (i % 20),
                    2 + (i % 5),
                    1.5 + (i as f32 % 3.0),
                    i,
                    30 + (i % 60),
                    10 + (i % 20),
                    i % 2 == 0
                ));
            }
            config
        }
        "large" => {
            let mut config = String::new();
            for i in 0..100 {
                config.push_str(&format!(
                    r#"
application_{} {{
    /* Multi-line comment for application {}
       with detailed configuration options
       and /* nested comment */ support */
    
    name: "app-{}"
    version = "1.{}.0"
    environment {}
    debug {}
    
    server {{
        host = "0.0.0.0"
        port {}
        workers: {}
        threads {}
        
        tls {{
            enabled: {}
            cert_file = "/etc/ssl/certs/app{}.crt"
            key_file "/etc/ssl/private/app{}.key"
            protocols ["TLSv1.2", "TLSv1.3"]
            
            cipher_suites [
                "ECDHE-RSA-AES256-GCM-SHA384",
                "ECDHE-RSA-AES128-GCM-SHA256",
                "ECDHE-RSA-AES256-SHA384"
            ]
        }}
    }}
    
    database primary_db {{
        driver: "postgresql"
        host = "primary-db{}.example.com"
        port {}
        database app_{}
        username: "app_user_{}"
        password = "${{DB_PASSWORD_{}}}"
        
        connection_pool {{
            min_size: {}
            max_size = {}
            acquire_timeout "{}s"
            idle_timeout: "{}s"
            max_lifetime = "{}s"
        }}
        
        migrations {{
            enabled true
            directory: "/app/migrations"
            table = "schema_migrations"
            auto_migrate {}
        }}
    }}
    
    cache redis_cluster {{
        nodes: [
            "redis{}-1.cluster.local:6379",
            "redis{}-2.cluster.local:6379", 
            "redis{}-3.cluster.local:6379"
        ]
        
        sentinel {{
            enabled: {}
            master_name = "redis-{}"
            sentinels [
                "sentinel{}-1.cluster.local:26379",
                "sentinel{}-2.cluster.local:26379"
            ]
        }}
        
        options {{
            max_retries: {}
            retry_delay = "{}ms"
            command_timeout "{}s"
            connection_timeout: "{}s"
        }}
    }}
    
    // Logging configuration with C++ style comments
    logging {{
        level = "{}"
        format: "json"
        output stdout
        
        file_output {{
            enabled: {}
            path = "/var/log/app{}/app.log"
            max_size "{}MB"
            max_files: {}
            compress = true
        }}
        
        structured_logging {{
            include_caller: true
            include_timestamp = true
            timezone "UTC"
            
            fields {{
                service: "app-{}"
                version = "1.{}.0"
                environment {}
            }}
        }}
    }}
    
    monitoring prometheus {{
        enabled: true
        endpoint = "/metrics"
        port {}
        
        collectors {{
            process: true
            go = true
            http true
            database: true
        }}
        
        custom_metrics [
            {{
                name: "app_requests_total"
                type = "counter"
                help "Total number of requests"
                labels ["method", "endpoint", "status"]
            }},
            {{
                name = "app_request_duration_seconds"
                type: "histogram"
                help = "Request duration in seconds"
                buckets [0.1, 0.5, 1.0, 2.5, 5.0, 10.0]
            }}
        ]
    }}
}}
"#,
                    i,
                    i,
                    i,
                    i % 100,
                    if i % 3 == 0 {
                        "production"
                    } else if i % 3 == 1 {
                        "staging"
                    } else {
                        "development"
                    },
                    i % 2 == 0,
                    8000 + i,
                    2 + (i % 8),
                    4 + (i % 12),
                    i % 2 == 0,
                    i,
                    i,
                    i,
                    5432 + (i % 100),
                    i,
                    i,
                    i,
                    2 + (i % 8),
                    10 + (i % 40),
                    5 + (i % 25),
                    300 + (i % 1200),
                    3600 + (i % 7200),
                    i % 2 == 0,
                    i,
                    i,
                    i,
                    i % 2 == 0,
                    i,
                    i,
                    i,
                    2 + (i % 8),
                    100 + (i % 500),
                    5 + (i % 25),
                    3 + (i % 12),
                    if i % 4 == 0 {
                        "debug"
                    } else if i % 4 == 1 {
                        "info"
                    } else if i % 4 == 2 {
                        "warn"
                    } else {
                        "error"
                    },
                    i % 2 == 0,
                    i,
                    100 + (i % 900),
                    5 + (i % 15),
                    i,
                    i % 100,
                    if i % 3 == 0 {
                        "production"
                    } else if i % 3 == 1 {
                        "staging"
                    } else {
                        "development"
                    },
                    9090 + i
                ));
            }
            config
        }
        _ => "app { name test }".to_string(),
    }
}

/// Generate UCL with extended Unicode escapes
fn generate_unicode_heavy_config() -> String {
    r#"
messages = {
    welcome = "Welcome! \u{1F44B} \u{1F600}"
    success = "Success \u{2713} \u{1F389}"
    error = "Error \u{274C} \u{1F6A8}"
    warning = "Warning \u{26A0}\uFE0F"
    info = "Info \u{2139}\uFE0F"
    
    multilingual = {
        english = "Hello World!"
        chinese = "\u4F60\u597D\u4E16\u754C"  // 你好世界
        japanese = "\u3053\u3093\u306B\u3061\u306F\u4E16\u754C"  // こんにちは世界
        korean = "\u{C548}\u{B155}\u{D558}\u{C138}\u{C694} \u{C138}\u{ACC4}"  // 안녕하세요 세계
        arabic = "\u{645}\u{631}\u{62D}\u{628}\u{627} \u{628}\u{627}\u{644}\u{639}\u{627}\u{644}\u{645}"  // مرحبا بالعالم
        russian = "\u{41F}\u{440}\u{438}\u{432}\u{435}\u{442} \u{43C}\u{438}\u{440}"  // Привет мир
        emoji_mix = "Hello \u{1F30D} World \u{1F680} \u{2728}"
    }
    
    symbols = {
        math = "\u{2211}\u{222B}\u{221E}\u{03C0}\u{03B1}\u{03B2}\u{03B3}"
        arrows = "\u{2190}\u{2191}\u{2192}\u{2193}\u{21D0}\u{21D1}\u{21D2}\u{21D3}"
        currency = "\u{0024}\u{00A2}\u{00A3}\u{00A5}\u{20AC}\u{20A9}\u{20B9}\u{20BD}"
        technical = "\u{2699}\u{1F527}\u{1F528}\u{1F529}\u{1F52A}\u{1F52B}"
    }
}

config = {
    app_name = "Unicode Test \u{1F9EA}"
    version = "1.0.0-\u{03B1}"
    description = "Testing extended Unicode support \u{1F680}\u{2728}"
    
    features = {
        emoji_support = true  // \u{1F44D}
        multilingual = true   // \u{1F30D}
        rtl_support = true    // \u{21E6}
    }
}
"#.to_string()
}

/// Benchmark NGINX-style syntax parsing performance
fn bench_nginx_syntax_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("nginx_syntax_parsing");

    for size in ["small", "medium", "large"] {
        let nginx_config = generate_nginx_style_config(size);
        let content_size = nginx_config.len();

        group.throughput(Throughput::Bytes(content_size as u64));

        group.bench_with_input(
            BenchmarkId::new("nginx_style", size),
            &nginx_config,
            |b, content| {
                b.iter(|| {
                    let result: Result<Value, _> = from_str(black_box(content));
                    result
                });
            },
        );

        // Compare with equivalent explicit syntax
        let explicit_config = convert_to_explicit_syntax(&nginx_config);
        group.bench_with_input(
            BenchmarkId::new("explicit_equivalent", size),
            &explicit_config,
            |b, content| {
                b.iter(|| {
                    let result: Result<Value, _> = from_str(black_box(content));
                    result
                });
            },
        );
    }

    group.finish();
}

/// Benchmark mixed syntax parsing (implicit + explicit)
fn bench_mixed_syntax_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_syntax_parsing");

    for size in ["small", "medium", "large"] {
        let mixed_config = generate_mixed_syntax_config(size);
        let content_size = mixed_config.len();

        group.throughput(Throughput::Bytes(content_size as u64));

        group.bench_with_input(
            BenchmarkId::new("mixed_syntax", size),
            &mixed_config,
            |b, content| {
                b.iter(|| {
                    let result: Result<Value, _> = from_str(black_box(content));
                    result
                });
            },
        );
    }

    group.finish();
}

/// Benchmark C++ comment parsing impact
fn bench_cpp_comment_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("cpp_comment_parsing");

    let config_with_cpp_comments = r#"
// Main application configuration
app = {
    name = "test-app"  // Application name
    port = 8080        // Server port
    
    // Database configuration
    database = {
        host = "localhost"     // DB host
        port = 5432           // DB port  
        name = "myapp"        // DB name
        
        // Connection pool settings
        pool = {
            min_size = 5      // Minimum connections
            max_size = 20     // Maximum connections
            timeout = "30s"   // Connection timeout
        }
    }
    
    // Feature flags
    features = {
        logging = true    // Enable logging
        metrics = true    // Enable metrics
        tracing = false   // Disable tracing
    }
}
"#;

    let config_with_hash_comments = config_with_cpp_comments.replace("//", "#");
    let config_without_comments = remove_comments(&config_with_cpp_comments);

    group.throughput(Throughput::Bytes(config_with_cpp_comments.len() as u64));

    group.bench_function("with_cpp_comments", |b| {
        b.iter(|| {
            let result: Result<Value, _> = from_str(black_box(config_with_cpp_comments));
            result
        });
    });

    group.bench_function("with_hash_comments", |b| {
        b.iter(|| {
            let result: Result<Value, _> = from_str(black_box(&config_with_hash_comments));
            result
        });
    });

    group.bench_function("without_comments", |b| {
        b.iter(|| {
            let result: Result<Value, _> = from_str(black_box(&config_without_comments));
            result
        });
    });

    group.finish();
}

/// Benchmark extended Unicode escape parsing
fn bench_unicode_escape_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("unicode_escape_parsing");

    let unicode_config = generate_unicode_heavy_config();
    let ascii_config = convert_unicode_to_ascii(&unicode_config);

    group.throughput(Throughput::Bytes(unicode_config.len() as u64));

    group.bench_function("with_unicode_escapes", |b| {
        b.iter(|| {
            let result: Result<Value, _> = from_str(black_box(&unicode_config));
            result
        });
    });

    group.bench_function("ascii_equivalent", |b| {
        b.iter(|| {
            let result: Result<Value, _> = from_str(black_box(&ascii_config));
            result
        });
    });

    group.finish();
}

/// Benchmark bare word value parsing
fn bench_bare_word_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("bare_word_parsing");

    let bare_word_config = r#"
config = {
    environment production
    debug false
    log_level info
    workers auto
    timeout 30s
    
    database = {
        ssl_mode require
        pool_mode transaction
        application_name myapp
        connect_timeout 10s
    }
    
    features = {
        caching enabled
        compression gzip
        rate_limiting strict
        monitoring comprehensive
    }
}
"#;

    let quoted_config = convert_bare_words_to_quoted(&bare_word_config);

    group.throughput(Throughput::Bytes(bare_word_config.len() as u64));

    group.bench_function("with_bare_words", |b| {
        b.iter(|| {
            let result: Result<Value, _> = from_str(black_box(bare_word_config));
            result
        });
    });

    group.bench_function("with_quoted_strings", |b| {
        b.iter(|| {
            let result: Result<Value, _> = from_str(black_box(&quoted_config));
            result
        });
    });

    group.finish();
}

/// Benchmark syntax detection overhead
fn bench_syntax_detection_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("syntax_detection_overhead");

    let pure_explicit_config = r#"
{
    "server": {
        "listen": 80,
        "server_name": "example.com",
        "root": "/var/www/html"
    },
    "upstream": {
        "backend": {
            "server": "127.0.0.1:3000",
            "keepalive": 32
        }
    }
}
"#;

    let pure_nginx_config = r#"
server {
    listen 80
    server_name example.com
    root /var/www/html
}

upstream backend {
    server 127.0.0.1:3000
    keepalive 32
}
"#;

    let mixed_config = r#"
server {
    listen = 80
    server_name example.com
    root: "/var/www/html"
}

upstream backend {
    server = "127.0.0.1:3000"
    keepalive: 32
}
"#;

    group.bench_function("pure_explicit", |b| {
        b.iter(|| {
            let result: Result<Value, _> = from_str(black_box(pure_explicit_config));
            result
        });
    });

    group.bench_function("pure_nginx", |b| {
        b.iter(|| {
            let result: Result<Value, _> = from_str(black_box(pure_nginx_config));
            result
        });
    });

    group.bench_function("mixed_syntax", |b| {
        b.iter(|| {
            let result: Result<Value, _> = from_str(black_box(mixed_config));
            result
        });
    });

    group.finish();
}

/// Helper function to convert NGINX-style to explicit syntax (simplified)
fn convert_to_explicit_syntax(nginx_config: &str) -> String {
    // This is a simplified conversion for benchmarking purposes
    nginx_config
        .replace(" {", " = {")
        .replace("listen 80", "listen = 80")
        .replace("server_name ", "server_name = \"")
        .replace("root /", "root = \"/")
        .replace("keepalive ", "keepalive = ")
        .replace("weight=", "weight = ")
        .replace("timeout ", "timeout = \"")
        .replace("retries ", "retries = ")
}

/// Helper function to remove comments (simplified)
fn remove_comments(config: &str) -> String {
    config
        .lines()
        .map(|line| {
            if let Some(pos) = line.find("//") {
                &line[..pos]
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Helper function to convert Unicode escapes to ASCII (simplified)
fn convert_unicode_to_ascii(config: &str) -> String {
    config
        .replace("\\u{1F44B}", "wave")
        .replace("\\u{1F600}", "smile")
        .replace("\\u{2713}", "check")
        .replace("\\u{1F389}", "party")
        .replace("\\u{274C}", "x")
        .replace("\\u{1F6A8}", "siren")
        .replace("\\u{26A0}", "warning")
        .replace("\\u{2139}", "info")
        .replace("\\u{1F30D}", "world")
        .replace("\\u{1F680}", "rocket")
        .replace("\\u{2728}", "sparkles")
}

/// Helper function to convert bare words to quoted strings (simplified)
fn convert_bare_words_to_quoted(config: &str) -> String {
    config
        .replace("environment production", "environment = \"production\"")
        .replace("debug false", "debug = false")
        .replace("log_level info", "log_level = \"info\"")
        .replace("workers auto", "workers = \"auto\"")
        .replace("timeout 30s", "timeout = \"30s\"")
        .replace("ssl_mode require", "ssl_mode = \"require\"")
        .replace("pool_mode transaction", "pool_mode = \"transaction\"")
        .replace("application_name myapp", "application_name = \"myapp\"")
        .replace("connect_timeout 10s", "connect_timeout = \"10s\"")
        .replace("caching enabled", "caching = \"enabled\"")
        .replace("compression gzip", "compression = \"gzip\"")
        .replace("rate_limiting strict", "rate_limiting = \"strict\"")
        .replace("monitoring comprehensive", "monitoring = \"comprehensive\"")
}

criterion_group!(
    benches,
    bench_nginx_syntax_parsing,
    bench_mixed_syntax_parsing,
    bench_cpp_comment_parsing,
    bench_unicode_escape_parsing,
    bench_bare_word_parsing,
    bench_syntax_detection_overhead
);
criterion_main!(benches);

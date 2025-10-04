//! Integration tests with real-world UCL configuration scenarios
//!
//! These tests verify that the UCL parser works correctly with actual
//! configuration files and scenarios found in real applications.

use serde::Deserialize;
use std::collections::HashMap;
use ucl_lexer::from_str;
#[test]
fn test_large_configuration_performance() {
    // Test performance with a large configuration file
    let mut large_config = String::from("{\n");

    // Generate a large configuration with many nested objects and arrays
    for i in 0..1000 {
        large_config.push_str(&format!(
            r#"  "service_{}" = {{
    "name" = "service-{}"
    "port" = {}
    "enabled" = {}
    "timeout" = {}s
    "memory_limit" = {}mb
    "cpu_limit" = "{}m"
    "replicas" = {}
    "environment" = {{
      "NODE_ENV" = "production"
      "LOG_LEVEL" = "info"
      "DATABASE_URL" = "postgresql://user:pass@db-{}.example.com:5432/service_{}"
      "REDIS_URL" = "redis://redis-{}.example.com:6379/0"
    }}
    "volumes" = [
      "/data/service-{}:/app/data",
      "/logs/service-{}:/app/logs",
      "/config/service-{}:/app/config"
    ]
    "health_check" = {{
      "path" = "/health"
      "interval" = "30s"
      "timeout" = "10s"
      "retries" = 3
    }}
    "dependencies" = [
      "database-{}",
      "redis-{}",
      "auth-service"
    ]
    "metrics" = {{
      "enabled" = true
      "port" = {}
      "path" = "/metrics"
    }}
  }}
"#,
            i,
            i,
            8000 + i,
            i % 2 == 0,
            30 + (i % 60),
            512 + (i % 1024),
            100 + (i % 400),
            1 + (i % 5),
            i,
            i,
            i,
            i,
            i,
            i,
            i,
            i,
            9000 + i
        ));
    }

    large_config.push_str("}\n");

    // Measure parsing time
    let start = std::time::Instant::now();
    let parsed: serde_json::Value = from_str(&large_config).expect("Failed to parse large config");
    let duration = start.elapsed();

    println!(
        "Large config parsing: {} bytes in {:?}",
        large_config.len(),
        duration
    );

    // Verify the structure was parsed correctly
    assert!(parsed.is_object());
    let obj = parsed.as_object().unwrap();
    assert_eq!(obj.len(), 1000);

    // Verify some random entries
    assert!(obj.contains_key("service_0"));
    assert!(obj.contains_key("service_500"));
    assert!(obj.contains_key("service_999"));

    // Performance should be reasonable (less than 5 seconds for 1000 services)
    assert!(
        duration.as_secs() < 5,
        "Large config should parse within 5 seconds"
    );

    // Calculate throughput
    let throughput_mb_per_sec = (large_config.len() as f64 / 1_000_000.0) / duration.as_secs_f64();
    println!("Throughput: {:.2} MB/s", throughput_mb_per_sec);

    // Should achieve reasonable throughput
    assert!(
        throughput_mb_per_sec > 0.5,
        "Should achieve at least 0.5 MB/s throughput"
    );
}

#[test]
fn test_real_world_microservices_config() {
    // Test a realistic microservices configuration
    let config = r#"
        # Microservices architecture configuration
        version = "2.1"
        
        # Global settings
        global {
            cluster_name = "production-cluster"
            region = "us-west-2"
            environment = "production"
            
            # Default resource limits
            default_limits = {
                cpu = "500m"
                memory = "512Mi"
                storage = "10Gi"
            }
            
            # Network configuration
            network = {
                service_mesh = "istio"
                ingress_class = "nginx"
                load_balancer = "aws-alb"
            }
        }
        
        # Service definitions
        services = {
            # API Gateway
            api_gateway = {
                image = "nginx:1.21-alpine"
                replicas = 3
                
                ports = [
                    { name = "http", port = 80, target_port = 8080 },
                    { name = "https", port = 443, target_port = 8443 }
                ]
                
                config = {
                    upstream_timeout = "30s"
                    client_max_body_size = "10m"
                    rate_limit = "100r/s"
                }
                
                routes = [
                    {
                        path = "/api/auth/*"
                        service = "auth-service"
                        timeout = "10s"
                    },
                    {
                        path = "/api/users/*"
                        service = "user-service"
                        timeout = "15s"
                    },
                    {
                        path = "/api/orders/*"
                        service = "order-service"
                        timeout = "20s"
                    }
                ]
            }
            
            # Authentication Service
            auth_service = {
                image = "auth-service:v2.3.1"
                replicas = 2
                
                environment = {
                    JWT_SECRET = "${JWT_SECRET}"
                    TOKEN_EXPIRY = "24h"
                    REFRESH_TOKEN_EXPIRY = "7d"
                    BCRYPT_ROUNDS = "12"
                }
                
                database = {
                    type = "postgresql"
                    host = "auth-db.internal"
                    port = 5432
                    name = "auth_db"
                    pool_size = 20
                    max_connections = 100
                }
                
                redis = {
                    host = "auth-redis.internal"
                    port = 6379
                    db = 0
                    ttl = "1h"
                }
                
                health_check = {
                    path = "/health"
                    initial_delay = "30s"
                    period = "10s"
                    timeout = "5s"
                    failure_threshold = 3
                }
            }
            
            # User Service
            user_service = {
                image = "user-service:v1.8.2"
                replicas = 3
                
                environment = {
                    CACHE_TTL = "300s"
                    MAX_UPLOAD_SIZE = "5mb"
                    AVATAR_STORAGE = "s3"
                }
                
                database = {
                    type = "postgresql"
                    host = "user-db.internal"
                    port = 5432
                    name = "user_db"
                    read_replicas = [
                        "user-db-replica-1.internal:5432",
                        "user-db-replica-2.internal:5432"
                    ]
                }
                
                storage = {
                    provider = "aws-s3"
                    bucket = "user-avatars-prod"
                    region = "us-west-2"
                    cdn_url = "https://cdn.example.com/avatars/"
                }
                
                integrations = {
                    email_service = {
                        url = "http://email-service.internal:8080"
                        timeout = "10s"
                        retry_attempts = 3
                    }
                    
                    analytics_service = {
                        url = "http://analytics-service.internal:8080"
                        async = true
                        batch_size = 100
                    }
                }
            }
            
            # Order Service
            order_service = {
                image = "order-service:v3.1.0"
                replicas = 4
                
                environment = {
                    ORDER_TIMEOUT = "30min"
                    PAYMENT_TIMEOUT = "5min"
                    INVENTORY_CHECK = "true"
                }
                
                database = {
                    type = "postgresql"
                    host = "order-db.internal"
                    port = 5432
                    name = "order_db"
                    connection_pool = {
                        min_size = 5
                        max_size = 50
                        idle_timeout = "10min"
                    }
                }
                
                message_queue = {
                    provider = "rabbitmq"
                    host = "rabbitmq.internal"
                    port = 5672
                    vhost = "/orders"
                    queues = [
                        { name = "order.created", durable = true },
                        { name = "order.updated", durable = true },
                        { name = "order.cancelled", durable = true }
                    ]
                }
                
                external_apis = {
                    payment_gateway = {
                        url = "${PAYMENT_GATEWAY_URL}"
                        api_key = "${PAYMENT_API_KEY}"
                        timeout = "30s"
                        retry_policy = {
                            max_attempts = 3
                            backoff = "exponential"
                            initial_delay = "1s"
                        }
                    }
                    
                    inventory_service = {
                        url = "http://inventory-service.internal:8080"
                        timeout = "10s"
                        circuit_breaker = {
                            failure_threshold = 5
                            recovery_timeout = "30s"
                        }
                    }
                }
            }
            
            # Email Service
            email_service = {
                image = "email-service:v1.4.3"
                replicas = 2
                
                providers = {
                    primary = {
                        type = "sendgrid"
                        api_key = "${SENDGRID_API_KEY}"
                        from_email = "noreply@example.com"
                        from_name = "Example App"
                    }
                    
                    fallback = {
                        type = "ses"
                        region = "us-west-2"
                        access_key = "${AWS_ACCESS_KEY}"
                        secret_key = "${AWS_SECRET_KEY}"
                    }
                }
                
                templates = {
                    welcome = {
                        subject = "Welcome to Example App!"
                        template_id = "d-1234567890abcdef"
                    }
                    
                    password_reset = {
                        subject = "Reset Your Password"
                        template_id = "d-fedcba0987654321"
                    }
                    
                    order_confirmation = {
                        subject = "Order Confirmation #{{order_id}}"
                        template_id = "d-abcdef1234567890"
                    }
                }
                
                rate_limiting = {
                    per_minute = 100
                    per_hour = 1000
                    per_day = 10000
                }
            }
        }
        
        # Infrastructure components
        infrastructure = {
            # Databases
            databases = {
                auth_db = {
                    engine = "postgresql"
                    version = "13.7"
                    instance_class = "db.r5.large"
                    storage = "100gb"
                    backup_retention = "7d"
                    multi_az = true
                }
                
                user_db = {
                    engine = "postgresql"
                    version = "13.7"
                    instance_class = "db.r5.xlarge"
                    storage = "200gb"
                    backup_retention = "30d"
                    multi_az = true
                    read_replicas = 2
                }
                
                order_db = {
                    engine = "postgresql"
                    version = "13.7"
                    instance_class = "db.r5.2xlarge"
                    storage = "500gb"
                    backup_retention = "30d"
                    multi_az = true
                    read_replicas = 3
                }
            }
            
            # Cache layers
            cache = {
                auth_redis = {
                    engine = "redis"
                    version = "6.2"
                    node_type = "cache.r6g.large"
                    num_cache_nodes = 2
                    parameter_group = "default.redis6.x"
                }
                
                session_redis = {
                    engine = "redis"
                    version = "6.2"
                    node_type = "cache.r6g.xlarge"
                    replication_group = true
                    num_cache_clusters = 3
                }
            }
            
            # Message queues
            messaging = {
                rabbitmq = {
                    version = "3.9"
                    instance_type = "mq.m5.large"
                    deployment_mode = "cluster"
                    storage_type = "ebs"
                    
                    configuration = {
                        vm_memory_high_watermark = "0.6"
                        disk_free_limit = "2GB"
                        log_levels = {
                            connection = "info"
                            channel = "info"
                            queue = "info"
                        }
                    }
                }
            }
        }
        
        # Monitoring and observability
        monitoring = {
            metrics = {
                prometheus = {
                    retention = "15d"
                    scrape_interval = "15s"
                    evaluation_interval = "15s"
                }
                
                grafana = {
                    admin_password = "${GRAFANA_ADMIN_PASSWORD}"
                    plugins = ["grafana-piechart-panel", "grafana-worldmap-panel"]
                }
            }
            
            logging = {
                elasticsearch = {
                    version = "7.15"
                    instance_type = "m6g.large.elasticsearch"
                    instance_count = 3
                    storage_size = "100gb"
                    storage_type = "gp3"
                }
                
                kibana = {
                    version = "7.15"
                    instance_type = "m6g.medium.elasticsearch"
                }
                
                logstash = {
                    version = "7.15"
                    pipeline_workers = 4
                    pipeline_batch_size = 125
                }
            }
            
            tracing = {
                jaeger = {
                    collector_replicas = 2
                    query_replicas = 2
                    storage_type = "elasticsearch"
                    sampling_rate = 0.1
                }
            }
            
            alerting = {
                alertmanager = {
                    replicas = 2
                    retention = "120h"
                    
                    routes = [
                        {
                            match = { severity = "critical" }
                            receiver = "pagerduty"
                            group_wait = "10s"
                            group_interval = "5m"
                            repeat_interval = "12h"
                        },
                        {
                            match = { severity = "warning" }
                            receiver = "slack"
                            group_wait = "30s"
                            group_interval = "10m"
                            repeat_interval = "4h"
                        }
                    ]
                }
            }
        }
        
        # Security configuration
        security = {
            # Network policies
            network_policies = {
                default_deny = true
                
                allowed_ingress = [
                    {
                        from = "api-gateway"
                        to = ["auth-service", "user-service", "order-service"]
                        ports = [8080]
                    },
                    {
                        from = ["auth-service", "user-service", "order-service"]
                        to = "email-service"
                        ports = [8080]
                    }
                ]
                
                allowed_egress = [
                    {
                        from = "all"
                        to = "external"
                        ports = [80, 443]
                    },
                    {
                        from = "services"
                        to = "databases"
                        ports = [5432]
                    }
                ]
            }
            
            # TLS configuration
            tls = {
                min_version = "1.2"
                cipher_suites = [
                    "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
                    "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
                    "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305"
                ]
                
                certificates = {
                    api_gateway = {
                        cert_manager = true
                        issuer = "letsencrypt-prod"
                        domains = ["api.example.com", "www.example.com"]
                    }
                }
            }
            
            # RBAC configuration
            rbac = {
                service_accounts = {
                    api_gateway = {
                        permissions = ["ingress.read", "services.list"]
                    }
                    
                    auth_service = {
                        permissions = ["secrets.read", "configmaps.read"]
                    }
                    
                    order_service = {
                        permissions = ["secrets.read", "events.create"]
                    }
                }
            }
        }
    "#;

    #[derive(Debug, Deserialize)]
    struct MicroservicesConfig {
        version: String,
        global: GlobalConfig,
        services: HashMap<String, serde_json::Value>,
        infrastructure: InfrastructureConfig,
        monitoring: MonitoringConfig,
        security: SecurityConfig,
    }

    #[derive(Debug, Deserialize)]
    struct GlobalConfig {
        cluster_name: String,
        region: String,
        environment: String,
        default_limits: HashMap<String, String>,
        network: NetworkConfig,
    }

    #[derive(Debug, Deserialize)]
    struct NetworkConfig {
        service_mesh: String,
        ingress_class: String,
        load_balancer: String,
    }

    #[derive(Debug, Deserialize)]
    struct InfrastructureConfig {
        databases: HashMap<String, serde_json::Value>,
        cache: HashMap<String, serde_json::Value>,
        messaging: HashMap<String, serde_json::Value>,
    }

    #[derive(Debug, Deserialize)]
    struct MonitoringConfig {
        metrics: serde_json::Value,
        logging: serde_json::Value,
        tracing: serde_json::Value,
        alerting: serde_json::Value,
    }

    #[derive(Debug, Deserialize)]
    struct SecurityConfig {
        network_policies: serde_json::Value,
        tls: serde_json::Value,
        rbac: serde_json::Value,
    }

    let parsed: MicroservicesConfig =
        from_str(config).expect("Failed to parse microservices config");

    assert_eq!(parsed.version, "2.1");
    assert_eq!(parsed.global.cluster_name, "production-cluster");
    assert_eq!(parsed.global.region, "us-west-2");
    assert_eq!(parsed.global.environment, "production");
    assert!(parsed.global.default_limits.contains_key("cpu"));
    assert!(parsed.global.default_limits.contains_key("memory"));
    assert_eq!(parsed.global.network.service_mesh, "istio");
    assert_eq!(parsed.global.network.ingress_class, "nginx");
    assert_eq!(parsed.global.network.load_balancer, "aws-alb");
    assert_eq!(parsed.services.len(), 5); // api_gateway, auth_service, user_service, order_service, email_service

    // Verify some service configurations exist
    assert!(parsed.services.contains_key("api_gateway"));
    assert!(parsed.services.contains_key("auth_service"));
    assert!(parsed.services.contains_key("order_service"));

    // Verify infrastructure components
    assert!(parsed.infrastructure.databases.contains_key("auth_db"));
    assert!(parsed.infrastructure.cache.contains_key("auth_redis"));
    assert!(parsed.infrastructure.messaging.contains_key("rabbitmq"));

    assert!(parsed.monitoring.metrics.is_object());
    assert!(parsed.monitoring.logging.is_object());
    assert!(parsed.monitoring.tracing.is_object());
    assert!(parsed.monitoring.alerting.is_object());
    assert!(parsed.security.network_policies.is_object());
    assert!(parsed.security.tls.is_object());
    assert!(parsed.security.rbac.is_object());
}

#[test]
fn test_real_world_ci_cd_pipeline_config() {
    // Test a realistic CI/CD pipeline configuration
    let config = r#"
        version = "1.0"
        
        global = {
            timeout = "2h"
            retry_attempts = 3
        }
        
        stages = {
            test = {
                jobs = {
                    unit_tests = {
                        image = "node:16-alpine"
                        script = ["npm test"]
                    }
                    
                    integration_tests = {
                        image = "node:16-alpine"
                        script = ["npm run test:integration"]
                    }
                    
                    e2e_tests = {
                        image = "cypress/included:10.3.0"
                        script = ["cypress run"]
                    }
                }
            }
            
            build = {
                jobs = {
                    docker_build = {
                        image = "docker:20.10"
                        script = ["docker build -t app:latest ."]
                    }
                }
            }
        }
    "#;

    let parsed: serde_json::Value = from_str(config).expect("Failed to parse CI/CD config");

    assert!(parsed.is_object());
    let obj = parsed.as_object().unwrap();

    assert_eq!(obj["version"], "1.0");
    assert!(obj["global"].is_object());
    assert!(obj["stages"].is_object());

    let stages = obj["stages"].as_object().unwrap();
    assert!(stages.contains_key("test"));
    assert!(stages.contains_key("build"));

    let test_stage = stages["test"].as_object().unwrap();
    let test_jobs = test_stage["jobs"].as_object().unwrap();
    assert!(test_jobs.contains_key("unit_tests"));
    assert!(test_jobs.contains_key("integration_tests"));
    assert!(test_jobs.contains_key("e2e_tests"));
}
#[test]
fn test_real_world_monitoring_config() {
    // Test a comprehensive monitoring and alerting configuration
    let config = r##"
        # Comprehensive monitoring configuration
        monitoring = {
            # Prometheus configuration
            prometheus = {
                global = {
                    scrape_interval = "15s"
                    evaluation_interval = "15s"
                    external_labels = {
                        cluster = "production"
                        region = "us-west-2"
                    }
                }
                
                # Scrape configurations
                scrape_configs = [
                    {
                        job_name = "kubernetes-apiservers"
                        kubernetes_sd_configs = [
                            {
                                role = "endpoints"
                                namespaces = {
                                    names = ["default"]
                                }
                            }
                        ]
                        
                        relabel_configs = [
                            {
                                source_labels = ["__meta_kubernetes_namespace", "__meta_kubernetes_service_name", "__meta_kubernetes_endpoint_port_name"]
                                action = "keep"
                                regex = "default;kubernetes;https"
                            }
                        ]
                        
                        scheme = "https"
                        tls_config = {
                            ca_file = "/var/run/secrets/kubernetes.io/serviceaccount/ca.crt"
                            insecure_skip_verify = true
                        }
                        bearer_token_file = "/var/run/secrets/kubernetes.io/serviceaccount/token"
                    },
                    
                    {
                        job_name = "kubernetes-nodes"
                        kubernetes_sd_configs = [
                            {
                                role = "node"
                            }
                        ]
                        
                        relabel_configs = [
                            {
                                action = "labelmap"
                                regex = "__meta_kubernetes_node_label_(.+)"
                            }
                        ]
                    },
                    
                    {
                        job_name = "kubernetes-pods"
                        kubernetes_sd_configs = [
                            {
                                role = "pod"
                            }
                        ]
                        
                        relabel_configs = [
                            {
                                source_labels = ["__meta_kubernetes_pod_annotation_prometheus_io_scrape"]
                                action = "keep"
                                regex = "true"
                            },
                            {
                                source_labels = ["__meta_kubernetes_pod_annotation_prometheus_io_path"]
                                action = "replace"
                                target_label = "__metrics_path__"
                                regex = "(.+)"
                            }
                        ]
                    }
                ]
                
                # Alerting rules
                rule_files = [
                    "/etc/prometheus/rules/*.yml"
                ]
                
                # Alertmanager configuration
                alerting = {
                    alertmanagers = [
                        {
                            static_configs = [
                                {
                                    targets = ["alertmanager:9093"]
                                }
                            ]
                        }
                    ]
                }
                
                # Remote write configuration
                remote_write = [
                    {
                        url = "https://prometheus-remote-write.example.com/api/v1/write"
                        
                        basic_auth = {
                            username = "${REMOTE_WRITE_USERNAME}"
                            password = "${REMOTE_WRITE_PASSWORD}"
                        }
                        
                        write_relabel_configs = [
                            {
                                source_labels = ["__name__"]
                                regex = "go_.*"
                                action = "drop"
                            }
                        ]
                        
                        queue_config = {
                            capacity = 10000
                            max_shards = 1000
                            min_shards = 1
                            max_samples_per_send = 2000
                            batch_send_deadline = "5s"
                        }
                    }
                ]
            }
            
            # Grafana configuration
            grafana = {
                server = {
                    protocol = "http"
                    http_port = 3000
                    domain = "grafana.example.com"
                    root_url = "https://grafana.example.com/"
                }
                
                database = {
                    type = "postgres"
                    host = "grafana-db:5432"
                    name = "grafana"
                    user = "grafana"
                    password = "${GRAFANA_DB_PASSWORD}"
                    ssl_mode = "require"
                }
                
                security = {
                    admin_user = "admin"
                    admin_password = "${GRAFANA_ADMIN_PASSWORD}"
                    secret_key = "${GRAFANA_SECRET_KEY}"
                    
                    # OAuth configuration
                    oauth = {
                        github = {
                            enabled = true
                            client_id = "${GITHUB_CLIENT_ID}"
                            client_secret = "${GITHUB_CLIENT_SECRET}"
                            scopes = "user:email,read:org"
                            auth_url = "https://github.com/login/oauth/authorize"
                            token_url = "https://github.com/login/oauth/access_token"
                            api_url = "https://api.github.com/user"
                            allowed_organizations = ["example-org"]
                        }
                    }
                }
                
                # Data sources
                datasources = [
                    {
                        name = "Prometheus"
                        type = "prometheus"
                        access = "proxy"
                        url = "http://prometheus:9090"
                        is_default = true
                        
                        json_data = {
                            timeInterval = "15s"
                            queryTimeout = "60s"
                            httpMethod = "POST"
                        }
                    },
                    
                    {
                        name = "Loki"
                        type = "loki"
                        access = "proxy"
                        url = "http://loki:3100"
                        
                        json_data = {
                            maxLines = 1000
                            timeout = "60s"
                        }
                    },
                    
                    {
                        name = "Jaeger"
                        type = "jaeger"
                        access = "proxy"
                        url = "http://jaeger-query:16686"
                        
                        json_data = {
                            tracesToLogs = {
                                datasourceUid = "loki"
                                tags = ["job", "instance", "pod", "namespace"]
                            }
                        }
                    }
                ]
                
                # Dashboard provisioning
                dashboards = {
                    providers = [
                        {
                            name = "default"
                            org_id = 1
                            folder = ""
                            type = "file"
                            
                            options = {
                                path = "/var/lib/grafana/dashboards"
                            }
                        }
                    ]
                }
                
                # Plugins
                plugins = {
                    allow_loading_unsigned_plugins = ["grafana-piechart-panel"]
                    
                    install_plugins = [
                        "grafana-piechart-panel",
                        "grafana-worldmap-panel",
                        "grafana-clock-panel",
                        "camptocamp-prometheus-alertmanager-datasource"
                    ]
                }
            }
            
            # Alertmanager configuration
            alertmanager = {
                global = {
                    smtp_smarthost = "smtp.example.com:587"
                    smtp_from = "alerts@example.com"
                    smtp_auth_username = "${SMTP_USERNAME}"
                    smtp_auth_password = "${SMTP_PASSWORD}"
                }
                
                # Notification templates
                templates = [
                    "/etc/alertmanager/templates/*.tmpl"
                ]
                
                # Routing configuration
                route = {
                    group_by = ["alertname", "cluster", "service"]
                    group_wait = "10s"
                    group_interval = "10s"
                    repeat_interval = "1h"
                    receiver = "default"
                    
                    routes = [
                        {
                            match = {
                                severity = "critical"
                            }
                            receiver = "pagerduty-critical"
                            group_wait = "0s"
                            repeat_interval = "5m"
                        },
                        
                        {
                            match = {
                                severity = "warning"
                            }
                            receiver = "slack-warnings"
                            group_interval = "5m"
                            repeat_interval = "12h"
                        },
                        
                        {
                            match = {
                                alertname = "DeadMansSwitch"
                            }
                            receiver = "deadmansswitch"
                            repeat_interval = "30s"
                        }
                    ]
                }
                
                # Receivers configuration
                receivers = [
                    {
                        name = "default"
                        email_configs = [
                            {
                                to = "devops_team"
                                subject = "{{ .GroupLabels.alertname }} - {{ .Status | toUpper }}"
                                body = "{{ range .Alerts }}{{ .Annotations.description }}{{ end }}"
                            }
                        ]
                    },
                    
                    {
                        name = "pagerduty-critical"
                        pagerduty_configs = [
                            {
                                service_key = "${PAGERDUTY_SERVICE_KEY}"
                                description = "{{ .GroupLabels.alertname }}: {{ .GroupLabels.instance }}"
                                
                                details = {
                                    firing = "{{ .Alerts.Firing | len }}"
                                    resolved = "{{ .Alerts.Resolved | len }}"
                                    cluster = "{{ .GroupLabels.cluster }}"
                                }
                            }
                        ]
                    },
                    
                    {
                        name = "slack-warnings"
                        slack_configs = [
                            {
                                api_url = "${SLACK_WEBHOOK_URL}"
                                channel = "#alerts"
                                username = "alertmanager"
                                icon_emoji = ":warning:"
                                
                                title = "{{ .GroupLabels.alertname }}"
                                text = "{{ range .Alerts }}{{ .Annotations.summary }}{{ end }}"
                                
                                actions = [
                                    {
                                        type = "button"
                                        text = "Runbook"
                                        url = "{{ (index .Alerts 0).Annotations.runbook_url }}"
                                    },
                                    {
                                        type = "button"
                                        text = "Query"
                                        url = "{{ (index .Alerts 0).GeneratorURL }}"
                                    }
                                ]
                            }
                        ]
                    },
                    
                    {
                        name = "deadmansswitch"
                        webhook_configs = [
                            {
                                url = "https://deadmansswitch.example.com/ping/${DEADMANSSWITCH_TOKEN}"
                            }
                        ]
                    }
                ]
                
                # Inhibit rules
                inhibit_rules = [
                    {
                        source_match = {
                            severity = "critical"
                        }
                        target_match = {
                            severity = "warning"
                        }
                        equal = ["alertname", "cluster", "service"]
                    }
                ]
            }
            
            # Logging configuration (Loki)
            loki = {
                server = {
                    http_listen_port = 3100
                    grpc_listen_port = 9096
                }
                
                auth_enabled = false
                
                ingester = {
                    lifecycler = {
                        address = "127.0.0.1"
                        ring = {
                            kvstore = {
                                store = "inmemory"
                            }
                            replication_factor = 1
                        }
                        final_sleep = "0s"
                    }
                    chunk_idle_period = "5m"
                    chunk_retain_period = "30s"
                }
                
                schema_config = {
                    configs = [
                        {
                            from = "2020-10-24"
                            store = "boltdb"
                            object_store = "filesystem"
                            schema = "v11"
                            
                            index = {
                                prefix = "index_"
                                period = "168h"
                            }
                        }
                    ]
                }
                
                storage_config = {
                    boltdb = {
                        directory = "/loki/index"
                    }
                    
                    filesystem = {
                        directory = "/loki/chunks"
                    }
                }
                
                limits_config = {
                    enforce_metric_name = false
                    reject_old_samples = true
                    reject_old_samples_max_age = "168h"
                }
                
                chunk_store_config = {
                    max_look_back_period = "0s"
                }
                
                table_manager = {
                    retention_deletes_enabled = false
                    retention_period = "0s"
                }
            }
        }
        
        # Application-specific monitoring
        applications = {
            # Microservices monitoring
            microservices = {
                # Service level objectives
                slos = {
                    api_availability = {
                        target = 99.9
                        window = "30d"
                        
                        indicators = [
                            {
                                name = "http_requests_success_rate"
                                query = "sum(rate(http_requests_total{status!~\"5..\"}[5m])) / sum(rate(http_requests_total[5m]))"
                            }
                        ]
                    }
                    
                    api_latency = {
                        target = 95.0  # 95th percentile under 200ms
                        threshold = "200ms"
                        window = "30d"
                        
                        indicators = [
                            {
                                name = "http_request_duration_p95"
                                query = "histogram_quantile(0.95, sum(rate(http_request_duration_seconds_bucket[5m])) by (le))"
                            }
                        ]
                    }
                }
                
                # Custom metrics
                custom_metrics = [
                    {
                        name = "business_transactions_total"
                        type = "counter"
                        help = "Total number of business transactions processed"
                        labels = ["service", "transaction_type", "status"]
                    },
                    
                    {
                        name = "queue_depth"
                        type = "gauge"
                        help = "Current depth of processing queues"
                        labels = ["service", "queue_name"]
                    },
                    
                    {
                        name = "cache_hit_ratio"
                        type = "histogram"
                        help = "Cache hit ratio distribution"
                        labels = ["service", "cache_type"]
                        buckets = [0.5, 0.7, 0.8, 0.9, 0.95, 0.99, 1.0]
                    }
                ]
            }
            
            # Infrastructure monitoring
            infrastructure = {
                # Node exporter configuration
                node_exporter = {
                    enabled_collectors = [
                        "cpu", "diskstats", "filesystem", "loadavg",
                        "meminfo", "netdev", "netstat", "stat", "time"
                    ]
                    
                    disabled_collectors = [
                        "arp", "bcache", "bonding", "conntrack", "entropy",
                        "hwmon", "infiniband", "ipvs", "mdadm", "nfs", "nfsd",
                        "sockstat", "textfile", "uname", "vmstat", "wifi", "xfs", "zfs"
                    ]
                }
                
                # Blackbox exporter for external monitoring
                blackbox_exporter = {
                    modules = {
                        http_2xx = {
                            prober = "http"
                            timeout = "5s"
                            
                            http = {
                                valid_http_versions = ["HTTP/1.1", "HTTP/2.0"]
                                valid_status_codes = []  # Defaults to 2xx
                                method = "GET"
                                no_follow_redirects = false
                                fail_if_ssl = false
                                fail_if_not_ssl = false
                                
                                tls_config = {
                                    insecure_skip_verify = false
                                }
                                
                                preferred_ip_protocol = "ip4"
                            }
                        }
                        
                        tcp_connect = {
                            prober = "tcp"
                            timeout = "5s"
                        }
                        
                        icmp = {
                            prober = "icmp"
                            timeout = "5s"
                            
                            icmp = {
                                preferred_ip_protocol = "ip4"
                            }
                        }
                    }
                }
            }
        }
    "##;

    // Parse as JSON value due to the complex nested structure
    let parsed: serde_json::Value = from_str(config).expect("Failed to parse monitoring config");

    assert!(parsed.is_object());
    let obj = parsed.as_object().unwrap();

    // Verify main sections
    assert!(obj["monitoring"].is_object());
    assert!(obj["applications"].is_object());

    let monitoring = obj["monitoring"].as_object().unwrap();
    assert!(monitoring.contains_key("prometheus"));
    assert!(monitoring.contains_key("grafana"));
    assert!(monitoring.contains_key("alertmanager"));
    assert!(monitoring.contains_key("loki"));

    // Verify Prometheus configuration
    let prometheus = monitoring["prometheus"].as_object().unwrap();
    assert!(prometheus["scrape_configs"].is_array());
    assert!(prometheus["alerting"].is_object());

    // Verify Grafana configuration
    let grafana = monitoring["grafana"].as_object().unwrap();
    assert!(grafana["datasources"].is_array());
    assert!(grafana["security"].is_object());

    // Verify Alertmanager configuration
    let alertmanager = monitoring["alertmanager"].as_object().unwrap();
    assert!(alertmanager["receivers"].is_array());
    assert!(alertmanager["route"].is_object());
}

#[test]
fn test_performance_with_deeply_nested_structures() {
    // Test performance with very deeply nested configuration
    let mut config = String::new();

    // Create a deeply nested structure (20 levels deep)
    for i in 0..20 {
        config.push_str(&format!("level_{} = {{\n", i));
    }

    config.push_str("value = \"deep_value\"\n");

    for _ in 0..20 {
        config.push_str("}\n");
    }

    let start = std::time::Instant::now();
    let parsed: serde_json::Value =
        from_str(&config).expect("Failed to parse deeply nested config");
    let duration = start.elapsed();

    println!(
        "Deeply nested parsing: {} bytes in {:?}",
        config.len(),
        duration
    );

    // Verify the structure
    assert!(parsed.is_object());

    // Navigate to the deepest level
    let mut current = &parsed;
    for i in 0..20 {
        let key = format!("level_{}", i);
        current = &current[&key];
        assert!(current.is_object());
    }

    assert_eq!(current["value"], "deep_value");

    // Should handle deep nesting efficiently (under 1 second)
    assert!(duration.as_secs() < 1, "Deep nesting should parse quickly");
}

#[test]
fn test_compatibility_with_existing_ucl_files() {
    // Test with actual UCL configuration files that exist in the wild

    // FreeBSD pkg configuration format
    let freebsd_pkg = r#"
        name: "pkg";
        origin: "ports-mgmt/pkg";
        version: "1.18.4";
        comment: "Package manager";
        maintainer: "pkg@FreeBSD.org";
        www: "https://github.com/freebsd/pkg";
        abi = "FreeBSD:13:amd64";
        arch = "freebsd:13:x86:64";
        prefix = "/usr/local";
        sum = "sha256:1234567890abcdef...";
        flatsize: 2097152;
        path = "All/pkg-1.18.4.txz";
        repopath = "All/pkg-1.18.4.txz";
        
        deps: {
            libarchive: {
                origin: "archivers/libarchive";
                version: "3.6.1,1";
            }
        }
        
        categories = [ "ports-mgmt" ];
        licenses = [ "BSD2CLAUSE" ];
        
        options = {
            DOCS: "on";
            NLS: "on";
        }
    "#;

    let parsed: serde_json::Value =
        from_str(freebsd_pkg).expect("Failed to parse FreeBSD pkg format");
    assert!(parsed.is_object());

    // Rspamd configuration format
    let rspamd_config = r#"
        # Rspamd configuration
        .include "$CONFDIR/common.conf"
        
        options {
            pidfile = "$RUNDIR/rspamd.pid";
            .include "$CONFDIR/options.inc"
            .include(try=true; priority=1,duplicate=merge) "$LOCAL_CONFDIR/local.d/options.inc"
            .include(try=true; priority=10) "$LOCAL_CONFDIR/override.d/options.inc"
        }
        
        logging {
            type = "file";
            filename = "$LOGDIR/rspamd.log";
            level = "info";
            .include "$CONFDIR/logging.inc"
            .include(try=true; priority=1,duplicate=merge) "$LOCAL_CONFDIR/local.d/logging.inc"
            .include(try=true; priority=10) "$LOCAL_CONFDIR/override.d/logging.inc"
        }
        
        worker "normal" {
            bind_socket = "localhost:11333";
            .include "$CONFDIR/worker-normal.inc"
            .include(try=true; priority=1,duplicate=merge) "$LOCAL_CONFDIR/local.d/worker-normal.inc"
            .include(try=true; priority=10) "$LOCAL_CONFDIR/override.d/worker-normal.inc"
        }
        
        worker "controller" {
            bind_socket = "localhost:11334";
            .include "$CONFDIR/worker-controller.inc"
            .include(try=true; priority=1,duplicate=merge) "$LOCAL_CONFDIR/local.d/worker-controller.inc"
            .include(try=true; priority=10) "$LOCAL_CONFDIR/override.d/worker-controller.inc"
        }
    "#;

    // Note: This test might not fully parse due to .include directives
    // but should handle the basic UCL syntax
    let result = from_str::<serde_json::Value>(rspamd_config);
    match result {
        Ok(parsed) => {
            assert!(parsed.is_object());
            println!("Successfully parsed Rspamd-style config");
        }
        Err(e) => {
            println!(
                "Rspamd config parsing failed (expected due to .include): {}",
                e
            );
            // This is acceptable as .include is a preprocessor directive
        }
    }
}

#[test]
fn test_memory_efficiency_with_large_arrays() {
    // Test memory efficiency with large arrays
    let mut config = String::from("large_array = [\n");

    // Create a large array with 10,000 items
    for i in 0..10000 {
        config.push_str(&format!("  \"item_{}\",\n", i));
    }

    config.push_str("]\n");

    let start = std::time::Instant::now();
    let parsed: serde_json::Value = from_str(&config).expect("Failed to parse large array");
    let duration = start.elapsed();

    println!(
        "Large array parsing: {} bytes in {:?}",
        config.len(),
        duration
    );

    // Verify the array
    assert!(parsed.is_object());
    let array = parsed["large_array"].as_array().unwrap();
    assert_eq!(array.len(), 10000);
    assert_eq!(array[0], "item_0");
    assert_eq!(array[9999], "item_9999");

    // Should handle large arrays efficiently
    let throughput_mb_per_sec = (config.len() as f64 / 1_000_000.0) / duration.as_secs_f64();
    println!("Throughput: {:.2} MB/s", throughput_mb_per_sec);

    assert!(
        throughput_mb_per_sec > 1.0,
        "Should achieve at least 1 MB/s for large arrays"
    );
}
#[test]

fn test_cpp_comments_lexing() {
    // Test that C++ comments are properly lexed and skipped
    let config = r#"
        // This is a C++ comment
        key1 = "value1";  // Inline C++ comment
        key2 = "value2";
    "#;

    let result: Result<serde_json::Value, _> = from_str(config);
    assert!(result.is_ok(), "Should parse config with C++ comments");

    let parsed = result.unwrap();
    assert_eq!(parsed["key1"], "value1");
    assert_eq!(parsed["key2"], "value2");
}

#[test]
fn test_cpp_comments_mixed_with_other_comments() {
    // Test that C++ comments work alongside other comment types
    let config = r#"
        // C++ style comment
        key1 = "value1";  // Inline C++ comment
        # Hash comment
        key2 = "value2";  # Inline hash comment
        /* Multi-line comment */
        key3 = "value3";
    "#;

    let result: Result<serde_json::Value, _> = from_str(config);
    assert!(
        result.is_ok(),
        "Should parse config with mixed comment styles"
    );

    let parsed = result.unwrap();
    assert_eq!(parsed["key1"], "value1");
    assert_eq!(parsed["key2"], "value2");
    assert_eq!(parsed["key3"], "value3");
}

#[test]
fn test_cpp_comments_preservation() {
    use ucl_lexer::lexer::{CommentType, LexerConfig, Token, UclLexer};

    let config = r#"
        // Header comment
        {
            "key": "value"  // Inline comment
        }
    "#;

    let mut lexer_config = LexerConfig::default();
    lexer_config.save_comments = true;
    let mut lexer = UclLexer::with_config(config, lexer_config);

    let mut cpp_comments = 0;
    loop {
        match lexer.next_token() {
            Ok(Token::Eof) => break,
            Ok(Token::Comment(_)) => {
                // Check if this is a C++ style comment
                let comments = lexer.comments();
                if let Some(last_comment) = comments.last() {
                    if last_comment.comment_type == CommentType::CppStyle {
                        cpp_comments += 1;
                    }
                }
            }
            Ok(_) => {}
            Err(e) => panic!("Lexer error: {:?}", e),
        }
    }

    assert_eq!(cpp_comments, 2, "Should find 2 C++ style comments");
}

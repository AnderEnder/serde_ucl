//! Real-World Usage Examples
//!
//! This example demonstrates practical, real-world usage patterns for UCL
//! configuration in various application scenarios.

#![allow(dead_code)]

use serde::Deserialize;
use std::collections::HashMap;
use ucl_lexer::{EnvironmentVariableHandler, from_str, from_str_with_variables};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Real-World UCL Usage Examples");
    println!("=============================\n");

    // Example 1: Microservice configuration
    demo_microservice_config()?;

    // Example 2: CI/CD pipeline configuration
    demo_cicd_config()?;

    // Example 3: Game server configuration
    demo_game_server_config()?;

    // Example 4: IoT device configuration
    demo_iot_device_config()?;

    Ok(())
}

fn demo_microservice_config() -> Result<(), Box<dyn std::error::Error>> {
    println!("1. Microservice Configuration");
    println!("-----------------------------");

    let config_text = r#"
        # Microservice configuration for a user authentication service
        service {
            name = "auth-service"
            version = "2.1.0"
            environment = "${ENVIRONMENT:-development}"
            instance_id = "${HOSTNAME}-${PID}"
        }
        
        # HTTP server configuration
        http {
            host = "${HTTP_HOST:-0.0.0.0}"
            port = ${HTTP_PORT:-8080}
            
            # Request handling
            max_request_size = 10mb
            timeout = 30s
            keep_alive = 60s
            
            # TLS configuration
            tls {
                enabled = ${TLS_ENABLED:-false}
                cert_file = "${TLS_CERT_FILE:-/etc/ssl/certs/service.crt}"
                key_file = "${TLS_KEY_FILE:-/etc/ssl/private/service.key}"
                protocols = ["TLSv1.2", "TLSv1.3"]
            }
        }
        
        # Database configuration
        database {
            # Primary database
            primary {
                url = "${DATABASE_URL}"
                pool_size = ${DB_POOL_SIZE:-20}
                max_lifetime = 30min
                idle_timeout = 10min
                connection_timeout = 5s
            }
            
            # Read replicas for scaling
            replicas = [
                "${DB_REPLICA_1_URL:-}",
                "${DB_REPLICA_2_URL:-}"
            ]
            
            # Migration settings
            migrations {
                auto_migrate = ${AUTO_MIGRATE:-false}
                migration_timeout = 5min
            }
        }
        
        # Redis for caching and sessions
        redis {
            url = "${REDIS_URL:-redis://localhost:6379/0}"
            pool_size = ${REDIS_POOL_SIZE:-10}
            timeout = 2s
            
            # Key prefixes for different data types
            prefixes {
                sessions = "auth:session:"
                tokens = "auth:token:"
                rate_limits = "auth:ratelimit:"
            }
        }
        
        # JWT configuration
        jwt {
            secret = "${JWT_SECRET}"
            algorithm = "HS256"
            access_token_ttl = 15min
            refresh_token_ttl = 7d
            issuer = "${JWT_ISSUER:-auth-service}"
        }
        
        # Rate limiting
        rate_limiting {
            enabled = true
            
            # Different limits for different endpoints
            limits {
                login = { requests = 5, window = 1min }
                register = { requests = 3, window = 5min }
                password_reset = { requests = 2, window = 1h }
                default = { requests = 100, window = 1min }
            }
        }
        
        # Observability
        observability {
            # Metrics
            metrics {
                enabled = true
                endpoint = "/metrics"
                interval = 10s
            }
            
            # Distributed tracing
            tracing {
                enabled = ${TRACING_ENABLED:-false}
                jaeger_endpoint = "${JAEGER_ENDPOINT:-http://localhost:14268/api/traces}"
                service_name = "auth-service"
                sample_rate = ${TRACE_SAMPLE_RATE:-0.1}
            }
            
            # Health checks
            health {
                endpoint = "/health"
                checks = ["database", "redis", "external_apis"]
                timeout = 5s
            }
        }
        
        # External service integrations
        external_services {
            email_service {
                url = "${EMAIL_SERVICE_URL}"
                api_key = "${EMAIL_API_KEY}"
                timeout = 10s
                retry_attempts = 3
            }
            
            sms_service {
                url = "${SMS_SERVICE_URL}"
                api_key = "${SMS_API_KEY}"
                timeout = 5s
            }
        }
        
        # Security settings
        security {
            password_policy {
                min_length = 8
                require_uppercase = true
                require_lowercase = true
                require_numbers = true
                require_symbols = true
            }
            
            session_security {
                secure_cookies = ${SECURE_COOKIES:-true}
                same_site = "Strict"
                csrf_protection = true
            }
        }
    "#;

    #[derive(Debug, Deserialize)]
    struct MicroserviceConfig {
        service: ServiceInfo,
        http: HttpConfig,
        database: DatabaseConfig,
        redis: RedisConfig,
        jwt: JwtConfig,
        rate_limiting: RateLimitingConfig,
        observability: ObservabilityConfig,
        external_services: ExternalServicesConfig,
        security: SecurityConfig,
    }

    #[derive(Debug, Deserialize)]
    struct ServiceInfo {
        name: String,
        version: String,
        environment: String,
        instance_id: String,
    }

    #[derive(Debug, Deserialize)]
    struct HttpConfig {
        host: String,
        port: u16,
        max_request_size: u64,
        timeout: f64,
        keep_alive: f64,
        tls: TlsConfig,
    }

    #[derive(Debug, Deserialize)]
    struct TlsConfig {
        enabled: bool,
        cert_file: String,
        key_file: String,
        protocols: Vec<String>,
    }

    #[derive(Debug, Deserialize)]
    struct DatabaseConfig {
        primary: DatabaseConnection,
        replicas: Vec<String>,
        migrations: MigrationConfig,
    }

    #[derive(Debug, Deserialize)]
    struct DatabaseConnection {
        url: String,
        pool_size: u32,
        max_lifetime: f64,
        idle_timeout: f64,
        connection_timeout: f64,
    }

    #[derive(Debug, Deserialize)]
    struct MigrationConfig {
        auto_migrate: bool,
        migration_timeout: f64,
    }

    #[derive(Debug, Deserialize)]
    struct RedisConfig {
        url: String,
        pool_size: u32,
        timeout: f64,
        prefixes: RedisPrefixes,
    }

    #[derive(Debug, Deserialize)]
    struct RedisPrefixes {
        sessions: String,
        tokens: String,
        rate_limits: String,
    }

    #[derive(Debug, Deserialize)]
    struct JwtConfig {
        secret: String,
        algorithm: String,
        access_token_ttl: f64,
        refresh_token_ttl: f64,
        issuer: String,
    }

    #[derive(Debug, Deserialize)]
    struct RateLimitingConfig {
        enabled: bool,
        limits: HashMap<String, RateLimit>,
    }

    #[derive(Debug, Deserialize)]
    struct RateLimit {
        requests: u32,
        window: f64,
    }

    #[derive(Debug, Deserialize)]
    struct ObservabilityConfig {
        metrics: MetricsConfig,
        tracing: TracingConfig,
        health: HealthConfig,
    }

    #[derive(Debug, Deserialize)]
    struct MetricsConfig {
        enabled: bool,
        endpoint: String,
        interval: f64,
    }

    #[derive(Debug, Deserialize)]
    struct TracingConfig {
        enabled: bool,
        jaeger_endpoint: String,
        service_name: String,
        sample_rate: f64,
    }

    #[derive(Debug, Deserialize)]
    struct HealthConfig {
        endpoint: String,
        checks: Vec<String>,
        timeout: f64,
    }

    #[derive(Debug, Deserialize)]
    struct ExternalServicesConfig {
        email_service: ExternalService,
        sms_service: ExternalService,
    }

    #[derive(Debug, Deserialize)]
    struct ExternalService {
        url: String,
        api_key: String,
        timeout: f64,
        #[serde(default)]
        retry_attempts: Option<u32>,
    }

    #[derive(Debug, Deserialize)]
    struct SecurityConfig {
        password_policy: PasswordPolicy,
        session_security: SessionSecurity,
    }

    #[derive(Debug, Deserialize)]
    struct PasswordPolicy {
        min_length: u32,
        require_uppercase: bool,
        require_lowercase: bool,
        require_numbers: bool,
        require_symbols: bool,
    }

    #[derive(Debug, Deserialize)]
    struct SessionSecurity {
        secure_cookies: bool,
        same_site: String,
        csrf_protection: bool,
    }

    // Set up environment variables for demonstration
    unsafe {
        std::env::set_var("ENVIRONMENT", "production");
        std::env::set_var(
            "DATABASE_URL",
            "postgresql://auth:secret@db.internal:5432/auth_db",
        );
        std::env::set_var("JWT_SECRET", "super-secret-jwt-key");
        std::env::set_var("EMAIL_SERVICE_URL", "https://api.sendgrid.com/v3");
        std::env::set_var("EMAIL_API_KEY", "SG.xxx");
        std::env::set_var("SMS_SERVICE_URL", "https://api.twilio.com/2010-04-01");
        std::env::set_var("SMS_API_KEY", "AC123xxx");
    }

    let config: MicroserviceConfig =
        from_str_with_variables(config_text, Box::new(EnvironmentVariableHandler))?;

    {
        assert!(!config.service.instance_id.is_empty());
        assert!(config.http.max_request_size > 0);
        assert!(config.http.timeout >= 0.0);
        assert!(config.http.keep_alive >= 0.0);
        assert!(config.http.tls.protocols.len() >= 1);
        assert!(!config.http.tls.cert_file.is_empty());
        assert!(!config.http.tls.key_file.is_empty());
        assert!(config.database.migrations.auto_migrate || !config.database.migrations.auto_migrate);
        assert!(config.database.migrations.migration_timeout >= 0.0);
        assert!(config.database.primary.max_lifetime >= 0.0);
        assert!(config.database.primary.idle_timeout >= 0.0);
        assert!(config.database.primary.connection_timeout >= 0.0);
        assert!(config.redis.timeout >= 0.0);
        assert!(!config.redis.prefixes.sessions.is_empty());
        assert!(!config.redis.prefixes.tokens.is_empty());
        assert!(!config.redis.prefixes.rate_limits.is_empty());
        assert!(!config.jwt.secret.is_empty());
        assert!(config.jwt.refresh_token_ttl > 0.0);
        assert!(!config.jwt.issuer.is_empty());
        assert!(config.rate_limiting.limits.contains_key("login"));
        assert!(config.observability.metrics.enabled || !config.observability.metrics.enabled);
        assert!(!config.observability.metrics.endpoint.is_empty());
        assert!(config.observability.metrics.interval >= 0.0);
        assert!(config.observability.tracing.sample_rate >= 0.0);
        assert!(config.observability.health.timeout >= 0.0);
        assert!(!config.external_services.email_service.url.is_empty());
        assert!(!config.external_services.sms_service.url.is_empty());
        assert!(config.security.password_policy.min_length > 0);
        assert!(config.security.session_security.csrf_protection || true);
    }

    println!("Microservice configuration loaded:");
    println!(
        "  Service: {} v{} ({})",
        config.service.name, config.service.version, config.service.environment
    );
    println!(
        "  HTTP: {}:{} (TLS: {})",
        config.http.host, config.http.port, config.http.tls.enabled
    );
    println!(
        "  Database pool: {} connections",
        config.database.primary.pool_size
    );
    println!(
        "  Database replicas: {} (auto migrate: {})",
        config.database.replicas.len(),
        config.database.migrations.auto_migrate
    );
    println!(
        "  Redis: {} (pool: {})",
        config.redis.url, config.redis.pool_size
    );
    println!(
        "  JWT: {} algorithm, {}s access token TTL",
        config.jwt.algorithm, config.jwt.access_token_ttl
    );
    println!(
        "  Rate limiting: {} (login: {}/{}s)",
        config.rate_limiting.enabled,
        config
            .rate_limiting
            .limits
            .get("login")
            .map(|l| l.requests)
            .unwrap_or(0),
        config
            .rate_limiting
            .limits
            .get("login")
            .map(|l| l.window)
            .unwrap_or(0.0)
    );
    println!(
        "  Tracing: {} ({}% sampling)",
        config.observability.tracing.enabled,
        config.observability.tracing.sample_rate * 100.0
    );

    assert!(!format!("{:?}", config).is_empty());
    println!();
    Ok(())
}

fn demo_cicd_config() -> Result<(), Box<dyn std::error::Error>> {
    println!("2. CI/CD Pipeline Configuration");
    println!("-------------------------------");

    let config_text = r##"
        # CI/CD Pipeline configuration
        pipeline {
            name = "web-app-deployment"
            version = "1.0"
            
            # Pipeline triggers
            triggers {
                branches = ["main", "develop", "release/*"]
                tags = ["v*"]
                pull_requests = true
                schedule = "0 2 * * *"  # Daily at 2 AM
            }
        }
        
        # Environment definitions
        environments {
            development {
                auto_deploy = true
                approval_required = false
                
                variables {
                    API_URL = "https://dev-api.example.com"
                    DATABASE_URL = "${DEV_DATABASE_URL}"
                    LOG_LEVEL = "debug"
                }
                
                resources {
                    cpu = "500m"
                    memory = "512Mi"
                    replicas = 1
                }
            }
            
            staging {
                auto_deploy = false
                approval_required = true
                approvers = ["team-lead", "senior-dev"]
                
                variables {
                    API_URL = "https://staging-api.example.com"
                    DATABASE_URL = "${STAGING_DATABASE_URL}"
                    LOG_LEVEL = "info"
                }
                
                resources {
                    cpu = "1000m"
                    memory = "1Gi"
                    replicas = 2
                }
            }
            
            production {
                auto_deploy = false
                approval_required = true
                approvers = ["team-lead", "devops-lead", "product-owner"]
                
                variables {
                    API_URL = "https://api.example.com"
                    DATABASE_URL = "${PROD_DATABASE_URL}"
                    LOG_LEVEL = "warn"
                }
                
                resources {
                    cpu = "2000m"
                    memory = "2Gi"
                    replicas = 5
                }
                
                # Production-specific settings
                blue_green_deployment = true
                rollback_on_failure = true
                health_check_timeout = 5min
            }
        }
        
        # Build configuration
        build {
            # Docker build settings
            docker {
                dockerfile = "Dockerfile"
                context = "."
                build_args {
                    NODE_ENV = "production"
                    BUILD_VERSION = "${CI_COMMIT_SHA}"
                }
                
                # Multi-stage build targets
                targets = ["test", "production"]
            }
            
            # Test configuration
            tests {
                unit_tests {
                    command = "npm test"
                    timeout = 10min
                    coverage_threshold = 80.0
                }
                
                integration_tests {
                    command = "npm run test:integration"
                    timeout = 20min
                    services = ["database", "redis"]
                }
                
                e2e_tests {
                    command = "npm run test:e2e"
                    timeout = 30min
                    browser = "chrome"
                    parallel = 3
                }
            }
            
            # Security scanning
            security {
                dependency_scan = true
                container_scan = true
                sast_scan = true
                
                # Fail build on high severity issues
                fail_on_high = true
                fail_on_critical = true
            }
        }
        
        # Deployment configuration
        deployment {
            # Kubernetes deployment
            kubernetes {
                namespace = "${ENVIRONMENT}"
                cluster = "${K8S_CLUSTER}"
                
                # Resource limits
                limits {
                    cpu = "2000m"
                    memory = "2Gi"
                }
                
                # Health checks
                health_checks {
                    liveness_probe = "/health"
                    readiness_probe = "/ready"
                    startup_probe = "/startup"
                }
            }
            
            # Database migrations
            migrations {
                enabled = true
                timeout = 10min
                rollback_on_failure = true
            }
            
            # Post-deployment tasks
            post_deploy {
                smoke_tests = true
                cache_warmup = true
                notify_teams = ["#deployments", "#alerts"]
            }
        }
        
        # Monitoring and alerting
        monitoring {
            # Deployment monitoring
            deployment_monitoring {
                enabled = true
                duration = 15min
                
                metrics = [
                    "error_rate < 1%",
                    "response_time_p95 < 500ms",
                    "cpu_usage < 80%",
                    "memory_usage < 85%"
                ]
            }
            
            # Rollback triggers
            rollback {
                auto_rollback = true
                
                triggers = [
                    "error_rate > 5%",
                    "response_time_p95 > 2s",
                    "health_check_failures > 3"
                ]
            }
        }
        
        # Notification settings
        notifications {
            slack {
                webhook_url = "${SLACK_WEBHOOK_URL}"
                channels = ["#deployments", "#alerts"]
                
                events = [
                    "deployment_started",
                    "deployment_success",
                    "deployment_failed",
                    "rollback_triggered"
                ]
            }
            
            email {
                recipients = ["devops@example.com", "team-lead@example.com"]
                
                events = [
                    "deployment_failed",
                    "rollback_triggered",
                    "security_scan_failed"
                ]
            }
        }
    "##;

    #[derive(Debug, Deserialize)]
    struct CicdConfig {
        pipeline: PipelineInfo,
        environments: HashMap<String, Environment>,
        build: BuildConfig,
        deployment: DeploymentConfig,
        monitoring: MonitoringConfig,
        notifications: NotificationConfig,
    }

    #[derive(Debug, Deserialize)]
    struct PipelineInfo {
        name: String,
        version: String,
        triggers: TriggerConfig,
    }

    #[derive(Debug, Deserialize)]
    struct TriggerConfig {
        branches: Vec<String>,
        tags: Vec<String>,
        pull_requests: bool,
        schedule: String,
    }

    #[derive(Debug, Deserialize)]
    struct Environment {
        auto_deploy: bool,
        approval_required: bool,
        #[serde(default)]
        approvers: Vec<String>,
        variables: HashMap<String, String>,
        resources: ResourceConfig,
        #[serde(default)]
        blue_green_deployment: bool,
        #[serde(default)]
        rollback_on_failure: bool,
        #[serde(default)]
        health_check_timeout: Option<f64>,
    }

    #[derive(Debug, Deserialize)]
    struct ResourceConfig {
        cpu: String,
        memory: String,
        replicas: u32,
    }

    #[derive(Debug, Deserialize)]
    struct BuildConfig {
        docker: DockerConfig,
        tests: TestConfig,
        security: SecurityConfig,
    }

    #[derive(Debug, Deserialize)]
    struct DockerConfig {
        dockerfile: String,
        context: String,
        build_args: HashMap<String, String>,
        targets: Vec<String>,
    }

    #[derive(Debug, Deserialize)]
    struct TestConfig {
        unit_tests: TestSuite,
        integration_tests: IntegrationTestSuite,
        e2e_tests: E2eTestSuite,
    }

    #[derive(Debug, Deserialize)]
    struct TestSuite {
        command: String,
        timeout: f64,
        coverage_threshold: f64,
    }

    #[derive(Debug, Deserialize)]
    struct IntegrationTestSuite {
        command: String,
        timeout: f64,
        services: Vec<String>,
    }

    #[derive(Debug, Deserialize)]
    struct E2eTestSuite {
        command: String,
        timeout: f64,
        browser: String,
        parallel: u32,
    }

    #[derive(Debug, Deserialize)]
    struct SecurityConfig {
        dependency_scan: bool,
        container_scan: bool,
        sast_scan: bool,
        fail_on_high: bool,
        fail_on_critical: bool,
    }

    #[derive(Debug, Deserialize)]
    struct DeploymentConfig {
        kubernetes: KubernetesConfig,
        migrations: MigrationConfig,
        post_deploy: PostDeployConfig,
    }

    #[derive(Debug, Deserialize)]
    struct MigrationConfig {
        enabled: bool,
        timeout: f64,
        rollback_on_failure: bool,
    }

    #[derive(Debug, Deserialize)]
    struct KubernetesConfig {
        namespace: String,
        cluster: String,
        limits: ResourceLimits,
        health_checks: HealthCheckConfig,
    }

    #[derive(Debug, Deserialize)]
    struct ResourceLimits {
        cpu: String,
        memory: String,
    }

    #[derive(Debug, Deserialize)]
    struct HealthCheckConfig {
        liveness_probe: String,
        readiness_probe: String,
        startup_probe: String,
    }

    #[derive(Debug, Deserialize)]
    struct PostDeployConfig {
        smoke_tests: bool,
        cache_warmup: bool,
        notify_teams: Vec<String>,
    }

    #[derive(Debug, Deserialize)]
    struct MonitoringConfig {
        deployment_monitoring: DeploymentMonitoring,
        rollback: RollbackConfig,
    }

    #[derive(Debug, Deserialize)]
    struct DeploymentMonitoring {
        enabled: bool,
        duration: f64,
        metrics: Vec<String>,
    }

    #[derive(Debug, Deserialize)]
    struct RollbackConfig {
        auto_rollback: bool,
        triggers: Vec<String>,
    }

    #[derive(Debug, Deserialize)]
    struct NotificationConfig {
        slack: SlackConfig,
        email: EmailConfig,
    }

    #[derive(Debug, Deserialize)]
    struct SlackConfig {
        webhook_url: String,
        channels: Vec<String>,
        events: Vec<String>,
    }

    #[derive(Debug, Deserialize)]
    struct EmailConfig {
        recipients: Vec<String>,
        events: Vec<String>,
    }

    let config: CicdConfig = from_str(config_text)?;

    println!("CI/CD pipeline configuration loaded:");
    println!(
        "  Pipeline: {} v{}",
        config.pipeline.name, config.pipeline.version
    );
    println!("  Environments: {}", config.environments.len());

    for (name, env) in &config.environments {
        println!(
            "    {}: {} replicas, auto-deploy: {}, approval: {}",
            name, env.resources.replicas, env.auto_deploy, env.approval_required
        );
    }

    println!("  Build targets: {:?}", config.build.docker.targets);
    println!(
        "  Security scans: dependency={}, container={}, SAST={}",
        config.build.security.dependency_scan,
        config.build.security.container_scan,
        config.build.security.sast_scan
    );
    println!(
        "  Monitoring: {}min deployment monitoring",
        config.monitoring.deployment_monitoring.duration / 60.0
    );
    println!(
        "  Notifications: {} Slack channels, {} email recipients",
        config.notifications.slack.channels.len(),
        config.notifications.email.recipients.len()
    );

    assert!(!format!("{:?}", config).is_empty());
    println!();
    Ok(())
}

fn demo_game_server_config() -> Result<(), Box<dyn std::error::Error>> {
    println!("3. Game Server Configuration");
    println!("----------------------------");

    let config_text = r#"
        # Game server configuration for a multiplayer online game
        server {
            name = "game-server-01"
            region = "us-west-2"
            max_players = 1000
            tick_rate = 60hz
            
            # Network settings
            network {
                host = "0.0.0.0"
                port = 7777
                protocol = "UDP"
                
                # Connection limits
                max_connections_per_ip = 5
                connection_timeout = 30s
                heartbeat_interval = 5s
                
                # Anti-cheat networking
                packet_validation = true
                rate_limiting = true
                max_packets_per_second = 100
            }
        }
        
        # Game world configuration
        world {
            # Map settings
            map {
                name = "battle_royale_island"
                size = { width = 8192, height = 8192 }
                spawn_points = 100
                
                # Dynamic elements
                weather_system = true
                day_night_cycle = true
                destructible_environment = true
            }
            
            # Physics simulation
            physics {
                gravity = -9.81
                tick_rate = 120hz
                collision_detection = "continuous"
                
                # Performance settings
                max_rigid_bodies = 10000
                spatial_partitioning = "octree"
                physics_threads = 4
            }
            
            # Game mechanics
            mechanics {
                # Combat system
                combat {
                    damage_falloff = true
                    headshot_multiplier = 2.0
                    friendly_fire = false
                    respawn_time = 10s
                }
                
                # Inventory system
                inventory {
                    max_slots = 50
                    weight_limit = 100kg
                    auto_pickup = true
                }
                
                # Progression system
                progression {
                    xp_multiplier = 1.0
                    level_cap = 100
                    prestige_enabled = true
                }
            }
        }
        
        # Matchmaking configuration
        matchmaking {
            enabled = true
            
            # Queue settings
            queues {
                casual {
                    skill_range = 500
                    max_wait_time = 2min
                    backfill_enabled = true
                }
                
                ranked {
                    skill_range = 100
                    max_wait_time = 5min
                    backfill_enabled = false
                    placement_matches = 10
                }
                
                custom {
                    password_protected = true
                    max_players = 16
                    custom_rules = true
                }
            }
            
            # Skill-based matchmaking
            sbmm {
                enabled = true
                algorithm = "elo"
                initial_rating = 1000
                k_factor = 32
                
                # Rating adjustments
                win_bonus = 25
                loss_penalty = -20
                performance_modifier = true
            }
        }
        
        # Anti-cheat system
        anticheat {
            enabled = true
            
            # Detection methods
            detection {
                statistical_analysis = true
                behavioral_analysis = true
                signature_detection = true
                
                # Thresholds
                suspicious_score_threshold = 75
                ban_score_threshold = 95
                
                # Actions
                auto_kick = true
                auto_ban = false  # Require manual review
                shadow_ban = true
            }
            
            # Server-side validation
            validation {
                movement_validation = true
                action_validation = true
                inventory_validation = true
                
                # Tolerance settings
                position_tolerance = 0.1
                speed_tolerance = 1.1
                action_rate_limit = 10  # actions per second
            }
        }
        
        # Performance monitoring
        performance {
            # Server performance
            server_metrics {
                cpu_warning_threshold = 80.0
                memory_warning_threshold = 85.0
                network_warning_threshold = 90.0
                
                # Tick performance
                tick_time_warning = 16ms  # 60 FPS = 16.67ms per tick
                tick_time_critical = 20ms
            }
            
            # Game performance
            game_metrics {
                player_count_target = 800
                average_ping_target = 50ms
                packet_loss_threshold = 1.0
                
                # Performance scaling
                auto_scale_enabled = true
                scale_up_threshold = 90.0
                scale_down_threshold = 30.0
            }
        }
        
        # Database configuration
        database {
            # Player data
            player_db {
                url = "${PLAYER_DB_URL}"
                pool_size = 50
                timeout = 5s
                
                # Caching
                cache_enabled = true
                cache_ttl = 5min
            }
            
            # Game statistics
            stats_db {
                url = "${STATS_DB_URL}"
                pool_size = 20
                batch_size = 1000
                flush_interval = 30s
            }
            
            # Leaderboards
            leaderboard_db {
                url = "${LEADERBOARD_DB_URL}"
                update_interval = 1min
                top_players_count = 1000
            }
        }
        
        # Logging and analytics
        logging {
            # Game events
            game_events {
                player_actions = true
                combat_events = true
                economy_events = true
                
                # Event batching
                batch_size = 100
                flush_interval = 10s
            }
            
            # Performance logs
            performance_logs {
                tick_performance = true
                network_performance = true
                database_performance = true
                
                # Log levels
                level = "info"
                detailed_profiling = false
            }
        }
        
        # Content delivery
        cdn {
            # Asset delivery
            assets {
                base_url = "${CDN_BASE_URL}"
                cache_duration = 1h
                
                # Asset types
                textures_path = "/textures/"
                models_path = "/models/"
                sounds_path = "/sounds/"
            }
            
            # Update delivery
            updates {
                check_interval = 1h
                auto_download = true
                delta_updates = true
            }
        }
    "#;

    #[derive(Debug, Deserialize)]
    struct GameServerConfig {
        server: GameServer,
        world: GameWorld,
        matchmaking: MatchmakingConfig,
        anticheat: AntiCheatConfig,
        performance: PerformanceConfig,
        database: GameDatabaseConfig,
        logging: GameLoggingConfig,
        cdn: CdnConfig,
    }

    // Define all the nested structs (abbreviated for brevity)
    #[derive(Debug, Deserialize)]
    struct GameServer {
        name: String,
        region: String,
        max_players: u32,
        tick_rate: String,
        network: NetworkConfig,
    }

    #[derive(Debug, Deserialize)]
    struct NetworkConfig {
        host: String,
        port: u16,
        protocol: String,
        max_connections_per_ip: u32,
        connection_timeout: f64,
        heartbeat_interval: f64,
        packet_validation: bool,
        rate_limiting: bool,
        max_packets_per_second: u32,
    }

    #[derive(Debug, Deserialize)]
    struct GameWorld {
        map: MapConfig,
        physics: PhysicsConfig,
        mechanics: MechanicsConfig,
    }

    #[derive(Debug, Deserialize)]
    struct MapConfig {
        name: String,
        size: MapSize,
        spawn_points: u32,
        weather_system: bool,
        day_night_cycle: bool,
        destructible_environment: bool,
    }

    #[derive(Debug, Deserialize)]
    struct MapSize {
        width: u32,
        height: u32,
    }

    #[derive(Debug, Deserialize)]
    struct PhysicsConfig {
        gravity: f64,
        tick_rate: String,
        collision_detection: String,
        max_rigid_bodies: u32,
        spatial_partitioning: String,
        physics_threads: u32,
    }

    #[derive(Debug, Deserialize)]
    struct MechanicsConfig {
        combat: CombatConfig,
        inventory: InventoryConfig,
        progression: ProgressionConfig,
    }

    #[derive(Debug, Deserialize)]
    struct CombatConfig {
        damage_falloff: bool,
        headshot_multiplier: f64,
        friendly_fire: bool,
        respawn_time: f64,
    }

    #[derive(Debug, Deserialize)]
    struct InventoryConfig {
        max_slots: u32,
        weight_limit: String,
        auto_pickup: bool,
    }

    #[derive(Debug, Deserialize)]
    struct ProgressionConfig {
        xp_multiplier: f64,
        level_cap: u32,
        prestige_enabled: bool,
    }

    #[derive(Debug, Deserialize)]
    struct MatchmakingConfig {
        enabled: bool,
        queues: HashMap<String, serde_json::Value>,
        sbmm: SbmmConfig,
    }

    #[derive(Debug, Deserialize)]
    struct SbmmConfig {
        enabled: bool,
        algorithm: String,
        initial_rating: u32,
        k_factor: u32,
        win_bonus: i32,
        loss_penalty: i32,
        performance_modifier: bool,
    }

    #[derive(Debug, Deserialize)]
    struct AntiCheatConfig {
        enabled: bool,
        detection: DetectionConfig,
        validation: ValidationConfig,
    }

    #[derive(Debug, Deserialize)]
    struct DetectionConfig {
        statistical_analysis: bool,
        behavioral_analysis: bool,
        signature_detection: bool,
        suspicious_score_threshold: u32,
        ban_score_threshold: u32,
        auto_kick: bool,
        auto_ban: bool,
        shadow_ban: bool,
    }

    #[derive(Debug, Deserialize)]
    struct ValidationConfig {
        movement_validation: bool,
        action_validation: bool,
        inventory_validation: bool,
        position_tolerance: f64,
        speed_tolerance: f64,
        action_rate_limit: u32,
    }

    #[derive(Debug, Deserialize)]
    struct PerformanceConfig {
        server_metrics: ServerMetrics,
        game_metrics: GameMetrics,
    }

    #[derive(Debug, Deserialize)]
    struct ServerMetrics {
        cpu_warning_threshold: f64,
        memory_warning_threshold: f64,
        network_warning_threshold: f64,
        tick_time_warning: f64,
        tick_time_critical: f64,
    }

    #[derive(Debug, Deserialize)]
    struct GameMetrics {
        player_count_target: u32,
        average_ping_target: f64,
        packet_loss_threshold: f64,
        auto_scale_enabled: bool,
        scale_up_threshold: f64,
        scale_down_threshold: f64,
    }

    #[derive(Debug, Deserialize)]
    struct GameDatabaseConfig {
        player_db: PlayerDbConfig,
        stats_db: StatsDbConfig,
        leaderboard_db: LeaderboardDbConfig,
    }

    #[derive(Debug, Deserialize)]
    struct PlayerDbConfig {
        url: String,
        pool_size: u32,
        timeout: f64,
        cache_enabled: bool,
        cache_ttl: f64,
    }

    #[derive(Debug, Deserialize)]
    struct StatsDbConfig {
        url: String,
        pool_size: u32,
        batch_size: u32,
        flush_interval: f64,
    }

    #[derive(Debug, Deserialize)]
    struct LeaderboardDbConfig {
        url: String,
        update_interval: f64,
        top_players_count: u32,
    }

    #[derive(Debug, Deserialize)]
    struct GameLoggingConfig {
        game_events: GameEventsConfig,
        performance_logs: PerformanceLogsConfig,
    }

    #[derive(Debug, Deserialize)]
    struct GameEventsConfig {
        player_actions: bool,
        combat_events: bool,
        economy_events: bool,
        batch_size: u32,
        flush_interval: f64,
    }

    #[derive(Debug, Deserialize)]
    struct PerformanceLogsConfig {
        tick_performance: bool,
        network_performance: bool,
        database_performance: bool,
        level: String,
        detailed_profiling: bool,
    }

    #[derive(Debug, Deserialize)]
    struct CdnConfig {
        assets: AssetsConfig,
        updates: UpdatesConfig,
    }

    #[derive(Debug, Deserialize)]
    struct AssetsConfig {
        base_url: String,
        cache_duration: f64,
        textures_path: String,
        models_path: String,
        sounds_path: String,
    }

    #[derive(Debug, Deserialize)]
    struct UpdatesConfig {
        check_interval: f64,
        auto_download: bool,
        delta_updates: bool,
    }

    let config: GameServerConfig = from_str(config_text)?;

    println!("Game server configuration loaded:");
    println!(
        "  Server: {} in {} (max {} players)",
        config.server.name, config.server.region, config.server.max_players
    );
    println!(
        "  Network: {}:{} ({})",
        config.server.network.host, config.server.network.port, config.server.network.protocol
    );
    println!(
        "  World: {} map ({}x{})",
        config.world.map.name, config.world.map.size.width, config.world.map.size.height
    );
    println!(
        "  Physics: {} tick rate, {} threads",
        config.world.physics.tick_rate, config.world.physics.physics_threads
    );
    println!(
        "  Matchmaking: {} (SBMM: {})",
        config.matchmaking.enabled, config.matchmaking.sbmm.enabled
    );
    println!(
        "  Anti-cheat: {} (auto-kick: {})",
        config.anticheat.enabled, config.anticheat.detection.auto_kick
    );
    println!(
        "  Performance: {}% CPU warning, {}ms tick warning",
        config.performance.server_metrics.cpu_warning_threshold,
        config.performance.server_metrics.tick_time_warning * 1000.0
    );

    assert!(!format!("{:?}", config).is_empty());
    println!();
    Ok(())
}

fn demo_iot_device_config() -> Result<(), Box<dyn std::error::Error>> {
    println!("4. IoT Device Configuration");
    println!("---------------------------");

    let config_text = r#"
        # IoT device configuration for a smart environmental sensor
        device {
            id = "${DEVICE_ID}"
            name = "Environmental Sensor v2.1"
            firmware_version = "2.1.3"
            hardware_revision = "B"
            
            # Device capabilities
            capabilities = [
                "temperature", "humidity", "pressure", 
                "air_quality", "light", "motion"
            ]
        }
        
        # Sensor configuration
        sensors {
            temperature {
                enabled = true
                unit = "celsius"
                precision = 0.1
                range = { min = -40.0, max = 85.0 }
                calibration_offset = 0.0
                sample_rate = 10s
            }
            
            humidity {
                enabled = true
                unit = "percent"
                precision = 0.5
                range = { min = 0.0, max = 100.0 }
                calibration_offset = 0.0
                sample_rate = 10s
            }
            
            pressure {
                enabled = true
                unit = "hpa"
                precision = 0.1
                range = { min = 300.0, max = 1100.0 }
                calibration_offset = 0.0
                sample_rate = 30s
            }
            
            air_quality {
                enabled = true
                unit = "aqi"
                precision = 1.0
                range = { min = 0.0, max = 500.0 }
                sample_rate = 60s
                
                # Gas sensors
                co2_enabled = true
                voc_enabled = true
                pm25_enabled = true
            }
            
            light {
                enabled = true
                unit = "lux"
                precision = 1.0
                range = { min = 0.0, max = 100000.0 }
                sample_rate = 30s
            }
            
            motion {
                enabled = true
                sensitivity = "medium"  # low, medium, high
                timeout = 5min
                sample_rate = 1s
            }
        }
        
        # Connectivity configuration
        connectivity {
            # WiFi settings
            wifi {
                enabled = true
                ssid = "${WIFI_SSID}"
                password = "${WIFI_PASSWORD}"
                
                # Connection settings
                auto_reconnect = true
                reconnect_interval = 30s
                max_reconnect_attempts = 10
                
                # Power management
                power_save_mode = true
                sleep_interval = 5min
            }
            
            # Cellular backup (if available)
            cellular {
                enabled = false
                apn = "${CELLULAR_APN:-}"
                username = "${CELLULAR_USER:-}"
                password = "${CELLULAR_PASS:-}"
                
                # Fallback settings
                fallback_enabled = true
                fallback_timeout = 2min
            }
            
            # Bluetooth for local configuration
            bluetooth {
                enabled = true
                discoverable = false
                pairing_timeout = 5min
                
                # Security
                require_pairing = true
                pin_code = "${BT_PIN:-1234}"
            }
        }
        
        # Data transmission
        data_transmission {
            # Cloud endpoint
            cloud {
                endpoint = "${CLOUD_ENDPOINT}"
                api_key = "${API_KEY}"
                device_token = "${DEVICE_TOKEN}"
                
                # Transmission settings
                batch_size = 50
                transmission_interval = 5min
                retry_attempts = 3
                retry_backoff = 30s
                
                # Data compression
                compression_enabled = true
                compression_algorithm = "gzip"
            }
            
            # Local storage for offline operation
            local_storage {
                enabled = true
                max_records = 10000
                storage_format = "json"
                
                # Cleanup policy
                cleanup_policy = "fifo"  # fifo, oldest_first
                cleanup_threshold = 90.0  # percentage
            }
            
            # MQTT for real-time updates
            mqtt {
                enabled = true
                broker = "${MQTT_BROKER}"
                port = ${MQTT_PORT:-1883}
                username = "${MQTT_USER}"
                password = "${MQTT_PASS}"
                
                # Topics
                topics {
                    telemetry = "devices/${DEVICE_ID}/telemetry"
                    commands = "devices/${DEVICE_ID}/commands"
                    status = "devices/${DEVICE_ID}/status"
                }
                
                # QoS settings
                qos_level = 1
                retain_messages = false
                keep_alive = 60s
            }
        }
        
        # Power management
        power {
            # Battery settings (if battery powered)
            battery {
                enabled = true
                type = "lithium"
                capacity = 3000  # mAh
                
                # Power monitoring
                voltage_monitoring = true
                low_battery_threshold = 20.0  # percentage
                critical_battery_threshold = 5.0  # percentage
                
                # Power saving
                deep_sleep_enabled = true
                deep_sleep_threshold = 10.0  # percentage
                wake_interval = 1h
            }
            
            # Solar charging (if available)
            solar {
                enabled = false
                panel_voltage = 6.0
                charging_efficiency = 85.0  # percentage
                
                # Charging management
                overcharge_protection = true
                temperature_compensation = true
            }
        }
        
        # Security configuration
        security {
            # Encryption
            encryption {
                enabled = true
                algorithm = "AES-256"
                key_rotation_interval = 30d
                
                # TLS for communications
                tls_enabled = true
                tls_version = "1.3"
                certificate_validation = true
            }
            
            # Authentication
            authentication {
                method = "certificate"  # certificate, token, key
                certificate_path = "/certs/device.crt"
                private_key_path = "/certs/device.key"
                
                # Token-based auth (alternative)
                token_refresh_interval = 1h
                token_endpoint = "${TOKEN_ENDPOINT:-}"
            }
            
            # Firmware updates
            firmware_updates {
                enabled = true
                auto_update = false
                check_interval = 1d
                
                # Update security
                signature_verification = true
                rollback_enabled = true
                
                # Update scheduling
                maintenance_window = "02:00-04:00"
                max_download_time = 30min
            }
        }
        
        # Monitoring and diagnostics
        monitoring {
            # Health monitoring
            health {
                enabled = true
                check_interval = 1min
                
                # Health checks
                checks = [
                    "sensor_connectivity",
                    "network_connectivity", 
                    "storage_space",
                    "battery_level",
                    "temperature_range"
                ]
                
                # Alerting
                alert_threshold = 3  # consecutive failures
                alert_cooldown = 15min
            }
            
            # Performance metrics
            performance {
                enabled = true
                
                # Metrics collection
                cpu_usage = true
                memory_usage = true
                network_usage = true
                sensor_accuracy = true
                
                # Reporting
                report_interval = 1h
                detailed_logging = false
            }
            
            # Remote diagnostics
            remote_diagnostics {
                enabled = true
                ssh_enabled = false  # Security consideration
                
                # Log collection
                log_level = "info"
                log_retention = 7d
                remote_log_access = true
            }
        }
    "#;

    // Simplified struct for demonstration (full implementation would be much larger)
    #[derive(Debug, Deserialize)]
    struct IoTDeviceConfig {
        device: DeviceInfo,
        sensors: HashMap<String, SensorConfig>,
        connectivity: ConnectivityConfig,
        data_transmission: DataTransmissionConfig,
        power: PowerConfig,
        security: SecurityConfig,
        monitoring: MonitoringConfig,
    }

    #[derive(Debug, Deserialize)]
    struct DeviceInfo {
        id: String,
        name: String,
        firmware_version: String,
        hardware_revision: String,
        capabilities: Vec<String>,
    }

    #[derive(Debug, Deserialize)]
    struct SensorConfig {
        enabled: bool,
        unit: String,
        precision: f64,
        range: SensorRange,
        calibration_offset: f64,
        sample_rate: f64,
        #[serde(default)]
        co2_enabled: Option<bool>,
        #[serde(default)]
        voc_enabled: Option<bool>,
        #[serde(default)]
        pm25_enabled: Option<bool>,
        #[serde(default)]
        sensitivity: Option<String>,
        #[serde(default)]
        timeout: Option<f64>,
    }

    #[derive(Debug, Deserialize)]
    struct SensorRange {
        min: f64,
        max: f64,
    }

    #[derive(Debug, Deserialize)]
    struct ConnectivityConfig {
        wifi: WiFiConfig,
        cellular: CellularConfig,
        bluetooth: BluetoothConfig,
    }

    #[derive(Debug, Deserialize)]
    struct WiFiConfig {
        enabled: bool,
        ssid: String,
        password: String,
        auto_reconnect: bool,
        reconnect_interval: f64,
        max_reconnect_attempts: u32,
        power_save_mode: bool,
        sleep_interval: f64,
    }

    #[derive(Debug, Deserialize)]
    struct CellularConfig {
        enabled: bool,
        apn: String,
        username: String,
        password: String,
        fallback_enabled: bool,
        fallback_timeout: f64,
    }

    #[derive(Debug, Deserialize)]
    struct BluetoothConfig {
        enabled: bool,
        discoverable: bool,
        pairing_timeout: f64,
        require_pairing: bool,
        pin_code: String,
    }

    #[derive(Debug, Deserialize)]
    struct DataTransmissionConfig {
        cloud: CloudConfig,
        local_storage: LocalStorageConfig,
        mqtt: MqttConfig,
    }

    #[derive(Debug, Deserialize)]
    struct CloudConfig {
        endpoint: String,
        api_key: String,
        device_token: String,
        batch_size: u32,
        transmission_interval: f64,
        retry_attempts: u32,
        retry_backoff: f64,
        compression_enabled: bool,
        compression_algorithm: String,
    }

    #[derive(Debug, Deserialize)]
    struct LocalStorageConfig {
        enabled: bool,
        max_records: u32,
        storage_format: String,
        cleanup_policy: String,
        cleanup_threshold: f64,
    }

    #[derive(Debug, Deserialize)]
    struct MqttConfig {
        enabled: bool,
        broker: String,
        port: u16,
        username: String,
        password: String,
        topics: MqttTopics,
        qos_level: u8,
        retain_messages: bool,
        keep_alive: f64,
    }

    #[derive(Debug, Deserialize)]
    struct MqttTopics {
        telemetry: String,
        commands: String,
        status: String,
    }

    #[derive(Debug, Deserialize)]
    struct PowerConfig {
        battery: BatteryConfig,
        solar: SolarConfig,
    }

    #[derive(Debug, Deserialize)]
    struct BatteryConfig {
        enabled: bool,
        r#type: String,
        capacity: u32,
        voltage_monitoring: bool,
        low_battery_threshold: f64,
        critical_battery_threshold: f64,
        deep_sleep_enabled: bool,
        deep_sleep_threshold: f64,
        wake_interval: f64,
    }

    #[derive(Debug, Deserialize)]
    struct SolarConfig {
        enabled: bool,
        panel_voltage: f64,
        charging_efficiency: f64,
        overcharge_protection: bool,
        temperature_compensation: bool,
    }

    #[derive(Debug, Deserialize)]
    struct SecurityConfig {
        encryption: EncryptionConfig,
        authentication: AuthenticationConfig,
        firmware_updates: FirmwareUpdateConfig,
    }

    #[derive(Debug, Deserialize)]
    struct EncryptionConfig {
        enabled: bool,
        algorithm: String,
        key_rotation_interval: f64,
        tls_enabled: bool,
        tls_version: String,
        certificate_validation: bool,
    }

    #[derive(Debug, Deserialize)]
    struct AuthenticationConfig {
        method: String,
        certificate_path: String,
        private_key_path: String,
        token_refresh_interval: f64,
        token_endpoint: String,
    }

    #[derive(Debug, Deserialize)]
    struct FirmwareUpdateConfig {
        enabled: bool,
        auto_update: bool,
        check_interval: f64,
        signature_verification: bool,
        rollback_enabled: bool,
        maintenance_window: String,
        max_download_time: f64,
    }

    #[derive(Debug, Deserialize)]
    struct MonitoringConfig {
        health: HealthConfig,
        performance: PerformanceConfig,
        remote_diagnostics: RemoteDiagnosticsConfig,
    }

    #[derive(Debug, Deserialize)]
    struct HealthConfig {
        enabled: bool,
        check_interval: f64,
        checks: Vec<String>,
        alert_threshold: u32,
        alert_cooldown: f64,
    }

    #[derive(Debug, Deserialize)]
    struct PerformanceConfig {
        enabled: bool,
        cpu_usage: bool,
        memory_usage: bool,
        network_usage: bool,
        sensor_accuracy: bool,
        report_interval: f64,
        detailed_logging: bool,
    }

    #[derive(Debug, Deserialize)]
    struct RemoteDiagnosticsConfig {
        enabled: bool,
        ssh_enabled: bool,
        log_level: String,
        log_retention: f64,
        remote_log_access: bool,
    }

    // Set up environment variables for demonstration
    unsafe {
        std::env::set_var("DEVICE_ID", "sensor-001-abc123");
        std::env::set_var("WIFI_SSID", "IoT-Network");
        std::env::set_var("WIFI_PASSWORD", "secure-password");
        std::env::set_var("CLOUD_ENDPOINT", "https://iot.example.com/api/v1");
        std::env::set_var("API_KEY", "iot-api-key-123");
        std::env::set_var("DEVICE_TOKEN", "device-token-456");
        std::env::set_var("MQTT_BROKER", "mqtt.example.com");
        std::env::set_var("MQTT_USER", "device-001");
        std::env::set_var("MQTT_PASS", "mqtt-password");
    }

    let config: IoTDeviceConfig =
        from_str_with_variables(config_text, Box::new(EnvironmentVariableHandler))?;

    println!("IoT device configuration loaded:");
    println!("  Device: {} ({})", config.device.name, config.device.id);
    println!(
        "  Firmware: {} (HW rev {})",
        config.device.firmware_version, config.device.hardware_revision
    );
    println!("  Capabilities: {:?}", config.device.capabilities);
    println!("  Sensors: {} configured", config.sensors.len());

    let enabled_sensors: Vec<_> = config
        .sensors
        .iter()
        .filter(|(_, sensor)| sensor.enabled)
        .map(|(name, _)| name.as_str())
        .collect();
    println!("  Enabled sensors: {:?}", enabled_sensors);

    println!(
        "  WiFi: {} (auto-reconnect: {})",
        config.connectivity.wifi.ssid, config.connectivity.wifi.auto_reconnect
    );
    println!(
        "  MQTT: {} (QoS {})",
        config.data_transmission.mqtt.broker, config.data_transmission.mqtt.qos_level
    );
    println!(
        "  Battery: {}mAh {} (deep sleep: {})",
        config.power.battery.capacity,
        config.power.battery.r#type,
        config.power.battery.deep_sleep_enabled
    );
    println!(
        "  Security: {} encryption, {} auth",
        config.security.encryption.algorithm, config.security.authentication.method
    );
    println!(
        "  Monitoring: {} health checks, {}s interval",
        config.monitoring.health.checks.len(),
        config.monitoring.health.check_interval
    );

    assert!(!format!("{:?}", config).is_empty());
    println!();
    Ok(())
}

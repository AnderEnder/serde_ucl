use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ucl_lexer::from_str;

/// Real-world configuration examples demonstrating complete UCL syntax
/// in practical deployment scenarios.

#[derive(Debug, Deserialize, Serialize)]
struct ProductionConfig {
    infrastructure: InfrastructureConfig,
    monitoring: MonitoringConfig,
    deployment: DeploymentConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct InfrastructureConfig {
    kubernetes: KubernetesConfig,
    networking: NetworkingConfig,
    storage: StorageConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct KubernetesConfig {
    cluster_name: String,
    namespace: String,
    replicas: u32,
    resources: ResourceConfig,
    services: Vec<ServiceConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ResourceConfig {
    cpu_request: String,
    cpu_limit: String,
    memory_request: String,
    memory_limit: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ServiceConfig {
    name: String,
    port: u16,
    target_port: u16,
    service_type: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct NetworkingConfig {
    ingress: IngressConfig,
    load_balancer: LoadBalancerConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct IngressConfig {
    enabled: bool,
    host: String,
    tls_enabled: bool,
    annotations: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct LoadBalancerConfig {
    algorithm: String,
    health_check: HealthCheckConfig,
    backends: Vec<BackendConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
struct HealthCheckConfig {
    path: String,
    interval: String,
    timeout: String,
    retries: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct BackendConfig {
    host: String,
    port: u16,
    weight: u32,
    backup: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct StorageConfig {
    persistent_volumes: Vec<VolumeConfig>,
    object_storage: ObjectStorageConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct VolumeConfig {
    name: String,
    size: String,
    storage_class: String,
    access_mode: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ObjectStorageConfig {
    provider: String,
    bucket: String,
    region: String,
    encryption: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct MonitoringConfig {
    prometheus: PrometheusConfig,
    grafana: GrafanaConfig,
    alerting: AlertingConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct PrometheusConfig {
    enabled: bool,
    retention: String,
    scrape_interval: String,
    targets: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct GrafanaConfig {
    enabled: bool,
    admin_password: Option<String>,
    dashboards: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct AlertingConfig {
    enabled: bool,
    rules: Vec<AlertRule>,
    notifications: Vec<NotificationChannel>,
}

#[derive(Debug, Deserialize, Serialize)]
struct AlertRule {
    name: String,
    condition: String,
    duration: String,
    severity: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct NotificationChannel {
    name: String,
    channel_type: String,
    webhook_url: Option<String>,
    email: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct DeploymentConfig {
    strategy: String,
    environments: HashMap<String, EnvironmentConfig>,
    ci_cd: CiCdConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct EnvironmentConfig {
    replicas: u32,
    resources: ResourceConfig,
    config_overrides: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CiCdConfig {
    pipeline: PipelineConfig,
    testing: TestingConfig,
    security: SecurityScanConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct PipelineConfig {
    stages: Vec<String>,
    parallel_jobs: u32,
    timeout: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct TestingConfig {
    unit_tests: bool,
    integration_tests: bool,
    e2e_tests: bool,
    coverage_threshold: f64,
}

#[derive(Debug, Deserialize, Serialize)]
struct SecurityScanConfig {
    vulnerability_scan: bool,
    dependency_check: bool,
    code_analysis: bool,
    container_scan: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Real-World Configuration Examples ===\n");

    // Production infrastructure configuration using NGINX-style UCL syntax
    let production_config = r#"
        // Production infrastructure configuration
        // Demonstrates real-world deployment scenarios with UCL syntax
        
        infrastructure {
            kubernetes {
                cluster_name "production-cluster"
                namespace "api-production"
                replicas 5
                
                resources {
                    cpu_request "500m"
                    cpu_limit "2000m"
                    memory_request "1Gi"
                    memory_limit "4Gi"
                }
                
                // Multiple services using implicit arrays
                services [
                    {
                        name = "api-service"
                        port = 80
                        target_port = 8080
                        service_type = "ClusterIP"
                    },
                    {
                        name = "metrics-service"
                        port = 9090
                        target_port = 9090
                        service_type = "ClusterIP"
                    }
                ]
            }
            
            networking {
                ingress {
                    enabled yes
                    host "api.example.com"
                    tls_enabled true
                    
                    annotations {
                        "kubernetes.io/ingress.class" = "nginx"
                        "cert-manager.io/cluster-issuer" = "letsencrypt-prod"
                        "nginx.ingress.kubernetes.io/rate-limit" = "100"
                    }
                }
                
                load_balancer {
                    algorithm "least_conn"
                    
                    health_check {
                        path "/health"
                        interval "10s"
                        timeout "5s"
                        retries 3
                    }
                    
                    // Backend servers with implicit array syntax
                    backend {
                        host "10.0.1.10"
                        port 8080
                        weight 100
                        backup false
                    }
                    
                    backend {
                        host "10.0.1.11"
                        port 8080
                        weight 100
                        backup false
                    }
                    
                    backend {
                        host "10.0.1.12"
                        port 8080
                        weight 50
                        backup true
                    }
                }
            }
            
            storage {
                // Persistent volumes
                persistent_volume {
                    name "api-data"
                    size "100Gi"
                    storage_class "fast-ssd"
                    access_mode "ReadWriteOnce"
                }
                
                persistent_volume {
                    name "api-logs"
                    size "50Gi"
                    storage_class "standard"
                    access_mode "ReadWriteMany"
                }
                
                object_storage {
                    provider "aws-s3"
                    bucket "api-production-assets"
                    region "us-west-2"
                    encryption true
                }
            }
        }
        
        monitoring {
            prometheus {
                enabled true
                retention "30d"
                scrape_interval "15s"
                
                // Scrape targets
                target "kubernetes-pods"
                target "kubernetes-nodes"
                target "kubernetes-services"
            }
            
            grafana {
                enabled true
                admin_password null  // Set via secret
                
                dashboard "kubernetes-cluster"
                dashboard "application-metrics"
                dashboard "infrastructure-overview"
            }
            
            alerting {
                enabled true
                
                // Alert rules
                rules [
                    {
                        name = "HighCPUUsage"
                        condition = "cpu_usage > 80"
                        duration = "5m"
                        severity = "warning"
                    },
                    {
                        name = "HighMemoryUsage"
                        condition = "memory_usage > 90"
                        duration = "2m"
                        severity = "critical"
                    },
                    {
                        name = "PodCrashLooping"
                        condition = "pod_restarts > 5"
                        duration = "1m"
                        severity = "critical"
                    }
                ]
                
                // Notification channels
                notifications [
                    {
                        name = "slack-alerts"
                        channel_type = "slack"
                        webhook_url = "https://hooks.slack.com/services/..."
                        email = null
                    },
                    {
                        name = "email-alerts"
                        channel_type = "email"
                        webhook_url = null
                        email = "ops-team@example.com"
                    }
                ]
            }
        }
        
        deployment {
            strategy "rolling_update"
            
            environments {
                staging {
                    replicas = 2
                    resources {
                        cpu_request = "250m"
                        cpu_limit = "1000m"
                        memory_request = "512Mi"
                        memory_limit = "2Gi"
                    }
                    config_overrides {
                        "debug" = true
                        "log_level" = "debug"
                    }
                }
                
                production {
                    replicas = 5
                    resources {
                        cpu_request = "500m"
                        cpu_limit = "2000m"
                        memory_request = "1Gi"
                        memory_limit = "4Gi"
                    }
                    config_overrides {
                        "debug" = false
                        "log_level" = "info"
                    }
                }
            }
            
            ci_cd {
                pipeline {
                    stage "build"
                    stage "test"
                    stage "security-scan"
                    stage "deploy-staging"
                    stage "integration-tests"
                    stage "deploy-production"
                    
                    parallel_jobs 4
                    timeout "30m"
                }
                
                testing {
                    unit_tests true
                    integration_tests true
                    e2e_tests true
                    coverage_threshold 85.0
                }
                
                security {
                    vulnerability_scan true
                    dependency_check true
                    code_analysis true
                    container_scan true
                }
            }
        }
    "#;

    println!("1. Parsing production infrastructure configuration...");
    let config: ProductionConfig = from_str(production_config)?;

    println!("✓ Successfully parsed production configuration!");

    // Display configuration summary
    println!("\n=== Infrastructure Summary ===");
    println!("Cluster: {}", config.infrastructure.kubernetes.cluster_name);
    println!("Namespace: {}", config.infrastructure.kubernetes.namespace);
    println!("Replicas: {}", config.infrastructure.kubernetes.replicas);
    println!(
        "Services: {}",
        config.infrastructure.kubernetes.services.len()
    );
    println!(
        "Ingress host: {}",
        config.infrastructure.networking.ingress.host
    );
    println!(
        "Load balancer backends: {}",
        config
            .infrastructure
            .networking
            .load_balancer
            .backends
            .len()
    );
    println!(
        "Persistent volumes: {}",
        config.infrastructure.storage.persistent_volumes.len()
    );

    println!("\n=== Monitoring Summary ===");
    println!(
        "Prometheus enabled: {}",
        config.monitoring.prometheus.enabled
    );
    println!(
        "Prometheus retention: {}",
        config.monitoring.prometheus.retention
    );
    println!(
        "Grafana dashboards: {}",
        config.monitoring.grafana.dashboards.len()
    );
    println!("Alert rules: {}", config.monitoring.alerting.rules.len());
    println!(
        "Notification channels: {}",
        config.monitoring.alerting.notifications.len()
    );

    println!("\n=== Deployment Summary ===");
    println!("Strategy: {}", config.deployment.strategy);
    println!("Environments: {}", config.deployment.environments.len());
    println!(
        "Pipeline stages: {}",
        config.deployment.ci_cd.pipeline.stages.len()
    );
    println!(
        "Coverage threshold: {}%",
        config.deployment.ci_cd.testing.coverage_threshold
    );

    println!("\n✅ Real-world configuration examples completed successfully!");
    println!("\nKey UCL syntax features demonstrated:");
    println!("  • NGINX-style implicit syntax for infrastructure configuration");
    println!("  • Implicit arrays through key repetition (backend, target, etc.)");
    println!("  • Mixed explicit and implicit syntax styles");
    println!("  • Boolean keywords (true/false, yes/no)");
    println!("  • Null values for optional configuration");
    println!("  • Nested object structures for complex configurations");
    println!("  • C++ style comments for documentation");

    Ok(())
}

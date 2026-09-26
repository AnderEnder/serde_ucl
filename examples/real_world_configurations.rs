//! A production infrastructure configuration assembled from several files.
//!
//! Real deployments rarely keep their configuration in one file. This example builds the
//! effective configuration of a service cluster from a shared `base.conf`, a `production.conf`
//! overlay and an optional `local.conf`, then maps it onto typed structs:
//!
//! - `.include "file"` reads another file into the current object; `.include(try=true)` skips a
//!   file that does not exist;
//! - `.include(duplicate="merge", priority=5)` merges the overlay's objects into the base's, and
//!   its higher priority makes its scalars replace the base's (spec §8.3, §8.4, §9.4);
//! - `.inherit "defaults"` copies the entries of a root object into an environment, whose own
//!   keys then replace the copies (§9.7);
//! - repeated `backend { }` blocks and repeated `target` keys become lists; `alert [ { }, { } ]`
//!   is an explicit array of objects;
//! - the files come from a `MemoryLoader`, as for an application that embeds its configuration
//!   or a test; `FsLoader` reads the same layout from disk, and a default parser reads no files;
//! - the effective configuration is written back with `to_string` for an audit snapshot, which
//!   reads back as the same value.
//!
//! It also shows the one include form that surprises: `.try_include` of a missing file ends the
//! whole parse early, which the crate reports as `UclError::Stopped`.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_ucl::emit::{Emitter, Format};
use serde_ucl::parse::{ErrorKind, MemoryLoader, ParserBuilder};
use serde_ucl::{UclError, from_str, from_value, to_string};
use std::time::Duration;

const MAIN_CONF: &str = r#"
# Entry point: shared settings, the production overlay, then an optional local file.
.include "base.conf"
.include(duplicate="merge", priority=5) "production.conf"
.include(try=true) "local.conf"
"#;

const BASE_CONF: &str = r#"
# Settings shared by every environment.
cluster {
    name staging-cluster;
    namespace api;
    replicas 2;
    # Kubernetes quantities stay strings: unquoted, 500m would be the number 500000000.
    resources {
        cpu_request "250m";
        cpu_limit "1";
        memory_request "512Mi";
        memory_limit "1Gi";
    }
}

ingress {
    host api.staging.example.com;
    tls false;
    annotation {
        "kubernetes.io/ingress.class" nginx;
        "nginx.ingress.kubernetes.io/limit-rps" "100";
    }
}

load_balancer {
    algorithm least_conn;
    health_check { path /health; interval 10s; timeout 5s; retries 3; }
    backend { host 10.0.1.10; port 8080; weight 100; }
    backend { host 10.0.1.11; port 8080; weight 100; }
    backend { host 10.0.1.12; port 8080; weight 50; backup true; }
}

monitoring {
    retention 15d;
    scrape_interval 30s;
    target kubernetes-pods;
    target kubernetes-nodes;
    alert [
        { name = HighCpu, condition = "cpu_usage > 80", duration = 5min, severity = warning },
        { name = PodCrashLooping, condition = "restarts > 5", duration = 1min, severity = critical },
    ]
    pager = null;
}

# Defaults that each environment inherits; its own keys win.
defaults {
    replicas 1;
    log_level info;
    debug false;
}
environments {
    staging { .inherit "defaults"; replicas 2; debug true; }
    production { .inherit "defaults"; replicas 6; }
}
"#;

const PRODUCTION_CONF: &str = r#"
# Production overrides. Merged into what base.conf set, at a higher priority.
cluster {
    name production-cluster;
    replicas 6;
    resources { cpu_request "500m"; cpu_limit "2"; memory_request "1Gi"; memory_limit "4Gi"; }
}
ingress { host api.example.com; tls true; }
monitoring { retention 30d; pager "https://events.pagerduty.example/v2"; }
environments { production { log_level warn; } }
"#;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Infrastructure {
    cluster: Cluster,
    ingress: Ingress,
    load_balancer: LoadBalancer,
    monitoring: Monitoring,
    environments: IndexMap<String, Environment>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Cluster {
    name: String,
    namespace: String,
    replicas: u32,
    resources: Resources,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Resources {
    cpu_request: String,
    cpu_limit: String,
    memory_request: String,
    memory_limit: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Ingress {
    host: String,
    tls: bool,
    #[serde(rename = "annotation")]
    annotations: IndexMap<String, String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct LoadBalancer {
    algorithm: String,
    health_check: HealthCheck,
    #[serde(rename = "backend")]
    backends: Vec<Backend>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct HealthCheck {
    path: String,
    #[serde(with = "serde_ucl::time")]
    interval: Duration,
    #[serde(with = "serde_ucl::time")]
    timeout: Duration,
    retries: u32,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Backend {
    host: String,
    port: u16,
    weight: u32,
    #[serde(default)]
    backup: bool,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Monitoring {
    #[serde(with = "serde_ucl::time")]
    retention: Duration,
    #[serde(with = "serde_ucl::time")]
    scrape_interval: Duration,
    #[serde(rename = "target")]
    targets: Vec<String>,
    #[serde(rename = "alert")]
    alerts: Vec<Alert>,
    pager: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Alert {
    name: String,
    condition: String,
    #[serde(with = "serde_ucl::time")]
    duration: Duration,
    severity: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Environment {
    replicas: u32,
    log_level: String,
    debug: bool,
}

/// The configuration files, under `/etc/infra`.
fn loader() -> MemoryLoader {
    let mut loader = MemoryLoader::new();
    loader.add_file("/etc/infra/main.conf", MAIN_CONF);
    loader.add_file("/etc/infra/base.conf", BASE_CONF);
    loader.add_file("/etc/infra/production.conf", PRODUCTION_CONF);
    loader
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Production configuration from several files ===\n");

    let mut parser = ParserBuilder::new()
        .with_loader(loader())
        .with_base_dir("/etc/infra")
        .build();
    // Relative include paths resolve against the base directory, never against the including
    // file (spec §9.3).
    let value = parser.parse_file("main.conf")?;

    // Priorities stay in the tree: the entries the overlay replaced carry its priority 5.
    let root = value.as_object().unwrap();
    let cluster = root["cluster"].as_object().unwrap();
    for (key, entry) in cluster.iter() {
        let source = match entry.slots()[0].priority() {
            5 => "production.conf",
            _ => "base.conf",
        };
        println!("cluster.{key:<10} from {source}");
    }

    let infra: Infrastructure = from_value(value.clone())?;

    let c = &infra.cluster;
    println!(
        "\ncluster {} in namespace {}: {} replicas, cpu {}..{}, memory {}..{}",
        c.name,
        c.namespace,
        c.replicas,
        c.resources.cpu_request,
        c.resources.cpu_limit,
        c.resources.memory_request,
        c.resources.memory_limit
    );
    assert_eq!(c.name, "production-cluster");
    assert_eq!(c.namespace, "api"); // kept from base.conf by the merge
    assert_eq!(c.replicas, 6);
    assert_eq!(c.resources.cpu_request, "500m");

    println!(
        "ingress {} (tls {}), annotations {:?}",
        infra.ingress.host, infra.ingress.tls, infra.ingress.annotations
    );
    assert!(infra.ingress.tls);
    assert_eq!(
        infra.ingress.annotations["kubernetes.io/ingress.class"],
        "nginx"
    );

    let lb = &infra.load_balancer;
    println!(
        "load balancer {}: health {} every {:?}",
        lb.algorithm, lb.health_check.path, lb.health_check.interval
    );
    for b in &lb.backends {
        println!(
            "  backend {}:{} weight {}{}",
            b.host,
            b.port,
            b.weight,
            if b.backup { " (backup)" } else { "" }
        );
    }
    assert_eq!(lb.backends.len(), 3);
    assert!(lb.backends[2].backup);

    let m = &infra.monitoring;
    println!(
        "monitoring: keep {} days, scrape every {:?}, targets {:?}, pager {:?}",
        m.retention.as_secs() / 86_400,
        m.scrape_interval,
        m.targets,
        m.pager
    );
    for a in &m.alerts {
        println!(
            "  alert {} when {} for {:?} ({})",
            a.name, a.condition, a.duration, a.severity
        );
    }
    assert_eq!(m.retention, Duration::from_secs(30 * 86_400));
    assert_eq!(m.alerts[0].duration, Duration::from_secs(300));
    assert!(m.pager.is_some());

    for (name, env) in &infra.environments {
        println!(
            "environment {name}: {} replicas, log level {}, debug {}",
            env.replicas, env.log_level, env.debug
        );
    }
    let staging = &infra.environments["staging"];
    assert_eq!((staging.replicas, staging.debug), (2, true));
    assert_eq!(staging.log_level, "info"); // inherited from `defaults`
    assert_eq!(infra.environments["production"].log_level, "warn");

    // libucl's own JSON of a subtree, for tools that want JSON.
    let environments = Emitter::new(Format::JsonCompact).emit(&root["environments"]);
    println!("\nenvironments as JSON: {environments}");

    // An audit snapshot of the effective configuration, which reads back as the same value.
    let snapshot = to_string(&infra)?;
    println!(
        "\nsnapshot ({} lines), first lines:",
        snapshot.lines().count()
    );
    for line in snapshot.lines().take(6) {
        println!("  {line}");
    }
    let reread: Infrastructure = from_str(&snapshot)?;
    assert_eq!(reread, infra);

    // A required include that is missing is an error with a position.
    let mut parser = ParserBuilder::new()
        .with_loader(loader())
        .with_base_dir("/etc/infra")
        .build();
    let err = UclError::from(
        parser
            .parse(b"a = 1\n.include \"secrets.conf\"\n")
            .unwrap_err(),
    );
    let parse_error = err.parse_error().expect("a parse error");
    assert!(matches!(parse_error.kind(), ErrorKind::FileNotFound { .. }));
    println!(
        "\nmissing include: {err} (line {})",
        err.position().unwrap().line
    );

    // `.try_include` of a missing file ends the whole parse there, keeping what came before:
    // `b` is lost. Use `.include(try=true)` to skip an optional file instead.
    let err = UclError::from(
        parser
            .parse(b"a = 1\n.try_include \"local.conf\"\nb = 2\n")
            .unwrap_err(),
    );
    let UclError::Stopped(stop) = &err else {
        panic!("expected a silent stop, got {err}");
    };
    let partial = stop.partial().unwrap().as_object().unwrap();
    println!(
        ".try_include stopped the parse; kept keys {:?}",
        partial.keys().collect::<Vec<_>>()
    );
    assert!(partial.contains_key("a") && !partial.contains_key("b"));

    println!("\nThe effective configuration is assembled and checked.");
    Ok(())
}

//! Documents and types shared by the benchmarks. Every generated document is valid libucl.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write;

/// A configuration in the style UCL is written in: a braced section per service under
/// `services`, nginx-style `key value` entries, a repeated key (an implicit array), arrays,
/// numbers with multipliers, times, the three string forms, and comments, nested block comments
/// among them. Each service adds about 600 bytes.
pub fn config(services: usize) -> String {
    let mut out = String::from(
        "# generated benchmark configuration\n\
         global {\n    workers = 8;\n    pid_file = \"/var/run/app.pid\";\n    log_level = info;\n}\n\n\
         services {\n",
    );
    for i in 0..services {
        write!(
            out,
            r#"    svc{i} {{
        enabled = {enabled}
        listen = "0.0.0.0:{port}"
        upstream = backend-{i}-a.example   # repeated: two values
        upstream = backend-{i}-b.example
        timeout = {timeout}s
        max_body = {mb}mb
        ratio = 0.{i}5
        tags = [web, "api", 'internal']
        /* a block comment
           /* with a nested one */ */
        headers {{
            X-Service = "svc{i}";
            Cache-Control "no-store";
        }}
        script = <<EOD
#!/bin/sh
echo "service {i}"
EOD
    }}
"#,
            enabled = i % 3 != 0,
            port = 8000 + i % 1000,
            timeout = 5 + i % 30,
            mb = 1 + i % 64,
        )
        .unwrap();
    }
    out.push_str("}\n");
    out
}

/// A JSON document: an array of `items` records.
pub fn json(items: usize) -> String {
    let mut out = String::from("{\"items\": [\n");
    for i in 0..items {
        if i > 0 {
            out.push_str(",\n");
        }
        write!(
            out,
            r#"  {{"id": {i}, "name": "item-{i}", "price": {price}.25, "active": {active}, "tags": ["a", "b", "c"], "dims": {{"w": {i}, "h": 2, "d": 3.5}}, "note": null}}"#,
            price = i % 500,
            active = i % 2 == 0,
        )
        .unwrap();
    }
    out.push_str("\n]}\n");
    out
}

/// Objects nested `depth` deep, one key each, with a value at the bottom.
pub fn nested(depth: usize) -> String {
    let mut out = String::new();
    for i in 0..depth {
        write!(out, "l{i} {{ ").unwrap();
    }
    out.push_str("leaf = 1;");
    for _ in 0..depth {
        out.push_str(" }");
    }
    out.push('\n');
    out
}

/// The variables that [`variables`] refers to.
pub const VARIABLES: [(&str, &str); 4] = [
    ("HOST", "example.org"),
    ("PORT", "8443"),
    ("ROOT", "/srv/app"),
    ("USER", "deploy"),
];

/// `entries` string values with braced and unbraced variable references (spec §7).
pub fn variables(entries: usize) -> String {
    let mut out = String::new();
    for i in 0..entries {
        writeln!(
            out,
            "url{i} = \"https://${{HOST}}:$PORT/{i}\"; path{i} = \"$ROOT/data/{i}\"; owner{i} = \"${{USER}}@$HOST\";"
        )
        .unwrap();
    }
    out
}

/// The typed form of [`config`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub global: Global,
    pub services: BTreeMap<String, Service>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Global {
    pub workers: u32,
    pub pid_file: String,
    pub log_level: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Service {
    pub enabled: bool,
    pub listen: String,
    pub upstream: Vec<String>,
    pub timeout: f64,
    pub max_body: u64,
    pub ratio: f64,
    pub tags: Vec<String>,
    pub headers: BTreeMap<String, String>,
    pub script: String,
}

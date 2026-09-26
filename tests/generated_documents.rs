//! Generated documents: many entries, long strings, many numbers and deep nesting parse to the
//! values they spell out. Every generated input gives the same value in libucl (checked with the
//! oracle in clean-room work item C5).
//!
//! These were performance tests; their wall-clock assertions were removed in C5. `benches/`
//! measures throughput.

use serde_json::{Map, Value, json};
use serde_ucl::{UclValue, from_str};

#[test]
fn test_small_config() {
    let small_config = r#"
        app = "test"
        port = 8080
        debug = true
        
        server {
            host = "localhost"
            timeout = 30s
        }
        
        features = ["auth", "logging"]
    "#;

    let parsed: Value = from_str(small_config).unwrap();
    assert_eq!(
        parsed,
        json!({
            "app": "test",
            "port": 8080,
            "debug": true,
            "server": { "host": "localhost", "timeout": 30.0 },
            "features": ["auth", "logging"],
        })
    );
}

#[test]
fn test_generated_configs_of_several_sizes() {
    for items in [50, 100, 200, 500, 1000] {
        let config = generate_config(items);
        let parsed: Value = from_str(&config).unwrap();
        let root = parsed.as_object().unwrap();
        assert_eq!(root.len(), items, "{items} items");
        for i in 0..items {
            let item = &root[&format!("item_{i}")];
            // `{n}s` is a time, read as seconds; `{n}mb` an integer times 2^20 (spec §5.4).
            let expected = json!({
                "id": i,
                "name": format!("Item {i}"),
                "enabled": i % 2 == 0,
                "timeout": (10 + (i % 50)) as f64,
                "memory": (64 + (i % 256)) << 20,
                "tags": [format!("tag-{}", i % 10), format!("category-{}", i % 5)],
                "metadata": {
                    "created": 1600000000 + i,
                    "version": format!("1.{}.0", i % 100),
                },
            });
            assert_eq!(item, &expected, "item_{i} of {items}");
        }
    }
}

#[test]
fn test_string_heavy_config() {
    for items in [1000, 2000] {
        let config = generate_string_heavy_config(items);
        let parsed: Value = from_str(&config).unwrap();
        let root = parsed.as_object().unwrap();
        assert_eq!(root.len(), items);
        for i in 0..items {
            let entry = &root[&format!("string_{i}")];
            assert_eq!(entry["short_string"], format!("value_{i}"));
            assert_eq!(
                entry["medium_string"],
                format!(
                    "This is a medium length string for item {i} with some additional content \
                     to make it longer"
                )
            );
            assert!(
                entry["long_string"]
                    .as_str()
                    .unwrap()
                    .starts_with(&format!("This is a very long string for item {i} that "))
            );
            assert_eq!(
                entry["path_string"],
                format!("/very/long/path/to/some/file/or/directory/structure/item_{i}/config.json")
            );
            assert_eq!(
                entry["url_string"],
                format!(
                    "https://api.example.com/v1/items/{i}/details?param1=value1&param2=value2\
                     &param3=value3"
                )
            );
        }
    }
}

#[test]
fn test_number_heavy_config() {
    let items = 500;
    let config = generate_number_heavy_config(items);
    let value = serde_ucl::parse::parse(config.as_bytes()).unwrap();
    let root = value.as_object().unwrap();
    assert_eq!(root.len(), items);
    for i in 0..items {
        let entry = root[format!("numbers_{i}").as_str()].as_object().unwrap();
        let exp = (i % 10) as i32 - 5;
        let i64_ = |n: usize| UclValue::Integer(n as i64);
        // spec §5.1 (decimal integers and floats; `-0` is `int 0`), §5.2 (hex), §5.4 (suffixes).
        assert_eq!(entry["integer"], i64_(i));
        assert_eq!(
            entry["float"],
            UclValue::Float(format!("{i}.{}", i % 1000).parse().unwrap())
        );
        assert_eq!(entry["negative"], UclValue::Integer(-((i * 2) as i64)));
        assert_eq!(entry["large"], i64_(i * 1000000));
        assert_eq!(entry["memory_mb"], i64_((256 + (i % 768)) << 20));
        assert_eq!(entry["size_kb"], i64_((64 + (i % 192)) << 10));
        assert_eq!(entry["duration_s"], UclValue::Time((30 + (i % 120)) as f64));
        assert_eq!(entry["hex"], i64_(i));
        assert_eq!(
            entry["scientific"],
            UclValue::Float(format!("{i}e{exp}").parse().unwrap())
        );
    }
}

#[test]
fn test_deeply_nested_config() {
    fn expected(current_depth: usize, max_depth: usize, width: usize) -> Value {
        if current_depth >= max_depth {
            return json!("leaf_value");
        }
        let mut level = Map::new();
        for i in 0..width {
            level.insert(
                format!("level_{current_depth}_{i}"),
                expected(current_depth + 1, max_depth, width),
            );
        }
        Value::Object(level)
    }

    let deeply_nested = generate_deeply_nested_config(10, 3);
    let parsed: Value = from_str(&deeply_nested).unwrap();
    assert_eq!(parsed, json!({ "root": expected(0, 10, 3) }));
}

// Helper functions to generate test configurations

fn generate_config(items: usize) -> String {
    let mut config = String::new();

    for i in 0..items {
        config.push_str(&format!(
            r#"item_{} {{
    id = {}
    name = "Item {}"
    enabled = {}
    timeout = {}s
    memory = {}mb
    tags = ["tag-{}", "category-{}"]
    metadata {{
      created = {}
      version = "1.{}.0"
    }}
  }}
"#,
            i,
            i,
            i,
            if i % 2 == 0 { "true" } else { "false" },
            10 + (i % 50),
            64 + (i % 256),
            i % 10,
            i % 5,
            1600000000 + i,
            i % 100
        ));
    }

    config
}

fn generate_string_heavy_config(items: usize) -> String {
    let mut config = String::new();

    for i in 0..items {
        config.push_str(&format!(
            r#"string_{i} {{
    short_string = "value_{i}"
    medium_string = "This is a medium length string for item {i} with some additional content to make it longer"
    long_string = "This is a very long string for item {i} that contains a lot of text and should test the string parsing performance of the UCL lexer. It includes various characters and should be representative of real-world string content that might appear in configuration files."
    path_string = "/very/long/path/to/some/file/or/directory/structure/item_{i}/config.json"
    url_string = "https://api.example.com/v1/items/{i}/details?param1=value1&param2=value2&param3=value3"
  }}
"#
        ));
    }

    config
}

fn generate_number_heavy_config(items: usize) -> String {
    let mut config = String::new();

    for i in 0..items {
        let exp = (i % 10) as i32 - 5;
        config.push_str(&format!(
            r#"numbers_{} {{
    integer = {}
    float = {}.{}
    negative = -{}
    large = {}
    memory_mb = {}mb
    size_kb = {}kb
    duration_s = {}s
    hex = 0x{:X}
    scientific = {}e{}
  }}
"#,
            i,
            i,
            i,
            i % 1000,
            i * 2,
            i * 1000000,
            256 + (i % 768),
            64 + (i % 192),
            30 + (i % 120),
            i,
            i,
            exp
        ));
    }

    config
}

fn generate_deeply_nested_config(depth: usize, width: usize) -> String {
    fn generate_level(current_depth: usize, max_depth: usize, width: usize) -> String {
        if current_depth >= max_depth {
            return r#""leaf_value""#.to_string();
        }

        let mut level = String::from("{\n");

        for i in 0..width {
            level.push_str(&format!(
                r#"    level_{}_{} = {}
"#,
                current_depth,
                i,
                generate_level(current_depth + 1, max_depth, width)
            ));
        }

        level.push_str("  }");
        level
    }

    format!("root = {}\n", generate_level(0, depth, width))
}

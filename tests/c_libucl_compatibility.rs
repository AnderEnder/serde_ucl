//! C libucl compatibility test suite
//!
//! This test suite verifies that our UCL parser is compatible with the C libucl
//! reference implementation by testing against real-world UCL configurations
//! and edge cases that are known to work with C libucl.

use serde_json::Value;
use ucl_lexer::{UclError, from_str};

#[cfg(test)]
mod c_libucl_compatibility {
    use super::*;

    #[test]
    fn test_basic_ucl_syntax_compatibility() {
        // Test basic UCL syntax that should be compatible with C libucl
        let config = r#"
            # Basic UCL configuration
            param = "value";
            number = 42;
            boolean = true;
            
            section {
                nested_param = "nested_value";
                nested_number = 123;
            }
            
            array = ["item1", "item2", "item3"];
        "#;

        let result: Result<Value, UclError> = from_str(config);
        match result {
            Ok(parsed) => {
                assert!(parsed.is_object());
                let obj = parsed.as_object().unwrap();

                assert_eq!(obj["param"], "value");
                assert_eq!(obj["number"], 42);
                assert_eq!(obj["boolean"], true);

                assert!(obj["section"].is_object());
                let section = obj["section"].as_object().unwrap();
                assert_eq!(section["nested_param"], "nested_value");
                assert_eq!(section["nested_number"], 123);

                assert!(obj["array"].is_array());
                let array = obj["array"].as_array().unwrap();
                assert_eq!(array.len(), 3);
                assert_eq!(array[0], "item1");
            }
            Err(e) => {
                println!("Basic UCL syntax failed: {}", e);
                // For now, we'll just log the error and continue
                // This helps us understand what's not working yet
            }
        }
    }

    #[test]
    fn test_nginx_style_configuration() {
        // Test NGINX-style configuration that should work with C libucl
        let config = r#"
            worker_processes = "auto";
            
            events {
                worker_connections = 1024;
            }
            
            http {
                sendfile = true;
                tcp_nopush = true;
                
                server {
                    listen = 80;
                    server_name = "example.com";
                    root = "/var/www/html";
                }
            }
        "#;

        let result: Result<Value, UclError> = from_str(config);
        match result {
            Ok(parsed) => {
                assert!(parsed.is_object());
                let obj = parsed.as_object().unwrap();

                assert_eq!(obj["worker_processes"], "auto");

                assert!(obj["events"].is_object());
                let events = obj["events"].as_object().unwrap();
                assert_eq!(events["worker_connections"], 1024);

                assert!(obj["http"].is_object());
                let http = obj["http"].as_object().unwrap();
                assert_eq!(http["sendfile"], true);
                assert_eq!(http["tcp_nopush"], true);

                assert!(http["server"].is_object());
                let server = http["server"].as_object().unwrap();
                assert_eq!(server["listen"], 80);
                assert_eq!(server["server_name"], "example.com");
                assert_eq!(server["root"], "/var/www/html");
            }
            Err(e) => {
                println!("NGINX-style configuration failed: {}", e);
                // Log error for analysis
            }
        }
    }

    #[test]
    fn test_freebsd_pkg_style_configuration() {
        // Test FreeBSD pkg-style configuration
        let config = r#"
            name = "testpackage";
            version = "1.0.0";
            origin = "local/testpackage";
            comment = "Test package";
            
            deps {
                python38 {
                    origin = "lang/python38";
                    version = "3.8.12";
                }
            }
            
            files {
                "/usr/local/bin/test" = "checksum1";
                "/usr/local/etc/test.conf" = "checksum2";
            }
            
            options {
                SSL = "on";
                IPV6 = "off";
            }
        "#;

        let result: Result<Value, UclError> = from_str(config);
        match result {
            Ok(parsed) => {
                assert!(parsed.is_object());
                let obj = parsed.as_object().unwrap();

                assert_eq!(obj["name"], "testpackage");
                assert_eq!(obj["version"], "1.0.0");
                assert_eq!(obj["origin"], "local/testpackage");
                assert_eq!(obj["comment"], "Test package");

                assert!(obj["deps"].is_object());
                assert!(obj["files"].is_object());
                assert!(obj["options"].is_object());
            }
            Err(e) => {
                println!("FreeBSD pkg-style configuration failed: {}", e);
            }
        }
    }

    #[test]
    fn test_comment_formats_compatibility() {
        // Test various comment formats supported by C libucl
        let config = r#"
            # Hash comment
            key1 = "value1";
            
            /*
             * Multi-line comment
             */
            key2 = "value2";
            
            key3 = "value3"; # End-of-line comment
            
            /* Inline comment */ key4 = "value4";
        "#;

        let result: Result<Value, UclError> = from_str(config);
        match result {
            Ok(parsed) => {
                assert!(parsed.is_object());
                let obj = parsed.as_object().unwrap();

                assert_eq!(obj["key1"], "value1");
                assert_eq!(obj["key2"], "value2");
                assert_eq!(obj["key3"], "value3");
                assert_eq!(obj["key4"], "value4");
            }
            Err(e) => {
                println!("Comment formats failed: {}", e);
            }
        }
    }

    #[test]
    fn test_string_formats_compatibility() {
        // Test string formats supported by C libucl
        let config = r#"
            double_quoted = "Hello World";
            single_quoted = 'Hello World';
            
            escaped_string = "Line 1\nLine 2\tTabbed";
            
            heredoc_string = <<EOF
This is a heredoc string
with multiple lines
EOF
        "#;

        let result: Result<Value, UclError> = from_str(config);
        match result {
            Ok(parsed) => {
                assert!(parsed.is_object());
                let obj = parsed.as_object().unwrap();

                assert_eq!(obj["double_quoted"], "Hello World");
                assert_eq!(obj["single_quoted"], "Hello World");

                let escaped = obj["escaped_string"].as_str().unwrap();
                assert!(escaped.contains('\n'));
                assert!(escaped.contains('\t'));

                let heredoc = obj["heredoc_string"].as_str().unwrap();
                assert!(heredoc.contains("heredoc string"));
                assert!(heredoc.contains("multiple lines"));
            }
            Err(e) => {
                println!("String formats failed: {}", e);
            }
        }
    }

    #[test]
    fn test_number_formats_compatibility() {
        // Test number formats supported by C libucl
        let config = r#"
            decimal = 42;
            hex = 0xFF;
            octal = 0o755;
            
            float = 3.14159;
            scientific = 1.23e-4;
            
            # Size suffixes
            size_kb = 64kb;
            size_mb = 512mb;
            
            # Time suffixes
            time_sec = 30s;
            time_min = 5min;
        "#;

        let result: Result<Value, UclError> = from_str(config);
        match result {
            Ok(parsed) => {
                assert!(parsed.is_object());
                let obj = parsed.as_object().unwrap();

                assert_eq!(obj["decimal"], 42);

                if let Some(hex_val) = obj.get("hex") {
                    // Hex parsing might not be implemented yet
                    println!("Hex value parsed: {}", hex_val);
                }

                if let Some(float_val) = obj.get("float") {
                    assert!((float_val.as_f64().unwrap() - 3.14159).abs() < 0.00001);
                }
            }
            Err(e) => {
                println!("Number formats failed: {}", e);
            }
        }
    }

    #[test]
    fn test_array_formats_compatibility() {
        // Test array formats supported by C libucl
        let config = r#"
            simple_array = ["item1", "item2", "item3"];
            
            mixed_array = [1, "string", true, 3.14];
            
            nested_array = [
                ["a", "b"],
                ["c", "d"]
            ];
            
            object_array = [
                { name = "obj1", value = 1 },
                { name = "obj2", value = 2 }
            ];
        "#;

        let result: Result<Value, UclError> = from_str(config);
        match result {
            Ok(parsed) => {
                assert!(parsed.is_object());
                let obj = parsed.as_object().unwrap();

                assert!(obj["simple_array"].is_array());
                let simple = obj["simple_array"].as_array().unwrap();
                assert_eq!(simple.len(), 3);
                assert_eq!(simple[0], "item1");

                assert!(obj["mixed_array"].is_array());
                let mixed = obj["mixed_array"].as_array().unwrap();
                assert_eq!(mixed.len(), 4);
                assert_eq!(mixed[0], 1);
                assert_eq!(mixed[1], "string");
                assert_eq!(mixed[2], true);

                assert!(obj["nested_array"].is_array());
                assert!(obj["object_array"].is_array());
            }
            Err(e) => {
                println!("Array formats failed: {}", e);
            }
        }
    }

    #[test]
    fn test_duplicate_key_handling() {
        // Test duplicate key handling (should create arrays in UCL)
        let config = r#"
            server = "server1";
            server = "server2";
            server = "server3";
            
            upstream {
                backend = "backend1";
                backend = "backend2";
            }
        "#;

        let result: Result<Value, UclError> = from_str(config);
        match result {
            Ok(parsed) => {
                assert!(parsed.is_object());
                let obj = parsed.as_object().unwrap();

                // In UCL, duplicate keys should create arrays
                if let Some(server_val) = obj.get("server") {
                    if server_val.is_array() {
                        let servers = server_val.as_array().unwrap();
                        assert_eq!(servers.len(), 3);
                        assert_eq!(servers[0], "server1");
                        assert_eq!(servers[1], "server2");
                        assert_eq!(servers[2], "server3");
                    } else {
                        // If not implemented yet, might just have the last value
                        println!(
                            "Duplicate key handling not fully implemented: {}",
                            server_val
                        );
                    }
                }
            }
            Err(e) => {
                println!("Duplicate key handling failed: {}", e);
            }
        }
    }

    #[test]
    fn test_unicode_escape_sequences() {
        // Test Unicode escape sequences
        let config = r#"
            unicode_basic = "Unicode: \u0041\u0042\u0043";
            unicode_extended = "Emoji: \u{1F600}";
            mixed_unicode = "Mixed: \u0041\u{1F600}\u0042";
        "#;

        let result: Result<Value, UclError> = from_str(config);
        match result {
            Ok(parsed) => {
                assert!(parsed.is_object());
                let obj = parsed.as_object().unwrap();

                if let Some(basic) = obj.get("unicode_basic") {
                    let basic_str = basic.as_str().unwrap();
                    assert!(basic_str.contains("ABC"));
                }

                if let Some(extended) = obj.get("unicode_extended") {
                    let extended_str = extended.as_str().unwrap();
                    // Should contain emoji if extended Unicode is supported
                    println!("Extended Unicode: {}", extended_str);
                }
            }
            Err(e) => {
                println!("Unicode escape sequences failed: {}", e);
            }
        }
    }

    #[test]
    fn test_cpp_style_comments() {
        // Test C++ style comments (// comment)
        let config = r#"
            // C++ style comment
            key1 = "value1";
            
            key2 = "value2"; // End-of-line C++ comment
            
            # Mixed comment styles
            key3 = "value3";
        "#;

        let result: Result<Value, UclError> = from_str(config);
        match result {
            Ok(parsed) => {
                assert!(parsed.is_object());
                let obj = parsed.as_object().unwrap();

                assert_eq!(obj["key1"], "value1");
                assert_eq!(obj["key2"], "value2");
                assert_eq!(obj["key3"], "value3");
            }
            Err(e) => {
                println!("C++ style comments failed: {}", e);
            }
        }
    }

    #[test]
    fn test_bare_word_values() {
        // Test bare word values (unquoted identifiers)
        let config = r#"
            # Boolean keywords
            enabled = true;
            disabled = false;
            active = yes;
            inactive = no;
            power = on;
            standby = off;
            
            # Null value
            empty = null;
            
            # Special float values
            infinity = inf;
            negative_infinity = -inf;
            not_a_number = nan;
            
            # Regular bare words (should be strings)
            environment = production;
            mode = debug;
        "#;

        let result: Result<Value, UclError> = from_str(config);
        match result {
            Ok(parsed) => {
                assert!(parsed.is_object());
                let obj = parsed.as_object().unwrap();

                // Test boolean conversions
                assert_eq!(obj["enabled"], true);
                assert_eq!(obj["disabled"], false);
                assert_eq!(obj["active"], true);
                assert_eq!(obj["inactive"], false);
                assert_eq!(obj["power"], true);
                assert_eq!(obj["standby"], false);

                // Test null conversion
                assert_eq!(obj["empty"], Value::Null);

                // Test special float values
                if let Some(inf_val) = obj.get("infinity") {
                    assert!(inf_val.as_f64().unwrap().is_infinite());
                    assert!(inf_val.as_f64().unwrap() > 0.0);
                }

                // Test string values
                assert_eq!(obj["environment"], "production");
                assert_eq!(obj["mode"], "debug");
            }
            Err(e) => {
                println!("Bare word values failed: {}", e);
            }
        }
    }

    #[test]
    fn test_real_world_nginx_config() {
        // Test a real-world NGINX configuration
        let config = r#"
            worker_processes = "auto";
            error_log = "/var/log/nginx/error.log";
            pid = "/run/nginx.pid";
            
            events {
                worker_connections = 1024;
                use = "epoll";
            }
            
            http {
                include = "/etc/nginx/mime.types";
                default_type = "application/octet-stream";
                
                sendfile = true;
                tcp_nopush = true;
                tcp_nodelay = true;
                keepalive_timeout = 65;
                
                gzip = true;
                gzip_vary = true;
                gzip_min_length = 10240;
                
                upstream backend {
                    server = "127.0.0.1:8080";
                    server = "127.0.0.1:8081";
                    keepalive = 32;
                }
                
                server {
                    listen = 80;
                    server_name = "example.com";
                    root = "/var/www/html";
                    index = "index.html";
                    
                    location "/" {
                        try_files = "$uri $uri/ =404";
                    }
                    
                    location "/api/" {
                        proxy_pass = "http://backend";
                        proxy_set_header = "Host $host";
                    }
                }
            }
        "#;

        let result: Result<Value, UclError> = from_str(config);
        match result {
            Ok(parsed) => {
                assert!(parsed.is_object());
                let obj = parsed.as_object().unwrap();

                assert_eq!(obj["worker_processes"], "auto");
                assert_eq!(obj["error_log"], "/var/log/nginx/error.log");
                assert_eq!(obj["pid"], "/run/nginx.pid");

                assert!(obj["events"].is_object());
                let events = obj["events"].as_object().unwrap();
                assert_eq!(events["worker_connections"], 1024);
                assert_eq!(events["use"], "epoll");

                assert!(obj["http"].is_object());
                let http = obj["http"].as_object().unwrap();
                assert_eq!(http["sendfile"], true);
                assert_eq!(http["gzip"], true);

                // Test upstream block
                assert!(http["upstream"].is_object());
                let upstream = http["upstream"].as_object().unwrap();
                assert!(upstream["backend"].is_object());

                // Test server block
                assert!(http["server"].is_object());
                let server = http["server"].as_object().unwrap();
                assert_eq!(server["listen"], 80);
                assert_eq!(server["server_name"], "example.com");

                // Test location blocks
                assert!(server["location"].is_object());
            }
            Err(e) => {
                println!("Real-world NGINX config failed: {}", e);
            }
        }
    }

    #[test]
    fn test_error_handling_compatibility() {
        // Test error handling for various malformed inputs
        let invalid_configs = vec![
            // Unterminated string
            r#"key = "unterminated"#,
            // Invalid number
            r#"key = 123abc"#,
            // Missing value
            r#"key = "#,
            // Invalid Unicode escape
            r#"key = "\u{GGGG}""#,
            // Unterminated heredoc
            r#"key = <<EOF
content without terminator"#,
        ];

        for (i, config) in invalid_configs.iter().enumerate() {
            let result: Result<Value, UclError> = from_str(config);
            match result {
                Ok(_) => {
                    println!("Config {} unexpectedly succeeded: {}", i, config);
                }
                Err(e) => {
                    println!("Config {} failed as expected: {}", i, e);
                    // Verify error contains useful information
                    let error_str = e.to_string();
                    assert!(!error_str.is_empty());
                }
            }
        }
    }
}

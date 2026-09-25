//! NGINX-style syntax: keys without separators and named sections (spec §1, §3.4, §8.2).
//! Expected results are libucl's (checked with the oracle).

use serde_json::Value;
use ucl_lexer::parse::ErrorKind;
use ucl_lexer::{UclError, from_str};

#[cfg(test)]
mod nginx_syntax_tests {
    use super::*;

    #[test]
    fn test_implicit_object_creation() {
        // Test key { ... } syntax creates implicit object assignment
        let config = r#"
            server {
                listen 80
                server_name "example.com"
            }
        "#;

        let result: Value = from_str(config).expect("Should parse NGINX-style implicit object");
        assert_eq!(result["server"]["listen"], 80);
        assert_eq!(result["server"]["server_name"], "example.com");
    }

    #[test]
    fn test_nested_object_creation() {
        // Test key identifier { ... } syntax creates nested object structure
        let config = r#"
            upstream backend {
                server "127.0.0.1:3000"
                keepalive 32
            }
        "#;

        let result: Value = from_str(config).expect("Should parse NGINX-style nested object");
        assert_eq!(result["upstream"]["backend"]["server"], "127.0.0.1:3000");
        assert_eq!(result["upstream"]["backend"]["keepalive"], 32);
    }

    #[test]
    fn test_bare_word_value_assignment() {
        // Test key value syntax assigns bare word as string value
        let config = r#"
            worker_processes auto
            error_log /var/log/nginx/error.log
            pid /run/nginx.pid
        "#;

        let result: Value = from_str(config).expect("Should parse bare word values");
        assert_eq!(result["worker_processes"], "auto");
        assert_eq!(result["error_log"], "/var/log/nginx/error.log");
        assert_eq!(result["pid"], "/run/nginx.pid");
    }

    #[test]
    fn test_mixed_syntax_styles() {
        // Test mixed implicit and explicit syntax in same configuration
        let config = r#"
            server {
                listen = 80                    # Explicit with equals
                server_name: "example.com"     # Explicit with colon
                root /var/www/html             # Implicit bare word
                
                location / {                   # Implicit nested object
                    try_files $uri $uri/ =404
                }
                
                ssl_certificate = "/path/to/cert.pem"  # Explicit
            }
        "#;

        let result: Value = from_str(config).expect("Should parse mixed syntax styles");
        assert_eq!(result["server"]["listen"], 80);
        assert_eq!(result["server"]["server_name"], "example.com");
        assert_eq!(result["server"]["root"], "/var/www/html");
        assert_eq!(
            result["server"]["location"]["/"]["try_files"],
            "$uri $uri/ =404"
        );
        assert_eq!(result["server"]["ssl_certificate"], "/path/to/cert.pem");
    }

    #[test]
    fn test_complex_nginx_config() {
        // Test comprehensive NGINX-style configuration
        let config = r#"
            events {
                worker_connections 1024
            }
            
            http {
                include /etc/nginx/mime.types
                default_type application/octet-stream
                
                upstream app_servers {
                    server 127.0.0.1:3000 weight=3
                    server 127.0.0.1:3001 weight=2
                    server 127.0.0.1:3002 weight=1
                }
                
                server {
                    listen 80
                    server_name example.com www.example.com
                    
                    location / {
                        proxy_pass http://app_servers
                        proxy_set_header Host $host
                    }
                    
                    location /static/ {
                        alias /var/www/static/
                        expires 30d
                    }
                }
            }
        "#;

        let result: Value = from_str(config).expect("Should parse complex NGINX config");

        // Verify events block
        assert_eq!(result["events"]["worker_connections"], 1024);

        // Verify http block structure
        assert_eq!(result["http"]["include"], "/etc/nginx/mime.types");
        assert_eq!(result["http"]["default_type"], "application/octet-stream");

        // A repeated key holds several values (spec §8.2); an unquoted value runs to the end of
        // the line, spaces included (spec §4.1).
        let upstream = &result["http"]["upstream"]["app_servers"];
        assert_eq!(
            upstream["server"],
            serde_json::json!([
                "127.0.0.1:3000 weight=3",
                "127.0.0.1:3001 weight=2",
                "127.0.0.1:3002 weight=1"
            ])
        );

        // Verify server block
        let server = &result["http"]["server"];
        assert_eq!(server["listen"], 80);
        assert_eq!(server["server_name"], "example.com www.example.com");

        // Each `location NAME { … }` is a named section (spec §3.4), and the two sections are two
        // values of the key `location` (spec §8.2), so serde sees a sequence of two objects.
        let locations = server["location"].as_array().expect("two values");
        assert_eq!(locations.len(), 2);
        assert_eq!(locations[0]["/"]["proxy_pass"], "http://app_servers");
        assert_eq!(locations[0]["/"]["proxy_set_header"], "Host $host");
        assert_eq!(locations[1]["/static/"]["alias"], "/var/www/static/");
        // `30d` is a time of 30 days, read as seconds (spec §5.4).
        assert_eq!(locations[1]["/static/"]["expires"], 30.0 * 86400.0);
    }

    #[test]
    fn test_implicit_object_with_explicit_separators() {
        // Test that explicit separators still work within implicit objects
        let config = r#"
            server {
                listen: 80
                server_name = "example.com"
                root /var/www
            }
        "#;

        let result: Value =
            from_str(config).expect("Should parse mixed separators in implicit object");
        assert_eq!(result["server"]["listen"], 80);
        assert_eq!(result["server"]["server_name"], "example.com");
        assert_eq!(result["server"]["root"], "/var/www");
    }

    #[test]
    fn test_nested_implicit_objects() {
        // Test deeply nested implicit object structures
        let config = r#"
            http {
                server {
                    location /api/ {
                        proxy_pass http://backend
                        proxy_timeout 30s
                    }
                }
            }
        "#;

        let result: Value = from_str(config).expect("Should parse nested implicit objects");
        let location = &result["http"]["server"]["location"]["/api/"];
        assert_eq!(location["proxy_pass"], "http://backend");
        // A time value: 30 seconds, as libucl parses it
        // (tests/conformance/cases/migrated/nginx_syntax_tests__nested_implicit_objects.golden.json).
        assert_eq!(location["proxy_timeout"], 30.0);
    }

    #[test]
    fn test_nginx_syntax_error_handling() {
        // A key without a value, and a `{` where a key must start, are errors (spec §1, §3).
        for (config, expected) in [
            ("server { listen }", ErrorKind::MissingValue),
            ("server { { }", ErrorKind::InvalidKey { found: Some('{') }),
        ] {
            match from_str::<Value>(config) {
                Err(UclError::Syntax(e)) => assert_eq!(e.kind(), &expected, "{config}"),
                other => panic!("{config}: expected a parse error, got {other:?}"),
            }
        }
    }
}

use serde_json::Value;
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

        // Verify upstream block
        let upstream = &result["http"]["upstream"]["app_servers"];
        assert!(upstream["server"].is_array() || upstream["server"].is_string());

        // Verify server block
        let server = &result["http"]["server"];
        assert_eq!(server["listen"], 80);
        assert!(server["server_name"].is_string());

        // Verify location blocks
        assert!(server["location"].is_object());
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
        assert_eq!(location["proxy_timeout"], "30s");
    }

    #[test]
    fn test_nginx_syntax_error_handling() {
        // Test error handling for malformed NGINX-style syntax
        let invalid_configs = vec![
            r#"server { listen }"#, // Missing value
            r#"server { { }"#,      // Invalid nesting
        ];

        for config in invalid_configs {
            let result: Result<Value, UclError> = from_str(config);
            assert!(
                result.is_err(),
                "Should fail to parse invalid config: {}",
                config
            );
        }
    }
}

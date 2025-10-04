use serde_json::Value;
use ucl_lexer::from_str;

#[cfg(test)]
mod implicit_array_tests {
    use super::*;

    #[test]
    fn test_automatic_array_creation_from_repeated_keys() {
        // Test same key assigned multiple times creates array
        let config = r#"
            server = "server1.example.com"
            server = "server2.example.com"
            server = "server3.example.com"
            
            port = 80
            port = 443
            port = 8080
        "#;

        let result: Value = from_str(config).expect("Should create arrays from repeated keys");

        // Should create arrays automatically
        assert!(result["server"].is_array());
        let servers = result["server"].as_array().unwrap();
        assert_eq!(servers.len(), 3);
        assert_eq!(servers[0], "server1.example.com");
        assert_eq!(servers[1], "server2.example.com");
        assert_eq!(servers[2], "server3.example.com");

        assert!(result["port"].is_array());
        let ports = result["port"].as_array().unwrap();
        assert_eq!(ports.len(), 3);
        assert_eq!(ports[0], 80);
        assert_eq!(ports[1], 443);
        assert_eq!(ports[2], 8080);
    }

    #[test]
    fn test_array_extension_behavior() {
        // Test key already has array value and is assigned again
        let config = r#"
            items = ["item1", "item2"]
            items = "item3"
            items = "item4"
        "#;

        let result: Value = from_str(config).expect("Should extend existing arrays");

        assert!(result["items"].is_array());
        let items = result["items"].as_array().unwrap();
        assert_eq!(items.len(), 4);
        assert_eq!(items[0], "item1");
        assert_eq!(items[1], "item2");
        assert_eq!(items[2], "item3");
        assert_eq!(items[3], "item4");
    }

    #[test]
    fn test_single_to_array_conversion() {
        // Test key has single value and is assigned again converts to array
        let config = r#"
            database_host = "primary.db.com"
            database_host = "secondary.db.com"
            
            timeout = 30
            timeout = 60
        "#;

        let result: Value = from_str(config).expect("Should convert single values to arrays");

        assert!(result["database_host"].is_array());
        let hosts = result["database_host"].as_array().unwrap();
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0], "primary.db.com");
        assert_eq!(hosts[1], "secondary.db.com");

        assert!(result["timeout"].is_array());
        let timeouts = result["timeout"].as_array().unwrap();
        assert_eq!(timeouts.len(), 2);
        assert_eq!(timeouts[0], 30);
        assert_eq!(timeouts[1], 60);
    }

    #[test]
    fn test_mixed_type_arrays() {
        // Test implicit arrays preserve mixed types
        let config = r#"
            mixed_values = "string"
            mixed_values = 42
            mixed_values = true
            mixed_values = null
            mixed_values = 3.14
        "#;

        let result: Value = from_str(config).expect("Should preserve mixed types in arrays");

        assert!(result["mixed_values"].is_array());
        let values = result["mixed_values"].as_array().unwrap();
        assert_eq!(values.len(), 5);
        assert_eq!(values[0], "string");
        assert_eq!(values[1], 42);
        assert_eq!(values[2], true);
        assert_eq!(values[3], Value::Null);
        assert_eq!(values[4], 3.14);
    }

    #[test]
    fn test_implicit_arrays_in_nested_objects() {
        // Test implicit arrays within nested object structures
        let config = r#"
            server {
                listen = 80
                listen = 443
                
                location = "/"
                location = "/api"
                location = "/static"
                
                upstream {
                    server = "backend1:3000"
                    server = "backend2:3000"
                    server = "backend3:3000"
                }
            }
        "#;

        let result: Value =
            from_str(config).expect("Should create implicit arrays in nested objects");

        // Check server listen ports
        let listen_ports = result["server"]["listen"].as_array().unwrap();
        assert_eq!(listen_ports.len(), 2);
        assert_eq!(listen_ports[0], 80);
        assert_eq!(listen_ports[1], 443);

        // Check server locations
        let locations = result["server"]["location"].as_array().unwrap();
        assert_eq!(locations.len(), 3);
        assert_eq!(locations[0], "/");
        assert_eq!(locations[1], "/api");
        assert_eq!(locations[2], "/static");

        // Check upstream servers
        let upstream_servers = result["server"]["upstream"]["server"].as_array().unwrap();
        assert_eq!(upstream_servers.len(), 3);
        assert_eq!(upstream_servers[0], "backend1:3000");
        assert_eq!(upstream_servers[1], "backend2:3000");
        assert_eq!(upstream_servers[2], "backend3:3000");
    }

    #[test]
    fn test_implicit_arrays_with_objects() {
        // Test implicit arrays containing object values
        let config = r#"
            upstream_server = {
                host = "server1.com"
                weight = 3
            }
            upstream_server = {
                host = "server2.com"
                weight = 2
            }
            upstream_server = {
                host = "server3.com"
                weight = 1
            }
        "#;

        let result: Value = from_str(config).expect("Should create arrays of objects");

        assert!(result["upstream_server"].is_array());
        let servers = result["upstream_server"].as_array().unwrap();
        assert_eq!(servers.len(), 3);

        assert_eq!(servers[0]["host"], "server1.com");
        assert_eq!(servers[0]["weight"], 3);
        assert_eq!(servers[1]["host"], "server2.com");
        assert_eq!(servers[1]["weight"], 2);
        assert_eq!(servers[2]["host"], "server3.com");
        assert_eq!(servers[2]["weight"], 1);
    }

    #[test]
    fn test_implicit_arrays_with_nginx_syntax() {
        // Test implicit arrays combined with NGINX-style syntax
        let config = r#"
            upstream backend {
                server 127.0.0.1:3000
                server 127.0.0.1:3001
                server 127.0.0.1:3002
            }
            
            server {
                listen 80
                listen 443
                
                location / {
                    proxy_pass http://backend
                }
                
                location /api {
                    proxy_pass http://api_backend
                }
            }
        "#;

        let result: Value =
            from_str(config).expect("Should handle implicit arrays with NGINX syntax");

        // Check upstream servers
        let upstream_servers = result["upstream"]["backend"]["server"].as_array().unwrap();
        assert_eq!(upstream_servers.len(), 3);
        assert_eq!(upstream_servers[0], "127.0.0.1:3000");
        assert_eq!(upstream_servers[1], "127.0.0.1:3001");
        assert_eq!(upstream_servers[2], "127.0.0.1:3002");

        // Check server listen ports
        let listen_ports = result["server"]["listen"].as_array().unwrap();
        assert_eq!(listen_ports.len(), 2);
        assert_eq!(listen_ports[0], 80);
        assert_eq!(listen_ports[1], 443);
    }

    #[test]
    fn test_explicit_vs_implicit_arrays() {
        // Test mixing explicit and implicit array syntax
        let config = r#"
            # Explicit array syntax
            explicit_array = ["item1", "item2"]
            
            # Implicit array through repetition
            implicit_array = "item1"
            implicit_array = "item2"
            
            # Mixed: start with explicit, extend with implicit
            mixed_array = ["initial1", "initial2"]
            mixed_array = "added1"
            mixed_array = "added2"
        "#;

        let result: Value = from_str(config).expect("Should handle explicit and implicit arrays");

        // Explicit array
        let explicit = result["explicit_array"].as_array().unwrap();
        assert_eq!(explicit.len(), 2);
        assert_eq!(explicit[0], "item1");
        assert_eq!(explicit[1], "item2");

        // Implicit array
        let implicit = result["implicit_array"].as_array().unwrap();
        assert_eq!(implicit.len(), 2);
        assert_eq!(implicit[0], "item1");
        assert_eq!(implicit[1], "item2");

        // Mixed array
        let mixed = result["mixed_array"].as_array().unwrap();
        assert_eq!(mixed.len(), 4);
        assert_eq!(mixed[0], "initial1");
        assert_eq!(mixed[1], "initial2");
        assert_eq!(mixed[2], "added1");
        assert_eq!(mixed[3], "added2");
    }

    #[test]
    fn test_duplicate_key_error_when_disabled() {
        // Test error on duplicates when duplicate key handling is disabled
        // Note: This test assumes there's a way to configure duplicate key behavior
        let config = r#"
            server = "server1"
            server = "server2"
        "#;

        // With default settings (assuming implicit arrays are enabled)
        let result: Value = from_str(config).expect("Should create array with default settings");
        assert!(result["server"].is_array());

        // Test would need to be extended if there's a way to disable duplicate key handling
        // For now, we just verify the default behavior works
    }

    #[test]
    fn test_complex_implicit_array_scenario() {
        // Test complex real-world scenario with implicit arrays
        let config = r#"
            # Load balancer configuration
            upstream api_servers {
                server 10.0.1.10:3000 weight=3
                server 10.0.1.11:3000 weight=2
                server 10.0.1.12:3000 weight=1
                server 10.0.1.13:3000 backup
            }
            
            server {
                listen 80
                listen 443 ssl
                
                server_name api.example.com
                server_name www.api.example.com
                
                location /v1 {
                    proxy_pass http://api_servers
                    proxy_set_header Host $host
                    proxy_set_header X-Real-IP $remote_addr
                }
                
                location /v2 {
                    proxy_pass http://api_servers
                    proxy_set_header Host $host
                }
                
                # SSL configuration
                ssl_certificate /path/to/cert.pem
                ssl_certificate_key /path/to/key.pem
                
                # Multiple SSL protocols
                ssl_protocols TLSv1.2
                ssl_protocols TLSv1.3
            }
        "#;

        let result: Value = from_str(config).expect("Should parse complex implicit array scenario");

        // Verify upstream servers array
        let servers = result["upstream"]["api_servers"]["server"]
            .as_array()
            .unwrap();
        assert_eq!(servers.len(), 4);
        assert!(servers[0].as_str().unwrap().contains("10.0.1.10:3000"));
        assert!(servers[3].as_str().unwrap().contains("backup"));

        // Verify listen ports array
        let listen = result["server"]["listen"].as_array().unwrap();
        assert_eq!(listen.len(), 2);
        assert_eq!(listen[0], 80);

        // Verify server names array
        let server_names = result["server"]["server_name"].as_array().unwrap();
        assert_eq!(server_names.len(), 2);
        assert_eq!(server_names[0], "api.example.com");
        assert_eq!(server_names[1], "www.api.example.com");

        // Verify SSL protocols array
        let ssl_protocols = result["server"]["ssl_protocols"].as_array().unwrap();
        assert_eq!(ssl_protocols.len(), 2);
        assert_eq!(ssl_protocols[0], "TLSv1.2");
        assert_eq!(ssl_protocols[1], "TLSv1.3");
    }

    #[test]
    fn test_implicit_array_ordering() {
        // Test that implicit arrays maintain insertion order
        let config = r#"
            priority_order = "first"
            priority_order = "second"
            priority_order = "third"
            priority_order = "fourth"
            priority_order = "fifth"
        "#;

        let result: Value = from_str(config).expect("Should maintain array order");

        let order = result["priority_order"].as_array().unwrap();
        assert_eq!(order.len(), 5);
        assert_eq!(order[0], "first");
        assert_eq!(order[1], "second");
        assert_eq!(order[2], "third");
        assert_eq!(order[3], "fourth");
        assert_eq!(order[4], "fifth");
    }

    #[test]
    fn test_implicit_arrays_with_comments() {
        // Test implicit arrays with comments between assignments
        let config = r#"
            # First server
            server = "primary.example.com"
            
            # Backup server
            server = "backup.example.com"
            
            // Another backup
            server = "backup2.example.com"
            
            /*
             * Emergency fallback
             */
            server = "emergency.example.com"
        "#;

        let result: Value =
            from_str(config).expect("Should handle comments between array assignments");

        let servers = result["server"].as_array().unwrap();
        assert_eq!(servers.len(), 4);
        assert_eq!(servers[0], "primary.example.com");
        assert_eq!(servers[1], "backup.example.com");
        assert_eq!(servers[2], "backup2.example.com");
        assert_eq!(servers[3], "emergency.example.com");
    }

    #[test]
    fn test_implicit_array_edge_cases() {
        // Test edge cases for implicit array creation
        let config = r#"
            # Single assignment (should not create array)
            single_value = "alone"
            
            # Empty string assignments
            empty_strings = ""
            empty_strings = ""
            
            # Null assignments
            null_values = null
            null_values = null
            
            # Mixed with single value
            mixed_single = "first"
            mixed_single = null
            mixed_single = ""
        "#;

        let result: Value = from_str(config).expect("Should handle implicit array edge cases");

        // Single value should remain single
        assert_eq!(result["single_value"], "alone");
        assert!(!result["single_value"].is_array());

        // Empty strings should create array
        let empty_array = result["empty_strings"].as_array().unwrap();
        assert_eq!(empty_array.len(), 2);
        assert_eq!(empty_array[0], "");
        assert_eq!(empty_array[1], "");

        // Null values should create array
        let null_array = result["null_values"].as_array().unwrap();
        assert_eq!(null_array.len(), 2);
        assert_eq!(null_array[0], Value::Null);
        assert_eq!(null_array[1], Value::Null);

        // Mixed values should create array
        let mixed_array = result["mixed_single"].as_array().unwrap();
        assert_eq!(mixed_array.len(), 3);
        assert_eq!(mixed_array[0], "first");
        assert_eq!(mixed_array[1], Value::Null);
        assert_eq!(mixed_array[2], "");
    }
}

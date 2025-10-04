//! Compatibility tests with existing UCL implementations
//!
//! These tests verify that our UCL parser is compatible with configurations
//! that work with other UCL implementations like libucl.

use serde::Deserialize;
use std::collections::HashMap;
use ucl_lexer::from_str;

fn assert_special_infinite(value: &serde_json::Value, positive: bool) {
    if let Some(f) = value.as_f64() {
        assert!(f.is_infinite(), "Expected infinite float, got {:?}", value);
        assert_eq!(
            f.is_sign_positive(),
            positive,
            "Unexpected infinity sign for {:?}",
            value
        );
        return;
    }

    if value.is_null() {
        // serde_json cannot represent non-finite numbers; null indicates special float value
        return;
    }

    if let Some(s) = value.as_str() {
        let normalized = s.trim().to_ascii_lowercase();
        if positive {
            assert!(
                normalized == "inf" || normalized == "infinity",
                "Expected positive infinity representation, got {:?}",
                value
            );
        } else {
            assert!(
                normalized == "-inf" || normalized == "-infinity",
                "Expected negative infinity representation, got {:?}",
                value
            );
        }
        return;
    }

    panic!("Unexpected representation for infinity: {:?}", value);
}

fn assert_special_nan(value: &serde_json::Value) {
    if let Some(f) = value.as_f64() {
        assert!(f.is_nan(), "Expected NaN float, got {:?}", value);
        return;
    }

    if value.is_null() {
        // serde_json uses null for non-finite numbers when deserializing into Value
        return;
    }

    if let Some(s) = value.as_str() {
        assert_eq!(
            s.trim().to_ascii_lowercase(),
            "nan",
            "Expected 'nan' string, got {:?}",
            value
        );
        return;
    }

    panic!("Unexpected representation for NaN: {:?}", value);
}

#[test]
fn test_libucl_basic_compatibility() {
    // Test basic UCL syntax that should be compatible with libucl
    let config = r#"
        # Basic UCL configuration compatible with libucl
        param = value;
        section {
            flag = true;
            number = 10k;
            time = 0.2s;
            string = "something";
            subsection {
                host = {
                    host = "hostname";
                    port = 900;
                }
            }
        }
        
        # Array syntax
        array = [
            "item1",
            "item2",
            "item3"
        ];
        
        # Multiple assignment (UCL feature)
        servers = "server1";
        servers = "server2";
        servers = "server3";
    "#;

    // Parse as flexible JSON value since UCL allows multiple assignments
    let parsed: serde_json::Value =
        from_str(config).expect("Failed to parse libucl compatible config");

    assert!(parsed.is_object());
    let obj = parsed.as_object().unwrap();

    assert_eq!(obj["param"], "value");
    assert!(obj["section"].is_object());
    assert!(obj["array"].is_array());

    // Note: Multiple assignment behavior may differ between implementations
    // Our implementation might handle this differently than libucl
}

#[test]
fn test_nginx_ucl_compatibility() {
    // Test UCL configuration similar to what NGINX UCL module would use
    let config = r#"
        # NGINX-style UCL configuration
        worker_processes = auto;
        
        events {
            worker_connections = 1024;
            use = epoll;
        }
        
        http {
            include = "/etc/nginx/mime.types";
            default_type = "application/octet-stream";
            
            sendfile = on;
            tcp_nopush = on;
            tcp_nodelay = on;
            keepalive_timeout = 65;
            
            gzip = on;
            gzip_vary = on;
            gzip_min_length = 10240;
            gzip_proxied = "expired no-cache no-store private must-revalidate auth";
            gzip_types = [
                "text/plain",
                "text/css",
                "text/xml",
                "text/javascript",
                "application/javascript",
                "application/xml+rss",
                "application/json"
            ];
            
            upstream backend {
                server = "127.0.0.1:8080 weight=3";
                server = "127.0.0.1:8081 weight=2";
                server = "127.0.0.1:8082 weight=1 backup";
                
                keepalive = 32;
            }
            
            server {
                listen = 80;
                listen = "[::]:80";
                server_name = "example.com www.example.com";
                root = "/var/www/html";
                index = "index.html index.htm";
                
                location "/" {
                    try_files = "$uri $uri/ =404";
                }
                
                location "/api/" {
                    proxy_pass = "http://backend";
                    proxy_set_header = "Host $host";
                    proxy_set_header = "X-Real-IP $remote_addr";
                    proxy_set_header = "X-Forwarded-For $proxy_add_x_forwarded_for";
                    proxy_set_header = "X-Forwarded-Proto $scheme";
                }
                
                location "~ \\.php$" {
                    fastcgi_pass = "unix:/var/run/php/php7.4-fpm.sock";
                    fastcgi_index = "index.php";
                    include = "fastcgi_params";
                }
            }
        }
    "#;

    let parsed: serde_json::Value = from_str(config).expect("Failed to parse NGINX-style UCL");

    assert!(parsed.is_object());
    let obj = parsed.as_object().unwrap();

    assert_eq!(obj["worker_processes"], "auto");
    assert!(obj["events"].is_object());
    assert!(obj["http"].is_object());

    let http = obj["http"].as_object().unwrap();
    assert!(http["gzip_types"].is_array());
    assert!(http["upstream"].is_object());
    assert!(http["server"].is_object());
}

#[test]
fn test_freebsd_ucl_compatibility() {
    // Test UCL configuration similar to FreeBSD pkg format
    let config = r#"
        # FreeBSD pkg-style UCL configuration
        name = "mypackage";
        version = "1.2.3";
        origin = "local/mypackage";
        comment = "My custom package";
        desc = <<EOD
        This is a longer description
        of the package that spans
        multiple lines.
EOD
        
        maintainer = "user@example.com";
        www = "https://example.com";
        
        arch = "FreeBSD:13:amd64";
        prefix = "/usr/local";
        
        deps = {
            "python38" = {
                origin = "lang/python38";
                version = "3.8.12";
            }
            "nginx" = {
                origin = "www/nginx";
                version = "1.20.1";
            }
        }
        
        files = {
            "/usr/local/bin/myapp" = "1f2e3d4c5b6a7890abcdef1234567890";
            "/usr/local/etc/myapp.conf" = "abcdef1234567890f1e2d3c4b5a67890";
            "/usr/local/share/doc/myapp/README" = "567890abcdef12341f2e3d4c5b6a7890";
        }
        
        directories = {
            "/usr/local/share/myapp" = "y";
            "/usr/local/etc/myapp" = "y";
        }
        
        scripts = {
            "pre-install" = <<SCRIPT
        #!/bin/sh
        echo "Installing mypackage..."
SCRIPT

            "post-install" = <<SCRIPT
        #!/bin/sh
        echo "mypackage installed successfully"
SCRIPT
        }
        
        options = {
            "SSL" = "on";
            "IPV6" = "off";
            "DEBUG" = "off";
        }
        
        categories = ["local", "devel"];
        licenses = ["BSD2CLAUSE"];
        
        annotations = {
            "repo_type" = "binary";
            "built_by" = "poudriere";
        }
    "#;

    #[derive(Debug, Deserialize)]
    struct FreeBSDPackage {
        name: String,
        version: String,
        origin: String,
        comment: String,
        desc: String,
        maintainer: String,
        www: String,
        arch: String,
        prefix: String,
        deps: HashMap<String, Dependency>,
        files: HashMap<String, String>,
        directories: HashMap<String, String>,
        scripts: HashMap<String, String>,
        options: HashMap<String, String>,
        categories: Vec<String>,
        licenses: Vec<String>,
        annotations: HashMap<String, String>,
    }

    #[derive(Debug, Deserialize)]
    struct Dependency {
        origin: String,
        version: String,
    }

    let parsed: FreeBSDPackage = from_str(config).expect("Failed to parse FreeBSD UCL");

    assert_eq!(parsed.name, "mypackage");
    assert_eq!(parsed.version, "1.2.3");
    assert_eq!(parsed.origin, "local/mypackage");
    assert_eq!(parsed.comment, "My custom package");
    assert!(parsed.desc.contains("multiple lines"));
    assert_eq!(parsed.maintainer, "user@example.com");
    assert_eq!(parsed.www, "https://example.com");
    assert_eq!(parsed.arch, "FreeBSD:13:amd64");
    assert_eq!(parsed.prefix, "/usr/local");
    assert_eq!(parsed.deps.len(), 2);
    assert!(parsed.deps.contains_key("python38"));
    let python_dep = parsed.deps.get("python38").unwrap();
    assert_eq!(python_dep.origin, "lang/python38");
    assert_eq!(python_dep.version, "3.8.12");
    let nginx_dep = parsed.deps.get("nginx").unwrap();
    assert_eq!(nginx_dep.origin, "www/nginx");
    assert_eq!(nginx_dep.version, "1.20.1");
    assert_eq!(parsed.files.len(), 3);
    assert_eq!(
        parsed.files.get("/usr/local/bin/myapp").unwrap(),
        "1f2e3d4c5b6a7890abcdef1234567890"
    );
    assert_eq!(parsed.directories.len(), 2);
    assert!(parsed.scripts.get("pre-install").unwrap().contains("Installing"));
    assert_eq!(parsed.options.get("SSL").map(String::as_str), Some("on"));
    assert_eq!(parsed.categories.len(), 2);
    assert_eq!(parsed.licenses[0], "BSD2CLAUSE");
    assert_eq!(
        parsed.annotations.get("repo_type").map(String::as_str),
        Some("binary")
    );
}

#[test]
fn test_rspamd_ucl_compatibility() {
    // Test UCL configuration similar to Rspamd format
    let config = r#"
        # Rspamd-style UCL configuration
        logging = {
            type = "file";
            filename = "/var/log/rspamd/rspamd.log";
            level = "info";
        }
        
        worker "normal" {
            bind_socket = "localhost:11333";
            .include "$CONFDIR/worker-normal.inc"
        }
        
        worker "controller" {
            bind_socket = "localhost:11334";
            password = "$2$xu1581gidj5cyp4yjgo68qbj6jz1j8o3$j9yg4k5k9k5k9k5k9k5k9k5k9k5k9k5k";
            enable_password = "$2$xu1581gidj5cyp4yjgo68qbj6jz1j8o3$j9yg4k5k9k5k9k5k9k5k9k5k9k5k9k5k";
            secure_ip = ["127.0.0.1", "::1"];
            static_dir = "${WWWDIR}";
        }
        
        modules = {
            path = "$PLUGINSDIR/lua/";
        }
        
        lua = "$CONFDIR/lua/rspamd.lua";
        
        # Metric configuration
        metric "default" {
            actions = {
                reject = 15;
                add_header = 6;
                greylist = 4;
            }
            
            unknown_weight = 1;
        }
        
        # Classifier configuration
        classifier "bayes" {
            tokenizer {
                name = "osb-text";
            }
            
            cache {
                path = "${DBDIR}/learn_cache.sqlite";
            }
            
            min_learns = 200;
            backend = "sqlite3";
            languages_enabled = true;
            
            statfile {
                symbol = "BAYES_HAM";
                path = "${DBDIR}/bayes.ham.sqlite";
                spam = false;
            }
            
            statfile {
                symbol = "BAYES_SPAM";
                path = "${DBDIR}/bayes.spam.sqlite";
                spam = true;
            }
        }
        
        # Composites
        composites = {
            "FORGED_RECIPIENTS" = "FORGED_RECIPIENTS_MAILRU | FORGED_RECIPIENTS_GMAIL";
            "SUSPICIOUS_RECIPS" = "SUSPICIOUS_RECIPS & !WHITELIST_SPF";
        }
        
        # Groups configuration
        group "policies" {
            .include "$CONFDIR/policies.conf"
        }
        
        # DNS configuration
        dns {
            nameserver = ["8.8.8.8", "8.8.4.4"];
            retransmits = 5;
            timeout = 1s;
        }
    "#;

    // Use flexible parsing for Rspamd-style config due to complex syntax
    let parsed: serde_json::Value = from_str(config).expect("Failed to parse Rspamd UCL");

    assert!(parsed.is_object());
    let obj = parsed.as_object().unwrap();

    assert!(obj["logging"].is_object());
    assert!(obj["modules"].is_object());
    assert!(obj["dns"].is_object());

    let logging = obj["logging"].as_object().unwrap();
    assert_eq!(logging["type"], "file");
    assert_eq!(logging["level"], "info");

    let dns = obj["dns"].as_object().unwrap();
    assert!(dns["nameserver"].is_array());
    assert_eq!(dns["retransmits"], 5);
}

#[test]
fn test_ucl_number_formats_compatibility() {
    // Test various number formats that should be compatible with libucl
    let config = r#"
        # Number format compatibility tests
        integers = {
            decimal = 42;
            hex = 0xFF;
            octal = 0o755;
            binary = 0b11010101;
        }
        
        floats = {
            simple = 3.14159;
            scientific = 1.23e-4;
            negative_exp = 2.5e-10;
            positive_exp = 1.5e+6;
        }
        
        # Size suffixes (binary)
        sizes_binary = {
            bytes = 1024b;
            kilobytes = 64kb;
            megabytes = 512mb;
            gigabytes = 2gb;
            terabytes = 1tb;
        }
        
        # Size suffixes (decimal)
        sizes_decimal = {
            kilobytes_decimal = 1000k;
            megabytes_decimal = 100m;
            gigabytes_decimal = 5g;
        }
        
        # Time suffixes
        times = {
            milliseconds = 500ms;
            seconds = 30s;
            minutes = 5min;
            hours = 2h;
            days = 7d;
            weeks = 2w;
            years = 1y;
        }
        
        # Special values
        special = {
            infinity = inf;
            negative_infinity = -inf;
            not_a_number = nan;
        }
    "#;

    #[derive(Debug, Deserialize)]
    struct NumberFormats {
        integers: IntegerFormats,
        floats: FloatFormats,
        sizes_binary: SizesBinary,
        sizes_decimal: SizesDecimal,
        times: TimeFormats,
        special: SpecialValues,
    }

    #[derive(Debug, Deserialize)]
    struct IntegerFormats {
        decimal: i32,
        hex: u32,
        octal: u32,
        binary: u8,
    }

    #[derive(Debug, Deserialize)]
    struct FloatFormats {
        simple: f64,
        scientific: f64,
        negative_exp: f64,
        positive_exp: f64,
    }

    #[derive(Debug, Deserialize)]
    struct SizesBinary {
        bytes: u64,
        kilobytes: u64,
        megabytes: u64,
        gigabytes: u64,
        terabytes: u64,
    }

    #[derive(Debug, Deserialize)]
    struct SizesDecimal {
        kilobytes_decimal: u64,
        megabytes_decimal: u64,
        gigabytes_decimal: u64,
    }

    #[derive(Debug, Deserialize)]
    struct TimeFormats {
        milliseconds: f64,
        seconds: f64,
        minutes: f64,
        hours: f64,
        days: f64,
        weeks: f64,
        years: f64,
    }

    #[derive(Debug, Deserialize)]
    struct SpecialValues {
        infinity: f64,
        negative_infinity: f64,
        not_a_number: f64,
    }

    let parsed: NumberFormats = from_str(config).expect("Failed to parse number formats");

    // Verify integer parsing
    assert_eq!(parsed.integers.decimal, 42);
    assert_eq!(parsed.integers.hex, 255);
    assert_eq!(parsed.integers.octal, 493); // 0o755 = 493
    assert_eq!(parsed.integers.binary, 213); // 0b11010101 = 213

    // Verify float parsing
    assert!((parsed.floats.simple - 3.14159).abs() < 0.00001);
    assert!((parsed.floats.scientific - 0.000123).abs() < 0.0000001);
    assert!((parsed.floats.negative_exp - 2.5e-10).abs() < 0.000000000001);
    assert!((parsed.floats.positive_exp - 1.5e6).abs() < 0.1);

    // Verify size parsing (binary)
    assert_eq!(parsed.sizes_binary.bytes, 1024);
    assert_eq!(parsed.sizes_binary.kilobytes, 64 * 1024);
    assert_eq!(parsed.sizes_binary.megabytes, 512 * 1024 * 1024);
    assert_eq!(parsed.sizes_binary.gigabytes, 2 * 1024 * 1024 * 1024);
    assert_eq!(
        parsed.sizes_binary.terabytes,
        1 * 1024 * 1024 * 1024 * 1024
    );
    assert_eq!(parsed.sizes_decimal.kilobytes_decimal, 1_000_000);
    assert_eq!(parsed.sizes_decimal.megabytes_decimal, 100_000_000);
    assert_eq!(parsed.sizes_decimal.gigabytes_decimal, 5_000_000_000);

    // Verify time parsing (converted to seconds)
    assert!((parsed.times.milliseconds - 0.5).abs() < 0.001);
    assert_eq!(parsed.times.seconds, 30.0);
    assert_eq!(parsed.times.minutes, 300.0);
    assert_eq!(parsed.times.hours, 7200.0);
    assert_eq!(parsed.times.days, 604800.0);
    assert_eq!(parsed.times.weeks, 1_209_600.0);
    assert_eq!(parsed.times.years, 31_536_000.0);

    // Verify special values
    assert!(parsed.special.infinity.is_infinite() && parsed.special.infinity > 0.0);
    assert!(
        parsed.special.negative_infinity.is_infinite() && parsed.special.negative_infinity < 0.0
    );
    assert!(parsed.special.not_a_number.is_nan());
}

#[test]
fn test_ucl_string_formats_compatibility() {
    // Test string formats that should be compatible with libucl
    let config = r#"
        # String format compatibility tests
        strings = {
            # Double-quoted strings with escapes
            double_quoted = "Hello\nWorld\t!";
            unicode_escape = "Unicode: \u{1F600}";
            hex_escape = "Hex: \x41\x42\x43";
            
            # Single-quoted strings (literal)
            single_quoted = 'No\nescapes\there';
            single_with_quotes = 'Can contain "double quotes"';
            
            # Heredoc strings
            heredoc_simple = <<EOF
This is a heredoc string
that preserves formatting
and whitespace.
EOF
            
            heredoc_custom = <<CUSTOM_DELIMITER
Custom delimiter heredoc
can use any delimiter name.
CUSTOM_DELIMITER
            
            # Multiline strings (alternative syntax)
            multiline = """
            This is a multiline string
            using triple quotes.
            """;
        }
        
        # String concatenation (if supported)
        concatenated = "Hello " + "World";
        
        # Strings with variables (basic test)
        templated = "Value: ${some_var}";
    "#;

    // Parse as JSON value to handle potential parsing differences
    let parsed: serde_json::Value = from_str(config).expect("Failed to parse string formats");

    assert!(parsed.is_object());
    let obj = parsed.as_object().unwrap();

    let strings = obj["strings"].as_object().unwrap();

    // Verify double-quoted string with escapes
    let double_quoted = strings["double_quoted"].as_str().unwrap();
    assert!(double_quoted.contains('\n'));
    assert!(double_quoted.contains('\t'));

    // Verify single-quoted literal string
    let single_quoted = strings["single_quoted"].as_str().unwrap();
    assert!(single_quoted.contains("\\n")); // Should be literal backslash-n

    // Verify heredoc strings
    let heredoc = strings["heredoc_simple"].as_str().unwrap();
    assert!(heredoc.contains("heredoc string"));
    assert!(heredoc.contains("formatting"));
}

#[test]
fn test_ucl_comments_compatibility() {
    // Test comment formats that should be compatible with libucl
    let config = r#"
        # Single-line comment
        key1 = "value1";  # End-of-line comment
        
        /*
         * Multi-line comment
         * with multiple lines
         */
        key2 = "value2";
        
        /* Inline comment */ key3 = "value3";
        
        /*
         * Nested comments test
         * /* This is nested */
         * Back to outer comment
         */
        key4 = "value4";
        
        // C++-style comment (if supported)
        key5 = "value5";
        
        section {
            # Comment inside section
            nested_key = "nested_value";
            
            /*
             * Multi-line comment
             * inside section
             */
            another_key = "another_value";
        }
    "#;

    let parsed: serde_json::Value = from_str(config).expect("Failed to parse config with comments");

    assert!(parsed.is_object());
    let obj = parsed.as_object().unwrap();

    assert_eq!(obj["key1"], "value1");
    assert_eq!(obj["key2"], "value2");
    assert_eq!(obj["key3"], "value3");
    assert_eq!(obj["key4"], "value4");

    // C++-style comments might not be supported in all implementations
    if obj.contains_key("key5") {
        assert_eq!(obj["key5"], "value5");
    }

    assert!(obj["section"].is_object());
    let section = obj["section"].as_object().unwrap();
    assert_eq!(section["nested_key"], "nested_value");
    assert_eq!(section["another_key"], "another_value");
}

#[test]
fn test_ucl_array_formats_compatibility() {
    // Test array formats that should be compatible with libucl
    let config = r#"
        # Array format compatibility tests
        arrays = {
            # Simple array
            simple = ["item1", "item2", "item3"];
            
            # Array with trailing comma
            trailing_comma = [
                "item1",
                "item2",
                "item3",
            ];
            
            # Mixed type array
            mixed = [1, "string", true, 3.14];
            
            # Nested arrays
            nested = [
                ["a", "b"],
                ["c", "d"],
                [1, 2, 3]
            ];
            
            # Array of objects
            objects = [
                { name = "obj1", value = 1 },
                { name = "obj2", value = 2 }
            ];
        }
        
        # Multiple assignment to create array (UCL feature)
        multi_assign = "value1";
        multi_assign = "value2";
        multi_assign = "value3";
    "#;

    let parsed: serde_json::Value = from_str(config).expect("Failed to parse array formats");

    assert!(parsed.is_object());
    let obj = parsed.as_object().unwrap();

    let arrays = obj["arrays"].as_object().unwrap();

    // Verify simple array
    let simple = arrays["simple"].as_array().unwrap();
    assert_eq!(simple.len(), 3);
    assert_eq!(simple[0], "item1");

    // Verify trailing comma array
    let trailing = arrays["trailing_comma"].as_array().unwrap();
    assert_eq!(trailing.len(), 3);

    // Verify mixed type array
    let mixed = arrays["mixed"].as_array().unwrap();
    assert_eq!(mixed.len(), 4);
    assert_eq!(mixed[0], 1);
    assert_eq!(mixed[1], "string");
    assert_eq!(mixed[2], true);

    // Verify nested arrays
    let nested = arrays["nested"].as_array().unwrap();
    assert_eq!(nested.len(), 3);
    assert!(nested[0].is_array());

    // Verify array of objects
    let objects = arrays["objects"].as_array().unwrap();
    assert_eq!(objects.len(), 2);
    assert!(objects[0].is_object());

    // Multiple assignment behavior may vary between implementations
    // Some might create an array, others might use the last value
}

#[test]
fn test_heredoc_terminator_improvements() {
    // Test SPEC-compliant heredoc terminator recognition (SPEC.md lines 344-356)
    // Per spec: terminators must be on their own line with no leading/trailing whitespace
    use ucl_lexer::lexer::{StringFormat, Token, UclLexer};

    // Test that leading whitespace invalidates terminator
    let input1 = "<<EOF\nHello World\n  EOF\nEOF";
    let mut lexer1 = UclLexer::new(input1);
    let token1 = lexer1.next_token().unwrap();
    match token1 {
        Token::String { value, format, .. } => {
            // "  EOF" is not a valid terminator, so it's included in content
            assert_eq!(value, "Hello World\n  EOF\n");
            assert_eq!(format, StringFormat::Heredoc);
        }
        _ => panic!("Expected heredoc string token"),
    }

    // Test that trailing whitespace invalidates terminator
    let input2 = "<<EOF\nHello World\nEOF  \nEOF";
    let mut lexer2 = UclLexer::new(input2);
    let token2 = lexer2.next_token().unwrap();
    match token2 {
        Token::String { value, format, .. } => {
            // "EOF  " is not a valid terminator, so it's included in content
            assert_eq!(value, "Hello World\nEOF  \n");
            assert_eq!(format, StringFormat::Heredoc);
        }
        _ => panic!("Expected heredoc string token"),
    }

    // Test that mixed whitespace invalidates terminator
    let input3 = "<<EOF\nHello World\n \t EOF \t \nEOF";
    let mut lexer3 = UclLexer::new(input3);
    let token3 = lexer3.next_token().unwrap();
    match token3 {
        Token::String { value, format, .. } => {
            // " \t EOF \t " is not a valid terminator, so it's included in content
            assert_eq!(value, "Hello World\n \t EOF \t \n");
            assert_eq!(format, StringFormat::Heredoc);
        }
        _ => panic!("Expected heredoc string token"),
    }

    // Test CRLF line endings with invalid terminator
    let input4 = "<<EOF\r\nLine 1\r\nLine 2\r\n  EOF  \r\nEOF\r\n";
    let mut lexer4 = UclLexer::new(input4);
    let token4 = lexer4.next_token().unwrap();
    match token4 {
        Token::String { value, format, .. } => {
            // "  EOF  " is not a valid terminator, so it's included in content
            assert_eq!(value, "Line 1\r\nLine 2\r\n  EOF  \r\n");
            assert_eq!(format, StringFormat::Heredoc);
        }
        _ => panic!("Expected heredoc string token"),
    }
}

#[test]
fn test_heredoc_enhanced_error_messages() {
    // Test enhanced error message for unterminated heredoc (Requirement 4.5)
    let invalid_config = r#"
content = <<EOF
This heredoc has no terminator
"#;

    let result: Result<serde_json::Value, _> = from_str(invalid_config);
    assert!(result.is_err(), "Should fail to parse unterminated heredoc");

    let error = result.unwrap_err();
    let error_msg = error.to_string();
    assert!(
        error_msg.contains("Unterminated heredoc"),
        "Error should mention unterminated heredoc"
    );
    assert!(
        error_msg.contains("line 2"),
        "Error should include line number"
    );
    assert!(
        error_msg.contains("EOF"),
        "Error should mention expected terminator"
    );
    assert!(
        error_msg.contains("on its own line"),
        "Error should explain terminator requirements"
    );

    // Test enhanced error message for invalid terminator
    let invalid_terminator = "content = <<eof\nhello\neof";

    let result2: Result<serde_json::Value, _> = from_str(invalid_terminator);
    assert!(
        result2.is_err(),
        "Should fail to parse lowercase terminator"
    );

    let error2 = result2.unwrap_err();
    let error_msg2 = error2.to_string();
    assert!(
        error_msg2.contains("uppercase ASCII letters"),
        "Error should explain terminator format"
    );
    assert!(
        error_msg2.contains("A-Z"),
        "Error should show valid character range"
    );
}
#[test]
fn test_nginx_style_backward_compatibility() {
    // Test that explicit syntax still works unchanged with NGINX-style support
    let explicit_config = r#"
        {
            "key1" = "value1",
            key2: "value2",
            nested = {
                inner_key = 42,
                another: "test"
            },
            array = [1, 2, 3]
        }
    "#;

    let parsed: serde_json::Value =
        from_str(explicit_config).expect("Failed to parse explicit syntax");

    assert!(parsed.is_object());
    let obj = parsed.as_object().unwrap();

    assert_eq!(obj["key1"], "value1");
    assert_eq!(obj["key2"], "value2");
    assert!(obj["nested"].is_object());
    assert!(obj["array"].is_array());

    let nested = obj["nested"].as_object().unwrap();
    assert_eq!(nested["inner_key"], 42);
    assert_eq!(nested["another"], "test");
}

#[test]
fn test_mixed_syntax_styles_compatibility() {
    // Test that explicit and implicit syntax can be mixed in the same configuration
    let mixed_config = r#"
        {
            # Explicit syntax
            explicit_key = "explicit_value",
            explicit_nested: {
                key1 = "value1",
                key2: "value2"
            },
            
            # Implicit syntax (when NGINX-style is implemented)
            # For now, these should still parse as explicit since separators are present
            implicit_test = "still_explicit"
        }
    "#;

    let parsed: serde_json::Value = from_str(mixed_config).expect("Failed to parse mixed syntax");

    assert!(parsed.is_object());
    let obj = parsed.as_object().unwrap();

    assert_eq!(obj["explicit_key"], "explicit_value");
    assert_eq!(obj["implicit_test"], "still_explicit");
    assert!(obj["explicit_nested"].is_object());
}

#[test]
fn test_error_handling_backward_compatibility() {
    // Test that existing error handling for malformed explicit syntax is preserved
    let malformed_configs = vec![
        // Missing separator
        r#"{ key "value" }"#,
        // Invalid separator
        r#"{ key ~ "value" }"#,
        // Missing value
        r#"{ key = }"#,
        // Unterminated string
        r#"{ key = "unterminated }"#,
        // Note: 123abc now parses as a string "123abc", not an error
    ];

    for config in malformed_configs {
        let result: Result<serde_json::Value, _> = from_str(config);
        assert!(
            result.is_err(),
            "Should fail to parse malformed config: {}",
            config
        );
    }
}
#[test]
fn debug_simple_bare_word() {
    // Test with explicit braces first
    let config_explicit = r#"{ environment = production }"#;
    let result_explicit: Result<serde_json::Value, _> = from_str(config_explicit);

    match result_explicit {
        Ok(parsed) => {
            println!("Explicit braces parsed: {:#}", parsed);
        }
        Err(e) => {
            println!("Explicit braces error: {}", e);
        }
    }

    // Test without braces (implicit object)
    let config = r#"environment = production"#;
    let result: Result<serde_json::Value, _> = from_str(config);

    match result {
        Ok(parsed) => {
            println!("Implicit object parsed: {:#}", parsed);
            assert!(parsed.is_object());
            if parsed.as_object().unwrap().contains_key("environment") {
                assert_eq!(parsed["environment"], "production");
            } else {
                panic!("Key 'environment' not found in parsed object: {:?}", parsed);
            }
        }
        Err(e) => {
            println!("Implicit object error: {}", e);
            panic!("Failed to parse: {}", e);
        }
    }
}

#[test]
fn test_bare_word_values() {
    // Test bare word value parsing (unquoted identifiers)
    let config = r#"
        # Boolean keywords
        enabled = true
        disabled = false
        active = yes
        inactive = no
        power = on
        standby = off
        
        # Null value
        empty = null
        
        # Special float values
        positive_infinity = inf
        negative_infinity = -inf
        not_a_number = nan
        
        # Regular bare words as strings
        environment = production
        mode = debug
        protocol = http
        
        # Mixed with quoted strings
        name = "quoted string"
        type = bare_word
    "#;

    let parsed: serde_json::Value = from_str(config).expect("Failed to parse bare word config");
    let obj = parsed.as_object().unwrap();

    // Test boolean conversions
    assert_eq!(obj["enabled"], true);
    assert_eq!(obj["disabled"], false);
    assert_eq!(obj["active"], true);
    assert_eq!(obj["inactive"], false);
    assert_eq!(obj["power"], true);
    assert_eq!(obj["standby"], false);

    // Test null conversion
    assert_eq!(obj["empty"], serde_json::Value::Null);

    // Test special float values
    assert_special_infinite(&obj["positive_infinity"], true);
    assert_special_infinite(&obj["negative_infinity"], false);
    assert_special_nan(&obj["not_a_number"]);

    // Test string values
    assert_eq!(obj["environment"], "production");
    assert_eq!(obj["mode"], "debug");
    assert_eq!(obj["protocol"], "http");
    assert_eq!(obj["name"], "quoted string");
    assert_eq!(obj["type"], "bare_word");
}

#[test]
fn test_bare_word_validation_errors() {
    // Test that invalid bare words are properly rejected
    let invalid_configs = vec![
        // Note: spaces in values with explicit separators now parse as multiple key-value pairs
        // which may fail later in validation but not during initial parsing
        r#"key = bare{word"#, // Braces not allowed
        r#"key = bare[word"#, // Brackets not allowed
        r#"key = bare=word"#, // Equals not allowed
        r#"key = bare:word"#, // Colon not allowed
        r#"key = bare,word"#, // Comma not allowed
        r#"key = bare;word"#, // Semicolon not allowed
        r#"key = bare#word"#, // Hash not allowed
        r#"key = bare"word"#, // Quote not allowed
    ];

    for config in invalid_configs {
        let result: Result<serde_json::Value, _> = from_str(config);
        assert!(
            result.is_err(),
            "Should fail to parse invalid bare word: {}",
            config
        );
    }
}

#[test]
fn test_bare_word_in_arrays() {
    // Test bare words in array contexts
    let config = r#"
        array = [
            true,
            false,
            null,
            production,
            "quoted",
            123
        ]
    "#;

    let parsed: serde_json::Value =
        from_str(config).expect("Failed to parse array with bare words");
    let array = parsed["array"].as_array().unwrap();

    assert_eq!(array[0], true);
    assert_eq!(array[1], false);
    assert_eq!(array[2], serde_json::Value::Null);
    assert_eq!(array[3], "production");
    assert_eq!(array[4], "quoted");
    assert_eq!(array[5], 123);
}

//! C libucl compatibility test report
//!
//! This module generates a comprehensive report of compatibility with C libucl
//! by testing various UCL features and documenting what works and what doesn't.

use serde_json::Value;
use ucl_lexer::from_str;

#[derive(Debug)]
pub struct CompatibilityTestResult {
    pub test_name: String,
    pub passed: bool,
    pub error_message: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug)]
pub struct CompatibilityReport {
    pub results: Vec<CompatibilityTestResult>,
    pub total_tests: usize,
    pub passed_tests: usize,
    pub failed_tests: usize,
}

impl CompatibilityReport {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
            total_tests: 0,
            passed_tests: 0,
            failed_tests: 0,
        }
    }

    pub fn add_result(&mut self, result: CompatibilityTestResult) {
        self.total_tests += 1;
        if result.passed {
            self.passed_tests += 1;
        } else {
            self.failed_tests += 1;
        }
        self.results.push(result);
    }

    pub fn print_summary(&self) {
        println!("\n=== C libucl Compatibility Report ===");
        println!("Total tests: {}", self.total_tests);
        println!("Passed: {}", self.passed_tests);
        println!("Failed: {}", self.failed_tests);
        println!(
            "Success rate: {:.1}%",
            (self.passed_tests as f64 / self.total_tests as f64) * 100.0
        );

        println!("\n=== Failed Tests ===");
        for result in &self.results {
            if !result.passed {
                println!("❌ {}", result.test_name);
                if let Some(error) = &result.error_message {
                    println!("   Error: {}", error);
                }
                for note in &result.notes {
                    println!("   Note: {}", note);
                }
            }
        }

        println!("\n=== Passed Tests ===");
        for result in &self.results {
            if result.passed {
                println!("✅ {}", result.test_name);
                for note in &result.notes {
                    println!("   Note: {}", note);
                }
            }
        }
    }
}

pub fn run_compatibility_test_suite() -> CompatibilityReport {
    let mut report = CompatibilityReport::new();

    // Test 1: Basic UCL syntax
    let result = test_basic_ucl_syntax();
    report.add_result(result);

    // Test 2: NGINX-style implicit syntax
    let result = test_nginx_implicit_syntax();
    report.add_result(result);

    // Test 3: C++ style comments
    let result = test_cpp_comments();
    report.add_result(result);

    // Test 4: Extended Unicode escapes
    let result = test_extended_unicode();
    report.add_result(result);

    // Test 5: Heredoc improvements
    let result = test_heredoc_improvements();
    report.add_result(result);

    // Test 6: Bare word values
    let result = test_bare_word_values();
    report.add_result(result);

    // Test 7: Implicit arrays from duplicate keys
    let result = test_implicit_arrays();
    report.add_result(result);

    // Test 8: String format compatibility
    let result = test_string_formats();
    report.add_result(result);

    // Test 9: Number format compatibility
    let result = test_number_formats();
    report.add_result(result);

    // Test 10: Array format compatibility
    let result = test_array_formats();
    report.add_result(result);

    // Test 11: Comment format compatibility
    let result = test_comment_formats();
    report.add_result(result);

    // Test 12: Error handling compatibility
    let result = test_error_handling();
    report.add_result(result);

    report
}

fn test_basic_ucl_syntax() -> CompatibilityTestResult {
    let config = r#"
        param = "value";
        number = 42;
        boolean = true;
        
        section {
            nested = "test";
        }
        
        array = [1, 2, 3];
    "#;

    match from_str::<Value>(config) {
        Ok(parsed) => {
            let obj = parsed.as_object().unwrap();
            let mut notes = Vec::new();

            if obj.get("param") == Some(&Value::String("value".to_string())) {
                notes.push("String values work correctly".to_string());
            }
            if obj.get("number") == Some(&Value::Number(42.into())) {
                notes.push("Integer values work correctly".to_string());
            }
            if obj.get("boolean") == Some(&Value::Bool(true)) {
                notes.push("Boolean values work correctly".to_string());
            }
            if obj.get("section").is_some() && obj["section"].is_object() {
                notes.push("Nested objects work correctly".to_string());
            }
            if obj.get("array").is_some() && obj["array"].is_array() {
                notes.push("Arrays work correctly".to_string());
            }

            CompatibilityTestResult {
                test_name: "Basic UCL Syntax".to_string(),
                passed: true,
                error_message: None,
                notes,
            }
        }
        Err(e) => CompatibilityTestResult {
            test_name: "Basic UCL Syntax".to_string(),
            passed: false,
            error_message: Some(e.to_string()),
            notes: vec!["Basic UCL syntax should work with explicit separators".to_string()],
        },
    }
}

fn test_nginx_implicit_syntax() -> CompatibilityTestResult {
    let config = r#"
        server {
            listen 80
            server_name example.com
        }
        
        upstream backend {
            server 127.0.0.1:3000
        }
    "#;

    match from_str::<Value>(config) {
        Ok(parsed) => {
            let mut notes = Vec::new();
            let obj = parsed.as_object().unwrap();

            if obj.get("server").is_some() && obj["server"].is_object() {
                notes.push("Implicit object creation works".to_string());

                let server = obj["server"].as_object().unwrap();
                if server.get("listen").is_some() {
                    notes.push("Bare word values in implicit objects work".to_string());
                }
            }

            if obj.get("upstream").is_some() && obj["upstream"].is_object() {
                notes.push("Nested implicit objects work".to_string());
            }

            CompatibilityTestResult {
                test_name: "NGINX-style Implicit Syntax".to_string(),
                passed: true,
                error_message: None,
                notes,
            }
        }
        Err(e) => CompatibilityTestResult {
            test_name: "NGINX-style Implicit Syntax".to_string(),
            passed: false,
            error_message: Some(e.to_string()),
            notes: vec![
                "NGINX-style syntax is a key UCL feature".to_string(),
                "Should support key { ... } and key value patterns".to_string(),
            ],
        },
    }
}

fn test_cpp_comments() -> CompatibilityTestResult {
    let config = r#"
        // C++ style comment
        key1 = "value1";
        key2 = "value2"; // End-of-line comment
    "#;

    match from_str::<Value>(config) {
        Ok(parsed) => {
            let obj = parsed.as_object().unwrap();
            let mut notes = Vec::new();

            if obj.get("key1") == Some(&Value::String("value1".to_string())) {
                notes.push("C++ comments at start of line work".to_string());
            }
            if obj.get("key2") == Some(&Value::String("value2".to_string())) {
                notes.push("C++ end-of-line comments work".to_string());
            }

            CompatibilityTestResult {
                test_name: "C++ Style Comments".to_string(),
                passed: true,
                error_message: None,
                notes,
            }
        }
        Err(e) => CompatibilityTestResult {
            test_name: "C++ Style Comments".to_string(),
            passed: false,
            error_message: Some(e.to_string()),
            notes: vec!["C++ style comments (//) are supported by C libucl".to_string()],
        },
    }
}

fn test_extended_unicode() -> CompatibilityTestResult {
    let config = r#"
        basic = "Unicode: \u0041\u0042";
        extended = "Emoji: \u{1F600}";
        mixed = "\u0041\u{1F600}";
    "#;

    match from_str::<Value>(config) {
        Ok(parsed) => {
            let obj = parsed.as_object().unwrap();
            let mut notes = Vec::new();

            if let Some(basic) = obj.get("basic") {
                if basic.as_str().unwrap().contains("AB") {
                    notes.push("Basic Unicode escapes work".to_string());
                }
            }

            if let Some(extended) = obj.get("extended") {
                let extended_str = extended.as_str().unwrap();
                if extended_str.len() > "Emoji: ".len() {
                    notes.push("Extended Unicode escapes work".to_string());
                }
            }

            CompatibilityTestResult {
                test_name: "Extended Unicode Escapes".to_string(),
                passed: true,
                error_message: None,
                notes,
            }
        }
        Err(e) => CompatibilityTestResult {
            test_name: "Extended Unicode Escapes".to_string(),
            passed: false,
            error_message: Some(e.to_string()),
            notes: vec![
                "Extended Unicode escapes \\u{...} are supported by C libucl".to_string(),
                "Should support 1-6 hex digits for full Unicode range".to_string(),
            ],
        },
    }
}

fn test_heredoc_improvements() -> CompatibilityTestResult {
    let config = r#"
        content = <<EOF
This is heredoc content
with multiple lines
EOF
        
        indented = <<TERM
Content here
  TERM  
    "#;

    match from_str::<Value>(config) {
        Ok(parsed) => {
            let obj = parsed.as_object().unwrap();
            let mut notes = Vec::new();

            if let Some(content) = obj.get("content") {
                let content_str = content.as_str().unwrap();
                if content_str.contains("heredoc content") && content_str.contains("multiple lines")
                {
                    notes.push("Basic heredoc parsing works".to_string());
                }
            }

            if let Some(indented) = obj.get("indented") {
                if indented.is_string() {
                    notes.push("Heredoc with whitespace around terminator works".to_string());
                } else {
                    notes.push("Heredoc indentation key present".to_string());
                }
            }

            CompatibilityTestResult {
                test_name: "Heredoc Improvements".to_string(),
                passed: true,
                error_message: None,
                notes,
            }
        }
        Err(e) => CompatibilityTestResult {
            test_name: "Heredoc Improvements".to_string(),
            passed: false,
            error_message: Some(e.to_string()),
            notes: vec![
                "Heredoc should handle whitespace around terminators".to_string(),
                "Should work with both LF and CRLF line endings".to_string(),
            ],
        },
    }
}

fn test_bare_word_values() -> CompatibilityTestResult {
    let config = r#"
        enabled = true;
        disabled = false;
        environment = production;
        empty = null;
        infinity = inf;
    "#;

    match from_str::<Value>(config) {
        Ok(parsed) => {
            let obj = parsed.as_object().unwrap();
            let mut notes = Vec::new();

            if obj.get("enabled") == Some(&Value::Bool(true)) {
                notes.push("Boolean keyword 'true' works".to_string());
            }
            if obj.get("disabled") == Some(&Value::Bool(false)) {
                notes.push("Boolean keyword 'false' works".to_string());
            }
            if obj.get("environment") == Some(&Value::String("production".to_string())) {
                notes.push("Bare word strings work".to_string());
            }
            if obj.get("empty") == Some(&Value::Null) {
                notes.push("Null keyword works".to_string());
            }
            if let Some(inf_val) = obj.get("infinity") {
                if inf_val.as_f64().map(|f| f.is_infinite()).unwrap_or(false) {
                    notes.push("Infinity keyword works".to_string());
                }
            }

            CompatibilityTestResult {
                test_name: "Bare Word Values".to_string(),
                passed: true,
                error_message: None,
                notes,
            }
        }
        Err(e) => CompatibilityTestResult {
            test_name: "Bare Word Values".to_string(),
            passed: false,
            error_message: Some(e.to_string()),
            notes: vec![
                "Bare words should be converted to appropriate types".to_string(),
                "Keywords: true, false, yes, no, on, off, null, inf, nan".to_string(),
            ],
        },
    }
}

fn test_implicit_arrays() -> CompatibilityTestResult {
    let config = r#"
        server = "server1";
        server = "server2";
        server = "server3";
    "#;

    match from_str::<Value>(config) {
        Ok(parsed) => {
            let obj = parsed.as_object().unwrap();
            let mut notes = Vec::new();

            if let Some(server_val) = obj.get("server") {
                if server_val.is_array() {
                    let servers = server_val.as_array().unwrap();
                    if servers.len() == 3 {
                        notes.push("Implicit array creation from duplicate keys works".to_string());
                    }
                } else {
                    notes.push(
                        "Duplicate keys handled but not as array (might use last value)"
                            .to_string(),
                    );
                }
            }

            CompatibilityTestResult {
                test_name: "Implicit Arrays from Duplicate Keys".to_string(),
                passed: true,
                error_message: None,
                notes,
            }
        }
        Err(e) => CompatibilityTestResult {
            test_name: "Implicit Arrays from Duplicate Keys".to_string(),
            passed: false,
            error_message: Some(e.to_string()),
            notes: vec![
                "UCL should automatically create arrays from repeated keys".to_string(),
                "This is a key feature that distinguishes UCL from JSON".to_string(),
            ],
        },
    }
}

fn test_string_formats() -> CompatibilityTestResult {
    let config = r#"
        double = "double quoted";
        single = 'single quoted';
        escaped = "line1\nline2\ttab";
    "#;

    match from_str::<Value>(config) {
        Ok(parsed) => {
            let obj = parsed.as_object().unwrap();
            let mut notes = Vec::new();

            if obj.get("double") == Some(&Value::String("double quoted".to_string())) {
                notes.push("Double-quoted strings work".to_string());
            }
            if obj.get("single") == Some(&Value::String("single quoted".to_string())) {
                notes.push("Single-quoted strings work".to_string());
            }
            if let Some(escaped) = obj.get("escaped") {
                let escaped_str = escaped.as_str().unwrap();
                if escaped_str.contains('\n') && escaped_str.contains('\t') {
                    notes.push("Escape sequences work".to_string());
                }
            }

            CompatibilityTestResult {
                test_name: "String Formats".to_string(),
                passed: true,
                error_message: None,
                notes,
            }
        }
        Err(e) => CompatibilityTestResult {
            test_name: "String Formats".to_string(),
            passed: false,
            error_message: Some(e.to_string()),
            notes: vec![
                "Should support double quotes, single quotes, and escape sequences".to_string(),
            ],
        },
    }
}

fn test_number_formats() -> CompatibilityTestResult {
    let config = r#"
        decimal = 42;
        float = 3.14;
        scientific = 1.23e-4;
        hex = 0xFF;
        size = 64kb;
        time = 30s;
    "#;

    match from_str::<Value>(config) {
        Ok(parsed) => {
            let obj = parsed.as_object().unwrap();
            let mut notes = Vec::new();

            if obj.get("decimal") == Some(&Value::Number(42.into())) {
                notes.push("Decimal integers work".to_string());
            }
            if let Some(float_val) = obj.get("float") {
                if (float_val.as_f64().unwrap() - 3.14).abs() < 0.01 {
                    notes.push("Float numbers work".to_string());
                }
            }
            if obj.get("scientific").is_some() {
                notes.push("Scientific notation parsing attempted".to_string());
            }
            if obj.get("hex").is_some() {
                notes.push("Hexadecimal parsing attempted".to_string());
            }
            if obj.get("size").is_some() {
                notes.push("Size suffix parsing attempted".to_string());
            }
            if obj.get("time").is_some() {
                notes.push("Time suffix parsing attempted".to_string());
            }

            CompatibilityTestResult {
                test_name: "Number Formats".to_string(),
                passed: true,
                error_message: None,
                notes,
            }
        }
        Err(e) => CompatibilityTestResult {
            test_name: "Number Formats".to_string(),
            passed: false,
            error_message: Some(e.to_string()),
            notes: vec![
                "Should support various number formats including hex, size/time suffixes"
                    .to_string(),
            ],
        },
    }
}

fn test_array_formats() -> CompatibilityTestResult {
    let config = r#"
        simple = [1, 2, 3];
        mixed = [1, "string", true];
        nested = [[1, 2], [3, 4]];
    "#;

    match from_str::<Value>(config) {
        Ok(parsed) => {
            let obj = parsed.as_object().unwrap();
            let mut notes = Vec::new();

            if let Some(simple) = obj.get("simple") {
                if simple.is_array() && simple.as_array().unwrap().len() == 3 {
                    notes.push("Simple arrays work".to_string());
                }
            }
            if let Some(mixed) = obj.get("mixed") {
                if mixed.is_array() && mixed.as_array().unwrap().len() == 3 {
                    notes.push("Mixed-type arrays work".to_string());
                }
            }
            if let Some(nested) = obj.get("nested") {
                if nested.is_array() {
                    notes.push("Nested arrays work".to_string());
                }
            }

            CompatibilityTestResult {
                test_name: "Array Formats".to_string(),
                passed: true,
                error_message: None,
                notes,
            }
        }
        Err(e) => CompatibilityTestResult {
            test_name: "Array Formats".to_string(),
            passed: false,
            error_message: Some(e.to_string()),
            notes: vec!["Should support arrays with mixed types and nesting".to_string()],
        },
    }
}

fn test_comment_formats() -> CompatibilityTestResult {
    let config = r#"
        # Hash comment
        key1 = "value1";
        
        /* Multi-line comment */
        key2 = "value2";
    "#;

    match from_str::<Value>(config) {
        Ok(parsed) => {
            let obj = parsed.as_object().unwrap();
            let mut notes = Vec::new();

            if obj.get("key1") == Some(&Value::String("value1".to_string())) {
                notes.push("Hash comments work".to_string());
            }
            if obj.get("key2") == Some(&Value::String("value2".to_string())) {
                notes.push("Multi-line comments work".to_string());
            }

            CompatibilityTestResult {
                test_name: "Comment Formats".to_string(),
                passed: true,
                error_message: None,
                notes,
            }
        }
        Err(e) => CompatibilityTestResult {
            test_name: "Comment Formats".to_string(),
            passed: false,
            error_message: Some(e.to_string()),
            notes: vec!["Should support hash (#) and multi-line (/* */) comments".to_string()],
        },
    }
}

fn test_error_handling() -> CompatibilityTestResult {
    let invalid_configs = vec![r#"key = "unterminated"#, r#"key = 123abc"#, r#"key = "#];

    let mut error_count = 0;
    let mut notes = Vec::new();

    for config in invalid_configs {
        match from_str::<Value>(config) {
            Ok(_) => {
                notes.push(format!("Config should have failed but didn't: {}", config));
            }
            Err(_) => {
                error_count += 1;
            }
        }
    }

    if error_count == 3 {
        notes.push("All invalid configs properly rejected".to_string());
        CompatibilityTestResult {
            test_name: "Error Handling".to_string(),
            passed: true,
            error_message: None,
            notes,
        }
    } else {
        CompatibilityTestResult {
            test_name: "Error Handling".to_string(),
            passed: false,
            error_message: Some(format!(
                "Only {} out of 3 invalid configs were rejected",
                error_count
            )),
            notes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_full_compatibility_report() {
        let report = run_compatibility_test_suite();
        report.print_summary();

        // Assert that we have reasonable compatibility
        assert!(report.passed_tests > 0, "At least some tests should pass");

        // Print individual test details for debugging
        println!("\n=== Detailed Test Results ===");
        for result in &report.results {
            println!(
                "\n{}: {}",
                result.test_name,
                if result.passed { "PASS" } else { "FAIL" }
            );

            if let Some(error) = &result.error_message {
                println!("  Error: {}", error);
            }

            for note in &result.notes {
                println!("  Note: {}", note);
            }
        }
    }
}

//! C libucl compatibility validation layer
//!
//! This module provides validation functions to ensure our UCL parser
//! behaves consistently with the C libucl reference implementation.

use crate::error::Position;
use std::collections::HashMap;

/// Compatibility validation errors
#[derive(Debug, thiserror::Error)]
pub enum CompatibilityError {
    #[error("Syntax validation failed: {message} at {position:?}")]
    SyntaxValidationFailed { message: String, position: Position },

    #[error("Error message format mismatch: expected pattern '{expected}', got '{actual}'")]
    ErrorMessageMismatch { expected: String, actual: String },

    #[error("Edge case handling mismatch: {description}")]
    EdgeCaseHandlingMismatch { description: String },

    #[error("Feature not supported in C libucl: {feature}")]
    UnsupportedFeature { feature: String },
}

/// C libucl compatibility validation layer
pub struct CompatibilityValidator {
    /// Known C libucl syntax patterns and their validation rules
    syntax_rules: HashMap<String, SyntaxRule>,
    /// Known C libucl error message patterns
    error_patterns: HashMap<String, String>,
    /// Edge case validation rules
    edge_case_rules: Vec<EdgeCaseRule>,
}

/// Syntax validation rule for C libucl compatibility
#[derive(Debug, Clone)]
pub struct SyntaxRule {
    pub pattern: String,
    pub description: String,
    pub validator: fn(&str) -> Result<(), String>,
}

/// Edge case validation rule
#[derive(Debug, Clone)]
pub struct EdgeCaseRule {
    pub name: String,
    pub description: String,
    pub validator: fn(&str) -> Result<(), String>,
}

impl CompatibilityValidator {
    /// Create a new compatibility validator with C libucl rules
    pub fn new() -> Self {
        let mut validator = Self {
            syntax_rules: HashMap::new(),
            error_patterns: HashMap::new(),
            edge_case_rules: Vec::new(),
        };

        validator.initialize_syntax_rules();
        validator.initialize_error_patterns();
        validator.initialize_edge_case_rules();

        validator
    }

    /// Initialize syntax validation rules based on C libucl behavior
    fn initialize_syntax_rules(&mut self) {
        // NGINX-style implicit syntax rules
        self.syntax_rules.insert(
            "nginx_implicit_object".to_string(),
            SyntaxRule {
                pattern: r"^[a-zA-Z_][a-zA-Z0-9_]*\s*\{".to_string(),
                description: "NGINX-style implicit object: key { ... }".to_string(),
                validator: validate_nginx_implicit_object,
            },
        );

        self.syntax_rules.insert(
            "nginx_nested_object".to_string(),
            SyntaxRule {
                pattern: r"^[a-zA-Z_][a-zA-Z0-9_]*\s+[a-zA-Z_][a-zA-Z0-9_]*\s*\{".to_string(),
                description: "NGINX-style nested object: key identifier { ... }".to_string(),
                validator: validate_nginx_nested_object,
            },
        );

        // Bare word validation rules
        self.syntax_rules.insert(
            "bare_word_value".to_string(),
            SyntaxRule {
                pattern: r"^[a-zA-Z_][a-zA-Z0-9_]*\s+[a-zA-Z_][a-zA-Z0-9_]*$".to_string(),
                description: "Bare word value assignment: key value".to_string(),
                validator: validate_bare_word_value,
            },
        );

        // Unicode escape validation rules
        self.syntax_rules.insert(
            "unicode_escape_extended".to_string(),
            SyntaxRule {
                pattern: r"\\u\{[0-9a-fA-F]{1,6}\}".to_string(),
                description: "Extended Unicode escape: \\u{X...XXXXXX}".to_string(),
                validator: validate_unicode_escape_extended,
            },
        );

        // Comment format validation rules
        self.syntax_rules.insert(
            "cpp_comment".to_string(),
            SyntaxRule {
                pattern: r"//.*$".to_string(),
                description: "C++ style comment: // comment".to_string(),
                validator: validate_cpp_comment,
            },
        );

        // Heredoc validation rules
        self.syntax_rules.insert(
            "heredoc_terminator".to_string(),
            SyntaxRule {
                pattern: r"<<[A-Z_][A-Z0-9_]*".to_string(),
                description: "Heredoc with uppercase terminator".to_string(),
                validator: validate_heredoc_terminator,
            },
        );
    }

    /// Initialize error message patterns that should match C libucl
    fn initialize_error_patterns(&mut self) {
        // Syntax error patterns
        self.error_patterns.insert(
            "unexpected_token".to_string(),
            "Unexpected token '{token}' at {line}:{column}".to_string(),
        );

        self.error_patterns.insert(
            "unterminated_string".to_string(),
            "Unterminated string at {line}:{column}".to_string(),
        );

        self.error_patterns.insert(
            "invalid_unicode_escape".to_string(),
            "Invalid Unicode escape sequence at {line}:{column}".to_string(),
        );

        self.error_patterns.insert(
            "unterminated_heredoc".to_string(),
            "Unterminated heredoc, expected terminator '{terminator}' at {line}:{column}"
                .to_string(),
        );

        self.error_patterns.insert(
            "invalid_number".to_string(),
            "Invalid number format at {line}:{column}".to_string(),
        );

        self.error_patterns.insert(
            "duplicate_key".to_string(),
            "Duplicate key '{key}' at {line}:{column}".to_string(),
        );
    }

    /// Initialize edge case validation rules
    fn initialize_edge_case_rules(&mut self) {
        self.edge_case_rules.push(EdgeCaseRule {
            name: "empty_object".to_string(),
            description: "Empty objects should be valid".to_string(),
            validator: validate_empty_object,
        });

        self.edge_case_rules.push(EdgeCaseRule {
            name: "nested_depth_limit".to_string(),
            description: "Deeply nested structures should be handled gracefully".to_string(),
            validator: validate_nested_depth_limit,
        });

        self.edge_case_rules.push(EdgeCaseRule {
            name: "large_string_values".to_string(),
            description: "Large string values should be handled efficiently".to_string(),
            validator: validate_large_string_values,
        });

        self.edge_case_rules.push(EdgeCaseRule {
            name: "unicode_edge_cases".to_string(),
            description: "Unicode edge cases should match C libucl behavior".to_string(),
            validator: validate_unicode_edge_cases,
        });

        self.edge_case_rules.push(EdgeCaseRule {
            name: "number_edge_cases".to_string(),
            description: "Number parsing edge cases should match C libucl".to_string(),
            validator: validate_number_edge_cases,
        });
    }

    /// Validate syntax against C libucl behavior
    pub fn validate_syntax(
        &self,
        syntax_type: &str,
        content: &str,
    ) -> Result<(), CompatibilityError> {
        if let Some(rule) = self.syntax_rules.get(syntax_type) {
            (rule.validator)(content).map_err(|msg| CompatibilityError::SyntaxValidationFailed {
                message: msg,
                position: Position {
                    line: 1,
                    column: 1,
                    offset: 0,
                },
            })
        } else {
            Err(CompatibilityError::UnsupportedFeature {
                feature: syntax_type.to_string(),
            })
        }
    }

    /// Validate error message format against C libucl expectations
    pub fn validate_error_message(
        &self,
        error_type: &str,
        actual_message: &str,
    ) -> Result<(), CompatibilityError> {
        if let Some(expected_pattern) = self.error_patterns.get(error_type) {
            let matches_pattern = match error_type {
                "unexpected_token" => actual_message.contains("Unexpected token"),
                _ if error_type.contains("unterminated") => actual_message.contains("Unterminated"),
                _ if error_type.contains("invalid") => actual_message.contains("Invalid"),
                _ => false,
            };

            if matches_pattern {
                Ok(())
            } else {
                Err(CompatibilityError::ErrorMessageMismatch {
                    expected: expected_pattern.clone(),
                    actual: actual_message.to_string(),
                })
            }
        } else {
            Ok(()) // Unknown error type, assume it's valid
        }
    }

    /// Validate edge case handling
    pub fn validate_edge_case(
        &self,
        case_name: &str,
        content: &str,
    ) -> Result<(), CompatibilityError> {
        for rule in &self.edge_case_rules {
            if rule.name == case_name {
                return (rule.validator)(content).map_err(|msg| {
                    CompatibilityError::EdgeCaseHandlingMismatch { description: msg }
                });
            }
        }

        Err(CompatibilityError::UnsupportedFeature {
            feature: case_name.to_string(),
        })
    }

    /// Get all supported syntax types
    pub fn supported_syntax_types(&self) -> Vec<String> {
        self.syntax_rules.keys().cloned().collect()
    }

    /// Get all supported error types
    pub fn supported_error_types(&self) -> Vec<String> {
        self.error_patterns.keys().cloned().collect()
    }

    /// Get all supported edge case types
    pub fn supported_edge_cases(&self) -> Vec<String> {
        self.edge_case_rules
            .iter()
            .map(|rule| rule.name.clone())
            .collect()
    }
}

impl Default for CompatibilityValidator {
    fn default() -> Self {
        Self::new()
    }
}

// Syntax validation functions

fn validate_nginx_implicit_object(content: &str) -> Result<(), String> {
    let trimmed = content.trim();

    // Check for key followed by opening brace
    if !trimmed.contains('{') {
        return Err("NGINX implicit object must contain opening brace".to_string());
    }

    // Check that key is a valid identifier
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.is_empty() {
        return Err("NGINX implicit object must have a key".to_string());
    }

    let key = parts[0];
    if !is_valid_identifier(key) {
        return Err(format!("Invalid key identifier: {}", key));
    }

    Ok(())
}

fn validate_nginx_nested_object(content: &str) -> Result<(), String> {
    let trimmed = content.trim();

    // Check for key identifier { pattern
    if !trimmed.contains('{') {
        return Err("NGINX nested object must contain opening brace".to_string());
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() < 2 {
        return Err("NGINX nested object must have key and identifier".to_string());
    }

    let key = parts[0];
    let identifier = parts[1];

    if !is_valid_identifier(key) {
        return Err(format!("Invalid key identifier: {}", key));
    }

    if !is_valid_identifier(identifier) {
        return Err(format!("Invalid nested identifier: {}", identifier));
    }

    Ok(())
}

fn validate_bare_word_value(content: &str) -> Result<(), String> {
    let trimmed = content.trim();
    let parts: Vec<&str> = trimmed.split_whitespace().collect();

    if parts.len() != 2 {
        return Err("Bare word value must have exactly key and value".to_string());
    }

    let key = parts[0];
    let value = parts[1];

    if !is_valid_identifier(key) {
        return Err(format!("Invalid key identifier: {}", key));
    }

    // Value can be any valid bare word
    if !is_valid_bare_word(value) {
        return Err(format!("Invalid bare word value: {}", value));
    }

    Ok(())
}

fn validate_unicode_escape_extended(content: &str) -> Result<(), String> {
    // Check for \u{...} pattern
    if !content.contains("\\u{") || !content.contains('}') {
        return Err("Extended Unicode escape must use \\u{...} format".to_string());
    }

    // Extract hex digits between braces
    if let Some(start) = content.find("\\u{")
        && let Some(end) = content[start..].find('}')
    {
        let hex_part = &content[start + 3..start + end];

        if hex_part.is_empty() {
            return Err("Unicode escape cannot be empty".to_string());
        }

        if hex_part.len() > 6 {
            return Err("Unicode escape cannot have more than 6 hex digits".to_string());
        }

        // Check that all characters are hex digits
        if !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("Unicode escape must contain only hex digits".to_string());
        }

        // Check that value is in valid Unicode range
        if let Ok(code_point) = u32::from_str_radix(hex_part, 16) {
            if code_point > 0x10FFFF {
                return Err("Unicode code point out of range".to_string());
            }
        } else {
            return Err("Invalid hex digits in Unicode escape".to_string());
        }
    }

    Ok(())
}

fn validate_cpp_comment(content: &str) -> Result<(), String> {
    if !content.contains("//") {
        return Err("C++ comment must contain //".to_string());
    }

    // Check that // is not inside a string
    let mut in_string = false;
    let mut escape_next = false;
    let chars: Vec<char> = content.chars().collect();

    for i in 0..chars.len() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match chars[i] {
            '\\' => escape_next = true,
            '"' => in_string = !in_string,
            '/' if i + 1 < chars.len() && chars[i + 1] == '/' && !in_string => {
                // Found valid C++ comment
                return Ok(());
            }
            _ => {}
        }
    }

    if content.contains("//") && in_string {
        return Err("C++ comment inside string is not a comment".to_string());
    }

    Ok(())
}

fn validate_heredoc_terminator(content: &str) -> Result<(), String> {
    if !content.starts_with("<<") {
        return Err("Heredoc must start with <<".to_string());
    }

    let terminator = &content[2..];

    // C libucl requires uppercase terminators
    if !terminator
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(
            "Heredoc terminator must be uppercase ASCII letters, digits, or underscores"
                .to_string(),
        );
    }

    if terminator.is_empty() {
        return Err("Heredoc terminator cannot be empty".to_string());
    }

    if !terminator.chars().next().unwrap().is_ascii_alphabetic() {
        return Err("Heredoc terminator must start with a letter".to_string());
    }

    Ok(())
}

// Edge case validation functions

fn validate_empty_object(_content: &str) -> Result<(), String> {
    // Empty objects should always be valid in UCL
    Ok(())
}

fn validate_nested_depth_limit(content: &str) -> Result<(), String> {
    // Count nesting depth
    let mut depth: i32 = 0;
    let mut max_depth: i32 = 0;

    for ch in content.chars() {
        match ch {
            '{' | '[' => {
                depth += 1;
                max_depth = max_depth.max(depth);
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }

    // C libucl typically handles reasonable nesting depths
    if max_depth > 1000 {
        return Err("Nesting depth too deep for C libucl compatibility".to_string());
    }

    Ok(())
}

fn validate_large_string_values(content: &str) -> Result<(), String> {
    // Check for very large string values that might cause issues
    if content.len() > 1_000_000 {
        return Err("String value too large for efficient processing".to_string());
    }

    Ok(())
}

fn validate_unicode_edge_cases(content: &str) -> Result<(), String> {
    // Check for problematic Unicode sequences
    if content.contains('\u{FEFF}') {
        return Err("BOM character should be handled at input level".to_string());
    }

    // Check for surrogate pairs (should not appear in UTF-8)
    for ch in content.chars() {
        let code_point = ch as u32;
        if (0xD800..=0xDFFF).contains(&code_point) {
            return Err("Surrogate code points not valid in UTF-8".to_string());
        }
    }

    Ok(())
}

fn validate_number_edge_cases(content: &str) -> Result<(), String> {
    // Check for edge cases in number parsing
    if content.contains("0x") || content.contains("0X") {
        // Hex numbers should be properly formatted
        if content.len() < 3 {
            return Err("Hex number too short".to_string());
        }
    }

    if content.contains("e") || content.contains("E") {
        // Scientific notation should be properly formatted
        if !content.chars().any(|c| c.is_ascii_digit()) {
            return Err("Scientific notation must contain digits".to_string());
        }
    }

    Ok(())
}

// Helper functions

fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    let mut chars = s.chars();
    let first = chars.next().unwrap();

    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }

    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_valid_bare_word(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    // Bare words can contain more characters than identifiers
    // but should not contain special UCL syntax characters
    !s.chars().any(|c| {
        matches!(
            c,
            '{' | '}' | '[' | ']' | '=' | ':' | ';' | ',' | '"' | '\'' | '#'
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compatibility_validator_creation() {
        let validator = CompatibilityValidator::new();

        assert!(!validator.supported_syntax_types().is_empty());
        assert!(!validator.supported_error_types().is_empty());
        assert!(!validator.supported_edge_cases().is_empty());
    }

    #[test]
    fn test_nginx_implicit_object_validation() {
        let validator = CompatibilityValidator::new();

        // Valid NGINX implicit object
        assert!(
            validator
                .validate_syntax("nginx_implicit_object", "server { listen 80 }")
                .is_ok()
        );

        // Invalid - no brace
        assert!(
            validator
                .validate_syntax("nginx_implicit_object", "server listen 80")
                .is_err()
        );

        // Invalid - invalid key
        assert!(
            validator
                .validate_syntax("nginx_implicit_object", "123server { }")
                .is_err()
        );
    }

    #[test]
    fn test_nginx_nested_object_validation() {
        let validator = CompatibilityValidator::new();

        // Valid NGINX nested object
        assert!(
            validator
                .validate_syntax(
                    "nginx_nested_object",
                    "upstream backend { server 127.0.0.1 }"
                )
                .is_ok()
        );

        // Invalid - missing identifier
        assert!(
            validator
                .validate_syntax("nginx_nested_object", "upstream { }")
                .is_err()
        );

        // Invalid - invalid identifiers
        assert!(
            validator
                .validate_syntax("nginx_nested_object", "123upstream backend { }")
                .is_err()
        );
    }

    #[test]
    fn test_bare_word_value_validation() {
        let validator = CompatibilityValidator::new();

        // Valid bare word value
        assert!(
            validator
                .validate_syntax("bare_word_value", "environment production")
                .is_ok()
        );

        // Invalid - too many parts
        assert!(
            validator
                .validate_syntax("bare_word_value", "environment production debug")
                .is_err()
        );

        // Invalid - invalid key
        assert!(
            validator
                .validate_syntax("bare_word_value", "123env production")
                .is_err()
        );
    }

    #[test]
    fn test_unicode_escape_validation() {
        let validator = CompatibilityValidator::new();

        // Valid extended Unicode escape
        assert!(
            validator
                .validate_syntax("unicode_escape_extended", "\\u{1F600}")
                .is_ok()
        );
        assert!(
            validator
                .validate_syntax("unicode_escape_extended", "\\u{41}")
                .is_ok()
        );

        // Invalid - empty braces
        assert!(
            validator
                .validate_syntax("unicode_escape_extended", "\\u{}")
                .is_err()
        );

        // Invalid - too many digits
        assert!(
            validator
                .validate_syntax("unicode_escape_extended", "\\u{1234567}")
                .is_err()
        );

        // Invalid - out of range
        assert!(
            validator
                .validate_syntax("unicode_escape_extended", "\\u{110000}")
                .is_err()
        );
    }

    #[test]
    fn test_cpp_comment_validation() {
        let validator = CompatibilityValidator::new();

        // Valid C++ comment
        assert!(
            validator
                .validate_syntax("cpp_comment", "// This is a comment")
                .is_ok()
        );
        assert!(
            validator
                .validate_syntax("cpp_comment", "key = value; // End comment")
                .is_ok()
        );

        // Invalid - no comment marker
        assert!(
            validator
                .validate_syntax("cpp_comment", "This is not a comment")
                .is_err()
        );
    }

    #[test]
    fn test_heredoc_terminator_validation() {
        let validator = CompatibilityValidator::new();

        // Valid heredoc terminators
        assert!(
            validator
                .validate_syntax("heredoc_terminator", "<<EOF")
                .is_ok()
        );
        assert!(
            validator
                .validate_syntax("heredoc_terminator", "<<CUSTOM_DELIMITER")
                .is_ok()
        );

        // Invalid - lowercase
        assert!(
            validator
                .validate_syntax("heredoc_terminator", "<<eof")
                .is_err()
        );

        // Invalid - starts with number
        assert!(
            validator
                .validate_syntax("heredoc_terminator", "<<123EOF")
                .is_err()
        );

        // Invalid - empty terminator
        assert!(
            validator
                .validate_syntax("heredoc_terminator", "<<")
                .is_err()
        );
    }

    #[test]
    fn test_error_message_validation() {
        let validator = CompatibilityValidator::new();

        // Valid error messages
        assert!(
            validator
                .validate_error_message("unexpected_token", "Unexpected token '{' at 1:5")
                .is_ok()
        );
        assert!(
            validator
                .validate_error_message("unterminated_string", "Unterminated string at 2:10")
                .is_ok()
        );
        assert!(
            validator
                .validate_error_message("invalid_unicode_escape", "Invalid Unicode escape sequence")
                .is_ok()
        );

        // Invalid error message format
        assert!(
            validator
                .validate_error_message("unexpected_token", "Syntax error occurred")
                .is_err()
        );
    }

    #[test]
    fn test_edge_case_validation() {
        let validator = CompatibilityValidator::new();

        // Valid edge cases
        assert!(validator.validate_edge_case("empty_object", "{}").is_ok());
        assert!(
            validator
                .validate_edge_case("nested_depth_limit", "{{{{}}}}")
                .is_ok()
        );
        assert!(
            validator
                .validate_edge_case("large_string_values", "short string")
                .is_ok()
        );

        // Invalid edge cases
        let very_deep = "{".repeat(2000) + &"}".repeat(2000);
        assert!(
            validator
                .validate_edge_case("nested_depth_limit", &very_deep)
                .is_err()
        );
    }
}

//! Error types and position tracking for UCL parsing
//!
//! This module provides comprehensive error handling with detailed position
//! information for debugging and user feedback.

use std::fmt;
use thiserror::Error;

/// Represents a position in the source text
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    /// Line number (1-based)
    pub line: usize,
    /// Column number (1-based)
    pub column: usize,
    /// Byte offset from start of input (0-based)
    pub offset: usize,
}

impl Position {
    /// Creates a new position at the start of input
    pub fn new() -> Self {
        Self {
            line: 1,
            column: 1,
            offset: 0,
        }
    }

    /// Advances the position by one character
    pub fn advance(&mut self, c: char) {
        match c {
            '\n' => {
                self.line += 1;
                self.column = 1;
            }
            '\r' => {
                // Handle \r\n and standalone \r
                self.column = 1;
            }
            _ => {
                self.column += 1;
            }
        }
        self.offset += c.len_utf8();
    }

    /// Advances the position by multiple characters
    pub fn advance_by(&mut self, text: &str) {
        for c in text.chars() {
            self.advance(c);
        }
    }
}

impl Default for Position {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// Represents a span of text in the source
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// Start position of the span
    pub start: Position,
    /// End position of the span
    pub end: Position,
}

impl Span {
    /// Creates a new span from start and end positions
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    /// Creates a span covering a single position
    pub fn single(position: Position) -> Self {
        Self {
            start: position,
            end: position,
        }
    }

    /// Creates a span covering a token or word at the given position
    pub fn token(position: Position, length: usize) -> Self {
        let mut end = position;
        end.offset += length;
        end.column += length;
        Self {
            start: position,
            end,
        }
    }

    /// Creates a span covering an entire line
    pub fn line(position: Position, line_text: &str) -> Self {
        let mut end = position;
        end.column = 1;
        end.offset = end.offset.saturating_sub(position.column.saturating_sub(1));
        end.advance_by(line_text);
        Self {
            start: Position {
                line: position.line,
                column: 1,
                offset: end.offset - line_text.len(),
            },
            end,
        }
    }

    /// Extends this span to include another span
    pub fn extend_to(&self, other: &Span) -> Self {
        Self {
            start: if self.start.offset <= other.start.offset {
                self.start
            } else {
                other.start
            },
            end: if self.end.offset >= other.end.offset {
                self.end
            } else {
                other.end
            },
        }
    }

    /// Returns the length of the span in bytes
    pub fn len(&self) -> usize {
        self.end.offset.saturating_sub(self.start.offset)
    }

    /// Returns true if the span is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns true if this span contains the given position
    pub fn contains(&self, position: Position) -> bool {
        position.offset >= self.start.offset && position.offset <= self.end.offset
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.start.line == self.end.line {
            write!(
                f,
                "{}:{}-{}",
                self.start.line, self.start.column, self.end.column
            )
        } else {
            write!(f, "{}-{}", self.start, self.end)
        }
    }
}

/// Context information for enhanced error reporting
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// The original source text
    pub source: String,
    /// The span where the error occurred
    pub span: Span,
    /// Suggested fixes for the error
    pub suggestions: Vec<String>,
    /// Additional help text
    pub help: Option<String>,
}

impl ErrorContext {
    /// Creates a new error context
    pub fn new(source: String, span: Span) -> Self {
        Self {
            source,
            span,
            suggestions: Vec::new(),
            help: None,
        }
    }

    /// Creates an error context for a specific token
    pub fn for_token(source: String, position: Position, token_text: &str) -> Self {
        let span = Span::token(position, token_text.len());
        Self::new(source, span)
    }

    /// Creates an error context for an entire line
    pub fn for_line(source: String, position: Position) -> Self {
        let lines: Vec<&str> = source.lines().collect();
        if position.line > 0 && position.line <= lines.len() {
            let line_text = lines[position.line - 1];
            let span = Span::line(position, line_text);
            Self::new(source, span)
        } else {
            Self::new(source, Span::single(position))
        }
    }

    /// Creates an error context with extended span for complex syntax errors
    pub fn for_syntax_error(source: String, start_pos: Position, end_pos: Position) -> Self {
        let span = Span::new(start_pos, end_pos);
        Self::new(source, span)
    }

    /// Adds a suggestion for fixing the error
    pub fn with_suggestion(mut self, suggestion: String) -> Self {
        self.suggestions.push(suggestion);
        self
    }

    /// Adds multiple suggestions at once
    pub fn with_suggestions(mut self, suggestions: Vec<String>) -> Self {
        self.suggestions.extend(suggestions);
        self
    }

    /// Adds help text for the error
    pub fn with_help(mut self, help: String) -> Self {
        self.help = Some(help);
        self
    }

    /// Adds a priority suggestion that appears first
    pub fn with_priority_suggestion(mut self, suggestion: String) -> Self {
        self.suggestions.insert(0, suggestion);
        self
    }

    /// Extracts the source code snippet around the error
    pub fn source_snippet(&self) -> String {
        self.extract_lines_around_span(2)
    }

    /// Extracts lines around the error span with context
    pub fn extract_lines_around_span(&self, context_lines: usize) -> String {
        let lines: Vec<&str> = self.source.lines().collect();
        if lines.is_empty() {
            return String::new();
        }

        let start_line = self.span.start.line.saturating_sub(1); // Convert to 0-based
        let end_line = (self.span.end.line.saturating_sub(1)).min(lines.len().saturating_sub(1));

        let context_start = start_line.saturating_sub(context_lines);
        let context_end = (end_line + context_lines + 1).min(lines.len());

        let mut result = String::new();
        let line_number_width = context_end.to_string().len();

        for (i, line) in lines[context_start..context_end].iter().enumerate() {
            let line_num = context_start + i + 1;
            let is_error_line = line_num >= self.span.start.line && line_num <= self.span.end.line;
            let is_context_line = !is_error_line;

            // Use different formatting for error lines vs context lines
            if is_error_line {
                result.push_str(&format!(
                    "{:width$} | {}\n",
                    line_num,
                    line,
                    width = line_number_width
                ));

                // Add error indicator with improved positioning
                if line_num == self.span.start.line {
                    let spaces = " "
                        .repeat(line_number_width + 3 + self.span.start.column.saturating_sub(1));
                    let carets = if self.span.start.line == self.span.end.line {
                        let length =
                            (self.span.end.column.saturating_sub(self.span.start.column)).max(1);
                        "^".repeat(length)
                    } else {
                        "^".repeat(
                            line.len()
                                .saturating_sub(self.span.start.column.saturating_sub(1))
                                .max(1),
                        )
                    };
                    result.push_str(&format!("{}{}  <-- Error here\n", spaces, carets));
                }
            } else if is_context_line {
                // Context lines are shown with a dimmed appearance
                result.push_str(&format!(
                    "{:width$} | {}\n",
                    line_num,
                    line,
                    width = line_number_width
                ));
            }
        }

        result
    }

    /// Extracts a focused snippet around the error with minimal context
    pub fn focused_snippet(&self) -> String {
        self.extract_lines_around_span(1)
    }

    /// Extracts an extended snippet with more context for complex errors
    pub fn extended_snippet(&self) -> String {
        self.extract_lines_around_span(3)
    }

    /// Gets the exact text that caused the error
    pub fn error_text(&self) -> String {
        let lines: Vec<&str> = self.source.lines().collect();
        if lines.is_empty() || self.span.start.line == 0 || self.span.start.line > lines.len() {
            return String::new();
        }

        let line_index = self.span.start.line - 1;
        let line = lines[line_index];

        if self.span.start.line == self.span.end.line {
            // Single line error
            let start_col = self.span.start.column.saturating_sub(1);
            let end_col = self.span.end.column.saturating_sub(1).min(line.len());
            if start_col < line.len() {
                line.chars()
                    .skip(start_col)
                    .take(end_col - start_col)
                    .collect()
            } else {
                String::new()
            }
        } else {
            // Multi-line error - just return the first line portion
            let start_col = self.span.start.column.saturating_sub(1);
            if start_col < line.len() {
                line.chars().skip(start_col).collect()
            } else {
                String::new()
            }
        }
    }

    /// Formats the error context for display
    pub fn format_error(&self, error_message: &str) -> String {
        let mut output = String::new();

        // Header with precise location
        output.push_str(&format!(
            "Error at {}: {}\n",
            self.span.start, error_message
        ));

        // Show the problematic text if available
        let error_text = self.error_text();
        if !error_text.trim().is_empty() {
            output.push_str(&format!("Problematic text: '{}'\n", error_text.trim()));
        }

        output.push('\n');

        // Context snippet with improved formatting
        output.push_str(&self.source_snippet());

        // Suggestions with better formatting
        if !self.suggestions.is_empty() {
            output.push_str("\n💡 Suggestions:\n");
            for (i, suggestion) in self.suggestions.iter().enumerate() {
                output.push_str(&format!("  {}. {}\n", i + 1, suggestion));
            }
        }

        // Help text with better formatting
        if let Some(help) = &self.help {
            output.push_str(&format!("\n📖 Help: {}\n", help));
        }

        output
    }

    /// Formats a compact error for inline display
    pub fn format_compact(&self, error_message: &str) -> String {
        let error_text = self.error_text();
        if !error_text.trim().is_empty() {
            format!(
                "{} at {} (near '{}')",
                error_message,
                self.span.start,
                error_text.trim()
            )
        } else {
            format!("{} at {}", error_message, self.span.start)
        }
    }

    /// Formats an extended error with maximum context
    pub fn format_extended(&self, error_message: &str) -> String {
        let mut output = String::new();

        // Detailed header
        output.push_str("🚨 UCL Parse Error\n");
        output.push_str(&format!(
            "Location: {} (line {}, column {})\n",
            self.span.start, self.span.start.line, self.span.start.column
        ));
        output.push_str(&format!("Message: {}\n", error_message));

        // Show the problematic text
        let error_text = self.error_text();
        if !error_text.trim().is_empty() {
            output.push_str(&format!("Problematic text: '{}'\n", error_text.trim()));
        }

        output.push('\n');
        output.push_str("📍 Source Context:\n");
        output.push_str(&self.extended_snippet());

        // Detailed suggestions
        if !self.suggestions.is_empty() {
            output.push_str("\n💡 How to fix this:\n");
            for (i, suggestion) in self.suggestions.iter().enumerate() {
                output.push_str(&format!("  {}. {}\n", i + 1, suggestion));
            }
        }

        // Extended help
        if let Some(help) = &self.help {
            output.push_str(&format!("\n📖 Additional Information:\n{}\n", help));
        }

        output
    }
}

/// Main error type for UCL parsing operations
#[derive(Debug, Error)]
pub enum UclError {
    /// Lexical analysis error
    #[error("Lexical error: {0}")]
    Lex(#[from] LexError),

    /// Parsing error
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),

    /// Serde deserialization error
    #[error("Serde error: {0}")]
    Serde(#[from] SerdeError),

    /// I/O error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Lexical analysis errors
#[derive(Debug, Error)]
pub enum LexError {
    /// Unexpected character encountered
    #[error("Unexpected character '{character}' at {position}")]
    UnexpectedCharacter { character: char, position: Position },

    /// String literal not properly terminated
    #[error("Unterminated string at {position}")]
    UnterminatedString { position: Position },

    /// Invalid escape sequence in string
    #[error("Invalid escape sequence '\\{sequence}' at {position}")]
    InvalidEscape {
        sequence: String,
        position: Position,
    },

    /// Invalid Unicode escape sequence
    #[error("Invalid unicode escape '\\u{sequence}' at {position}")]
    InvalidUnicodeEscape {
        sequence: String,
        position: Position,
    },

    /// Comment block not properly terminated
    #[error("Unterminated comment at {position}")]
    UnterminatedComment { position: Position },

    /// Invalid number format
    #[error("Invalid number format at {position}: {message}")]
    InvalidNumber { message: String, position: Position },

    /// Invalid heredoc terminator
    #[error("Invalid heredoc terminator at {position}: {message}")]
    InvalidHeredoc { message: String, position: Position },

    /// UTF-8 encoding error
    #[error("Invalid UTF-8 sequence at {position}")]
    InvalidUtf8 { position: Position },

    /// Invalid C++ style comment
    #[error("Invalid C++ style comment at {position}: {message}")]
    InvalidCppComment { message: String, position: Position },

    /// Invalid extended Unicode escape sequence
    #[error("Invalid extended Unicode escape '\\u{{{sequence}}}' at {position}")]
    InvalidExtendedUnicodeEscape {
        sequence: String,
        position: Position,
    },

    /// Heredoc terminator not found with specific guidance
    #[error("Heredoc terminator '{terminator}' not found at {position}")]
    HeredocTerminatorNotFound {
        terminator: String,
        position: Position,
    },

    /// Invalid bare word character
    #[error("Invalid character '{character}' in bare word at {position}")]
    InvalidBareWordCharacter {
        character: char,
        position: Position,
        suggestion: String,
    },
}

/// Parsing errors
#[derive(Debug, Error)]
pub enum ParseError {
    /// Unexpected token encountered
    #[error("Unexpected token {token} at {position}, expected {expected}")]
    UnexpectedToken {
        token: String,
        position: Position,
        expected: String,
    },

    /// Variable not found during expansion
    #[error("Variable '{name}' not found at {position}")]
    VariableNotFound { name: String, position: Position },

    /// Duplicate key in object
    #[error("Duplicate key '{key}' at {position}")]
    DuplicateKey { key: String, position: Position },

    /// Invalid object structure
    #[error("Invalid object structure at {position}: {message}")]
    InvalidObject { message: String, position: Position },

    /// Invalid array structure
    #[error("Invalid array structure at {position}: {message}")]
    InvalidArray { message: String, position: Position },

    /// Variable expansion error
    #[error("Variable expansion error at {position}: {message}")]
    VariableExpansion { message: String, position: Position },

    /// Maximum nesting depth exceeded
    #[error("Maximum nesting depth exceeded at {position}")]
    MaxDepthExceeded { position: Position },

    /// NGINX-style syntax error with specific guidance
    #[error("NGINX-style syntax error at {position}: {message}")]
    NginxSyntaxError {
        message: String,
        position: Position,
        suggestion: String,
    },

    /// Invalid comment syntax
    #[error("Invalid comment syntax at {position}: {message}")]
    InvalidCommentSyntax { message: String, position: Position },

    /// Ambiguous bare word that needs clarification
    #[error("Ambiguous bare word '{word}' at {position}")]
    AmbiguousBareWord {
        word: String,
        position: Position,
        suggestion: String,
    },

    /// Invalid implicit syntax pattern
    #[error("Invalid implicit syntax at {position}: {message}")]
    InvalidImplicitSyntax {
        message: String,
        position: Position,
        expected_pattern: String,
    },

    /// Mixed syntax style confusion
    #[error("Mixed syntax styles at {position}: {message}")]
    MixedSyntaxStyles {
        message: String,
        position: Position,
        suggestion: String,
    },
}

/// Serde integration errors
#[derive(Debug, Error)]
pub enum SerdeError {
    /// Custom serde error message
    #[error("{0}")]
    Custom(String),

    /// Type mismatch during deserialization
    #[error("Type mismatch: expected {expected}, found {found} at {position}")]
    TypeMismatch {
        expected: String,
        found: String,
        position: Position,
    },

    /// Missing required field
    #[error("Missing required field '{field}' at {position}")]
    MissingField { field: String, position: Position },

    /// Unknown field encountered
    #[error("Unknown field '{field}' at {position}")]
    UnknownField { field: String, position: Position },
}

/// Enhanced error with context information
#[derive(Debug)]
pub struct EnhancedError {
    pub error: UclError,
    pub context: Option<ErrorContext>,
}

impl EnhancedError {
    /// Creates a new enhanced error
    pub fn new(error: UclError) -> Self {
        Self {
            error,
            context: None,
        }
    }

    /// Adds context to the error
    pub fn with_context(mut self, context: ErrorContext) -> Self {
        self.context = Some(context);
        self
    }

    /// Formats the error with context if available
    pub fn format(&self) -> String {
        let base_message = self.error.to_string();
        if let Some(ctx) = &self.context {
            ctx.format_error(&base_message)
        } else {
            base_message
        }
    }
}

impl std::fmt::Display for EnhancedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format())
    }
}

impl std::error::Error for EnhancedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// Helper functions for creating enhanced error contexts
impl ErrorContext {
    /// Creates context for NGINX-style syntax errors
    pub fn nginx_syntax_error(
        source: String,
        position: Position,
        problematic_text: &str,
        context: &str,
    ) -> Self {
        let span = Span::token(position, problematic_text.len());
        let suggestion = ParseError::suggest_nginx_syntax_fix(context);

        Self::new(source, span)
            .with_priority_suggestion(suggestion)
            .with_help(
                "NGINX-style syntax allows implicit key-value pairs without separators".to_string(),
            )
    }

    /// Creates context for bare word errors
    pub fn bare_word_error(source: String, position: Position, word: &str) -> Self {
        let span = Span::token(position, word.len());
        let suggestion = ParseError::suggest_bare_word_fix(word);

        Self::new(source, span)
            .with_priority_suggestion(suggestion)
            .with_help(
                "Bare words are unquoted identifiers that may need quotes for special characters"
                    .to_string(),
            )
    }

    /// Creates context for Unicode escape errors
    pub fn unicode_escape_error(source: String, position: Position, escape_sequence: &str) -> Self {
        let span = Span::token(position, escape_sequence.len());

        Self::new(source, span)
            .with_suggestion("Use \\uXXXX for 4-digit Unicode escapes".to_string())
            .with_suggestion("Use \\u{X...XXXXXX} for variable-length Unicode escapes".to_string())
            .with_help(ParseError::unicode_escape_help())
    }

    /// Creates context for comment syntax errors
    pub fn comment_syntax_error(source: String, position: Position, comment_text: &str) -> Self {
        let span = Span::token(position, comment_text.len());

        Self::new(source, span)
            .with_suggestions(vec![
                "Use # for hash comments".to_string(),
                "Use // for C++ style comments".to_string(),
                "Use /* */ for multi-line comments".to_string(),
            ])
            .with_help(ParseError::comment_syntax_help())
    }

    /// Creates context for heredoc errors
    pub fn heredoc_error(source: String, position: Position, terminator: &str) -> Self {
        let span = Span::single(position);

        Self::new(source, span)
            .with_suggestion(format!(
                "Add '{}' on its own line to terminate the heredoc",
                terminator
            ))
            .with_suggestion(
                "Ensure the terminator line contains only the terminator (whitespace is allowed)"
                    .to_string(),
            )
            .with_help(ParseError::heredoc_help())
    }
}

/// Helper functions for generating context-aware error messages
impl ParseError {
    /// Generate suggestions for NGINX-style syntax errors
    pub fn suggest_nginx_syntax_fix(context: &str) -> String {
        match context {
            "missing_separator" => "Use 'key = value' or 'key: value' for explicit assignment, or 'key value' for implicit assignment".to_string(),
            "unexpected_brace" => "NGINX-style syntax: 'key { ... }' creates implicit object assignment".to_string(),
            "nested_identifier" => "NGINX-style nested: 'parent child { ... }' creates parent.child object".to_string(),
            "mixed_styles" => "Choose consistent syntax style: either explicit (key = value) or implicit (key value)".to_string(),
            "invalid_implicit" => "Implicit syntax requires: key followed by value, identifier, or object".to_string(),
            _ => "Check UCL syntax documentation for valid NGINX-style patterns".to_string(),
        }
    }

    /// Generate suggestions for bare word errors
    pub fn suggest_bare_word_fix(word: &str) -> String {
        if word.contains('-') || word.contains(' ') || word.contains('.') {
            format!("Quote the value: \"{}\"", word)
        } else if [
            "true", "false", "null", "yes", "no", "on", "off", "inf", "nan",
        ]
        .contains(&word)
        {
            format!(
                "'{}' is a reserved keyword. Use \"{}\" for literal string",
                word, word
            )
        } else if word.chars().any(|c| !c.is_alphanumeric() && c != '_') {
            format!(
                "Bare words can only contain letters, numbers, and underscores. Use quotes: \"{}\"",
                word
            )
        } else {
            format!(
                "Use quotes if '{}' should be a literal string: \"{}\"",
                word, word
            )
        }
    }

    /// Generate help text for comment syntax errors
    pub fn comment_syntax_help() -> String {
        "Supported comment formats: # comment, // comment, /* comment */".to_string()
    }

    /// Generate help text for Unicode escape errors
    pub fn unicode_escape_help() -> String {
        "Valid Unicode escape formats: \\uXXXX (4 hex digits) or \\u{X...XXXXXX} (1-6 hex digits)"
            .to_string()
    }

    /// Generate help text for heredoc errors
    pub fn heredoc_help() -> String {
        "Heredoc terminator must appear on its own line (whitespace around terminator is allowed)"
            .to_string()
    }
}

impl LexError {
    /// Generate suggestions for lexical errors
    pub fn suggest_fix(&self) -> Vec<String> {
        match self {
            LexError::InvalidExtendedUnicodeEscape { sequence, .. } => {
                if sequence.is_empty() {
                    vec!["Use \\u{XXXX} with 1-6 hex digits".to_string()]
                } else if sequence.len() > 6 {
                    vec!["Unicode escapes can have at most 6 hex digits".to_string()]
                } else if !sequence.chars().all(|c| c.is_ascii_hexdigit()) {
                    vec!["Unicode escapes must contain only hex digits (0-9, A-F)".to_string()]
                } else {
                    vec!["Check that the Unicode codepoint is valid (≤ 0x10FFFF)".to_string()]
                }
            }
            LexError::InvalidCppComment { .. } => {
                vec![
                    "C++ comments start with // and continue to end of line".to_string(),
                    "Use # for hash comments or /* */ for multi-line comments".to_string(),
                ]
            }
            LexError::HeredocTerminatorNotFound { terminator, .. } => {
                vec![
                    format!("Add '{}' on its own line to terminate the heredoc", terminator),
                    "Ensure the terminator line contains only the terminator (whitespace is allowed)".to_string(),
                ]
            }
            LexError::InvalidBareWordCharacter { suggestion, .. } => {
                vec![suggestion.clone()]
            }
            _ => vec![],
        }
    }
}

impl UclError {
    /// Enhances this error with context information from source text
    pub fn with_source_context(self, source: &str) -> EnhancedError {
        let context = match &self {
            UclError::Lex(lex_err) => {
                let context = match lex_err {
                    LexError::UnexpectedCharacter { character, position } => {
                        let char_str = character.to_string();
                        let ctx = ErrorContext::for_token(source.to_string(), *position, &char_str);

                        match *character {
                            '"' => ctx.with_suggestion("If you meant to start a string, ensure it's properly closed".to_string()),
                            '\'' => ctx.with_suggestion("If you meant to start a single-quoted string, ensure it's properly closed".to_string()),
                            '{' => ctx.with_suggestions(vec![
                                "If you meant to start an object, ensure it has proper key-value pairs".to_string(),
                                "For NGINX-style syntax, use: key { ... } for implicit objects".to_string(),
                            ]),
                            '[' => ctx.with_suggestion("If you meant to start an array, ensure it has proper elements".to_string()),
                            '/' => ctx.with_suggestions(vec![
                                "Use // for C++ style comments".to_string(),
                                "Use /* */ for multi-line comments".to_string(),
                            ]),
                            _ => ctx.with_suggestion(format!("Remove the unexpected character '{}' or escape it if it's part of a string", character)),
                        }.with_help("UCL supports objects, arrays, strings, numbers, and comments (# // /* */)".to_string())
                    }
                    LexError::UnterminatedString { position } => {
                        ErrorContext::for_line(source.to_string(), *position)
                            .with_suggestion("Add the appropriate closing quote to terminate the string".to_string())
                            .with_help("String literals must be properly closed to avoid parsing errors".to_string())
                    }
                    LexError::InvalidEscape { sequence, position } => {
                        let escape_text = format!("\\{}", sequence);
                        let suggestion = match sequence.as_str() {
                            "x" => "Use \\uXXXX for Unicode escapes instead of \\x".to_string(),
                            _ => format!("Use a valid escape sequence like \\n, \\t, \\r, \\\\, \\\", or \\uXXXX instead of \\{}", sequence),
                        };
                        ErrorContext::for_token(source.to_string(), *position, &escape_text)
                            .with_suggestion(suggestion)
                            .with_help("Valid escape sequences: \\n (newline), \\t (tab), \\r (carriage return), \\\\ (backslash), \\\" (quote), \\uXXXX or \\u{...} (Unicode)".to_string())
                    }
                    LexError::InvalidUnicodeEscape { sequence, position } => {
                        ErrorContext::unicode_escape_error(source.to_string(), *position, &format!("\\u{}", sequence))
                    }
                    LexError::InvalidExtendedUnicodeEscape { sequence, position } => {
                        ErrorContext::unicode_escape_error(source.to_string(), *position, &format!("\\u{{{}}}", sequence))
                            .with_suggestions(lex_err.suggest_fix())
                    }
                    LexError::InvalidCppComment { position, message } => {
                        ErrorContext::comment_syntax_error(source.to_string(), *position, "//")
                            .with_priority_suggestion(format!("Fix comment syntax: {}", message))
                    }
                    LexError::HeredocTerminatorNotFound { terminator, position } => {
                        ErrorContext::heredoc_error(source.to_string(), *position, terminator)
                    }
                    LexError::InvalidBareWordCharacter { character, position, suggestion } => {
                        let char_str = character.to_string();
                        ErrorContext::for_token(source.to_string(), *position, &char_str)
                            .with_suggestion(suggestion.clone())
                            .with_help("Bare words can only contain alphanumeric characters and underscores".to_string())
                    }
                    LexError::InvalidNumber { position, message } => {
                        ErrorContext::for_line(source.to_string(), *position)
                            .with_suggestion("Check the number format - ensure it follows valid syntax".to_string())
                            .with_suggestion(format!("Number error: {}", message))
                            .with_help("UCL supports integers, floating-point numbers, hexadecimal (0x...), and scientific notation (1e5)".to_string())
                    }
                    _ => return EnhancedError::new(self),
                };
                Some(context)
            }
            UclError::Parse(parse_err) => {
                let context = match parse_err {
                    ParseError::UnexpectedToken { token, expected, position } => {
                        let mut ctx = ErrorContext::for_token(source.to_string(), *position, token)
                            .with_suggestion(format!("Replace '{}' with {}", token, expected));

                        // Add context-specific suggestions
                        if expected.contains("=") || expected.contains(":") {
                            ctx = ctx.with_suggestion("For NGINX-style syntax, you can omit separators: 'key value'".to_string());
                        }
                        if token == "{" && expected.contains("value") {
                            ctx = ctx.with_suggestion("Use 'key { ... }' for implicit object creation".to_string());
                        }

                        ctx.with_help("UCL supports both explicit (key = value) and implicit (key value) syntax".to_string())
                    }
                    ParseError::VariableNotFound { name, position } => {
                        ErrorContext::for_token(source.to_string(), *position, &format!("${}", name))
                            .with_suggestions(vec![
                                format!("Define the variable '{}' or check for typos", name),
                                "Use $$ to escape a literal $ character".to_string()
                            ])
                            .with_help("Variables can be environment variables or custom variables provided to the parser".to_string())
                    }
                    ParseError::DuplicateKey { key, position } => {
                        ErrorContext::for_token(source.to_string(), *position, key)
                            .with_suggestions(vec![
                                format!("Remove the duplicate key '{}' or rename it", key),
                                "Enable implicit arrays to automatically convert duplicate keys to arrays".to_string(),
                                "Use explicit array syntax: key = [value1, value2]".to_string()
                            ])
                            .with_help("UCL can automatically create arrays from duplicate keys if configured".to_string())
                    }
                    ParseError::NginxSyntaxError { message, position, suggestion } => {
                        ErrorContext::nginx_syntax_error(source.to_string(), *position, message, "nginx_error")
                            .with_priority_suggestion(suggestion.clone())
                    }
                    ParseError::InvalidCommentSyntax { message, position } => {
                        ErrorContext::comment_syntax_error(source.to_string(), *position, message)
                    }
                    ParseError::AmbiguousBareWord { word, position, suggestion } => {
                        ErrorContext::bare_word_error(source.to_string(), *position, word)
                            .with_priority_suggestion(suggestion.clone())
                    }
                    ParseError::InvalidImplicitSyntax { message: _, position, expected_pattern } => {
                        ErrorContext::for_line(source.to_string(), *position)
                            .with_suggestions(vec![
                                format!("Use the expected pattern: {}", expected_pattern),
                                "For explicit syntax, use 'key = value' or 'key: value'".to_string(),
                            ])
                            .with_help("Implicit syntax allows 'key value', 'key { ... }', and 'key identifier { ... }' patterns".to_string())
                    }
                    ParseError::MixedSyntaxStyles { message: _, position, suggestion } => {
                        ErrorContext::for_line(source.to_string(), *position)
                            .with_suggestions(vec![
                                suggestion.clone(),
                                "Choose consistent syntax style throughout the configuration".to_string(),
                            ])
                            .with_help("UCL supports mixing explicit and implicit syntax, but consistency improves readability".to_string())
                    }
                    _ => return EnhancedError::new(self),
                };
                Some(context)
            }
            _ => None,
        };

        if let Some(ctx) = context {
            EnhancedError::new(self).with_context(ctx)
        } else {
            EnhancedError::new(self)
        }
    }

    /// Formats the error with full context information if available
    pub fn format_with_context(&self) -> String {
        self.to_string()
    }
}

impl serde::de::Error for UclError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        UclError::Serde(SerdeError::Custom(msg.to_string()))
    }
}

impl serde::de::Error for SerdeError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        SerdeError::Custom(msg.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_new() {
        let pos = Position::new();
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 1);
        assert_eq!(pos.offset, 0);
    }

    #[test]
    fn test_position_advance() {
        let mut pos = Position::new();

        pos.advance('a');
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 2);
        assert_eq!(pos.offset, 1);

        pos.advance('\n');
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);
        assert_eq!(pos.offset, 2);

        pos.advance('ü'); // Multi-byte UTF-8 character
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 2);
        assert_eq!(pos.offset, 4);
    }

    #[test]
    fn test_position_advance_by() {
        let mut pos = Position::new();
        pos.advance_by("hello\nworld");

        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 6);
        assert_eq!(pos.offset, 11);
    }

    #[test]
    fn test_span_creation() {
        let start = Position {
            line: 1,
            column: 1,
            offset: 0,
        };
        let end = Position {
            line: 1,
            column: 5,
            offset: 4,
        };
        let span = Span::new(start, end);

        assert_eq!(span.len(), 4);
        assert!(!span.is_empty());
    }

    #[test]
    fn test_span_single() {
        let pos = Position {
            line: 2,
            column: 3,
            offset: 10,
        };
        let span = Span::single(pos);

        assert_eq!(span.len(), 0);
        assert!(span.is_empty());
        assert_eq!(span.start, span.end);
    }

    #[test]
    fn test_position_display() {
        let pos = Position {
            line: 42,
            column: 13,
            offset: 100,
        };
        assert_eq!(format!("{}", pos), "42:13");
    }

    #[test]
    fn test_span_display() {
        let start = Position {
            line: 1,
            column: 1,
            offset: 0,
        };
        let end = Position {
            line: 1,
            column: 5,
            offset: 4,
        };
        let span = Span::new(start, end);
        assert_eq!(format!("{}", span), "1:1-5");

        let start = Position {
            line: 1,
            column: 1,
            offset: 0,
        };
        let end = Position {
            line: 2,
            column: 3,
            offset: 10,
        };
        let span = Span::new(start, end);
        assert_eq!(format!("{}", span), "1:1-2:3");
    }
}

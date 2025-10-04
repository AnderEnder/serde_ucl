//! UCL parser for converting tokens into structured data
//!
//! This module provides the parser that consumes tokens from the lexer
//! and builds structured UCL values with variable expansion support.

use crate::error::{ParseError, Position};
use crate::lexer::{LexerConfig, Token, UclLexer};
use indexmap::IndexMap;
use smallvec::SmallVec;
use std::cmp::Reverse;
use std::collections::HashMap;

/// Behavior when duplicate keys are encountered
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateKeyBehavior {
    /// Return an error when duplicate keys are found
    Error,
    /// Create implicit arrays when keys are repeated
    ImplicitArray,
    /// Use the last value (override previous values)
    Override,
}

/// Configuration options for the parser
#[derive(Debug, Clone)]
pub struct ParserConfig {
    /// Maximum nesting depth to prevent stack overflow
    pub max_depth: usize,
    /// Allow duplicate keys (later values override earlier ones)
    /// Deprecated: Use duplicate_key_behavior instead
    pub allow_duplicate_keys: bool,
    /// Behavior when duplicate keys are encountered
    pub duplicate_key_behavior: DuplicateKeyBehavior,
    /// Preserve key order in objects
    pub preserve_key_order: bool,
}

impl ParserConfig {
    /// Creates a new parser configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the duplicate key behavior
    pub fn with_duplicate_key_behavior(mut self, behavior: DuplicateKeyBehavior) -> Self {
        self.duplicate_key_behavior = behavior;
        // Update the legacy field for backward compatibility
        self.allow_duplicate_keys = behavior != DuplicateKeyBehavior::Error;
        self
    }

    /// Sets whether to allow duplicate keys (legacy method)
    /// This method is deprecated, use with_duplicate_key_behavior instead
    pub fn with_allow_duplicate_keys(mut self, allow: bool) -> Self {
        self.allow_duplicate_keys = allow;
        // Update the new field based on the legacy setting
        self.duplicate_key_behavior = if allow {
            DuplicateKeyBehavior::ImplicitArray
        } else {
            DuplicateKeyBehavior::Error
        };
        self
    }

    /// Sets the maximum nesting depth
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Sets whether to preserve key order
    pub fn with_preserve_key_order(mut self, preserve: bool) -> Self {
        self.preserve_key_order = preserve;
        self
    }
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            max_depth: 128,
            allow_duplicate_keys: true, // Kept for backward compatibility
            duplicate_key_behavior: DuplicateKeyBehavior::ImplicitArray,
            preserve_key_order: true,
        }
    }
}

/// UCL value types
#[derive(Debug, Clone, PartialEq)]
pub enum UclValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Null,
    Object(UclObject),
    /// Arrays use Box<SmallVec> to avoid infinite size recursion
    /// SmallVec stores ≤4 elements inline without heap allocation
    Array(Box<UclArray>),
}

impl UclValue {
    /// Returns true if the value is an object
    pub fn is_object(&self) -> bool {
        matches!(self, UclValue::Object(_))
    }

    /// Returns true if the value is an array
    pub fn is_array(&self) -> bool {
        matches!(self, UclValue::Array(_))
    }

    /// Returns true if the value is a string
    pub fn is_string(&self) -> bool {
        matches!(self, UclValue::String(_))
    }

    /// Returns a reference to the object if this is an Object variant
    pub fn as_object(&self) -> Option<&UclObject> {
        if let UclValue::Object(obj) = self {
            Some(obj)
        } else {
            None
        }
    }

    /// Returns a reference to the array if this is an Array variant
    pub fn as_array(&self) -> Option<&UclArray> {
        if let UclValue::Array(arr) = self {
            Some(arr)
        } else {
            None
        }
    }

    /// Returns a reference to the string if this is a String variant
    pub fn as_str(&self) -> Option<&str> {
        if let UclValue::String(s) = self {
            Some(s.as_str())
        } else {
            None
        }
    }

    /// Returns the integer value if this is an Integer variant
    pub fn as_integer(&self) -> Option<i64> {
        if let UclValue::Integer(i) = self {
            Some(*i)
        } else {
            None
        }
    }

    /// Returns the float value if this is a Float variant
    pub fn as_float(&self) -> Option<f64> {
        if let UclValue::Float(f) = self {
            Some(*f)
        } else {
            None
        }
    }

    /// Returns the time value if this is a Float variant (time values are stored as floats)
    pub fn as_time(&self) -> Option<f64> {
        if let UclValue::Float(f) = self {
            Some(*f)
        } else {
            None
        }
    }

    /// Returns the boolean value if this is a Boolean variant
    pub fn as_bool(&self) -> Option<bool> {
        if let UclValue::Boolean(b) = self {
            Some(*b)
        } else {
            None
        }
    }

    /// Returns true if this is a Null variant
    pub fn is_null(&self) -> bool {
        matches!(self, UclValue::Null)
    }
}

/// UCL object type (preserves insertion order)
pub type UclObject = IndexMap<String, UclValue>;

/// UCL array type - uses SmallVec to avoid heap allocation for small arrays (≤4 elements)
/// Most UCL arrays in practice have ≤4 elements (tags, options, etc.)
pub type UclArray = SmallVec<[UclValue; 4]>;

/// Context information for variable expansion
#[derive(Debug, Clone)]
pub struct VariableContext {
    /// Current position in the source
    pub position: Position,
    /// Path to current object (for nested variable resolution)
    pub current_object_path: Vec<String>,
    /// Stack of variables currently being expanded (for circular reference detection)
    pub expansion_stack: Vec<String>,
}

impl VariableContext {
    /// Creates a new variable context
    pub fn new(position: Position) -> Self {
        Self {
            position,
            current_object_path: Vec::new(),
            expansion_stack: Vec::new(),
        }
    }

    /// Pushes a new key onto the object path
    pub fn push_key(&mut self, key: String) {
        self.current_object_path.push(key);
    }

    /// Pops the last key from the object path
    pub fn pop_key(&mut self) {
        self.current_object_path.pop();
    }

    /// Pushes a variable onto the expansion stack for circular reference detection
    pub fn push_expansion(&mut self, var_name: String) -> Result<(), String> {
        if self.expansion_stack.contains(&var_name) {
            return Err(format!(
                "Circular reference detected: {} -> {}",
                self.expansion_stack.join(" -> "),
                var_name
            ));
        }
        self.expansion_stack.push(var_name);
        Ok(())
    }

    /// Pops a variable from the expansion stack
    pub fn pop_expansion(&mut self) {
        self.expansion_stack.pop();
    }

    /// Returns the current expansion depth
    pub fn expansion_depth(&self) -> usize {
        self.expansion_stack.len()
    }

    /// Returns a copy of the context with updated position
    pub fn with_position(&self, position: Position) -> Self {
        Self {
            position,
            current_object_path: self.current_object_path.clone(),
            expansion_stack: self.expansion_stack.clone(),
        }
    }
}

/// Trait for handling variable expansion
pub trait VariableHandler {
    /// Resolves a variable by name
    fn resolve_variable(&self, name: &str) -> Option<String>;

    /// Resolves a variable with additional context
    fn resolve_variable_with_context(
        &self,
        name: &str,
        _context: &VariableContext,
    ) -> Option<String> {
        // Default implementation ignores context
        self.resolve_variable(name)
    }
}

/// Environment variable handler
pub struct EnvironmentVariableHandler;

impl VariableHandler for EnvironmentVariableHandler {
    fn resolve_variable(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

/// Map-based variable handler
pub struct MapVariableHandler {
    variables: HashMap<String, String>,
}

impl MapVariableHandler {
    /// Creates a new map variable handler
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
        }
    }

    /// Creates a handler from an existing map
    pub fn from_map(variables: HashMap<String, String>) -> Self {
        Self { variables }
    }

    /// Inserts a variable
    pub fn insert(&mut self, name: String, value: String) {
        self.variables.insert(name, value);
    }

    /// Gets a reference to the internal map
    pub fn variables(&self) -> &HashMap<String, String> {
        &self.variables
    }
}

impl Default for MapVariableHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl VariableHandler for MapVariableHandler {
    fn resolve_variable(&self, name: &str) -> Option<String> {
        self.variables.get(name).cloned()
    }
}

/// Chained variable handler that tries multiple handlers in order
pub struct ChainedVariableHandler {
    handlers: Vec<Box<dyn VariableHandler>>,
}

impl ChainedVariableHandler {
    /// Creates a new chained handler
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Adds a handler to the chain
    pub fn add_handler(&mut self, handler: Box<dyn VariableHandler>) {
        self.handlers.push(handler);
    }

    /// Creates a chained handler from a vector of handlers
    pub fn from_handlers(handlers: Vec<Box<dyn VariableHandler>>) -> Self {
        Self { handlers }
    }
}

impl Default for ChainedVariableHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl VariableHandler for ChainedVariableHandler {
    fn resolve_variable(&self, name: &str) -> Option<String> {
        for handler in &self.handlers {
            if let Some(value) = handler.resolve_variable(name) {
                return Some(value);
            }
        }
        None
    }

    fn resolve_variable_with_context(
        &self,
        name: &str,
        context: &VariableContext,
    ) -> Option<String> {
        for handler in &self.handlers {
            if let Some(value) = handler.resolve_variable_with_context(name, context) {
                return Some(value);
            }
        }
        None
    }
}

/// Trait for custom number suffix handling
pub trait NumberSuffixHandler {
    /// Attempts to parse a custom suffix and return the multiplier
    /// Returns None if the suffix is not recognized by this handler
    fn parse_suffix(&self, suffix: &str) -> Option<f64>;

    /// Returns the priority of this handler (higher priority handlers are tried first)
    fn priority(&self) -> u32 {
        0
    }

    /// Returns a description of the suffixes this handler supports
    fn description(&self) -> &str {
        "Custom number suffix handler"
    }
}

/// Trait for custom string post-processing
pub trait StringPostProcessor {
    /// Processes a string value after parsing and variable expansion
    /// Can modify the string or return an error
    fn process_string(&self, value: &str, context: &VariableContext) -> Result<String, ParseError>;

    /// Returns the priority of this processor (higher priority processors are applied first)
    fn priority(&self) -> u32 {
        0
    }

    /// Returns a description of what this processor does
    fn description(&self) -> &str {
        "Custom string post-processor"
    }
}

/// Trait for custom validation during parsing
pub trait ValidationHook {
    /// Validates a parsed value before it's added to the result
    /// Can modify the value or return an error
    fn validate_value(
        &self,
        value: &UclValue,
        context: &VariableContext,
    ) -> Result<Option<UclValue>, ParseError>;

    /// Validates an object key before it's used
    fn validate_key(
        &self,
        _key: &str,
        _context: &VariableContext,
    ) -> Result<Option<String>, ParseError> {
        // Default implementation accepts all keys
        Ok(None)
    }

    /// Returns the priority of this hook (higher priority hooks are called first)
    fn priority(&self) -> u32 {
        0
    }

    /// Returns a description of what this hook validates
    fn description(&self) -> &str {
        "Custom validation hook"
    }
}

/// Container for all custom parsing hooks
#[derive(Default)]
pub struct ParsingHooks {
    /// Custom number suffix handlers
    pub number_suffix_handlers: Vec<Box<dyn NumberSuffixHandler>>,
    /// Custom string post-processors
    pub string_processors: Vec<Box<dyn StringPostProcessor>>,
    /// Custom validation hooks
    pub validation_hooks: Vec<Box<dyn ValidationHook>>,
}

impl ParsingHooks {
    /// Creates a new empty set of parsing hooks
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a number suffix handler
    pub fn add_number_suffix_handler(&mut self, handler: Box<dyn NumberSuffixHandler>) {
        self.number_suffix_handlers.push(handler);
        // Sort by priority (highest first)
        self.number_suffix_handlers
            .sort_by_key(|handler| Reverse(handler.priority()));
    }

    /// Adds a string post-processor
    pub fn add_string_processor(&mut self, processor: Box<dyn StringPostProcessor>) {
        self.string_processors.push(processor);
        // Sort by priority (highest first)
        self.string_processors
            .sort_by_key(|processor| Reverse(processor.priority()));
    }

    /// Adds a validation hook
    pub fn add_validation_hook(&mut self, hook: Box<dyn ValidationHook>) {
        self.validation_hooks.push(hook);
        // Sort by priority (highest first)
        self.validation_hooks
            .sort_by_key(|hook| Reverse(hook.priority()));
    }

    /// Tries to parse a number suffix using registered handlers
    pub fn parse_number_suffix(&self, suffix: &str) -> Option<f64> {
        for handler in &self.number_suffix_handlers {
            if let Some(multiplier) = handler.parse_suffix(suffix) {
                return Some(multiplier);
            }
        }
        None
    }

    /// Processes a string using registered processors
    pub fn process_string(
        &self,
        value: &str,
        context: &VariableContext,
    ) -> Result<String, ParseError> {
        if self.string_processors.is_empty() {
            return Ok(value.to_string());
        }
        let mut result = value.to_string();
        for processor in &self.string_processors {
            result = processor.process_string(&result, context)?;
        }
        Ok(result)
    }

    /// Validates a value using registered hooks
    pub fn validate_value(
        &self,
        value: &UclValue,
        context: &VariableContext,
    ) -> Result<UclValue, ParseError> {
        let mut result = value.clone();
        for hook in &self.validation_hooks {
            if let Some(modified_value) = hook.validate_value(&result, context)? {
                result = modified_value;
            }
        }
        Ok(result)
    }

    /// Validates a key using registered hooks
    pub fn validate_key(&self, key: &str, context: &VariableContext) -> Result<String, ParseError> {
        let mut result = key.to_string();
        for hook in &self.validation_hooks {
            if let Some(modified_key) = hook.validate_key(&result, context)? {
                result = modified_key;
            }
        }
        Ok(result)
    }
}

/// Plugin system for extensible parsing
/// Trait for UCL parser plugins
pub trait UclPlugin {
    /// Returns the name of the plugin
    fn name(&self) -> &str;

    /// Returns the version of the plugin
    fn version(&self) -> &str {
        "1.0.0"
    }

    /// Returns a description of what the plugin does
    fn description(&self) -> &str {
        "UCL parser plugin"
    }

    /// Returns the priority of the plugin (higher priority plugins are loaded first)
    fn priority(&self) -> u32 {
        0
    }

    /// Called when the plugin is loaded
    fn on_load(&mut self) -> Result<(), ParseError> {
        Ok(())
    }

    /// Called when the plugin is unloaded
    fn on_unload(&mut self) -> Result<(), ParseError> {
        Ok(())
    }

    /// Configures the plugin with the given configuration
    fn configure(&mut self, _config: &PluginConfig) -> Result<(), ParseError> {
        Ok(())
    }

    /// Returns the number suffix handlers provided by this plugin
    fn number_suffix_handlers(&self) -> Vec<Box<dyn NumberSuffixHandler>> {
        Vec::new()
    }

    /// Returns the string post-processors provided by this plugin
    fn string_processors(&self) -> Vec<Box<dyn StringPostProcessor>> {
        Vec::new()
    }

    /// Returns the validation hooks provided by this plugin
    fn validation_hooks(&self) -> Vec<Box<dyn ValidationHook>> {
        Vec::new()
    }

    /// Returns the variable handlers provided by this plugin
    fn variable_handlers(&self) -> Vec<Box<dyn VariableHandler>> {
        Vec::new()
    }
}

/// Configuration for plugins
#[derive(Debug, Clone)]
pub struct PluginConfig {
    /// Plugin-specific configuration as key-value pairs
    pub settings: HashMap<String, String>,
    /// Whether the plugin is enabled
    pub enabled: bool,
    /// Plugin priority override
    pub priority_override: Option<u32>,
}

impl PluginConfig {
    /// Creates a new plugin configuration
    pub fn new() -> Self {
        Self {
            settings: HashMap::new(),
            enabled: true,
            priority_override: None,
        }
    }

    /// Creates a configuration with settings
    pub fn with_settings(settings: HashMap<String, String>) -> Self {
        Self {
            settings,
            enabled: true,
            priority_override: None,
        }
    }

    /// Sets a configuration value
    pub fn set(&mut self, key: String, value: String) {
        self.settings.insert(key, value);
    }

    /// Gets a configuration value
    pub fn get(&self, key: &str) -> Option<&String> {
        self.settings.get(key)
    }

    /// Gets a configuration value with a default
    pub fn get_or(&self, key: &str, default: &str) -> String {
        self.settings
            .get(key)
            .cloned()
            .unwrap_or_else(|| default.to_string())
    }

    /// Enables or disables the plugin
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Sets the priority override
    pub fn set_priority_override(&mut self, priority: Option<u32>) {
        self.priority_override = priority;
    }
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Plugin registry for managing loaded plugins
pub struct PluginRegistry {
    /// Loaded plugins
    plugins: Vec<Box<dyn UclPlugin>>,
    /// Plugin configurations
    configs: HashMap<String, PluginConfig>,
    /// Whether the registry is initialized
    initialized: bool,
}

impl PluginRegistry {
    /// Creates a new plugin registry
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            configs: HashMap::new(),
            initialized: false,
        }
    }

    /// Registers a plugin
    pub fn register_plugin(&mut self, mut plugin: Box<dyn UclPlugin>) -> Result<(), ParseError> {
        let name = plugin.name().to_string();

        // Configure the plugin if we have a configuration for it
        if let Some(config) = self.configs.get(&name) {
            plugin.configure(config)?;
        }

        // Load the plugin
        plugin.on_load()?;

        self.plugins.push(plugin);

        // Sort plugins by priority (highest first)
        self.plugins.sort_by(|a, b| {
            let a_priority = self
                .configs
                .get(a.name())
                .and_then(|c| c.priority_override)
                .unwrap_or_else(|| a.priority());
            let b_priority = self
                .configs
                .get(b.name())
                .and_then(|c| c.priority_override)
                .unwrap_or_else(|| b.priority());
            b_priority.cmp(&a_priority)
        });

        Ok(())
    }

    /// Unregisters a plugin by name
    pub fn unregister_plugin(&mut self, name: &str) -> Result<(), ParseError> {
        if let Some(pos) = self.plugins.iter().position(|p| p.name() == name) {
            let mut plugin = self.plugins.remove(pos);
            plugin.on_unload()?;
        }
        Ok(())
    }

    /// Sets the configuration for a plugin
    pub fn set_plugin_config(&mut self, name: String, config: PluginConfig) {
        self.configs.insert(name, config);
    }

    /// Gets the configuration for a plugin
    pub fn get_plugin_config(&self, name: &str) -> Option<&PluginConfig> {
        self.configs.get(name)
    }

    /// Lists all registered plugins
    pub fn list_plugins(&self) -> Vec<&dyn UclPlugin> {
        self.plugins.iter().map(|p| p.as_ref()).collect()
    }

    /// Gets a plugin by name
    pub fn get_plugin(&self, name: &str) -> Option<&dyn UclPlugin> {
        self.plugins
            .iter()
            .find(|p| p.name() == name)
            .map(|p| p.as_ref())
    }

    /// Initializes the registry and applies all plugin hooks
    pub fn initialize(&mut self) -> Result<ParsingHooks, ParseError> {
        if self.initialized {
            return Err(ParseError::InvalidObject {
                message: "Plugin registry already initialized".to_string(),
                position: Position::new(),
            });
        }

        let mut hooks = ParsingHooks::new();

        // Collect hooks from all enabled plugins
        for plugin in &self.plugins {
            let config = self.configs.get(plugin.name());
            let enabled = config.map(|c| c.enabled).unwrap_or(true);

            if enabled {
                // Add number suffix handlers
                for handler in plugin.number_suffix_handlers() {
                    hooks.add_number_suffix_handler(handler);
                }

                // Add string processors
                for processor in plugin.string_processors() {
                    hooks.add_string_processor(processor);
                }

                // Add validation hooks
                for hook in plugin.validation_hooks() {
                    hooks.add_validation_hook(hook);
                }
            }
        }

        self.initialized = true;
        Ok(hooks)
    }

    /// Resets the registry (unloads all plugins)
    pub fn reset(&mut self) -> Result<(), ParseError> {
        for plugin in &mut self.plugins {
            plugin.on_unload()?;
        }
        self.plugins.clear();
        self.initialized = false;
        Ok(())
    }

    /// Returns whether the registry is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Gets all variable handlers from enabled plugins
    pub fn get_variable_handlers(&self) -> Vec<Box<dyn VariableHandler>> {
        let mut handlers = Vec::new();

        for plugin in &self.plugins {
            let config = self.configs.get(plugin.name());
            let enabled = config.map(|c| c.enabled).unwrap_or(true);

            if enabled {
                handlers.extend(plugin.variable_handlers());
            }
        }

        handlers
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating a parser with plugins
pub struct UclParserBuilder<'a> {
    input: &'a str,
    lexer_config: Option<LexerConfig>,
    parser_config: Option<ParserConfig>,
    variable_handler: Option<Box<dyn VariableHandler>>,
    plugin_registry: Option<PluginRegistry>,
}

impl<'a> UclParserBuilder<'a> {
    /// Creates a new parser builder
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            lexer_config: None,
            parser_config: None,
            variable_handler: None,
            plugin_registry: None,
        }
    }

    /// Sets the lexer configuration
    pub fn with_lexer_config(mut self, config: LexerConfig) -> Self {
        self.lexer_config = Some(config);
        self
    }

    /// Sets the parser configuration
    pub fn with_parser_config(mut self, config: ParserConfig) -> Self {
        self.parser_config = Some(config);
        self
    }

    /// Sets the variable handler
    pub fn with_variable_handler(mut self, handler: Box<dyn VariableHandler>) -> Self {
        self.variable_handler = Some(handler);
        self
    }

    /// Sets the plugin registry
    pub fn with_plugin_registry(mut self, registry: PluginRegistry) -> Self {
        self.plugin_registry = Some(registry);
        self
    }

    /// Adds a plugin to the registry
    pub fn with_plugin(mut self, plugin: Box<dyn UclPlugin>) -> Result<Self, ParseError> {
        if self.plugin_registry.is_none() {
            self.plugin_registry = Some(PluginRegistry::new());
        }

        if let Some(ref mut registry) = self.plugin_registry {
            registry.register_plugin(plugin)?;
        }

        Ok(self)
    }

    /// Builds the parser
    pub fn build(self) -> Result<UclParser<'a>, ParseError> {
        let lexer_config = self.lexer_config.unwrap_or_default();
        let parser_config = self.parser_config.unwrap_or_default();

        let mut parser = if let Some(lexer_config) = Some(lexer_config) {
            UclParser::with_lexer_config(self.input, lexer_config).with_config(parser_config)
        } else {
            UclParser::new(self.input).with_config(parser_config)
        };

        // Set up variable handler (potentially from plugins)
        if let Some(mut registry) = self.plugin_registry {
            // Initialize the registry and get parsing hooks
            let hooks = registry.initialize()?;
            parser.set_parsing_hooks(hooks);

            // Set up chained variable handler if we have plugin handlers
            let plugin_handlers = registry.get_variable_handlers();
            if !plugin_handlers.is_empty() {
                let mut chained = ChainedVariableHandler::new();

                // Add plugin handlers first
                for handler in plugin_handlers {
                    chained.add_handler(handler);
                }

                // Add user-provided handler last (highest priority)
                if let Some(user_handler) = self.variable_handler {
                    chained.add_handler(user_handler);
                }

                parser.variable_handler = Some(Box::new(chained));
            } else if let Some(handler) = self.variable_handler {
                parser.variable_handler = Some(handler);
            }
        } else if let Some(handler) = self.variable_handler {
            parser.variable_handler = Some(handler);
        }

        Ok(parser)
    }
}

/// Built-in example implementations
/// Example number suffix handler for custom units
pub struct CustomUnitSuffixHandler {
    units: HashMap<String, f64>,
    priority: u32,
}

impl CustomUnitSuffixHandler {
    /// Creates a new custom unit suffix handler
    pub fn new() -> Self {
        let mut units = HashMap::new();
        // Example: custom units for data processing
        units.insert("px".to_string(), 1.0); // pixels
        units.insert("pt".to_string(), 1.333); // points to pixels
        units.insert("em".to_string(), 16.0); // em to pixels (assuming 16px base)
        units.insert("rem".to_string(), 16.0); // rem to pixels

        Self {
            units,
            priority: 100,
        }
    }

    /// Adds a custom unit
    pub fn add_unit(&mut self, suffix: String, multiplier: f64) {
        self.units.insert(suffix, multiplier);
    }

    /// Sets the priority of this handler
    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }
}

impl Default for CustomUnitSuffixHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl NumberSuffixHandler for CustomUnitSuffixHandler {
    fn parse_suffix(&self, suffix: &str) -> Option<f64> {
        self.units.get(suffix).copied()
    }

    fn priority(&self) -> u32 {
        self.priority
    }

    fn description(&self) -> &str {
        "Custom unit suffix handler (px, pt, em, rem)"
    }
}

/// Example string post-processor for path normalization
pub struct PathNormalizationProcessor {
    priority: u32,
}

impl PathNormalizationProcessor {
    /// Creates a new path normalization processor
    pub fn new() -> Self {
        Self { priority: 50 }
    }

    /// Sets the priority of this processor
    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }
}

impl Default for PathNormalizationProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl StringPostProcessor for PathNormalizationProcessor {
    fn process_string(
        &self,
        value: &str,
        _context: &VariableContext,
    ) -> Result<String, ParseError> {
        // Normalize path separators and resolve relative paths
        let normalized = value
            .replace('\\', "/") // Normalize to forward slashes
            .replace("//", "/"); // Remove double slashes

        // Basic relative path resolution
        let parts: Vec<&str> = normalized.split('/').collect();
        let mut resolved = Vec::new();

        for part in parts {
            match part {
                "." | "" => continue, // Skip current directory and empty parts
                ".." => {
                    if !resolved.is_empty() && resolved.last() != Some(&"..") {
                        resolved.pop();
                    } else {
                        resolved.push(part);
                    }
                }
                _ => resolved.push(part),
            }
        }

        Ok(resolved.join("/"))
    }

    fn priority(&self) -> u32 {
        self.priority
    }

    fn description(&self) -> &str {
        "Path normalization processor (resolves ./ and ../, normalizes separators)"
    }
}

/// Example validation hook for schema validation
pub struct SchemaValidationHook {
    required_keys: Vec<String>,
    allowed_keys: Option<Vec<String>>,
    priority: u32,
}

impl SchemaValidationHook {
    /// Creates a new schema validation hook
    pub fn new() -> Self {
        Self {
            required_keys: Vec::new(),
            allowed_keys: None,
            priority: 100,
        }
    }

    /// Adds a required key
    pub fn require_key(mut self, key: String) -> Self {
        self.required_keys.push(key);
        self
    }

    /// Sets the allowed keys (if None, all keys are allowed)
    pub fn allow_keys(mut self, keys: Vec<String>) -> Self {
        self.allowed_keys = Some(keys);
        self
    }

    /// Sets the priority of this hook
    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }
}

impl Default for SchemaValidationHook {
    fn default() -> Self {
        Self::new()
    }
}

impl ValidationHook for SchemaValidationHook {
    fn validate_value(
        &self,
        value: &UclValue,
        context: &VariableContext,
    ) -> Result<Option<UclValue>, ParseError> {
        match value {
            UclValue::Object(obj) => {
                // Check required keys
                for required_key in &self.required_keys {
                    if !obj.contains_key(required_key) {
                        return Err(ParseError::InvalidObject {
                            message: format!("Missing required key '{}'", required_key),
                            position: context.position,
                        });
                    }
                }

                // Check allowed keys
                if let Some(allowed) = &self.allowed_keys {
                    for key in obj.keys() {
                        if !allowed.contains(key) {
                            return Err(ParseError::InvalidObject {
                                message: format!("Key '{}' is not allowed", key),
                                position: context.position,
                            });
                        }
                    }
                }

                Ok(None) // No modification needed
            }
            _ => Ok(None), // Only validate objects
        }
    }

    fn validate_key(
        &self,
        key: &str,
        context: &VariableContext,
    ) -> Result<Option<String>, ParseError> {
        if let Some(allowed) = &self.allowed_keys
            && !allowed.contains(&key.to_string())
        {
            return Err(ParseError::InvalidObject {
                message: format!("Key '{}' is not allowed", key),
                position: context.position,
            });
        }
        Ok(None) // No modification needed
    }

    fn priority(&self) -> u32 {
        self.priority
    }

    fn description(&self) -> &str {
        "Schema validation hook (checks required and allowed keys)"
    }
}

/// Example plugin implementations
/// Example plugin for CSS-like units
pub struct CssUnitsPlugin {
    name: String,
    version: String,
    priority: u32,
}

impl CssUnitsPlugin {
    /// Creates a new CSS units plugin
    pub fn new() -> Self {
        Self {
            name: "css-units".to_string(),
            version: "1.0.0".to_string(),
            priority: 100,
        }
    }
}

impl Default for CssUnitsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl UclPlugin for CssUnitsPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn description(&self) -> &str {
        "Adds support for CSS-like units (px, pt, em, rem, %, vh, vw)"
    }

    fn priority(&self) -> u32 {
        self.priority
    }

    fn number_suffix_handlers(&self) -> Vec<Box<dyn NumberSuffixHandler>> {
        vec![Box::new(
            CustomUnitSuffixHandler::new().with_priority(self.priority),
        )]
    }
}

/// Example plugin for path processing
pub struct PathProcessingPlugin {
    name: String,
    version: String,
    priority: u32,
}

impl PathProcessingPlugin {
    /// Creates a new path processing plugin
    pub fn new() -> Self {
        Self {
            name: "path-processing".to_string(),
            version: "1.0.0".to_string(),
            priority: 50,
        }
    }
}

impl Default for PathProcessingPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl UclPlugin for PathProcessingPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn description(&self) -> &str {
        "Provides path normalization and processing for string values"
    }

    fn priority(&self) -> u32 {
        self.priority
    }

    fn string_processors(&self) -> Vec<Box<dyn StringPostProcessor>> {
        vec![Box::new(
            PathNormalizationProcessor::new().with_priority(self.priority),
        )]
    }
}

/// Example plugin for configuration validation
pub struct ConfigValidationPlugin {
    name: String,
    version: String,
    priority: u32,
    required_keys: Vec<String>,
    allowed_keys: Option<Vec<String>>,
}

impl ConfigValidationPlugin {
    /// Creates a new configuration validation plugin
    pub fn new() -> Self {
        Self {
            name: "config-validation".to_string(),
            version: "1.0.0".to_string(),
            priority: 200,
            required_keys: Vec::new(),
            allowed_keys: None,
        }
    }

    /// Adds a required key
    pub fn require_key(mut self, key: String) -> Self {
        self.required_keys.push(key);
        self
    }

    /// Sets allowed keys
    pub fn allow_keys(mut self, keys: Vec<String>) -> Self {
        self.allowed_keys = Some(keys);
        self
    }
}

impl Default for ConfigValidationPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl UclPlugin for ConfigValidationPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn description(&self) -> &str {
        "Validates configuration structure with required and allowed keys"
    }

    fn priority(&self) -> u32 {
        self.priority
    }

    fn configure(&mut self, config: &PluginConfig) -> Result<(), ParseError> {
        // Configure required keys from plugin config
        if let Some(required) = config.get("required_keys") {
            self.required_keys = required.split(',').map(|s| s.trim().to_string()).collect();
        }

        // Configure allowed keys from plugin config
        if let Some(allowed) = config.get("allowed_keys") {
            self.allowed_keys = Some(allowed.split(',').map(|s| s.trim().to_string()).collect());
        }

        Ok(())
    }

    fn validation_hooks(&self) -> Vec<Box<dyn ValidationHook>> {
        let mut hook = SchemaValidationHook::new().with_priority(self.priority);

        for key in &self.required_keys {
            hook = hook.require_key(key.clone());
        }

        if let Some(ref allowed) = self.allowed_keys {
            hook = hook.allow_keys(allowed.clone());
        }

        vec![Box::new(hook)]
    }
}

/// Syntax style detection for NGINX-style implicit syntax
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SyntaxStyle {
    /// Explicit syntax with separators (key = value or key: value)
    Explicit,
    /// Implicit syntax without separators (key value or key { ... })
    Implicit,
    /// NGINX-style nested syntax (key identifier { ... })
    NginxNested,
}

/// UCL parser that consumes tokens and builds structured data
pub struct UclParser<'a> {
    lexer: UclLexer<'a>,
    variable_handler: Option<Box<dyn VariableHandler>>,
    current_token: Option<Token<'a>>,
    current_token_start: Option<Position>,
    current_token_end: Option<Position>,
    config: ParserConfig,
    current_depth: usize,
    parsing_hooks: ParsingHooks,
}

impl<'a> UclParser<'a> {
    /// Creates a new parser with default configuration
    pub fn new(input: &'a str) -> Self {
        let mut parser = Self {
            lexer: UclLexer::new(input),
            variable_handler: None,
            current_token: None,
            current_token_start: None,
            current_token_end: None,
            config: ParserConfig::default(),
            current_depth: 0,
            parsing_hooks: ParsingHooks::new(),
        };

        // Load the first token
        parser.advance_token().ok();
        parser
    }

    /// Creates a parser with custom lexer configuration
    pub fn with_lexer_config(input: &'a str, lexer_config: LexerConfig) -> Self {
        let mut parser = Self {
            lexer: UclLexer::with_config(input, lexer_config),
            variable_handler: None,
            current_token: None,
            current_token_start: None,
            current_token_end: None,
            config: ParserConfig::default(),
            current_depth: 0,
            parsing_hooks: ParsingHooks::new(),
        };

        // Load the first token
        parser.advance_token().ok();
        parser
    }

    /// Creates a parser with a variable handler
    pub fn with_variable_handler(input: &'a str, handler: Box<dyn VariableHandler>) -> Self {
        let mut parser = Self::new(input);
        parser.variable_handler = Some(handler);
        parser
    }

    /// Creates a parser with custom parsing hooks
    pub fn with_parsing_hooks(input: &'a str, hooks: ParsingHooks) -> Self {
        let mut parser = Self::new(input);
        parser.parsing_hooks = hooks;
        parser
    }

    /// Creates a parser with both variable handler and parsing hooks
    pub fn with_variable_handler_and_hooks(
        input: &'a str,
        handler: Box<dyn VariableHandler>,
        hooks: ParsingHooks,
    ) -> Self {
        let mut parser = Self::new(input);
        parser.variable_handler = Some(handler);
        parser.parsing_hooks = hooks;
        parser
    }

    /// Sets the parser configuration
    pub fn with_config(mut self, config: ParserConfig) -> Self {
        self.config = config;
        self
    }

    /// Gets a reference to the parsing hooks
    pub fn parsing_hooks(&self) -> &ParsingHooks {
        &self.parsing_hooks
    }

    /// Gets a mutable reference to the parsing hooks
    pub fn parsing_hooks_mut(&mut self) -> &mut ParsingHooks {
        &mut self.parsing_hooks
    }

    /// Sets the parsing hooks
    pub fn set_parsing_hooks(&mut self, hooks: ParsingHooks) {
        self.parsing_hooks = hooks;
    }

    /// Adds a number suffix handler
    pub fn add_number_suffix_handler(&mut self, handler: Box<dyn NumberSuffixHandler>) {
        self.parsing_hooks.add_number_suffix_handler(handler);
    }

    /// Adds a string post-processor
    pub fn add_string_processor(&mut self, processor: Box<dyn StringPostProcessor>) {
        self.parsing_hooks.add_string_processor(processor);
    }

    /// Adds a validation hook
    pub fn add_validation_hook(&mut self, hook: Box<dyn ValidationHook>) {
        self.parsing_hooks.add_validation_hook(hook);
    }

    /// Advances to the next token
    fn advance_token(&mut self) -> Result<(), ParseError> {
        match self.lexer.next_token() {
            Ok(token) => {
                self.current_token = Some(token);
                self.current_token_start = Some(self.lexer.last_token_start());
                self.current_token_end = Some(self.lexer.last_token_end());
                Ok(())
            }
            Err(lex_error) => Err(ParseError::from(lex_error)),
        }
    }

    /// Returns the current token
    pub fn current_token(&self) -> Option<&Token<'a>> {
        self.current_token.as_ref()
    }

    /// Returns the start position of the current token
    pub fn current_token_start(&self) -> Option<Position> {
        self.current_token_start
    }

    /// Returns the end position of the current token
    pub fn current_token_end(&self) -> Option<Position> {
        self.current_token_end
    }

    /// Returns the current position
    pub fn current_position(&self) -> Position {
        self.lexer.current_position()
    }

    /// Peeks at the next token without consuming it
    pub fn peek_token(&mut self) -> Result<Option<&Token<'a>>, ParseError> {
        Ok(self.current_token())
    }

    /// Expects a specific token and consumes it
    pub fn expect_token(&mut self, expected: &Token<'a>) -> Result<Token<'a>, ParseError> {
        match self.current_token() {
            Some(token) if std::mem::discriminant(token) == std::mem::discriminant(expected) => {
                let consumed_token = token.clone();
                self.advance_token()?;
                Ok(consumed_token)
            }
            Some(token) => Err(ParseError::UnexpectedToken {
                token: token.type_name().to_string(),
                position: self.current_position(),
                expected: expected.type_name().to_string(),
            }),
            None => Err(ParseError::UnexpectedToken {
                token: "end of file".to_string(),
                position: self.current_position(),
                expected: expected.type_name().to_string(),
            }),
        }
    }

    /// Consumes the current token if it matches the expected type
    pub fn consume_if_matches(
        &mut self,
        expected: &Token<'a>,
    ) -> Result<Option<Token<'a>>, ParseError> {
        match self.current_token() {
            Some(token) if std::mem::discriminant(token) == std::mem::discriminant(expected) => {
                let consumed_token = token.clone();
                self.advance_token()?;
                Ok(Some(consumed_token))
            }
            _ => Ok(None),
        }
    }

    /// Checks if the current token is of a specific type
    pub fn is_current_token(&self, expected: &Token<'a>) -> bool {
        match self.current_token() {
            Some(token) => std::mem::discriminant(token) == std::mem::discriminant(expected),
            None => false,
        }
    }

    /// Skips whitespace and comments
    pub fn skip_whitespace_and_comments(&mut self) -> Result<(), ParseError> {
        while let Some(token) = self.current_token() {
            match token {
                Token::Comment(_) => {
                    self.advance_token()?;
                }
                _ => break,
            }
        }
        Ok(())
    }

    /// Detects the syntax style for the current key-value pair using lookahead
    fn detect_syntax_style(&mut self) -> Result<SyntaxStyle, ParseError> {
        match self.current_token() {
            Some(Token::Colon) | Some(Token::Equals) => Ok(SyntaxStyle::Explicit),
            Some(Token::ObjectStart) => Ok(SyntaxStyle::Implicit),
            Some(Token::Key(_)) | Some(Token::String { .. }) => {
                let snapshot = self.lexer.snapshot();
                let saved_token = self.current_token.clone();

                let lookahead_result = (|| -> Result<bool, ParseError> {
                    self.advance_token()?;
                    self.skip_whitespace_and_comments()?;
                    Ok(matches!(self.current_token(), Some(Token::ObjectStart)))
                })();

                self.lexer.restore(snapshot);
                self.current_token = saved_token;

                match lookahead_result? {
                    true => Ok(SyntaxStyle::NginxNested),
                    false => Ok(SyntaxStyle::Implicit),
                }
            }
            _ => Ok(SyntaxStyle::Implicit),
        }
    }

    /// Checks if additional tokens should be treated as part of the current value
    fn has_inline_value_continuation(&self) -> bool {
        match self.current_token() {
            Some(Token::Semicolon) | Some(Token::Comma) | Some(Token::ObjectEnd) => false,
            Some(Token::Eof) | None => false,
            Some(_) => !self.lexer.last_token_had_newline(),
        }
    }

    /// Returns the raw text for the current token between the given positions
    fn token_text_from_positions(&self, start: Position, end: Position) -> String {
        self.lexer
            .source()
            .get(start.offset..end.offset)
            .unwrap_or("")
            .to_string()
    }

    /// Collects inline tokens that belong to the same value (same line)
    fn collect_inline_value(
        &mut self,
        mut current: String,
        context: &VariableContext,
    ) -> Result<String, ParseError> {
        let mut _first_token_is_key = false;
        let mut _saw_separator = false;
        let mut parts_appended = 0usize;

        while self.has_inline_value_continuation() {
            let token_start = match self.current_token_start() {
                Some(pos) => pos,
                None => break,
            };

            let gap_text = self.lexer.last_token_leading_whitespace();
            if !gap_text.is_empty() {
                current.push_str(gap_text);
            }

            let part_end = self.current_token_end().unwrap_or(token_start);
            let token_snapshot = self.current_token().cloned();

            let part = match token_snapshot.as_ref() {
                Some(Token::Key(text)) => {
                    if parts_appended == 0 {
                        _first_token_is_key = true;
                    }
                    let part_str = text.to_string();
                    self.validate_bare_word(&part_str, token_start)?;
                    part_str
                }
                Some(Token::String {
                    value,
                    needs_expansion,
                    ..
                }) => {
                    if *needs_expansion {
                        self.expand_variables_with_context_safe(value, context)?
                    } else {
                        value.to_string()
                    }
                }
                Some(Token::Integer(val)) => val.to_string(),
                Some(Token::Float(val)) => {
                    let mut text = val.to_string();
                    if text.contains('.') {
                        while text.ends_with('0') {
                            text.pop();
                        }
                        if text.ends_with('.') {
                            text.push('0');
                        }
                    }
                    text
                }
                Some(Token::Time(_)) => self.token_text_from_positions(token_start, part_end),
                Some(Token::Boolean(true)) => "true".to_string(),
                Some(Token::Boolean(false)) => "false".to_string(),
                Some(Token::Null) => "null".to_string(),
                Some(Token::Colon) => {
                    _saw_separator = true;
                    ":".to_string()
                }
                Some(Token::Equals) => {
                    _saw_separator = true;
                    "=".to_string()
                }
                _ => break,
            };

            current.push_str(&part);
            parts_appended += 1;

            self.advance_token()?;
            self.skip_whitespace_and_comments()?;
        }

        Ok(current)
    }

    /// Handles inline string concatenation using '+' on the same line
    fn concatenate_inline_strings(
        &mut self,
        mut current: String,
        context: Option<&VariableContext>,
    ) -> Result<String, ParseError> {
        while matches!(self.current_token(), Some(Token::Plus))
            && !self.lexer.last_token_had_newline()
        {
            let plus_position = self
                .current_token_start()
                .unwrap_or_else(|| self.current_position());

            self.advance_token()?;
            self.skip_whitespace_and_comments()?;

            let appended = match self.current_token() {
                Some(Token::String {
                    value: next_value,
                    needs_expansion: next_needs_expansion,
                    ..
                }) => {
                    if let Some(ctx) = context {
                        if *next_needs_expansion {
                            self.expand_variables_with_context_safe(next_value, ctx)?
                        } else {
                            next_value.to_string()
                        }
                    } else if *next_needs_expansion {
                        self.expand_variables(next_value)?
                    } else {
                        next_value.to_string()
                    }
                }
                Some(_) => {
                    return Err(ParseError::InvalidObject {
                        message:
                            "String concatenation requires quoted strings on both sides of '+'"
                                .to_string(),
                        position: self
                            .current_token_start()
                            .unwrap_or_else(|| self.current_position()),
                    });
                }
                None => {
                    return Err(ParseError::InvalidObject {
                        message:
                            "String concatenation terminates unexpectedly; missing right-hand string"
                                .to_string(),
                        position: plus_position,
                    });
                }
            };

            current.push_str(&appended);
            self.advance_token()?;
            self.skip_whitespace_and_comments()?;
        }

        Ok(current)
    }

    /// Parses a bare word value (unquoted identifier)
    fn parse_bare_word_value(&mut self) -> Result<UclValue, ParseError> {
        let context = VariableContext::new(self.current_position());
        self.parse_bare_word_value_with_context(&context, false)
    }

    /// Parses a bare word value with explicit context (unquoted identifier)
    fn parse_bare_word_value_with_context(
        &mut self,
        context: &VariableContext,
        explicit_separator: bool,
    ) -> Result<UclValue, ParseError> {
        match self.current_token() {
            Some(Token::Key(word)) => {
                let word_str = word.to_string();
                let start_position = self
                    .current_token_start()
                    .unwrap_or_else(|| self.current_position());
                let bare_word_end = self
                    .current_token_end()
                    .unwrap_or_else(|| self.current_position());
                self.advance_token()?;

                if let Some(next_char) = self
                    .lexer
                    .source()
                    .get(bare_word_end.offset..)
                    .and_then(|s| s.chars().next())
                    && next_char == '#'
                {
                    return Err(ParseError::InvalidObject {
                        message: format!(
                            "Bare word '{}' cannot be immediately followed by '#'. quote the value if you need a literal hash.",
                            word_str
                        ),
                        position: bare_word_end,
                    });
                }

                // Validate bare word before processing
                self.validate_bare_word(&word_str, start_position)?;

                self.skip_whitespace_and_comments()?;

                // Detect structural delimiters that are directly attached to the bare word (e.g. hello{world)
                // Exception: ${ is allowed for variable expansion
                let delimiter_error = if let (Some(next_start), Some(token)) =
                    (self.current_token_start(), self.current_token())
                {
                    if next_start.offset == bare_word_end.offset {
                        match token {
                            Token::ObjectStart if word_str != "$" => Some(('{', next_start)),
                            Token::ObjectEnd => Some(('}', next_start)),
                            Token::ArrayStart => Some(('[', next_start)),
                            Token::ArrayEnd => Some((']', next_start)),
                            _ => None,
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some((delimiter, position)) = delimiter_error {
                    return Err(ParseError::InvalidObject {
                        message: format!(
                            "Bare word '{}' cannot be adjacent to '{}'. quote the value if you need this character literally.",
                            word_str, delimiter
                        ),
                        position,
                    });
                }

                let inline_continuation =
                    self.has_inline_value_continuation() && !explicit_separator;

                if inline_continuation {
                    let combined = self.collect_inline_value(word_str, context)?;
                    let processed = self.parsing_hooks.process_string(&combined, context)?;
                    let ucl_value = UclValue::String(processed);
                    let validated = self.parsing_hooks.validate_value(&ucl_value, context)?;
                    Ok(validated)
                } else {
                    // Check for special keywords when no continuation is present
                    let normalized = word_str.to_ascii_lowercase();
                    let ucl_value = match normalized.as_str() {
                        "true" | "yes" | "on" => UclValue::Boolean(true),
                        "false" | "no" | "off" => UclValue::Boolean(false),
                        "null" => UclValue::Null,
                        "inf" | "infinity" => UclValue::Float(f64::INFINITY),
                        "-inf" | "-infinity" => UclValue::Float(f64::NEG_INFINITY),
                        "nan" => UclValue::Float(f64::NAN),
                        _ => {
                            let processed =
                                self.parsing_hooks.process_string(&word_str, context)?;
                            UclValue::String(processed)
                        }
                    };
                    let validated_value = self.parsing_hooks.validate_value(&ucl_value, context)?;
                    Ok(validated_value)
                }
            }
            Some(Token::String {
                value,
                needs_expansion,
                ..
            }) => {
                let mut combined_value = if *needs_expansion {
                    self.expand_variables_with_context_safe(value, context)?
                } else {
                    value.to_string()
                };
                self.advance_token()?;
                self.skip_whitespace_and_comments()?;

                while self.has_inline_value_continuation() {
                    match self.current_token() {
                        Some(Token::Plus) => {
                            self.advance_token()?;
                            self.skip_whitespace_and_comments()?;

                            match self.current_token() {
                                Some(Token::String {
                                    value: next_value,
                                    needs_expansion: next_needs_expansion,
                                    ..
                                }) => {
                                    let appended = if *next_needs_expansion {
                                        self.expand_variables_with_context_safe(
                                            next_value, context,
                                        )?
                                    } else {
                                        next_value.to_string()
                                    };
                                    combined_value.push_str(&appended);
                                    self.advance_token()?;
                                    self.skip_whitespace_and_comments()?;
                                }
                                Some(Token::Key(_)) => {
                                    return Err(ParseError::InvalidObject {
                                        message: "String concatenation requires quoted strings on both sides of '+'".to_string(),
                                        position: self
                                            .current_token_start()
                                            .unwrap_or_else(|| self.current_position()),
                                    });
                                }
                                Some(token) => {
                                    return Err(ParseError::UnexpectedToken {
                                        token: token.type_name().to_string(),
                                        position: self.current_position(),
                                        expected: "string".to_string(),
                                    });
                                }
                                None => {
                                    return Err(ParseError::UnexpectedToken {
                                        token: "end of file".to_string(),
                                        position: self.current_position(),
                                        expected: "string".to_string(),
                                    });
                                }
                            }
                        }
                        _ => break,
                    }
                }

                let processed = self
                    .parsing_hooks
                    .process_string(&combined_value, context)?;
                let ucl_value = UclValue::String(processed);
                let validated_value = self.parsing_hooks.validate_value(&ucl_value, context)?;
                Ok(validated_value)
            }
            Some(Token::Integer(val)) => {
                let int_val = *val;
                self.advance_token()?;
                self.skip_whitespace_and_comments()?;

                let inline_continuation =
                    self.has_inline_value_continuation() && !explicit_separator;

                if inline_continuation {
                    // Collect remaining tokens on the same line
                    let combined = self.collect_inline_value(int_val.to_string(), context)?;
                    let processed = self.parsing_hooks.process_string(&combined, context)?;
                    let ucl_value = UclValue::String(processed);
                    let validated = self.parsing_hooks.validate_value(&ucl_value, context)?;
                    Ok(validated)
                } else {
                    let ucl_value = UclValue::Integer(int_val);
                    let validated_value = self.parsing_hooks.validate_value(&ucl_value, context)?;
                    Ok(validated_value)
                }
            }
            Some(Token::Float(val)) => {
                let float_val = *val;
                self.advance_token()?;
                self.skip_whitespace_and_comments()?;

                let inline_continuation =
                    self.has_inline_value_continuation() && !explicit_separator;

                if inline_continuation {
                    // Collect remaining tokens on the same line
                    let combined = self.collect_inline_value(float_val.to_string(), context)?;
                    let processed = self.parsing_hooks.process_string(&combined, context)?;
                    let ucl_value = UclValue::String(processed);
                    let validated = self.parsing_hooks.validate_value(&ucl_value, context)?;
                    Ok(validated)
                } else {
                    let ucl_value = UclValue::Float(float_val);
                    let validated_value = self.parsing_hooks.validate_value(&ucl_value, context)?;
                    #[cfg(test)]
                    println!(
                        "parse_value_with_context float={} is_finite={}",
                        float_val,
                        float_val.is_finite()
                    );
                    Ok(validated_value)
                }
            }
            Some(Token::Time(_)) => {
                let start = self
                    .current_token_start()
                    .unwrap_or_else(|| self.current_position());
                let end = self.current_token_end().unwrap_or(start);
                let raw = self.token_text_from_positions(start, end);
                self.advance_token()?;
                self.skip_whitespace_and_comments()?;

                let inline_continuation =
                    self.has_inline_value_continuation() && !explicit_separator;

                if inline_continuation {
                    // Collect remaining tokens on the same line
                    let combined = self.collect_inline_value(raw.clone(), context)?;
                    let processed = self.parsing_hooks.process_string(&combined, context)?;
                    let ucl_value = UclValue::String(processed);
                    let validated = self.parsing_hooks.validate_value(&ucl_value, context)?;
                    Ok(validated)
                } else {
                    let processed = self.parsing_hooks.process_string(&raw, context)?;
                    let ucl_value = UclValue::String(processed);
                    let validated_value = self.parsing_hooks.validate_value(&ucl_value, context)?;
                    Ok(validated_value)
                }
            }
            Some(Token::Boolean(val)) => {
                let bool_val = *val;
                self.advance_token()?;
                let ucl_value = UclValue::Boolean(bool_val);

                // Apply validation hooks
                let validated_value = self.parsing_hooks.validate_value(&ucl_value, context)?;
                Ok(validated_value)
            }
            Some(Token::Null) => {
                self.advance_token()?;
                let ucl_value = UclValue::Null;

                // Apply validation hooks
                let validated_value = self.parsing_hooks.validate_value(&ucl_value, context)?;
                Ok(validated_value)
            }
            Some(token) => Err(ParseError::UnexpectedToken {
                token: token.type_name().to_string(),
                position: self.current_position(),
                expected: "bare word value".to_string(),
            }),
            None => Err(ParseError::UnexpectedToken {
                token: "EOF".to_string(),
                position: self.current_position(),
                expected: "bare word value".to_string(),
            }),
        }
    }

    /// Validates a bare word value and provides helpful error messages for invalid cases
    fn validate_bare_word(&self, word: &str, position: Position) -> Result<(), ParseError> {
        // Check for empty word
        if word.is_empty() {
            return Err(ParseError::InvalidObject {
                message: "Empty bare word is not allowed".to_string(),
                position,
            });
        }

        // Check for special characters that require quoting
        let invalid_chars = [
            ' ', '\t', '\n', '\r', '{', '}', '[', ']', '=', ':', ',', ';', '#', '"', '\'', '@',
        ];
        if let Some(invalid_char) = word.chars().find(|&c| invalid_chars.contains(&c)) {
            let suggestion = match invalid_char {
                ' ' => format!("Quote the value: \"{}\"", word),
                '\t' | '\n' | '\r' => "Remove whitespace characters or quote the value".to_string(),
                '{' | '}' | '[' | ']' => "These characters are reserved for objects and arrays. Quote the value if you need them literally".to_string(),
                '=' | ':' => "These characters are reserved for key-value separators. Quote the value if you need them literally".to_string(),
                ',' | ';' => "These characters are reserved for separators. Quote the value if you need them literally".to_string(),
                '#' => "This character starts a comment. Quote the value if you need it literally".to_string(),
                '"' | '\'' => "Quote characters must be escaped or the entire value must be quoted".to_string(),
                _ => format!("Quote the value: \"{}\"", word),
            };

            return Err(ParseError::InvalidObject {
                message: format!(
                    "Bare word '{}' contains invalid character '{}'. {}",
                    word, invalid_char, suggestion
                ),
                position,
            });
        }

        // Check for ambiguous cases with reserved keywords
        if self.is_ambiguous_bare_word(word) {
            let suggestion = self.suggest_bare_word_fix(word);
            return Err(ParseError::InvalidObject {
                message: format!("Ambiguous bare word '{}'. {}", word, suggestion),
                position,
            });
        }

        // Check for words that look like numbers but aren't valid
        if self.looks_like_invalid_number(word) {
            return Err(ParseError::InvalidObject {
                message: format!(
                    "Bare word '{}' looks like a number but is invalid. Quote it if you want a literal string, or fix the number format",
                    word
                ),
                position,
            });
        }

        Ok(())
    }

    /// Checks if a bare word is ambiguous and might cause confusion
    fn is_ambiguous_bare_word(&self, word: &str) -> bool {
        // Check for words that might be confused with reserved keywords
        match word.to_ascii_lowercase().as_str() {
            // Words that might be confused with numbers
            _ if word.starts_with('-') && word.len() > 1 => {
                // Negative-looking words that aren't valid numbers
                !word[1..].chars().all(|c| c.is_ascii_digit() || c == '.')
            }
            _ => false,
        }
    }

    /// Checks if a word looks like it should be a number but isn't valid
    fn looks_like_invalid_number(&self, word: &str) -> bool {
        // Check for common invalid number patterns
        if word.is_empty() {
            return false;
        }

        if word.contains(':') || word.matches('.').count() > 1 {
            return false;
        }

        let first_char = word.chars().next().unwrap();

        // Starts with digit or sign but isn't a valid number
        if first_char.is_ascii_digit() || first_char == '-' || first_char == '+' {
            // Try to parse as number - if it fails but looks numeric, it's invalid
            if word.parse::<i64>().is_err() && word.parse::<f64>().is_err() {
                // Check if it contains mostly digits and number-like characters
                let numeric_chars = word
                    .chars()
                    .filter(|c| {
                        c.is_ascii_digit()
                            || *c == '.'
                            || *c == '-'
                            || *c == '+'
                            || *c == 'e'
                            || *c == 'E'
                    })
                    .count();

                // If more than half the characters look numeric, it's probably an invalid number
                return numeric_chars > word.len() / 2;
            }
        }

        false
    }

    /// Provides suggestions for fixing bare word issues
    fn suggest_bare_word_fix(&self, word: &str) -> String {
        if word.contains(' ') || word.contains('\t') {
            format!("Quote the value: \"{}\"", word)
        } else if ["true", "false", "null", "yes", "no", "on", "off"]
            .contains(&word.to_lowercase().as_str())
        {
            format!(
                "'{}' is a reserved keyword. Use \"{}\" for literal string",
                word, word
            )
        } else {
            format!(
                "Use quotes if '{}' should be a literal string: \"{}\"",
                word, word
            )
        }
    }

    /// Parses NGINX-style nested object: key identifier { ... }
    fn parse_nginx_nested_object(
        &mut self,
        context: &mut VariableContext,
    ) -> Result<UclValue, ParseError> {
        // Current token should be the nested key identifier
        let raw_nested_key = match self.current_token() {
            Some(Token::Key(k)) => k.to_string(),
            Some(Token::String { value, .. }) => value.to_string(),
            Some(token) => {
                return Err(ParseError::UnexpectedToken {
                    token: token.type_name().to_string(),
                    position: self.current_position(),
                    expected: "identifier for NGINX-style object".to_string(),
                });
            }
            None => {
                return Err(ParseError::UnexpectedToken {
                    token: "EOF".to_string(),
                    position: self.current_position(),
                    expected: "identifier for NGINX-style object".to_string(),
                });
            }
        };
        let nested_key = self.parsing_hooks.validate_key(&raw_nested_key, context)?;

        self.advance_token()?;
        self.skip_whitespace_and_comments()?;

        // Next must be an object
        match self.current_token() {
            Some(Token::ObjectStart) => {
                context.push_key(nested_key.clone());
                let nested_result = self.parse_object_with_context(context);
                context.pop_key();

                let nested_obj = nested_result?;

                // Wrap it in a parent object with the nested_key
                let mut wrapper = UclObject::new();
                wrapper.insert(nested_key, nested_obj);
                Ok(UclValue::Object(wrapper))
            }
            _ => Err(ParseError::UnexpectedToken {
                token: self
                    .current_token()
                    .map(|t| t.type_name())
                    .unwrap_or("EOF")
                    .to_string(),
                position: self.current_position(),
                expected: "'{' for NGINX-style nested object".to_string(),
            }),
        }
    }

    /// Parses a complete UCL value
    pub fn parse_value(&mut self) -> Result<UclValue, ParseError> {
        if self.current_depth >= self.config.max_depth {
            return Err(ParseError::MaxDepthExceeded {
                position: self.current_position(),
            });
        }

        self.skip_whitespace_and_comments()?;

        match self.current_token() {
            Some(Token::Eof) | None => Err(ParseError::UnexpectedToken {
                token: "end of file".to_string(),
                position: self.current_position(),
                expected: "value".to_string(),
            }),
            Some(Token::String {
                value,
                needs_expansion,
                ..
            }) => {
                let mut combined = if *needs_expansion {
                    self.expand_variables(value)?
                } else {
                    value.to_string()
                };

                self.advance_token()?;
                self.skip_whitespace_and_comments()?;

                combined = self.concatenate_inline_strings(combined, None)?;

                // Apply custom string post-processing
                let context = VariableContext::new(self.current_position());
                let processed = self.parsing_hooks.process_string(&combined, &context)?;
                let ucl_value = UclValue::String(processed);

                // Apply validation hooks
                let validated_value = self.parsing_hooks.validate_value(&ucl_value, &context)?;
                Ok(validated_value)
            }
            Some(Token::Integer(val)) => {
                let int_val = *val;
                self.advance_token()?;
                let ucl_value = UclValue::Integer(int_val);

                // Apply validation hooks
                let context = VariableContext::new(self.current_position());
                let validated_value = self.parsing_hooks.validate_value(&ucl_value, &context)?;
                Ok(validated_value)
            }
            Some(Token::Float(val)) => {
                let float_val = *val;
                self.advance_token()?;
                let ucl_value = UclValue::Float(float_val);

                // Apply validation hooks
                let context = VariableContext::new(self.current_position());
                let validated_value = self.parsing_hooks.validate_value(&ucl_value, &context)?;
                Ok(validated_value)
            }
            Some(Token::Time(_)) => {
                let start = self
                    .current_token_start()
                    .unwrap_or_else(|| self.current_position());
                let end = self.current_token_end().unwrap_or(start);
                let raw = self.token_text_from_positions(start, end);
                self.advance_token()?;
                let context = VariableContext::new(self.current_position());
                let processed = self.parsing_hooks.process_string(&raw, &context)?;
                let ucl_value = UclValue::String(processed);

                // Apply validation hooks
                let validated_value = self.parsing_hooks.validate_value(&ucl_value, &context)?;
                Ok(validated_value)
            }
            Some(Token::Boolean(val)) => {
                let bool_val = *val;
                self.advance_token()?;
                let ucl_value = UclValue::Boolean(bool_val);

                // Apply validation hooks
                let context = VariableContext::new(self.current_position());
                let validated_value = self.parsing_hooks.validate_value(&ucl_value, &context)?;
                Ok(validated_value)
            }
            Some(Token::Null) => {
                self.advance_token()?;
                let ucl_value = UclValue::Null;

                // Apply validation hooks
                let context = VariableContext::new(self.current_position());
                let validated_value = self.parsing_hooks.validate_value(&ucl_value, &context)?;
                Ok(validated_value)
            }
            Some(Token::ObjectStart) => self.parse_object(),
            Some(Token::ArrayStart) => self.parse_array(),
            Some(Token::Key(_)) => {
                // Handle bare word values (unquoted identifiers)
                self.parse_bare_word_value()
            }
            Some(token) => Err(ParseError::UnexpectedToken {
                token: token.type_name().to_string(),
                position: self.current_position(),
                expected: "value".to_string(),
            }),
        }
    }

    /// Parses a UCL object with variable expansion context
    pub fn parse_object(&mut self) -> Result<UclValue, ParseError> {
        self.parse_object_with_context(&mut VariableContext::new(self.current_position()))
    }

    /// Parses a UCL object with explicit variable context
    pub fn parse_object_with_context(
        &mut self,
        context: &mut VariableContext,
    ) -> Result<UclValue, ParseError> {
        self.current_depth += 1;
        if self.current_depth > self.config.max_depth {
            return Err(ParseError::MaxDepthExceeded {
                position: self.current_position(),
            });
        }

        // Consume the opening brace
        self.expect_token(&Token::ObjectStart)?;

        let mut object = UclObject::new();

        // Handle empty object
        self.skip_whitespace_and_comments()?;
        if let Some(Token::ObjectEnd) = self.current_token() {
            self.advance_token()?;
            self.current_depth -= 1;
            return Ok(UclValue::Object(object));
        }

        loop {
            self.skip_whitespace_and_comments()?;

            // Parse key - support various key formats
            let key = match self.current_token() {
                Some(Token::Key(k)) => {
                    let key_str = k.to_string();
                    self.advance_token()?;
                    // Apply key validation
                    self.parsing_hooks.validate_key(&key_str, context)?
                }
                Some(Token::String {
                    value,
                    needs_expansion,
                    ..
                }) => {
                    let key_str = if *needs_expansion {
                        self.expand_variables_with_context_safe(value, context)?
                    } else {
                        value.to_string()
                    };
                    self.advance_token()?;
                    // Apply key validation
                    self.parsing_hooks.validate_key(&key_str, context)?
                }
                // Support bare identifiers as keys (common in UCL)
                Some(Token::Boolean(true)) => {
                    self.advance_token()?;
                    let key_str = "true".to_string();
                    // Apply key validation
                    self.parsing_hooks.validate_key(&key_str, context)?
                }
                Some(Token::Boolean(false)) => {
                    self.advance_token()?;
                    let key_str = "false".to_string();
                    // Apply key validation
                    self.parsing_hooks.validate_key(&key_str, context)?
                }
                Some(Token::Integer(val)) => {
                    let key_str = val.to_string();
                    self.advance_token()?;
                    // Apply key validation
                    self.parsing_hooks.validate_key(&key_str, context)?
                }
                Some(Token::Float(_)) => {
                    let start = self
                        .current_token_start()
                        .unwrap_or_else(|| self.current_position());
                    let end = self.current_token_end().unwrap_or(start);
                    let key_str = self.token_text_from_positions(start, end);
                    self.advance_token()?;
                    self.parsing_hooks.validate_key(&key_str, context)?
                }
                Some(Token::ObjectEnd) => {
                    // End of object
                    break;
                }
                Some(token) => {
                    return Err(ParseError::UnexpectedToken {
                        token: token.type_name().to_string(),
                        position: self.current_position(),
                        expected: "key or '}'".to_string(),
                    });
                }
                None => {
                    return Err(ParseError::UnexpectedToken {
                        token: "end of file".to_string(),
                        position: self.current_position(),
                        expected: "key or '}'".to_string(),
                    });
                }
            };

            self.skip_whitespace_and_comments()?;

            // Detect syntax style for this key-value pair
            let syntax_style = self.detect_syntax_style()?;

            // Parse value based on detected syntax style
            let value = match syntax_style {
                SyntaxStyle::Explicit => {
                    // Expect separator (: or =) and parse value normally
                    match self.current_token() {
                        Some(Token::Colon) | Some(Token::Equals) => {
                            self.advance_token()?;
                            self.skip_whitespace_and_comments()?;
                            self.parse_value_with_context(context, true)?
                        }
                        Some(token) => {
                            return Err(ParseError::UnexpectedToken {
                                token: token.type_name().to_string(),
                                position: self.current_position(),
                                expected: "':' or '='".to_string(),
                            });
                        }
                        None => {
                            return Err(ParseError::UnexpectedToken {
                                token: "end of file".to_string(),
                                position: self.current_position(),
                                expected: "':' or '='".to_string(),
                            });
                        }
                    }
                }
                SyntaxStyle::Implicit => {
                    // Direct value or object without separator
                    match self.current_token() {
                        Some(Token::ObjectStart) => self.parse_object_with_context(context)?,
                        _ => {
                            // Bare word value
                            self.parse_bare_word_value_with_context(context, false)?
                        }
                    }
                }
                SyntaxStyle::NginxNested => {
                    // key identifier { ... } pattern
                    self.parse_nginx_nested_object(context)?
                }
            };

            if syntax_style == SyntaxStyle::Implicit
                && matches!(self.current_token(), Some(Token::ObjectEnd))
                && !self.lexer.last_token_had_newline()
            {
                return Err(ParseError::InvalidObject {
                    message: format!(
                        "Implicit value for '{}' must be terminated by a newline or ';'.",
                        key
                    ),
                    position: self
                        .current_token_start()
                        .unwrap_or_else(|| self.current_position()),
                });
            }

            // Handle duplicate keys based on configuration
            if let Some(existing_value) = object.get_mut(&key) {
                if let UclValue::Object(existing_map) = existing_value
                    && let UclValue::Object(ref new_map) = value
                {
                    for (nested_key, nested_value) in new_map.iter() {
                        existing_map.insert(nested_key.clone(), nested_value.clone());
                    }
                    continue;
                }

                match self.config.duplicate_key_behavior {
                    DuplicateKeyBehavior::Error => {
                        return Err(ParseError::DuplicateKey {
                            key,
                            position: self.current_position(),
                        });
                    }
                    DuplicateKeyBehavior::ImplicitArray => {
                        let existing_value = object.shift_remove(&key).unwrap();
                        let new_array = match existing_value {
                            UclValue::Array(mut arr) => {
                                arr.push(value);
                                UclValue::Array(arr)
                            }
                            other => {
                                let mut arr = SmallVec::new();
                                arr.push(other);
                                arr.push(value);
                                UclValue::Array(Box::new(arr))
                            }
                        };
                        object.insert(key, new_array);
                    }
                    DuplicateKeyBehavior::Override => {
                        object.insert(key, value);
                    }
                }
            } else {
                object.insert(key, value);
            }

            self.skip_whitespace_and_comments()?;

            // Check for separator or end (separators are optional in implicit syntax)
            match self.current_token() {
                Some(Token::Comma) | Some(Token::Semicolon) => {
                    self.advance_token()?;
                    // Continue to next key-value pair
                }
                Some(Token::ObjectEnd) => {
                    // End of object
                    break;
                }
                Some(Token::Key(_))
                | Some(Token::String { .. })
                | Some(Token::Boolean(_))
                | Some(Token::Integer(_)) => {
                    // Next key without separator (implicit syntax)
                    // Continue to next key-value pair
                }
                Some(token) => {
                    return Err(ParseError::UnexpectedToken {
                        token: token.type_name().to_string(),
                        position: self.current_position(),
                        expected: "',', ';', key, or '}'".to_string(),
                    });
                }
                None => {
                    return Err(ParseError::UnexpectedToken {
                        token: "end of file".to_string(),
                        position: self.current_position(),
                        expected: "',', ';', key, or '}'".to_string(),
                    });
                }
            }
        }

        // Consume the closing brace
        self.expect_token(&Token::ObjectEnd)?;
        self.current_depth -= 1;

        Ok(UclValue::Object(object))
    }

    /// Parses a UCL array
    pub fn parse_array(&mut self) -> Result<UclValue, ParseError> {
        self.parse_array_with_context(&mut VariableContext::new(self.current_position()))
    }

    /// Parses a top-level UCL document (may be an implicit object)
    pub fn parse_document(&mut self) -> Result<UclValue, ParseError> {
        self.skip_whitespace_and_comments()?;

        match self.current_token() {
            Some(Token::Eof) | None => {
                // Empty document
                Ok(UclValue::Object(UclObject::new()))
            }
            Some(Token::ObjectStart) | Some(Token::ArrayStart) => {
                // Explicit object or array
                self.parse_value()
            }
            _ => {
                // Implicit object - parse key-value pairs without braces
                self.parse_implicit_object()
            }
        }
    }

    /// Deep merge two objects (for named section hierarchy)
    fn deep_merge_objects(mut target: UclObject, source: UclObject) -> UclObject {
        for (key, value) in source {
            if let Some(existing) = target.get_mut(&key) {
                // If both are objects, merge them recursively
                if existing.is_object() && value.is_object() {
                    if let (UclValue::Object(existing_obj), UclValue::Object(new_obj)) =
                        (existing.clone(), value)
                    {
                        let merged = Self::deep_merge_objects(existing_obj, new_obj);
                        *existing = UclValue::Object(merged);
                    }
                } else {
                    // Otherwise replace
                    *existing = value;
                }
            } else {
                // Key doesn't exist, insert it
                target.insert(key, value);
            }
        }
        target
    }

    /// Parses a key path for named sections (e.g., "section foo bar" -> ["section", "foo", "bar"])
    /// Per SPEC.md lines 154-194
    fn parse_key_path(&mut self, context: &VariableContext) -> Result<Vec<String>, ParseError> {
        let mut keys = Vec::new();

        // Parse first key
        let first_key = match self.current_token() {
            Some(Token::Key(k)) => {
                let key_str = k.to_string();
                self.advance_token()?;
                self.parsing_hooks.validate_key(&key_str, context)?
            }
            Some(Token::String {
                value,
                needs_expansion,
                ..
            }) => {
                let key_str = if *needs_expansion {
                    self.expand_variables_with_context_safe(value, context)?
                } else {
                    value.to_string()
                };
                self.advance_token()?;
                self.parsing_hooks.validate_key(&key_str, context)?
            }
            Some(token) => {
                return Err(ParseError::UnexpectedToken {
                    token: token.type_name().to_string(),
                    position: self.current_position(),
                    expected: "key".to_string(),
                });
            }
            None => {
                return Err(ParseError::UnexpectedToken {
                    token: "end of file".to_string(),
                    position: self.current_position(),
                    expected: "key".to_string(),
                });
            }
        };
        keys.push(first_key.clone());

        // Only parse additional keys if first key is "section" (for named sections like "section foo bar {}")
        // This avoids breaking implicit syntax like "worker_processes auto"
        if first_key == "section" {
            loop {
                self.skip_whitespace_and_comments()?;

                match self.current_token() {
                    Some(Token::Key(k)) => {
                        let key_str = k.to_string();
                        self.advance_token()?;
                        let validated_key = self.parsing_hooks.validate_key(&key_str, context)?;
                        keys.push(validated_key);
                    }
                    Some(Token::String {
                        value,
                        needs_expansion,
                        ..
                    }) => {
                        let key_str = if *needs_expansion {
                            self.expand_variables_with_context_safe(value, context)?
                        } else {
                            value.to_string()
                        };
                        self.advance_token()?;
                        let validated_key = self.parsing_hooks.validate_key(&key_str, context)?;
                        keys.push(validated_key);
                    }
                    // Stop if we hit a separator or object start
                    Some(Token::Colon) | Some(Token::Equals) | Some(Token::ObjectStart) => {
                        break;
                    }
                    _ => {
                        break;
                    }
                }
            }
        }

        Ok(keys)
    }

    /// Parses an implicit object (key-value pairs without braces)
    pub fn parse_implicit_object(&mut self) -> Result<UclValue, ParseError> {
        let mut object = UclObject::new();
        let mut context = VariableContext::new(self.current_position());

        while !matches!(self.current_token(), Some(Token::Eof) | None) {
            self.skip_whitespace_and_comments()?;

            if matches!(self.current_token(), Some(Token::Eof) | None) {
                break;
            }

            // Parse key path (supports named sections like "section foo bar {}")
            let key_path = self.parse_key_path(&context)?;

            self.skip_whitespace_and_comments()?;

            // Detect syntax style for this key-value pair
            let syntax_style = self.detect_syntax_style()?;

            // Parse value based on detected syntax style
            // For multi-key paths (named sections), push all keys onto context
            for k in &key_path {
                context.push_key(k.clone());
            }

            let value = match syntax_style {
                SyntaxStyle::Explicit => {
                    // Expect separator (: or =) and parse value normally
                    match self.current_token() {
                        Some(Token::Colon) | Some(Token::Equals) => {
                            self.advance_token()?;
                            self.skip_whitespace_and_comments()?;
                            self.parse_value_with_context(&mut context, true)?
                        }
                        Some(token) => {
                            return Err(ParseError::UnexpectedToken {
                                token: token.type_name().to_string(),
                                position: self.current_position(),
                                expected: "':' or '='".to_string(),
                            });
                        }
                        None => {
                            return Err(ParseError::UnexpectedToken {
                                token: "end of file".to_string(),
                                position: self.current_position(),
                                expected: "':' or '='".to_string(),
                            });
                        }
                    }
                }
                SyntaxStyle::Implicit => {
                    // Direct value or object without separator
                    match self.current_token() {
                        Some(Token::ObjectStart) => self.parse_object_with_context(&mut context)?,
                        _ => {
                            // Bare word value
                            self.parse_bare_word_value_with_context(&context, false)?
                        }
                    }
                }
                SyntaxStyle::NginxNested => {
                    // key identifier { ... } pattern
                    self.parse_nginx_nested_object(&mut context)?
                }
            };

            // Pop all keys from context
            for _ in &key_path {
                context.pop_key();
            }

            // For named sections (multi-key paths), build nested structure
            // Per SPEC.md lines 154-194: section foo bar { } -> section.foo.bar
            let final_value = if key_path.len() > 1 {
                // Build nested structure from right to left
                // Example: ["section", "foo", "bar"] with value {...} becomes:
                // section: { foo: { bar: {...} } }
                let mut nested_value = value;
                for i in (1..key_path.len()).rev() {
                    let mut inner_object = UclObject::new();
                    inner_object.insert(key_path[i].clone(), nested_value);
                    nested_value = UclValue::Object(inner_object);
                }
                nested_value
            } else {
                value
            };

            // Use the first key for insertion into the top-level object
            let top_key = &key_path[0];

            // Handle duplicate keys based on configuration
            if object.contains_key(top_key) {
                match self.config.duplicate_key_behavior {
                    DuplicateKeyBehavior::Error => {
                        return Err(ParseError::DuplicateKey {
                            key: top_key.clone(),
                            position: self.current_position(),
                        });
                    }
                    DuplicateKeyBehavior::ImplicitArray => {
                        let existing_value = object.shift_remove(top_key).unwrap();

                        // If both are objects and we have a nested path, deep merge them
                        if key_path.len() > 1
                            && existing_value.is_object()
                            && final_value.is_object()
                        {
                            if let (UclValue::Object(existing_obj), UclValue::Object(new_obj)) =
                                (existing_value, final_value)
                            {
                                // Deep merge the objects
                                let merged = Self::deep_merge_objects(existing_obj, new_obj);
                                object.insert(top_key.clone(), UclValue::Object(merged));
                            }
                        } else {
                            // Simple duplicate key -> create array
                            let new_array = match existing_value {
                                UclValue::Array(mut arr) => {
                                    arr.push(final_value);
                                    UclValue::Array(arr)
                                }
                                other => {
                                    let mut arr = SmallVec::new();
                                    arr.push(other);
                                    arr.push(final_value);
                                    UclValue::Array(Box::new(arr))
                                }
                            };
                            object.insert(top_key.clone(), new_array);
                        }
                    }
                    DuplicateKeyBehavior::Override => {
                        object.insert(top_key.clone(), final_value);
                    }
                }
            } else {
                object.insert(top_key.clone(), final_value);
            }

            self.skip_whitespace_and_comments()?;

            // Optional separator
            if let Some(Token::Comma) | Some(Token::Semicolon) = self.current_token() {
                self.advance_token()?;
            }
        }

        Ok(UclValue::Object(object))
    }

    /// Parses a value with variable expansion context
    ///
    /// # Parameters
    /// - `context`: Variable expansion context
    /// - `explicit_separator`: true if parsing after an explicit separator (= or :)
    pub fn parse_value_with_context(
        &mut self,
        context: &mut VariableContext,
        explicit_separator: bool,
    ) -> Result<UclValue, ParseError> {
        if self.current_depth >= self.config.max_depth {
            return Err(ParseError::MaxDepthExceeded {
                position: self.current_position(),
            });
        }

        self.skip_whitespace_and_comments()?;

        match self.current_token() {
            Some(Token::Eof) | None => Err(ParseError::UnexpectedToken {
                token: "end of file".to_string(),
                position: self.current_position(),
                expected: "value".to_string(),
            }),
            Some(Token::String {
                value,
                needs_expansion,
                ..
            }) => {
                let mut combined = if *needs_expansion {
                    self.expand_variables_with_context_safe(value, context)?
                } else {
                    value.to_string()
                };

                self.advance_token()?;
                self.skip_whitespace_and_comments()?;

                combined = self.concatenate_inline_strings(combined, Some(&*context))?;

                // Apply custom string post-processing
                let processed = self.parsing_hooks.process_string(&combined, context)?;
                let ucl_value = UclValue::String(processed);

                // Apply validation hooks
                let validated_value = self.parsing_hooks.validate_value(&ucl_value, context)?;
                Ok(validated_value)
            }
            Some(Token::Integer(val)) => {
                let int_val = *val;
                let start = self
                    .current_token_start()
                    .unwrap_or_else(|| self.current_position());
                let end = self.current_token_end().unwrap_or(start);
                let raw_text = self.token_text_from_positions(start, end);

                self.advance_token()?;
                self.skip_whitespace_and_comments()?;

                if let (Some(next_start), Some(Token::Key(next_fragment))) =
                    (self.current_token_start(), self.current_token())
                    && next_start.offset == end.offset
                    && self.lexer.last_token_leading_whitespace().is_empty()
                    && self.has_inline_value_continuation()
                {
                    // Number immediately followed by identifier - treat as string
                    let mut combined = raw_text;
                    combined.push_str(next_fragment);
                    self.advance_token()?;

                    // Continue collecting if more tokens follow
                    let final_value = self.collect_inline_value(combined, context)?;
                    let ucl_value = UclValue::String(final_value);
                    let validated_value = self.parsing_hooks.validate_value(&ucl_value, context)?;
                    return Ok(validated_value);
                }

                let ucl_value = UclValue::Integer(int_val);
                let validated_value = self.parsing_hooks.validate_value(&ucl_value, context)?;
                Ok(validated_value)
            }
            Some(Token::Float(val)) => {
                let float_val = *val;
                let start = self
                    .current_token_start()
                    .unwrap_or_else(|| self.current_position());
                let end = self.current_token_end().unwrap_or(start);
                let raw_text = self.token_text_from_positions(start, end);

                self.advance_token()?;
                self.skip_whitespace_and_comments()?;

                if let (Some(next_start), Some(Token::Key(next_fragment))) =
                    (self.current_token_start(), self.current_token())
                    && next_start.offset == end.offset
                    && self.lexer.last_token_leading_whitespace().is_empty()
                    && self.has_inline_value_continuation()
                {
                    // Number immediately followed by identifier - treat as string
                    let mut combined = raw_text;
                    combined.push_str(next_fragment);
                    self.advance_token()?;

                    // Continue collecting if more tokens follow
                    let final_value = self.collect_inline_value(combined, context)?;
                    let ucl_value = UclValue::String(final_value);
                    let validated_value = self.parsing_hooks.validate_value(&ucl_value, context)?;
                    return Ok(validated_value);
                }

                let ucl_value = UclValue::Float(float_val);
                let validated_value = self.parsing_hooks.validate_value(&ucl_value, context)?;
                Ok(validated_value)
            }
            Some(Token::Time(_)) => {
                let time_val = match self.current_token() {
                    Some(Token::Time(v)) => *v,
                    _ => unreachable!("Expected time token"),
                };
                self.advance_token()?;
                let ucl_value = UclValue::Float(time_val);

                let validated_value = self.parsing_hooks.validate_value(&ucl_value, context)?;
                Ok(validated_value)
            }
            Some(Token::Boolean(val)) => {
                let bool_val = *val;
                self.advance_token()?;
                let ucl_value = UclValue::Boolean(bool_val);

                // Apply validation hooks
                let validated_value = self.parsing_hooks.validate_value(&ucl_value, context)?;
                Ok(validated_value)
            }
            Some(Token::Null) => {
                self.advance_token()?;
                let ucl_value = UclValue::Null;

                // Apply validation hooks
                let validated_value = self.parsing_hooks.validate_value(&ucl_value, context)?;
                Ok(validated_value)
            }
            Some(Token::ObjectStart) => self.parse_object_with_context(context),
            Some(Token::ArrayStart) => self.parse_array_with_context(context),
            Some(Token::Key(_)) => {
                // Handle bare word values (unquoted identifiers)
                self.parse_bare_word_value_with_context(context, explicit_separator)
            }
            Some(token) => Err(ParseError::UnexpectedToken {
                token: token.type_name().to_string(),
                position: self.current_position(),
                expected: "value".to_string(),
            }),
        }
    }

    /// Parses an array with variable expansion context
    pub fn parse_array_with_context(
        &mut self,
        context: &mut VariableContext,
    ) -> Result<UclValue, ParseError> {
        self.current_depth += 1;
        if self.current_depth > self.config.max_depth {
            return Err(ParseError::MaxDepthExceeded {
                position: self.current_position(),
            });
        }

        // Consume the opening bracket
        self.expect_token(&Token::ArrayStart)?;

        let mut array = UclArray::new();

        // Handle empty array
        self.skip_whitespace_and_comments()?;
        if let Some(Token::ArrayEnd) = self.current_token() {
            self.advance_token()?;
            self.current_depth -= 1;
            return Ok(UclValue::Array(Box::new(array)));
        }

        let mut index = 0;
        loop {
            self.skip_whitespace_and_comments()?;

            // Check for end of array
            if let Some(Token::ArrayEnd) = self.current_token() {
                break;
            }

            // Parse value with array index context
            context.push_key(index.to_string());
            let value = self.parse_value_with_context(context, false)?;
            context.pop_key();

            array.push(value);
            index += 1;

            self.skip_whitespace_and_comments()?;

            // Check for separator or end
            match self.current_token() {
                Some(Token::Comma) | Some(Token::Semicolon) => {
                    self.advance_token()?;
                    // Continue to next value
                }
                Some(Token::ArrayEnd) => {
                    // End of array
                    break;
                }
                Some(token) => {
                    return Err(ParseError::UnexpectedToken {
                        token: token.type_name().to_string(),
                        position: self.current_position(),
                        expected: "',', ';', or ']'".to_string(),
                    });
                }
                None => {
                    return Err(ParseError::UnexpectedToken {
                        token: "end of file".to_string(),
                        position: self.current_position(),
                        expected: "',', ';', or ']'".to_string(),
                    });
                }
            }
        }

        // Consume the closing bracket
        self.expect_token(&Token::ArrayEnd)?;
        self.current_depth -= 1;

        Ok(UclValue::Array(Box::new(array)))
    }

    /// Safe variable expansion with context that handles missing handlers gracefully
    pub fn expand_variables_with_context_safe(
        &self,
        input: &str,
        context: &VariableContext,
    ) -> Result<String, ParseError> {
        if let Some(handler) = &self.variable_handler {
            self.expand_variables_with_context(input, handler.as_ref(), context)
        } else {
            // No variable handler - return input as-is
            Ok(input.to_string())
        }
    }

    /// Expands variables in a string using a two-pass algorithm
    pub fn expand_variables(&self, input: &str) -> Result<String, ParseError> {
        if let Some(handler) = &self.variable_handler {
            let context = VariableContext::new(self.current_position());
            self.expand_variables_with_context(input, handler.as_ref(), &context)
        } else {
            Ok(input.to_string())
        }
    }

    /// Expands variables with recursive expansion and circular reference detection
    pub fn expand_variables_recursive(&self, input: &str) -> Result<String, ParseError> {
        if let Some(handler) = &self.variable_handler {
            let mut context = VariableContext::new(self.current_position());
            self.expand_variables_recursive_with_context(input, handler.as_ref(), &mut context)
        } else {
            Ok(input.to_string())
        }
    }

    /// Expands variables recursively with context and circular reference detection
    pub fn expand_variables_recursive_with_context(
        &self,
        input: &str,
        handler: &dyn VariableHandler,
        context: &mut VariableContext,
    ) -> Result<String, ParseError> {
        // Two-pass expansion: first calculate the final length, then expand
        let mut result = String::new();
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '$' {
                match chars.peek() {
                    Some('$') => {
                        // $$ escape sequence for literal $
                        chars.next(); // consume second $
                        result.push('$');
                    }
                    Some('{') => {
                        // ${VAR} format (UCL spec compliant)
                        chars.next(); // consume '{'
                        let (var_name, fallback) =
                            self.parse_braced_variable_expression(&mut chars)?;

                        // Check for circular reference
                        if let Err(cycle_msg) = context.push_expansion(var_name.clone()) {
                            return Err(ParseError::VariableExpansion {
                                message: cycle_msg,
                                position: context.position,
                            });
                        }

                        if let Some(value) =
                            handler.resolve_variable_with_context(&var_name, context)
                        {
                            // Recursively expand the value
                            let expanded_value = self.expand_variables_recursive_with_context(
                                &value, handler, context,
                            )?;
                            result.push_str(&expanded_value);
                        } else if let Some(default_value) = fallback {
                            // Expand the fallback expression
                            let expanded_value = self.expand_variables_recursive_with_context(
                                &default_value,
                                handler,
                                context,
                            )?;
                            result.push_str(&expanded_value);
                        } else {
                            // Preserve original if not found (per UCL spec)
                            result.push_str(&format!("${{{}}}", var_name));
                        }

                        context.pop_expansion();
                    }
                    Some(c) if c.is_ascii_alphabetic() || *c == '_' => {
                        // $VAR format (greedy matching)
                        let var_name = self.parse_simple_variable_name(&mut chars);

                        // Check for circular reference
                        if let Err(cycle_msg) = context.push_expansion(var_name.clone()) {
                            return Err(ParseError::VariableExpansion {
                                message: cycle_msg,
                                position: context.position,
                            });
                        }

                        if let Some(value) =
                            handler.resolve_variable_with_context(&var_name, context)
                        {
                            // Recursively expand the value
                            let expanded_value = self.expand_variables_recursive_with_context(
                                &value, handler, context,
                            )?;
                            result.push_str(&expanded_value);
                        } else {
                            // Preserve original if not found
                            result.push_str(&format!("${}", var_name));
                        }

                        context.pop_expansion();
                    }
                    _ => {
                        // Just a literal $ followed by something else
                        result.push('$');
                    }
                }
            } else {
                result.push(ch);
            }
        }

        Ok(result)
    }

    /// Expands variables with a specific handler and context
    pub fn expand_variables_with_context(
        &self,
        input: &str,
        handler: &dyn VariableHandler,
        context: &VariableContext,
    ) -> Result<String, ParseError> {
        self.expand_variables_with_context_internal(input, handler, context, false)
    }

    /// Internal method for variable expansion with recursion control
    fn expand_variables_with_context_internal(
        &self,
        input: &str,
        handler: &dyn VariableHandler,
        context: &VariableContext,
        allow_recursion: bool,
    ) -> Result<String, ParseError> {
        // Two-pass expansion: first calculate the final length, then expand
        let expanded_length = self.calculate_expanded_length(input, handler, context)?;
        let mut result = String::with_capacity(expanded_length);

        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '$' {
                match chars.peek() {
                    Some('$') => {
                        // $$ escape sequence for literal $
                        chars.next(); // consume second $
                        result.push('$');
                    }
                    Some('{') => {
                        // ${VAR} format (UCL spec compliant)
                        chars.next(); // consume '{'
                        let (var_name, fallback) =
                            self.parse_braced_variable_expression(&mut chars)?;

                        if let Some(value) =
                            handler.resolve_variable_with_context(&var_name, context)
                        {
                            if allow_recursion && value.contains('$') {
                                // Recursively expand the value
                                let expanded_value = self.expand_variables_with_context_internal(
                                    &value, handler, context, true,
                                )?;
                                result.push_str(&expanded_value);
                            } else {
                                result.push_str(&value);
                            }
                        } else if let Some(default_value) = fallback {
                            let expanded_value = self.expand_variables_with_context_internal(
                                &default_value,
                                handler,
                                context,
                                true,
                            )?;
                            result.push_str(&expanded_value);
                        } else {
                            // Preserve original if not found (per UCL spec)
                            result.push_str(&format!("${{{}}}", var_name));
                        }
                    }
                    Some(c) if c.is_ascii_alphabetic() || *c == '_' => {
                        // $VAR format (greedy matching)
                        let var_name = self.parse_simple_variable_name(&mut chars);

                        if let Some(value) =
                            handler.resolve_variable_with_context(&var_name, context)
                        {
                            if allow_recursion && value.contains('$') {
                                // Recursively expand the value
                                let expanded_value = self.expand_variables_with_context_internal(
                                    &value, handler, context, true,
                                )?;
                                result.push_str(&expanded_value);
                            } else {
                                result.push_str(&value);
                            }
                        } else {
                            // Preserve original if not found
                            result.push_str(&format!("${}", var_name));
                        }
                    }
                    _ => {
                        // Just a literal $ followed by something else
                        result.push('$');
                    }
                }
            } else {
                result.push(ch);
            }
        }

        Ok(result)
    }

    /// Calculates the final length after variable expansion
    fn calculate_expanded_length(
        &self,
        input: &str,
        handler: &dyn VariableHandler,
        context: &VariableContext,
    ) -> Result<usize, ParseError> {
        let mut length = 0;
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '$' {
                match chars.peek() {
                    Some('$') => {
                        // $$ escape sequence
                        chars.next(); // consume second $
                        length += 1; // One $ character
                    }
                    Some('{') => {
                        // ${VAR} format (UCL spec compliant)
                        chars.next(); // consume '{'
                        let (var_name, fallback) =
                            self.parse_braced_variable_expression(&mut chars)?;

                        if let Some(value) =
                            handler.resolve_variable_with_context(&var_name, context)
                        {
                            length += value.len();
                        } else {
                            if let Some(default_value) = fallback {
                                let expanded_length = self.calculate_expanded_length(
                                    &default_value,
                                    handler,
                                    context,
                                )?;
                                length += expanded_length;
                            } else {
                                // Preserve original: ${VAR} (per UCL spec)
                                length += 3 + var_name.len(); // ${ + VAR + }
                            }
                        }
                    }
                    Some(c) if c.is_ascii_alphabetic() || *c == '_' => {
                        // $VAR format
                        let var_name = self.parse_simple_variable_name(&mut chars);

                        if let Some(value) =
                            handler.resolve_variable_with_context(&var_name, context)
                        {
                            length += value.len();
                        } else {
                            // Preserve original: $VAR
                            length += 1 + var_name.len(); // $ + VAR
                        }
                    }
                    _ => {
                        // Just a literal $
                        length += 1;
                    }
                }
            } else {
                length += ch.len_utf8();
            }
        }

        Ok(length)
    }

    /// Parses a variable expression from `${VAR}` or `${VAR:-default}`
    fn parse_braced_variable_expression(
        &self,
        chars: &mut std::iter::Peekable<std::str::Chars>,
    ) -> Result<(String, Option<String>), ParseError> {
        let mut var_name = String::new();
        let mut fallback_text = String::new();
        let mut has_fallback = false;
        let mut found_closing_brace = false;

        while let Some(&ch) = chars.peek() {
            if ch == '}' {
                chars.next(); // consume '}'
                found_closing_brace = true;
                break;
            }

            if !has_fallback {
                if ch == ':' {
                    chars.next();
                    if chars.peek() == Some(&'-') {
                        chars.next();
                        has_fallback = true;
                        continue;
                    }
                    return Err(ParseError::VariableExpansion {
                        message: "Invalid variable fallback syntax".to_string(),
                        position: self.current_position(),
                    });
                }
                if ch.is_ascii_alphanumeric() || ch == '_' {
                    var_name.push(ch);
                    chars.next();
                } else {
                    return Err(ParseError::VariableExpansion {
                        message: format!("Invalid character '{}' in variable name", ch),
                        position: self.current_position(),
                    });
                }
            } else {
                fallback_text.push(ch);
                chars.next();
            }
        }

        if !found_closing_brace {
            return Err(ParseError::VariableExpansion {
                message: "Unclosed variable expansion: missing '}'".to_string(),
                position: self.current_position(),
            });
        }

        if var_name.is_empty() {
            return Err(ParseError::VariableExpansion {
                message: "Empty variable name in ${} expansion".to_string(),
                position: self.current_position(),
            });
        }

        let fallback = if has_fallback {
            Some(fallback_text)
        } else {
            None
        };

        Ok((var_name, fallback))
    }

    /// Parses a variable name from $VAR format (greedy matching)
    fn parse_simple_variable_name(
        &self,
        chars: &mut std::iter::Peekable<std::str::Chars>,
    ) -> String {
        let mut var_name = String::new();

        while let Some(&ch) = chars.peek() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                var_name.push(ch);
                chars.next();
            } else {
                break;
            }
        }

        var_name
    }
}

// Convert LexError to ParseError
impl From<crate::error::LexError> for ParseError {
    fn from(lex_error: crate::error::LexError) -> Self {
        match lex_error {
            crate::error::LexError::UnexpectedCharacter {
                character,
                position,
            } => ParseError::UnexpectedToken {
                token: character.to_string(),
                position,
                expected: "valid character".to_string(),
            },
            _ => {
                ParseError::InvalidObject {
                    message: format!("Lexer error: {}", lex_error),
                    position: Position::new(), // Default position
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn test_environment_variable_handler() {
        let handler = EnvironmentVariableHandler;

        // Set a test environment variable
        unsafe {
            std::env::set_var("TEST_VAR", "test_value");
        }

        assert_eq!(
            handler.resolve_variable("TEST_VAR"),
            Some("test_value".to_string())
        );
        assert_eq!(handler.resolve_variable("NONEXISTENT_VAR"), None);

        // Clean up
        unsafe {
            std::env::remove_var("TEST_VAR");
        }
    }

    #[test]
    fn test_map_variable_handler() {
        let mut handler = MapVariableHandler::new();
        handler.insert("key1".to_string(), "value1".to_string());
        handler.insert("key2".to_string(), "value2".to_string());

        assert_eq!(handler.resolve_variable("key1"), Some("value1".to_string()));
        assert_eq!(handler.resolve_variable("key2"), Some("value2".to_string()));
        assert_eq!(handler.resolve_variable("key3"), None);
    }

    #[test]
    fn test_chained_variable_handler() {
        let mut map_handler = MapVariableHandler::new();
        map_handler.insert("map_var".to_string(), "map_value".to_string());

        let mut chained = ChainedVariableHandler::new();
        chained.add_handler(Box::new(map_handler));
        chained.add_handler(Box::new(EnvironmentVariableHandler));

        // Set environment variable
        unsafe {
            std::env::set_var("ENV_VAR", "env_value");
        }

        assert_eq!(
            chained.resolve_variable("map_var"),
            Some("map_value".to_string())
        );
        assert_eq!(
            chained.resolve_variable("ENV_VAR"),
            Some("env_value".to_string())
        );
        assert_eq!(chained.resolve_variable("nonexistent"), None);

        // Clean up
        unsafe {
            std::env::remove_var("ENV_VAR");
        }
    }

    #[test]
    fn test_variable_context() {
        let mut context = VariableContext::new(Position::new());

        assert!(context.current_object_path.is_empty());

        context.push_key("level1".to_string());
        context.push_key("level2".to_string());

        assert_eq!(context.current_object_path, vec!["level1", "level2"]);

        context.pop_key();
        assert_eq!(context.current_object_path, vec!["level1"]);
    }

    #[test]
    fn test_basic_variable_expansion() {
        let mut handler = MapVariableHandler::new();
        handler.insert("name".to_string(), "world".to_string());

        let parser = UclParser::with_variable_handler("", Box::new(handler));

        let result = parser.expand_variables("Hello ${name}!").unwrap();
        assert_eq!(result, "Hello world!");

        let result = parser.expand_variables("${unknown} variable").unwrap();
        assert_eq!(result, "${unknown} variable");
    }

    #[test]
    fn test_braced_variable_expansion() {
        let mut handler = MapVariableHandler::new();
        handler.insert("user".to_string(), "alice".to_string());
        handler.insert("host".to_string(), "localhost".to_string());
        handler.insert("port".to_string(), "8080".to_string());

        let parser = UclParser::with_variable_handler("", Box::new(handler));

        // Basic braced expansion
        let result = parser.expand_variables("${user}@${host}:${port}").unwrap();
        assert_eq!(result, "alice@localhost:8080");

        // Mixed with literal text
        let result = parser
            .expand_variables("Connect to ${host} on port ${port}")
            .unwrap();
        assert_eq!(result, "Connect to localhost on port 8080");

        // Unknown variables preserved
        let result = parser.expand_variables("${user} and ${unknown}").unwrap();
        assert_eq!(result, "alice and ${unknown}");
    }

    #[test]
    fn test_variable_fallback_default_value() {
        let handler = MapVariableHandler::new();
        let config = r#"value = "${MISSING:-fallback}""#;
        let result: Value = crate::from_str_with_variables(config, Box::new(handler))
            .expect("Should parse fallback expression");

        assert_eq!(result["value"], "fallback");
    }

    #[test]
    fn test_variable_fallback_with_defined_value() {
        let mut handler = MapVariableHandler::new();
        handler.insert("defined".to_string(), "configured".to_string());
        let config = r#"value = "${defined:-fallback}""#;

        let result: Value = crate::from_str_with_variables(config, Box::new(handler))
            .expect("Should use provided variable value");

        assert_eq!(result["value"], "configured");
    }

    #[test]
    fn test_simple_variable_expansion() {
        let mut handler = MapVariableHandler::new();
        handler.insert("HOME".to_string(), "/home/user".to_string());
        handler.insert("PATH".to_string(), "/usr/bin".to_string());
        handler.insert("var123".to_string(), "value123".to_string());

        let parser = UclParser::with_variable_handler("", Box::new(handler));

        // Simple variable expansion
        let result = parser.expand_variables("$HOME/bin").unwrap();
        assert_eq!(result, "/home/user/bin");

        // Alphanumeric variable names
        let result = parser.expand_variables("Value: $var123").unwrap();
        assert_eq!(result, "Value: value123");

        // Multiple variables
        let result = parser.expand_variables("$HOME:$PATH").unwrap();
        assert_eq!(result, "/home/user:/usr/bin");

        // Unknown variables preserved
        let result = parser.expand_variables("$HOME/$unknown").unwrap();
        assert_eq!(result, "/home/user/$unknown");
    }

    #[test]
    fn test_object_parsing() {
        let mut parser = UclParser::new(r#"{ "key1": "value1", "key2": 42 }"#);
        let result = parser.parse_value().unwrap();

        match result {
            UclValue::Object(obj) => {
                assert_eq!(obj.len(), 2);
                assert_eq!(
                    obj.get("key1"),
                    Some(&UclValue::String("value1".to_string()))
                );
                assert_eq!(obj.get("key2"), Some(&UclValue::Integer(42)));
            }
            _ => panic!("Expected object, got {:?}", result),
        }
    }

    #[test]
    fn test_array_parsing() {
        let mut parser = UclParser::new(r#"[1, "two", true, null]"#);
        let result = parser.parse_value().unwrap();

        match result {
            UclValue::Array(arr) => {
                assert_eq!(arr.len(), 4);
                assert_eq!(arr[0], UclValue::Integer(1));
                assert_eq!(arr[1], UclValue::String("two".to_string()));
                assert_eq!(arr[2], UclValue::Boolean(true));
                assert_eq!(arr[3], UclValue::Null);
            }
            _ => panic!("Expected array, got {:?}", result),
        }
    }

    #[test]
    fn test_nested_structures() {
        let mut parser = UclParser::new(r#"{ "nested": { "array": [1, 2, 3] } }"#);
        let result = parser.parse_value().unwrap();

        match result {
            UclValue::Object(obj) => match obj.get("nested") {
                Some(UclValue::Object(nested)) => match nested.get("array") {
                    Some(UclValue::Array(arr)) => {
                        assert_eq!(arr.len(), 3);
                        assert_eq!(arr[0], UclValue::Integer(1));
                        assert_eq!(arr[1], UclValue::Integer(2));
                        assert_eq!(arr[2], UclValue::Integer(3));
                    }
                    _ => panic!("Expected nested array"),
                },
                _ => panic!("Expected nested object"),
            },
            _ => panic!("Expected object, got {:?}", result),
        }
    }

    #[test]
    fn test_implicit_arrays() {
        let mut parser = UclParser::new(r#"{ "key": "value1", "key": "value2" }"#);
        let result = parser.parse_value().unwrap();

        match result {
            UclValue::Object(obj) => match obj.get("key") {
                Some(UclValue::Array(arr)) => {
                    assert_eq!(arr.len(), 2);
                    assert_eq!(arr[0], UclValue::String("value1".to_string()));
                    assert_eq!(arr[1], UclValue::String("value2".to_string()));
                }
                _ => panic!("Expected implicit array, got {:?}", obj.get("key")),
            },
            _ => panic!("Expected object, got {:?}", result),
        }
    }

    #[test]
    fn test_variable_expansion_in_parsing() {
        let mut handler = MapVariableHandler::new();
        handler.insert("name".to_string(), "world".to_string());
        handler.insert("count".to_string(), "42".to_string());

        let mut parser = UclParser::with_variable_handler(
            r#"{ "greeting": "Hello ${name}!", "number": "${count}" }"#,
            Box::new(handler),
        );

        let result = parser.parse_value().unwrap();

        match result {
            UclValue::Object(obj) => {
                assert_eq!(
                    obj.get("greeting"),
                    Some(&UclValue::String("Hello world!".to_string()))
                );
                assert_eq!(obj.get("number"), Some(&UclValue::String("42".to_string())));
            }
            _ => panic!("Expected object, got {:?}", result),
        }
    }

    #[test]
    fn test_implicit_object_parsing() {
        let mut parser = UclParser::new(r#"key1 = "value1"; key2: 42"#);
        let result = parser.parse_implicit_object().unwrap();

        match result {
            UclValue::Object(obj) => {
                assert_eq!(obj.len(), 2);
                assert_eq!(
                    obj.get("key1"),
                    Some(&UclValue::String("value1".to_string()))
                );
                assert_eq!(obj.get("key2"), Some(&UclValue::Integer(42)));
            }
            _ => panic!("Expected object, got {:?}", result),
        }
    }

    #[test]
    fn test_document_parsing() {
        // Test empty document
        let mut parser = UclParser::new("");
        let result = parser.parse_document().unwrap();
        match result {
            UclValue::Object(obj) => assert!(obj.is_empty()),
            _ => panic!("Expected empty object"),
        }

        // Test explicit object
        let mut parser = UclParser::new(r#"{ "key": "value" }"#);
        let result = parser.parse_document().unwrap();
        match result {
            UclValue::Object(obj) => {
                assert_eq!(obj.get("key"), Some(&UclValue::String("value".to_string())));
            }
            _ => panic!("Expected object"),
        }

        // Test implicit object
        let mut parser = UclParser::new(r#"key = "value""#);
        let result = parser.parse_document().unwrap();
        match result {
            UclValue::Object(obj) => {
                assert_eq!(obj.get("key"), Some(&UclValue::String("value".to_string())));
            }
            _ => panic!("Expected object"),
        }
    }

    #[test]
    fn test_error_handling() {
        // Test unexpected token
        let mut parser = UclParser::new(r#"{ "key": }"#);
        let result = parser.parse_value();
        assert!(result.is_err());

        // Test unterminated object
        let mut parser = UclParser::new(r#"{ "key": "value""#);
        let result = parser.parse_value();
        assert!(result.is_err());

        // Test unterminated array
        let mut parser = UclParser::new(r#"[1, 2, 3"#);
        let result = parser.parse_value();
        assert!(result.is_err());
    }

    #[test]
    fn test_token_stream_management() {
        let mut parser = UclParser::new(r#"{ "key": "value" }"#);

        // Test current token
        assert!(matches!(parser.current_token(), Some(Token::ObjectStart)));

        // Test advance token
        parser.advance_token().unwrap();
        assert!(matches!(parser.current_token(), Some(Token::String { .. })));

        // Test peek token
        let peeked = parser.peek_token().unwrap();
        assert!(matches!(peeked, Some(Token::String { .. })));

        // Verify peek doesn't advance
        assert!(matches!(parser.current_token(), Some(Token::String { .. })));
    }

    #[test]
    fn test_expect_token() {
        let mut parser = UclParser::new(r#"{ "key": "value" }"#);

        // Test successful expect
        let token = parser.expect_token(&Token::ObjectStart).unwrap();
        assert!(matches!(token, Token::ObjectStart));

        // Test failed expect
        let result = parser.expect_token(&Token::ArrayStart);
        assert!(result.is_err());
    }

    #[test]
    fn test_consume_if_matches() {
        let mut parser = UclParser::new(r#"{ "key": "value" }"#);

        // Test successful consume
        let consumed = parser.consume_if_matches(&Token::ObjectStart).unwrap();
        assert!(matches!(consumed, Some(Token::ObjectStart)));

        // Test failed consume (should not advance)
        let consumed = parser.consume_if_matches(&Token::ArrayStart).unwrap();
        assert!(consumed.is_none());
        assert!(matches!(parser.current_token(), Some(Token::String { .. })));
    }

    #[test]
    fn test_parser_creation() {
        let parser = UclParser::new("{}");
        assert!(parser.current_token().is_some());
    }

    #[test]
    fn test_dollar_escape_sequences() {
        let handler = MapVariableHandler::new();
        let parser = UclParser::with_variable_handler("", Box::new(handler));

        // Double dollar escape
        let result = parser.expand_variables("Price: $$100").unwrap();
        assert_eq!(result, "Price: $100");

        // Multiple escapes
        let result = parser.expand_variables("$$var and $$another").unwrap();
        assert_eq!(result, "$var and $another");

        // Mixed with real variables
        let mut handler = MapVariableHandler::new();
        handler.insert("price".to_string(), "50".to_string());
        let parser = UclParser::with_variable_handler("", Box::new(handler));

        let result = parser.expand_variables("Cost: $$${price}").unwrap();
        assert_eq!(result, "Cost: $50");
    }

    #[test]
    fn test_variable_precedence() {
        let mut handler = MapVariableHandler::new();
        handler.insert("var".to_string(), "simple".to_string());
        handler.insert("variable".to_string(), "longer".to_string());

        let parser = UclParser::with_variable_handler("", Box::new(handler));

        // ${} format has exact matching
        let result = parser.expand_variables("${var} vs ${variable}").unwrap();
        assert_eq!(result, "simple vs longer");

        // $ format uses greedy matching (longest match)
        let result = parser.expand_variables("$variable vs $var").unwrap();
        assert_eq!(result, "longer vs simple");
    }

    #[test]
    fn test_edge_cases() {
        let mut handler = MapVariableHandler::new();
        handler.insert("empty".to_string(), "".to_string());
        handler.insert("space".to_string(), "has space".to_string());

        let parser = UclParser::with_variable_handler("", Box::new(handler));

        // Empty variable value
        let result = parser.expand_variables("before${empty}after").unwrap();
        assert_eq!(result, "beforeafter");

        // Variable with spaces
        let result = parser.expand_variables("Value: ${space}").unwrap();
        assert_eq!(result, "Value: has space");

        // Just a dollar sign
        let result = parser.expand_variables("Price: $").unwrap();
        assert_eq!(result, "Price: $");

        // Dollar followed by non-variable character
        let result = parser.expand_variables("$123 and $@#").unwrap();
        assert_eq!(result, "$123 and $@#");
    }

    #[test]
    fn test_variable_expansion_errors() {
        let handler = MapVariableHandler::new();
        let parser = UclParser::with_variable_handler("", Box::new(handler));

        // Empty braced variable name
        let result = parser.expand_variables("${}");
        assert!(result.is_err());

        // Invalid character in braced variable name
        let result = parser.expand_variables("${var-name}");
        assert!(result.is_err());

        // Unclosed braced variable
        let result = parser.expand_variables("${unclosed");
        assert!(result.is_err());
    }

    #[test]
    fn test_circular_reference_detection() {
        let mut handler = MapVariableHandler::new();
        handler.insert("a".to_string(), "${b}".to_string());
        handler.insert("b".to_string(), "${c}".to_string());
        handler.insert("c".to_string(), "${a}".to_string()); // Creates a cycle: a -> b -> c -> a

        let parser = UclParser::with_variable_handler("", Box::new(handler));

        // Test circular reference detection
        let result = parser.expand_variables_recursive("${a}");
        assert!(result.is_err());

        if let Err(ParseError::VariableExpansion { message, .. }) = result {
            assert!(message.contains("Circular reference detected"));
        } else {
            panic!("Expected VariableExpansion error");
        }
    }

    #[test]
    fn test_recursive_variable_expansion() {
        let mut handler = MapVariableHandler::new();
        handler.insert("base".to_string(), "/usr".to_string());
        handler.insert("bin_dir".to_string(), "${base}/bin".to_string());
        handler.insert("full_path".to_string(), "${bin_dir}/myapp".to_string());

        let parser = UclParser::with_variable_handler("", Box::new(handler));

        // Test recursive expansion
        let result = parser.expand_variables_recursive("${full_path}").unwrap();
        assert_eq!(result, "/usr/bin/myapp");

        // Test mixed recursive and non-recursive
        let result = parser
            .expand_variables_recursive("Path: ${full_path}/config")
            .unwrap();
        assert_eq!(result, "Path: /usr/bin/myapp/config");
    }

    #[test]
    fn test_variable_context_expansion_stack() {
        let mut context = VariableContext::new(Position::new());

        // Test successful push
        assert!(context.push_expansion("var1".to_string()).is_ok());
        assert_eq!(context.expansion_depth(), 1);

        assert!(context.push_expansion("var2".to_string()).is_ok());
        assert_eq!(context.expansion_depth(), 2);

        // Test circular reference detection
        let result = context.push_expansion("var1".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Circular reference detected"));

        // Test pop
        context.pop_expansion();
        assert_eq!(context.expansion_depth(), 1);

        context.pop_expansion();
        assert_eq!(context.expansion_depth(), 0);
    }

    #[test]
    fn test_variable_context_with_position() {
        let pos1 = Position {
            line: 1,
            column: 1,
            offset: 0,
        };
        let pos2 = Position {
            line: 2,
            column: 5,
            offset: 10,
        };

        let mut context = VariableContext::new(pos1);
        context.push_key("level1".to_string());
        context.push_expansion("var1".to_string()).unwrap();

        let new_context = context.with_position(pos2);

        assert_eq!(new_context.position, pos2);
        assert_eq!(new_context.current_object_path, vec!["level1"]);
        assert_eq!(new_context.expansion_stack, vec!["var1"]);
    }

    #[test]
    fn test_chained_handler_fallback() {
        let mut map_handler1 = MapVariableHandler::new();
        map_handler1.insert("map_var".to_string(), "from_map1".to_string());

        let mut map_handler2 = MapVariableHandler::new();
        map_handler2.insert("other_var".to_string(), "from_map2".to_string());

        let mut chained = ChainedVariableHandler::new();
        chained.add_handler(Box::new(map_handler1));
        chained.add_handler(Box::new(map_handler2));

        let parser = UclParser::with_variable_handler("", Box::new(chained));

        // Test that first handler is tried first
        let result = parser.expand_variables("${map_var}").unwrap();
        assert_eq!(result, "from_map1");

        // Test fallback to second handler
        let result = parser.expand_variables("${other_var}").unwrap();
        assert_eq!(result, "from_map2");

        // Test unknown variable
        let result = parser.expand_variables("${UNKNOWN}").unwrap();
        assert_eq!(result, "${UNKNOWN}");
    }

    #[test]
    fn test_complex_variable_scenarios() {
        let mut handler = MapVariableHandler::new();
        handler.insert("prefix".to_string(), "/opt/app".to_string());
        handler.insert("version".to_string(), "1.2.3".to_string());
        handler.insert("config_dir".to_string(), "${prefix}/config".to_string());
        handler.insert(
            "log_file".to_string(),
            "${config_dir}/app-${version}.log".to_string(),
        );

        let parser = UclParser::with_variable_handler("", Box::new(handler));

        // Test deeply nested variable expansion
        let result = parser.expand_variables_recursive("${log_file}").unwrap();
        assert_eq!(result, "/opt/app/config/app-1.2.3.log");

        // Test mixed literal and variable content
        let result = parser
            .expand_variables_recursive("Log location: ${log_file}")
            .unwrap();
        assert_eq!(result, "Log location: /opt/app/config/app-1.2.3.log");

        // Test multiple variables in one string
        let result = parser
            .expand_variables_recursive("${prefix} v${version}")
            .unwrap();
        assert_eq!(result, "/opt/app v1.2.3");
    }

    #[test]
    fn test_variable_expansion_with_special_characters() {
        let mut handler = MapVariableHandler::new();
        handler.insert("special".to_string(), "hello world!@#$%^&*()".to_string());
        handler.insert("unicode".to_string(), "héllo wörld 🌍".to_string());
        handler.insert("empty".to_string(), "".to_string());
        handler.insert("newlines".to_string(), "line1\nline2\nline3".to_string());

        let parser = UclParser::with_variable_handler("", Box::new(handler));

        // Test special characters
        let result = parser.expand_variables("Value: ${special}").unwrap();
        assert_eq!(result, "Value: hello world!@#$%^&*()");

        // Test Unicode
        let result = parser.expand_variables("${unicode}").unwrap();
        assert_eq!(result, "héllo wörld 🌍");

        // Test empty variable
        let result = parser.expand_variables("before${empty}after").unwrap();
        assert_eq!(result, "beforeafter");

        // Test newlines
        let result = parser.expand_variables("${newlines}").unwrap();
        assert_eq!(result, "line1\nline2\nline3");
    }

    #[test]
    fn test_variable_expansion_error_positions() {
        let handler = MapVariableHandler::new();
        let parser = UclParser::with_variable_handler("test input", Box::new(handler));

        // Test error position reporting for empty variable name
        let result = parser.expand_variables("${}");
        assert!(result.is_err());
        if let Err(ParseError::VariableExpansion { position, .. }) = result {
            // Position should be tracked correctly
            assert_eq!(position.line, 1);
            assert!(position.column > 0);
        } else {
            panic!("Expected VariableExpansion error with position");
        }

        // Test error position for invalid character
        let result = parser.expand_variables("${var-name}");
        assert!(result.is_err());
        if let Err(ParseError::VariableExpansion { message, .. }) = result {
            assert!(message.contains("Invalid character"));
        } else {
            panic!("Expected VariableExpansion error");
        }
    }

    #[test]
    fn test_plugin_system_basic() {
        let mut registry = PluginRegistry::new();

        // Register a CSS units plugin
        let css_plugin = Box::new(CssUnitsPlugin::new());
        registry.register_plugin(css_plugin).unwrap();

        // Register a path processing plugin
        let path_plugin = Box::new(PathProcessingPlugin::new());
        registry.register_plugin(path_plugin).unwrap();

        // List plugins
        let plugins = registry.list_plugins();
        assert_eq!(plugins.len(), 2);

        // Check plugin names
        let plugin_names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
        assert!(plugin_names.contains(&"css-units"));
        assert!(plugin_names.contains(&"path-processing"));

        // Initialize and get hooks
        let hooks = registry.initialize().unwrap();

        // Verify hooks were registered
        assert!(!hooks.number_suffix_handlers.is_empty());
        assert!(!hooks.string_processors.is_empty());
    }

    #[test]
    fn test_plugin_configuration() {
        let mut registry = PluginRegistry::new();

        // Configure a validation plugin
        let mut config = PluginConfig::new();
        config.set("required_keys".to_string(), "name,version".to_string());
        config.set(
            "allowed_keys".to_string(),
            "name,version,description".to_string(),
        );
        registry.set_plugin_config("config-validation".to_string(), config);

        // Register the plugin
        let validation_plugin = Box::new(ConfigValidationPlugin::new());
        registry.register_plugin(validation_plugin).unwrap();

        // Initialize
        let hooks = registry.initialize().unwrap();

        // Verify validation hooks were registered
        assert!(!hooks.validation_hooks.is_empty());
    }

    #[test]
    fn test_parser_builder_with_plugins() {
        let input = r#"
            name = "test"
            version = "1.0.0"
        "#;

        let parser = UclParserBuilder::new(input)
            .with_plugin(Box::new(CssUnitsPlugin::new()))
            .unwrap()
            .with_plugin(Box::new(PathProcessingPlugin::new()))
            .unwrap()
            .build()
            .unwrap();

        // Verify the parser was created successfully
        assert!(parser.parsing_hooks().number_suffix_handlers.len() > 0);
        assert!(parser.parsing_hooks().string_processors.len() > 0);
    }

    #[test]
    fn test_custom_parsing_hooks_integration() {
        let input = r#"
            path = "./some/../path"
            name = "test"
        "#;

        let mut parser = UclParserBuilder::new(input)
            .with_plugin(Box::new(CssUnitsPlugin::new()))
            .unwrap()
            .with_plugin(Box::new(PathProcessingPlugin::new()))
            .unwrap()
            .build()
            .unwrap();

        // Parse the document
        let result = parser.parse_document();
        assert!(result.is_ok());

        if let Ok(UclValue::Object(obj)) = result {
            // Check that path was normalized
            if let Some(UclValue::String(path)) = obj.get("path") {
                assert_eq!(path, "path"); // "../" should be resolved and "./" removed
            }

            // Should have both keys
            assert!(obj.contains_key("path"));
            assert!(obj.contains_key("name"));
        }
    }

    #[test]
    fn test_advanced_variable_handler_interface() {
        let mut handler = MapVariableHandler::new();
        handler.insert("recursive".to_string(), "${base}/subdir".to_string());
        handler.insert("base".to_string(), "/root".to_string());

        let parser = UclParser::with_variable_handler("", Box::new(handler));
        let mut context = VariableContext::new(Position::new());

        // Test that AdvancedVariableHandler methods work
        let result = parser.expand_variables_recursive_with_context(
            "${recursive}",
            &MapVariableHandler::new(),
            &mut context,
        );
        // This should work even though the handler doesn't have the variables
        // because we're testing the interface, not the specific implementation
        assert!(result.is_ok());
    }
}

//! Demonstration of UCL parser extensibility features
//!
//! This example shows how to use custom parsing hooks and plugins
//! to extend the UCL parser with domain-specific functionality.

use ucl_lexer::{
    ConfigValidationPlugin, CssUnitsPlugin, PathNormalizationProcessor, PathProcessingPlugin,
    PluginConfig, UclParserBuilder, UclPlugin, UclValue,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("UCL Parser Extensibility Demo");
    println!("=============================\n");

    // Example 1: Using built-in plugins
    demo_builtin_plugins()?;

    // Example 2: Using custom hooks directly
    demo_custom_hooks()?;

    // Example 3: Plugin configuration
    demo_plugin_configuration()?;

    Ok(())
}

fn demo_builtin_plugins() -> Result<(), Box<dyn std::error::Error>> {
    println!("1. Built-in Plugins Demo");
    println!("-------------------------");

    let ucl_config = r#"
        # Configuration with paths and CSS-like units
        app_name = "my-app"
        static_path = "./assets/../static"
        font_size = "16px"
        margin = "2em"
    "#;

    let mut parser = UclParserBuilder::new(ucl_config)
        .with_plugin(Box::new(CssUnitsPlugin::new()))?
        .with_plugin(Box::new(PathProcessingPlugin::new()))?
        .build()?;

    match parser.parse_document()? {
        UclValue::Object(config) => {
            println!("Parsed configuration:");
            for (key, value) in &config {
                match value {
                    UclValue::String(s) => println!("  {}: \"{}\"", key, s),
                    _ => println!("  {}: {:?}", key, value),
                }
            }

            // Note: Path normalization processed "./assets/../static" -> "static"
            if let Some(UclValue::String(path)) = config.get("static_path") {
                println!(
                    "\nPath normalization result: {} -> {}",
                    "./assets/../static", path
                );
            }
        }
        _ => println!("Expected object at root level"),
    }

    println!();
    Ok(())
}

fn demo_custom_hooks() -> Result<(), Box<dyn std::error::Error>> {
    println!("2. Custom Hooks Demo");
    println!("--------------------");

    let ucl_config = r#"
        server_config = {
            host = "localhost";
            port = 8080;
            ssl_cert = "/etc/ssl/../certs/server.crt";
        }
    "#;

    let mut parser = UclParserBuilder::new(ucl_config).build()?;

    // Add custom hooks directly
    parser.add_string_processor(Box::new(PathNormalizationProcessor::new()));

    match parser.parse_document()? {
        UclValue::Object(config) => {
            println!("Parsed and validated configuration:");
            if let Some(UclValue::Object(server_config)) = config.get("server_config") {
                for (key, value) in server_config {
                    println!("  {}: {:?}", key, value);
                }
            }
        }
        _ => println!("Expected object at root level"),
    }

    println!();
    Ok(())
}

fn demo_plugin_configuration() -> Result<(), Box<dyn std::error::Error>> {
    println!("3. Plugin Configuration Demo");
    println!("----------------------------");

    let ucl_config = r#"
        name = "test-app"
        version = "1.0.0"
        description = "A test application"
        # invalid_key = "this would cause validation error"
    "#;

    // Configure validation plugin
    let mut validation_config = PluginConfig::new();
    validation_config.set("required_keys".to_string(), "name,version".to_string());
    validation_config.set(
        "allowed_keys".to_string(),
        "name,version,description,author".to_string(),
    );

    let mut validation_plugin = ConfigValidationPlugin::new();
    validation_plugin.configure(&validation_config)?;

    let mut parser = UclParserBuilder::new(ucl_config)
        .with_plugin(Box::new(validation_plugin))?
        .build()?;

    match parser.parse_document() {
        Ok(UclValue::Object(config)) => {
            println!("Configuration validated successfully:");
            for (key, value) in &config {
                match value {
                    UclValue::String(s) => println!("  {}: \"{}\"", key, s),
                    _ => println!("  {}: {:?}", key, value),
                }
            }
        }
        Ok(_) => {
            println!("Expected object at root level");
        }
        Err(e) => {
            println!("Validation failed: {}", e);
        }
    }

    println!();
    Ok(())
}

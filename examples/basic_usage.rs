//! Basic usage example for the UCL lexer
//!
//! This example demonstrates how to use the UCL lexer to parse
//! configuration files into Rust structs.

use serde::Deserialize;
use ucl_lexer::{UclError, from_str};

#[derive(Debug, Deserialize, PartialEq)]
struct ServerConfig {
    name: String,
    port: u16,
    debug: bool,
}

#[derive(Debug, Deserialize)]
struct DatabaseConfig {
    host: String,
    port: u16,
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct AppConfig {
    server: ServerConfig,
    database: DatabaseConfig,
}

fn main() -> Result<(), UclError> {
    // Example UCL configuration
    let ucl_text = r#"
        server {
            name = "my-app"
            port = 8080
            debug = true
        }
        
        database {
            host = "localhost"
            port = 5432
            username = "admin"
            password = "secret"
        }
    "#;

    // This will fail for now since parsing isn't fully implemented
    match from_str::<AppConfig>(ucl_text) {
        Ok(config) => {
            println!("Parsed configuration:");
            println!(
                "Server: {} on port {}",
                config.server.name, config.server.port
            );
            println!(
                "Database: {}:{}",
                config.database.host, config.database.port
            );
            println!("Database user: {}", config.database.username);
            println!(
                "Database password length: {}",
                config.database.password.len()
            );
        }
        Err(e) => {
            println!("Failed to parse configuration: {}", e);
            println!("This is expected until parsing is fully implemented.");
        }
    }

    Ok(())
}

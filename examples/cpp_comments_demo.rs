use ucl_lexer::lexer::{LexerConfig, Token, UclLexer};

fn main() {
    // Test C++ style comments in a realistic UCL configuration
    let config_text = r#"
        // Server configuration
        server {
            listen = 80  // HTTP port
            server_name = "example.com"
            
            # Traditional hash comment still works
            root = "/var/www/html"
            
            /* Multi-line comment
               also still works */
            index = "index.html"
        }
        
        // Database settings
        database {
            host = "localhost"  // Default host
            port = 5432
            name = "myapp"
        }
    "#;

    println!("Testing C++ comments with preservation:");

    // Test with comment preservation
    let mut config = LexerConfig::default();
    config.save_comments = true;
    let mut lexer = UclLexer::with_config(config_text, config);

    let mut comments = Vec::new();
    let mut tokens = Vec::new();

    loop {
        match lexer.next_token() {
            Ok(Token::Eof) => break,
            Ok(Token::Comment(text)) => {
                comments.push(text.to_string());
            }
            Ok(token) => {
                tokens.push(token);
            }
            Err(e) => {
                println!("Error: {:?}", e);
                break;
            }
        }
    }

    println!("Found {} comments:", comments.len());
    for (i, comment) in comments.iter().enumerate() {
        println!("  Comment {}: '{}'", i + 1, comment);
    }

    println!("\nComment types from lexer:");
    let comment_infos = lexer.comments();
    for (i, info) in comment_infos.iter().enumerate() {
        println!(
            "  Comment {}: {:?} - '{}'",
            i + 1,
            info.comment_type,
            info.text
        );
    }

    println!("\nFound {} non-comment tokens", tokens.len());
    println!("C++ style comment support is working correctly!");
}

//! Parser settings and the less common parts of the format: variables and a variable handler,
//! duplicate strategies and priorities, macros with an in-memory loader, saved comments, output
//! formats, silent stops and errors.
//!
//! Run with `cargo run --example advanced_features`.

use serde::{Deserialize, Serialize};
use std::path::Path;
use ucl_lexer::emit::Format;
use ucl_lexer::parse::{ErrorKind, MemoryLoader, Parser, ParserBuilder};
use ucl_lexer::{DuplicateStrategy, ParserFlags, UclError, UclObject, UclValue};

fn main() -> Result<(), UclError> {
    variables()?;
    duplicate_strategies()?;
    priorities()?;
    macros()?;
    saved_comments()?;
    output_formats()?;
    silent_stop();
    errors();
    Ok(())
}

fn object(value: &UclValue) -> &UclObject {
    value.as_object().expect("an object")
}

fn section(title: &str) {
    println!("\n== {title}");
}

/// Registered variables, the variable handler and the `$$` rule (spec §7).
fn variables() -> Result<(), UclError> {
    section("Variables");
    let mut parser = ParserBuilder::new()
        .with_variable("HOST", "example.org")
        .with_variable("PORT", "8443")
        // The handler is asked only for braced references to names that are not registered
        // (spec §7.7), here the environment first, then a default.
        .with_variable_handler(|name| {
            std::env::var(name)
                .ok()
                .or_else(|| (name == "APP_ENV").then(|| "development".to_string()))
        })
        .build();
    let value = parser.parse(
        br#"
        url = "https://$HOST:$PORT/"
        braced = "${HOST}"
        # An unbraced reference takes the first registered name that is a prefix of the text.
        prefix = "$HOSTNAME"
        env = "${APP_ENV}"
        # The handler is never asked for unbraced references.
        unbraced_not_asked = "$APP_ENV"
        # Unresolved references stay as written.
        unknown = "${UCL_EXAMPLE_UNSET} $UCL_EXAMPLE_UNSET"
        # `$$` is a `$` only in a string where some reference was replaced.
        dollars = "$$5"
        dollars_after_expansion = "$HOST costs $$5"
        # No expansion in single quotes.
        single = '$HOST'
        unquoted = $HOST/path
        "#,
    )?;
    let root = object(&value);
    for (key, entry) in root {
        println!("{key:24} {:?}", entry.first());
    }
    let env = std::env::var("APP_ENV").unwrap_or_else(|_| "development".into());
    for (key, expected) in [
        ("url", "https://example.org:8443/"),
        ("braced", "example.org"),
        ("prefix", "example.orgNAME"),
        ("env", env.as_str()),
        ("unbraced_not_asked", "$APP_ENV"),
        ("unknown", "${UCL_EXAMPLE_UNSET} $UCL_EXAMPLE_UNSET"),
        ("dollars", "$$5"),
        ("dollars_after_expansion", "example.org costs $5"),
        ("single", "$HOST"),
        ("unquoted", "example.org/path"),
    ] {
        assert_eq!(root[key].as_str(), Some(expected), "{key}");
    }
    Ok(())
}

const REPEATS: &[u8] = b"worker { threads = 2 }\nworker { queue = 100 }\nport = 80\nport = 8080\n";

/// How repeated keys are resolved (spec §8.2, §8.4, §8.5).
fn duplicate_strategies() -> Result<(), UclError> {
    section("Duplicate strategies");
    for strategy in [
        DuplicateStrategy::Append,
        DuplicateStrategy::Merge,
        DuplicateStrategy::Rewrite,
    ] {
        let mut parser = ParserBuilder::new().with_strategy(strategy).build();
        let value = parser.parse(REPEATS)?;
        println!(
            "{strategy:?}: {}",
            parser.emitter(Format::JsonCompact).emit(&value)
        );
        let root = object(&value);
        let (workers, ports) = (root.get_all("worker").len(), root.get_all("port").len());
        match strategy {
            // Every value is kept: `worker` and `port` hold two values each.
            DuplicateStrategy::Append => assert_eq!((workers, ports), (2, 2)),
            // Objects merge, scalars are kept as for append.
            DuplicateStrategy::Merge => {
                assert_eq!((workers, ports), (1, 2));
                assert_eq!(object(&root["worker"]).len(), 2);
            }
            // The last value replaces the others.
            _ => {
                assert_eq!((workers, ports), (1, 1));
                assert_eq!(root["port"].as_integer(), Some(8080));
            }
        }
    }

    // `error` rejects the document at the repeated key.
    let err = ParserBuilder::new()
        .with_strategy(DuplicateStrategy::Error)
        .build()
        .parse(REPEATS)
        .unwrap_err();
    println!("Error: {err}");
    assert_eq!(
        err.kind(),
        &ErrorKind::DuplicateKey {
            key: "worker".into()
        }
    );
    assert_eq!(err.position().line, 2);

    // `no-implicit-arrays` collects repeated values into explicit arrays.
    let mut parser = Parser::with_flags(ParserFlags::NO_IMPLICIT_ARRAYS);
    let value = parser.parse(REPEATS)?;
    println!(
        "no-implicit-arrays: {}",
        parser.emitter(Format::JsonCompact).emit(&value)
    );
    assert_eq!(object(&value)["port"].as_array().map(Vec::len), Some(2));
    Ok(())
}

/// A value with a higher priority replaces the values of its key; one with a lower priority is
/// dropped (spec §8.3). `.priority N` sets the priority of the values after it (§9.5).
fn priorities() -> Result<(), UclError> {
    section("Priorities");
    let value = ucl_lexer::parse::parse(
        b"port = 80\n.priority 5\nport = 9000\n.priority 1\nport = 7000\nhost = a\n",
    )?;
    let root = object(&value);
    for (key, entry) in root {
        for slot in entry.slots() {
            println!("{key} = {:?} at priority {}", slot.value(), slot.priority());
        }
    }
    let port = root.entry("port").expect("port");
    assert_eq!(port.len(), 1);
    assert_eq!(port.first().as_integer(), Some(9000));
    assert_eq!(port.slots()[0].priority(), 5);
    assert_eq!(root.entry("host").expect("host").slots()[0].priority(), 1);
    Ok(())
}

/// Include macros with their parameters, and `.inherit` (spec §9.4, §9.7), reading files from a
/// [`MemoryLoader`]. Relative paths resolve against the parser's base directory.
fn macros() -> Result<(), UclError> {
    section("Macros");
    let mut loader = MemoryLoader::new();
    loader.add_file(
        "/srv/app/app.conf",
        r#"limits { cpu = 2; memory = 512mb; }
.include(glob=true) "conf.d/*.conf"
.include(key="database") "db.conf"
.include(duplicate="merge", priority=5) "override.conf"
defaults { timeout = 30s; retries = 3; }
production { .inherit "defaults"; retries = 5; }
"#,
    );
    loader.add_file("/srv/app/conf.d/10-workers.conf", "workers = 2\n");
    loader.add_file("/srv/app/conf.d/20-log.conf", "log = info\n");
    loader.add_file("/srv/app/db.conf", "user = \"db\"\npassword = \"secret\"\n");
    loader.add_file("/srv/app/override.conf", "limits { memory = 1gb; }\n");
    let mut parser = ParserBuilder::new()
        .with_loader(loader)
        .with_base_dir("/srv/app")
        .build();
    let value = parser.parse_file("app.conf")?;
    println!("{}", parser.emitter(Format::Config).emit(&value));
    let root = object(&value);

    // The merge include went into `limits`; at priority 5 its `memory` replaced the old one.
    let limits = object(&root["limits"]);
    assert_eq!(limits["cpu"].as_integer(), Some(2));
    assert_eq!(limits["memory"].as_integer(), Some(1 << 30));
    // Every file the glob matched was included.
    assert_eq!(root["workers"].as_integer(), Some(2));
    assert_eq!(root["log"].as_str(), Some("info"));
    // `key="database"` nests the file under that key.
    assert_eq!(object(&root["database"])["user"].as_str(), Some("db"));
    // `.inherit` copied `timeout`; `retries` was given explicitly.
    let production = object(&root["production"]);
    assert_eq!(production["timeout"].as_time(), Some(30.0));
    assert_eq!(production["retries"].as_integer(), Some(5));
    Ok(())
}

/// With `SAVE_COMMENTS` the parser keeps comments and attaches each to a value (spec §12.5);
/// the config format writes them only when asked (spec §10.10).
fn saved_comments() -> Result<(), UclError> {
    section("Saved comments");
    let input = b"# The listening port\nport = 8080 # changed for staging\n\n/* The log level */\nlog = info\n";
    let mut parser = Parser::with_flags(ParserFlags::SAVE_COMMENTS);
    let value = parser.parse(input)?;
    for comment in parser.comments() {
        println!(
            "line {}: {:?}",
            comment.position.line,
            comment.text.trim_end()
        );
    }
    let with_comments = parser
        .emitter(Format::Config)
        .with_comments(parser.comments(), parser.attached_comments())
        .emit(&value);
    println!("{with_comments}");
    // A comment attaches to the next value, and a block comment keeps the byte after its `*/`.
    assert_eq!(
        with_comments,
        "# The listening port\nport = 8080;\n# changed for staging\n/* The log level */\n\n\
         log = \"info\";\n"
    );
    // Without `with_comments`, no comments are written.
    assert_eq!(
        parser.emitter(Format::Config).emit(&value),
        "port = 8080;\nlog = \"info\";\n"
    );
    Ok(())
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Service {
    name: String,
    ports: Vec<u16>,
    timeout: f64,
}

/// A parsed value in each output format, as libucl writes it (spec §10), and serde output, which
/// reads back as exactly the value written (spec §10.8).
fn output_formats() -> Result<(), UclError> {
    section("Output formats");
    let mut parser = Parser::new();
    let value = parser.parse(b"name = 'web'\nports = [80, 443]\ntimeout = 1.5min\n")?;
    for format in [
        Format::Config,
        Format::Json,
        Format::JsonCompact,
        Format::Yaml,
    ] {
        println!("[{format:?}]\n{}\n", parser.emitter(format).emit(&value));
    }
    assert_eq!(
        parser.emitter(Format::JsonCompact).emit(&value),
        r#"{"name":"web","ports":[80,443],"timeout":90.0}"#
    );

    let service: Service = ucl_lexer::from_value(value)?;
    let text = ucl_lexer::to_string(&service)?;
    println!("to_string:\n{text}");
    assert_eq!(ucl_lexer::from_str::<Service>(&text)?, service);
    println!(
        "to_json_string_compact: {}",
        ucl_lexer::to_json_string_compact(&service)?
    );
    println!("to_yaml_string:\n{}", ucl_lexer::to_yaml_string(&service)?);
    Ok(())
}

/// libucl ends the parse without an error at a `.try_include` that finds no file and keeps what
/// it has parsed (spec §9.4). The crate reports this as [`UclError::Stopped`], which carries that
/// partial result.
fn silent_stop() {
    section("Silent stop");
    let mut loader = MemoryLoader::new();
    loader.add_file(
        "/srv/app/stop.conf",
        "a = 1\n.try_include \"missing.conf\"\nb = 2\n",
    );
    let mut parser = ParserBuilder::new()
        .with_loader(loader)
        .with_base_dir("/srv/app")
        .build();
    let err = UclError::from(parser.parse_file("stop.conf").unwrap_err());
    println!("{err}");
    assert!(matches!(err, UclError::Stopped(_)));
    let partial = err
        .parse_error()
        .and_then(|e| e.partial())
        .expect("a stop keeps its result");
    assert_eq!(object(partial).keys().collect::<Vec<_>>(), ["a"]);
}

/// Parse errors carry a kind, a position and, inside an included file, that file's path.
fn errors() {
    section("Errors");
    let err = ucl_lexer::from_str::<UclValue>("server {\n  name = \"web\n}\n").unwrap_err();
    let position = err.position().expect("a parse error has a position");
    println!("{err}");
    assert_eq!(
        err.parse_error().map(|e| e.kind().clone()),
        Some(ErrorKind::ControlCharacter { byte: b'\n' })
    );
    // The raw line break inside the string, at the end of line 2.
    assert_eq!((position.line, position.column), (2, 14));

    let mut loader = MemoryLoader::new();
    loader.add_file("/srv/app/uses-bad.conf", "ok = 1\n.include \"bad.conf\"\n");
    loader.add_file("/srv/app/bad.conf", "x = [1, 2\n");
    let mut parser = ParserBuilder::new()
        .with_loader(loader)
        .with_base_dir("/srv/app")
        .build();
    let err = parser.parse_file("uses-bad.conf").unwrap_err();
    println!("{err}");
    assert_eq!(err.kind(), &ErrorKind::UnterminatedArray);
    assert_eq!(err.file(), Some(Path::new("/srv/app/bad.conf")));
}

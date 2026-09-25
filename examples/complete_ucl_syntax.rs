//! A tour of UCL syntax as libucl reads it (spec §1–§9): entries and separators, objects and
//! arrays, named sections, repeated keys, the string forms, comments, keywords, numbers,
//! variables and the include and priority macros.
//!
//! Run with `cargo run --example complete_ucl_syntax`.

use ucl_lexer::emit::Format;
use ucl_lexer::parse::{MemoryLoader, ParserBuilder};
use ucl_lexer::{UclError, UclObject, UclValue};

const TOUR: &str = r#"
# Entries: the key and its value are separated by `=`, `:` or only whitespace. An entry ends at
# a line break, `;` or `,`.
name = "tour";
version: 2,
enabled true

# Objects and arrays. JSON is valid UCL.
limits { cpu = 2; memory = 512mb; }
tags = [web, "api", 3]
json_style { "key": "value", "list": [1, 2] }

# Named sections: `a b { ... }` is `a { b { ... } }`, as in nginx.
server "example.org" {
    listen = 443
    location "/api" {
        proxy_pass = "http://127.0.0.1:8080"
    }
}

# A repeated key holds several values (an implicit array).
upstream = a.example
upstream = b.example

# Strings: double-quoted with JSON escapes, single-quoted literal, unquoted, heredoc.
double = "tab\there, é"
single = 'no $expansion, no \t escapes'
unquoted = /usr/local/etc
heredoc = <<EOD
first line
  second line
EOD

/* Block comments /* nest */ and may
   span lines. */
# Keywords: booleans are case-insensitive, `null`, `inf` and `nan` lowercase only.
yes_value = yes
off_value = OFF
nothing = null

# Variables expand in double-quoted, unquoted and heredoc strings.
data_dir = "$ROOT/data"
cache_dir = ${ROOT}/cache
"#;

fn main() -> Result<(), UclError> {
    syntax_tour()?;
    macros()?;
    Ok(())
}

fn object(value: &UclValue) -> &UclObject {
    value.as_object().expect("an object")
}

fn syntax_tour() -> Result<(), UclError> {
    let mut parser = ParserBuilder::new().with_variable("ROOT", "/srv").build();
    let value = parser.parse(TOUR.as_bytes())?;
    let root = object(&value);

    // Entries and separators.
    assert_eq!(root["name"].as_str(), Some("tour"));
    assert_eq!(root["version"].as_integer(), Some(2));
    assert_eq!(root["enabled"].as_bool(), Some(true));

    // Objects and arrays.
    assert_eq!(
        object(&root["limits"])["memory"].as_integer(),
        Some(512 * 1024 * 1024)
    );
    let tags = root["tags"].as_array().expect("an array");
    assert_eq!(tags[0].as_str(), Some("web"));
    assert_eq!(tags[2].as_integer(), Some(3));
    assert_eq!(object(&root["json_style"])["key"].as_str(), Some("value"));

    // Named sections become nested objects.
    let site = object(&object(&root["server"])["example.org"]);
    assert_eq!(site["listen"].as_integer(), Some(443));
    let api = object(&object(&site["location"])["/api"]);
    assert_eq!(api["proxy_pass"].as_str(), Some("http://127.0.0.1:8080"));

    // A repeated key: one entry with two values, in order.
    let upstream: Vec<_> = root
        .get_all("upstream")
        .filter_map(UclValue::as_str)
        .collect();
    assert_eq!(upstream, ["a.example", "b.example"]);

    // Strings.
    assert_eq!(root["double"].as_str(), Some("tab\there, é"));
    assert_eq!(
        root["single"].as_str(),
        Some(r"no $expansion, no \t escapes")
    );
    assert_eq!(root["unquoted"].as_str(), Some("/usr/local/etc"));
    assert_eq!(root["heredoc"].as_str(), Some("first line\n  second line"));

    // Keywords.
    assert_eq!(root["yes_value"].as_bool(), Some(true));
    assert_eq!(root["off_value"].as_bool(), Some(false));
    assert!(root["nothing"].is_null());

    // Variables.
    assert_eq!(root["data_dir"].as_str(), Some("/srv/data"));
    assert_eq!(root["cache_dir"].as_str(), Some("/srv/cache"));

    // The same document in libucl's config output: sections and repeated keys as written.
    println!("== The tour document in the UCL config format\n");
    println!("{}", parser.emitter(Format::Config).emit(&value));
    Ok(())
}

/// `.include` reads another file into the document; `.include(priority=N)` gives its values a
/// priority, so that they replace values of lower priority (spec §8.3, §9.4). `.priority N`
/// changes the priority of the values after it (§9.5). Files come from the parser's loader:
/// here a [`MemoryLoader`]; `FsLoader` reads the filesystem, and a default parser reads no
/// files.
fn macros() -> Result<(), UclError> {
    let mut loader = MemoryLoader::new();
    loader.add_file(
        "/etc/app/main.conf",
        "workers = 4\nlog = \"info\"\n.include(priority=5) \"local.conf\"\n\n\
         timeout = 10s\n.priority 3\ntimeout = 30s\n.priority 1\ntimeout = 5s\n",
    );
    loader.add_file("/etc/app/local.conf", "log = \"debug\"\n");
    // Relative include paths resolve against the base directory, never against the including
    // file (spec §9.3).
    let mut parser = ParserBuilder::new()
        .with_loader(loader)
        .with_base_dir("/etc/app")
        .build();
    let value = parser.parse_file("main.conf")?;
    let root = object(&value);

    assert_eq!(root["workers"].as_integer(), Some(4));
    // `local.conf` has priority 5, so its `log` replaces the main file's.
    assert_eq!(root["log"].as_str(), Some("debug"));
    assert_eq!(root.entry("log").expect("log").slots()[0].priority(), 5);
    // The value at priority 3 replaces the one at 0; the one at 1 is dropped.
    assert_eq!(root["timeout"].as_time(), Some(30.0));
    assert_eq!(root.entry("timeout").expect("timeout").len(), 1);

    println!("== main.conf with its include, as JSON\n");
    println!("{}", parser.emitter(Format::Json).emit(&value));
    Ok(())
}

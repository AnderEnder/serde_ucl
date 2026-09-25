# ucl-rust-lexer

[![CI](https://github.com/AnderEnder/ucl-rust-lexer/actions/workflows/ci.yml/badge.svg)](https://github.com/AnderEnder/ucl-rust-lexer/actions/workflows/ci.yml)

UCL (Universal Configuration Language) for Rust, with serde. The crate reads and writes UCL as
[libucl](https://github.com/vstakhov/libucl), the C library used by FreeBSD, does. That is
checked, not assumed: the conformance suite parses more than 1,300 documents and compares each
result, and each value written in each output format, with libucl's own output. The places where
the crate deliberately differs from libucl, and the libucl quirks it reproduces, are listed in
[docs/COMPATIBILITY.md](docs/COMPATIBILITY.md). The implementation is written independently of
libucl's source code, from a behaviour specification and libucl's observable output.

- Deserialize UCL into any `serde::Deserialize` type, or into the `UclValue` tree.
- Serialize any `serde::Serialize` value as UCL, JSON or YAML, in forms that read back as the same
  value, floats included.
- Parser settings: libucl's parser flags, duplicate-key strategies and priorities, variables and
  a variable handler, macros (`.include`, `.try_include`, `.priority`, `.inherit`, `.load`) with
  pluggable file loaders.
- Emitters that write a parsed document byte for byte as libucl writes it.

The package is `ucl-rust-lexer`; the library is `ucl_lexer`.

## Installation

The crate is not on crates.io yet. Depend on the repository:

```toml
[dependencies]
ucl-rust-lexer = { git = "https://github.com/AnderEnder/ucl-rust-lexer" }
serde = { version = "1", features = ["derive"] }
```

The minimum supported Rust version is 1.88.

## Reading configuration

`from_str`, `from_slice` and `from_reader` parse a document given as text and deserialize it:

```rust
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct Config {
    name: String,
    port: u16,
    #[serde(with = "ucl_lexer::time")]
    timeout: Duration,
    max_body: u64,
    upstream: Vec<String>,
    tls: Tls,
}

#[derive(Debug, Deserialize)]
struct Tls {
    enabled: bool,
    cert: String,
}

fn main() -> Result<(), ucl_lexer::UclError> {
    let text = r#"
# A comment. /* Block comments */ work too.
name = "my-server"
port: 8080
# A time: 1.5 minutes.
timeout = 1.5min
# A multiplier: 512 * 1024.
max_body = 512kb
# A repeated key holds every value.
upstream = a.example.org
upstream = b.example.org
tls {
    enabled = yes
    cert = "/etc/ssl/server.pem"
}
"#;
    let config: Config = ucl_lexer::from_str(text)?;
    assert_eq!(config.name, "my-server");
    assert_eq!(config.port, 8080);
    assert_eq!(config.timeout, Duration::from_secs(90));
    assert_eq!(config.max_body, 512 * 1024);
    assert_eq!(config.upstream, ["a.example.org", "b.example.org"]);
    assert!(config.tls.enabled);
    assert_eq!(config.tls.cert, "/etc/ssl/server.pem");
    Ok(())
}
```

`from_file` (Cargo feature `fs`, on by default) reads a file and the files it includes. Relative
include paths resolve against the directory of the file, also inside included files, and the
variables `$FILENAME` and `$CURDIR` are the file's path and directory:

```rust,no_run
use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
    name: String,
}

fn main() -> Result<(), ucl_lexer::UclError> {
    let config: Config = ucl_lexer::from_file("/etc/myapp/myapp.conf")?;
    println!("{}", config.name);
    Ok(())
}
```

How UCL values map onto serde types (a repeated key as a sequence, a time as seconds, and so on)
is described in the documentation of the `de` module.

### Format notes

UCL as libucl reads it differs from what some UCL guides describe. Among the differences
(`docs/spec/` has the full rules):

- Comments are `#` and `/* … */`, which nest. `//` does not start a comment.
- An unquoted value runs to the end of the line, a `,`, a `;` or a comment, spaces included:
  `k = 1 2 3` is the string `"1 2 3"`.
- A number with a suffix is a number only when a line break, `;`, `,`, `}`, `]`, `#` or the end
  of input follows directly: in `timeout = 30s # comment` the value is the string `"30s"`.
- `key name { … }` nests: `server web { port = 80 }` is `server { web { port = 80 } }`.
- A repeated key keeps every value. serde reads them as a sequence, so a field that is not a
  sequence fails with "invalid type: sequence"; use a `Vec`, `DuplicateStrategy::Rewrite`, or
  priorities to keep one value.
- Variable expansion always produces a string; there is no `${NAME:-default}` form.

## Writing

`to_string` writes any `Serialize` value in the UCL config format, `to_json_string`,
`to_json_string_compact` and `to_yaml_string` in the other output formats, and `to_writer` writes
the config format to an `io::Write`. The output reads back, in this crate and in libucl, as
exactly the value written, floats included. The JSON output is valid JSON: a time is written as
its number of seconds, and a NaN or infinite float or time is an error.

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Upstream {
    name: String,
    servers: Vec<String>,
    weight: f64,
}

fn main() -> Result<(), ucl_lexer::UclError> {
    let upstream = Upstream {
        name: "backend".into(),
        servers: vec!["10.0.0.1:80".into(), "10.0.0.2:80".into()],
        weight: 0.1,
    };

    let text = ucl_lexer::to_string(&upstream)?;
    assert_eq!(
        text,
        "name = \"backend\";\nservers [\n    \"10.0.0.1:80\",\n    \"10.0.0.2:80\",\n]\nweight = 0.1;\n"
    );
    let back: Upstream = ucl_lexer::from_str(&text)?;
    assert_eq!(back, upstream);

    assert_eq!(
        ucl_lexer::to_json_string_compact(&upstream)?,
        r#"{"name":"backend","servers":["10.0.0.1:80","10.0.0.2:80"],"weight":0.1}"#
    );
    Ok(())
}
```

`to_value` and `from_value` convert between Rust values and the `UclValue` tree.

## Parser settings

The functions above parse with default settings. Everything else is a setting of
`parse::Parser`, made with its setters or with `parse::ParserBuilder`. Parse to a `UclValue`,
then deserialize it with `from_value`, or pass the parser to `UclDeserializer::from_parser`:

```rust
use serde::Deserialize;
use ucl_lexer::parse::ParserBuilder;
use ucl_lexer::{DuplicateStrategy, ParserFlags};

#[derive(Debug, Deserialize)]
struct Config {
    url: String,
    workers: u32,
    timeout: String,
}

fn main() -> Result<(), ucl_lexer::UclError> {
    let mut parser = ParserBuilder::new()
        .with_flags(ParserFlags::NO_TIME)
        .with_strategy(DuplicateStrategy::Rewrite)
        .with_variable("HOST", "example.org")
        .build();
    let value = parser.parse(b"url = \"https://$HOST/\"\nworkers = 2\nworkers = 8\ntimeout = 30s\n")?;
    let config: Config = ucl_lexer::from_value(value)?;
    assert_eq!(config.url, "https://example.org/");
    // Rewrite: a repeated key replaces the value before it.
    assert_eq!(config.workers, 8);
    // NO_TIME: time suffixes are not read, so the value is a string.
    assert_eq!(config.timeout, "30s");
    Ok(())
}
```

### Flags

`ParserFlags` are libucl's parser flags, combined with `|`:

| Flag | Effect |
| --- | --- |
| `KEY_LOWERCASE` | Keys are lowercased (ASCII letters only). |
| `NO_TIME` | The suffixes `s`, `min`, `h`, `d`, `w` and `y` are not read: such values are strings. |
| `NO_IMPLICIT_ARRAYS` | A repeated key collects its values into an explicit array. |
| `SAVE_COMMENTS` | Comments are saved (`Parser::comments`) and attached to values (`Parser::attached_comments`). |
| `DISABLE_MACRO` | Macros are syntax errors and variables are not expanded. |
| `NO_FILEVARS` | `$FILENAME` and `$CURDIR` are not defined for text input. |
| `ZEROCOPY` | No effect on the result; accepted for compatibility. |

### Duplicate keys and priorities

A repeated key is resolved by the duplicate strategy (`DuplicateStrategy`) of the input it is
in. `Append`, the default, compares priorities: a value with the same priority as the key's first
value is added to the key's values, a higher priority replaces them, and a lower one is dropped.
`Merge` merges objects and appends to arrays, `Rewrite` keeps the last value, and `Error` rejects
the document. Priorities run from 0 to 15 and come from `ParserBuilder::with_priority`, the
`.priority` macro or an include's `priority` parameter; an include's `duplicate` parameter sets
the strategy for the included file. `docs/spec/08-duplicates.md` has the details.

### Variables

`$NAME` and `${NAME}` in double-quoted strings, unquoted values and heredocs expand to registered
variables; single-quoted strings and keys are never expanded. A variable handler is asked for
braced references `${NAME}` whose name is not registered; if it returns `None`, or if there is no
handler, the reference stays as written:

```rust
use std::collections::HashMap;
use ucl_lexer::UclDeserializer;
use ucl_lexer::parse::ParserBuilder;
use serde::Deserialize;

fn main() -> Result<(), ucl_lexer::UclError> {
    let text = br#"
url = "https://${HOST}:$PORT/"
literal = '$HOST'
secret = "${SECRET_DB}"
unbraced = "$SECRET_DB"
other = "${OTHER}"
"#;
    let parser = ParserBuilder::new()
        .with_variables([("HOST", "example.org"), ("PORT", "8443")])
        .with_variable_handler(|name| name.strip_prefix("SECRET_").map(|key| format!("<{key}>")))
        .build();
    let config = HashMap::<String, String>::deserialize(UclDeserializer::from_parser(parser, text))?;
    assert_eq!(config["url"], "https://example.org:8443/");
    assert_eq!(config["literal"], "$HOST");
    assert_eq!(config["secret"], "<DB>");
    assert_eq!(config["unbraced"], "$SECRET_DB");
    assert_eq!(config["other"], "${OTHER}");
    Ok(())
}
```

Shortcuts: `from_str_with_variables` registers `(name, value)` pairs, `from_str_with_map` the
entries of a `HashMap`, and `from_str_with_env` installs a handler that reads the process's
environment for braced references.

## Includes and loaders

Text input reads no files. `from_str`, `from_slice`, `from_reader` and a parser with its default
loader have no file access: an `.include` in such a document finds no file and fails, and a
`.try_include` stops the parse. Only `from_file` reads the filesystem without being asked to.

A parser reads the files its loader holds. `parse::MemoryLoader` serves files from memory;
`parse::FsLoader` (feature `fs`) reads the filesystem. Relative paths resolve against the
parser's base directory:

```rust
use ucl_lexer::parse::{ErrorKind, MemoryLoader, ParserBuilder};
use ucl_lexer::{UclError, UclValue};

fn main() -> Result<(), UclError> {
    let text = b".include \"common.conf\"\nport = 80\n";

    // Text input: the include finds no file.
    let err = ucl_lexer::from_slice::<UclValue>(text).unwrap_err();
    assert!(matches!(
        err.parse_error().map(|e| e.kind()),
        Some(ErrorKind::FileNotFound { .. })
    ));

    // A parser whose loader holds the file.
    let mut loader = MemoryLoader::new();
    loader.add_file("/etc/app/common.conf", "log_level = info\n");
    let mut parser = ParserBuilder::new()
        .with_loader(loader)
        .with_base_dir("/etc/app")
        .build();
    let value = parser.parse(text)?;
    let root = value.as_object().unwrap();
    assert_eq!(root["log_level"].as_str(), Some("info"));
    assert_eq!(root["port"].as_integer(), Some(80));
    Ok(())
}
```

To let text input read files, give its parser an `FsLoader`, and preferably a base directory; the
same parser also parses files by path with `Parser::parse_file`:

```rust,no_run
use ucl_lexer::parse::{FsLoader, ParserBuilder};

fn main() -> Result<(), ucl_lexer::UclError> {
    let text = std::fs::read("/etc/myapp/myapp.conf")?;
    let mut parser = ParserBuilder::new()
        .with_loader(FsLoader::new())
        .with_base_dir("/etc/myapp")
        .build();
    let value = parser.parse(&text)?;
    println!("{value:?}");
    Ok(())
}
```

The include macros can search a list of directories, which the `path` parameter of an include
sets for the rest of the document. `ParserBuilder::with_search_path` (or `Parser::set_search_path`)
puts such a list in effect from the start of every parse: `.include "x.conf"` then reads
`DIR/x.conf` from the first directory, and `.try_include "x.conf"` from the first directory that
has the file. A `path` parameter in the document replaces the list; `.load` does not use it.

Where libucl ends a parse without an error message, at a `.try_include` that finds no usable file
or an `.include` of a glob pattern that matches nothing, the crate returns `UclError::Stopped`
(for `Parser::parse`, an error for which `parse::Error::is_stopped` is true). The error holds what
was parsed before the stop:

```rust
use ucl_lexer::{UclError, UclValue};

fn main() {
    let err = ucl_lexer::from_str::<UclValue>("a = 1\n.try_include \"extra.conf\"\nb = 2\n")
        .unwrap_err();
    let UclError::Stopped(stop) = err else { panic!("{err}") };
    let partial = stop.partial().unwrap().as_object().unwrap();
    assert_eq!(partial["a"].as_integer(), Some(1));
    assert!(partial.get("b").is_none());
}
```

`.load`, which reads a file into a value, is available only with the Cargo feature `load`; without
it, `.load` fails with an "unsupported" error. The crate never fetches URLs and never checks
signatures: `.includes`, and `sign=true` on an include, fail with an "unsupported" error.

## Emitters

The `emit` module writes a parsed `UclValue` in libucl's output formats, byte for byte as libucl
writes them: `Format::Config`, `Format::Json`, `Format::JsonCompact` and `Format::Yaml`.
`Parser::emitter` gives an emitter that also uses what the parser remembered about the
document, such as which strings were single-quoted:

```rust
use ucl_lexer::emit::Format;
use ucl_lexer::parse::Parser;

fn main() -> Result<(), ucl_lexer::parse::Error> {
    let mut parser = Parser::new();
    let value = parser.parse(b"name = 'web'\nports = [80, 443]\ntimeout = 1.5")?;
    assert_eq!(
        parser.emitter(Format::Config).emit(&value),
        "name = 'web';\nports [\n    80,\n    443,\n]\ntimeout = 1.500000;\n"
    );
    assert_eq!(
        parser.emitter(Format::JsonCompact).emit(&value),
        r#"{"name":"web","ports":[80,443],"timeout":1.500000}"#
    );
    Ok(())
}
```

libucl's formats do not always read back as the same value (floats keep six decimals, for
example); the serde functions above do. Comments saved under `SAVE_COMMENTS` are written in the
config format only when asked for:

```rust
use ucl_lexer::ParserFlags;
use ucl_lexer::emit::Format;
use ucl_lexer::parse::Parser;

fn main() -> Result<(), ucl_lexer::parse::Error> {
    let mut parser = Parser::with_flags(ParserFlags::SAVE_COMMENTS);
    let value = parser.parse(b"# the port\nport = 80\n")?;
    let text = parser
        .emitter(Format::Config)
        .with_comments(parser.comments(), parser.attached_comments())
        .emit(&value);
    assert_eq!(text, "# the port\nport = 80;\n");
    Ok(())
}
```

## Errors

Every function returns `UclError`. A document the parser rejects is `UclError::Syntax`, whose
`parse::Error` has a kind (`parse::ErrorKind`), the position where the error was found (1-based
line and column, 0-based byte offset) and, for an error inside an included file, that file. A
document that parses but does not fit the target type is `UclError::Deserialize`: its
`error::DeserializeError` has serde's error, the path of the value it is about, and the position
where that value was written, in the document or in the included file it came from (also for
values that were merged, or copied by `.inherit`). `UclError::position` and `UclError::file` give
the position of either kind. A failure to read input is `UclError::Io`, and a value that cannot
be serialized `UclError::Serde`:

```rust
use serde::Deserialize;
use ucl_lexer::UclError;
use ucl_lexer::parse::ErrorKind;

#[derive(Debug, Deserialize)]
struct Config {
    port: u16,
}

fn main() {
    match ucl_lexer::from_str::<Config>("port = 80\nname = \"open") {
        Err(UclError::Syntax(e)) => {
            assert_eq!(e.kind(), &ErrorKind::UnterminatedString);
            assert_eq!((e.position().line, e.position().column), (2, 8));
            eprintln!("{e}");
        }
        other => panic!("{other:?}"),
    }

    match ucl_lexer::from_str::<Config>("# ports\nport = 70000") {
        Err(UclError::Deserialize(e)) => {
            let position = e.position().unwrap();
            assert_eq!((position.line, position.column), (2, 8));
            // invalid value: integer `70000`, expected u16 at port (line 2, column 8)
            eprintln!("{e}");
        }
        other => panic!("{other:?}"),
    }
}
```

The text functions (`from_str`, `from_slice`, `from_reader`, `from_file`, `from_str_with_*`) give
the path and position at no cost while deserialization succeeds: when it fails, they parse the
document again, recording where values were written, and deserialize the target again, recording
the path. That second parse asks the loader for the included files again, so a file changed in
between can move the position; the variable handler is not asked again. To use other parser
settings and still get positions, deserialize through `UclDeserializer::from_parser(parser,
input)`, which records paths as it goes, at some cost also when nothing fails. `from_value`
records nothing, and its errors have neither a path nor a position.

## Limits

- Containers nest at most 1024 deep, the root included (`parse::MAX_NESTING`), included files 16
  deep (`parse::MAX_INCLUDE_DEPTH`) and macro argument lists 64 deep
  (`parse::MAX_ARGUMENT_DEPTH`). The nesting limit also holds for the objects that `.inherit`
  copies: a copy that would be nested deeper is an error, which libucl does not report.
- Parsing, the emitters and serde handle every document the parser accepts on a 2 MiB thread
  stack, the default of a spawned thread, in a debug build. serde converts a `UclValue` or
  `UclObject`, also as a field of another type, with the same stack at any depth; the text
  functions write it up to the nesting limit.
- serde deserializes into, and serializes from, every other type by recursion through its serde
  impls, so there the nesting is limited to 128 maps and sequences, the outermost included
  (`MAX_SERDE_NESTING`, the limit `serde_json` also uses). Deeper nesting fails with
  `SerdeError::TooDeep`, inside `UclError::Deserialize` when deserializing. A value that the target skips, such as an unknown field, does not count.
- Keys and strings must be valid UTF-8; libucl accepts other bytes.
- There is no limit on the size of the input by default, as in libucl. For input from untrusted
  sources, `ParserBuilder::with_max_input_bytes(n)` (or `Parser::set_max_input_bytes`) caps the
  bytes one parse reads: the document and every file its `.include`, `.try_include` and `.load`
  read, together. Going over it fails with `parse::ErrorKind::InputTooLarge`, also under
  `try=true` and in `.try_include`, and files are read no further than needed to tell.

## Cargo features

| Feature | Default | Enables |
| --- | --- | --- |
| `fs` | yes | `parse::FsLoader` and `from_file` |
| `load` | no | the `.load` macro |

## Examples

The [examples](examples/) are programs that check their results with assertions; see
[examples/README.md](examples/README.md). Run one with `cargo run --example basic_usage`.

| Example | Shows |
| --- | --- |
| `basic_usage` | `from_str` into structs, parse and serde errors |
| `complete_ucl_syntax` | a tour of the format |
| `number_parsing` | numbers, multipliers, time suffixes and `NO_TIME` |
| `web_server_config` | `Duration` fields, layered files with `.include`, variables |
| `framework_integration` | configuration structs for web services, writing configuration back |
| `nginx_style_framework_integration` | nginx-style UCL mapped onto structs |
| `real_world_configurations` | a configuration assembled from several files, priorities, `.inherit` |
| `configuration_management` | layers, per-environment variables, validation |
| `real_world_usage` | four larger configurations |
| `advanced_features` | parser settings, loaders, comments, output formats, silent stops |

## Development

`scripts/ci.sh` runs every check CI runs: `cargo fmt --check`, clippy with warnings denied, the
tests, the examples, the benches' build and `cargo doc`. CI runs it on Linux and macOS with
stable Rust and with Rust 1.88.

```sh
scripts/ci.sh
cargo test                                          # all tests, the conformance suite included
cargo test --test conformance                       # the conformance suite alone
UCL_CONFORMANCE_REPORT=1 cargo test --test conformance -- --nocapture   # per-case detail
cargo bench                                         # benches/README.md
```

The conformance suite is in `tests/conformance/` (see its
[README](tests/conformance/README.md)). Its golden files are libucl's results, committed so that
running the tests needs no C toolchain. `scripts/regen-golden.sh` regenerates them: it clones
libucl at a pinned commit under `target/`, builds it and the dump tool in `tools/ucl-dump/`, and
runs every case. It needs git, CMake and a C compiler, and runs on macOS, where the committed files
were generated. `scripts/ci.sh golden` runs it, regenerates the serde corpus in
`tests/serde_corpus/` as well, and fails if any golden file changed; CI runs that every night.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

The repository also holds files from libucl's test suite, under `tests/conformance/libucl/`;
they are covered by libucl's BSD-2-Clause license, [LICENSE-libucl](LICENSE-libucl), and are not
part of the published package.

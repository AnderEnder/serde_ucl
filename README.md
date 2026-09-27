# serde_ucl

[![CI](https://github.com/AnderEnder/serde_ucl/actions/workflows/ci.yml/badge.svg)](https://github.com/AnderEnder/serde_ucl/actions/workflows/ci.yml)
[![Coverage](https://github.com/AnderEnder/serde_ucl/actions/workflows/coverage.yml/badge.svg)](https://github.com/AnderEnder/serde_ucl/actions/workflows/coverage.yml)
[![crates.io](https://img.shields.io/crates/v/serde_ucl.svg)](https://crates.io/crates/serde_ucl)
[![docs.rs](https://img.shields.io/docsrs/serde_ucl)](https://docs.rs/serde_ucl)
[![MSRV](https://img.shields.io/crates/msrv/serde_ucl)](https://github.com/AnderEnder/serde_ucl/blob/main/Cargo.toml)
[![License](https://img.shields.io/crates/l/serde_ucl.svg)](#license)

UCL (Universal Configuration Language) for Rust, with serde. The crate reads and writes UCL as
[libucl](https://github.com/vstakhov/libucl), the C library used by FreeBSD, does. A conformance
suite of 1,653 documents compares its parse results, and its output in four formats, with
libucl's. The implementation is written independently of libucl's code, from a behaviour
specification and libucl's observable output.

- Deserialize UCL into any `serde::Deserialize` type or into the `UclValue` tree. Serialize any
  `serde::Serialize` value as UCL, JSON or YAML that reads back as the same value.
- libucl's parser settings: flags, duplicate-key strategies and priorities, variables and a
  variable handler, and the macros `.include`, `.try_include`, `.priority`, `.inherit` and
  `.load`. A document given as text reads no files unless its parser is given a loader.
- Several inputs into one result, and macros registered by the application.
- Emitters that write a parsed document byte for byte as libucl does, with saved comments on
  request.

Release 0.3.0 renamed the package, the library and the repository to `serde_ucl`;
[CHANGELOG.md](CHANGELOG.md) says how to upgrade.

## Contents

- [Installation](#installation)
- [Quick start](#quick-start)
- [UCL as libucl reads it](#ucl-as-libucl-reads-it)
- [Compatibility with libucl](#compatibility-with-libucl)
- [Reading](#reading)
- [The value tree](#the-value-tree)
- [Writing](#writing)
- [Parser settings](#parser-settings)
- [Includes and file access](#includes-and-file-access)
- [Several inputs](#several-inputs)
- [Registered macros](#registered-macros)
- [Emitters and saved comments](#emitters-and-saved-comments)
- [Errors](#errors)
- [Limits](#limits)
- [Cargo features](#cargo-features)
- [Examples](#examples)
- [Benchmarks](#benchmarks)
- Contributing: [Repository layout](#repository-layout), [Tests](#tests),
  [Conformance suite and golden files](#conformance-suite-and-golden-files),
  [Differential fuzzer](#differential-fuzzer), [CI workflows](#ci-workflows)
- [License](#license)

## Installation

```toml
[dependencies]
serde_ucl = "0.3"
serde = { version = "1", features = ["derive"] }
```

It targets the latest stable Rust (1.98 at this release). See [Cargo features](#cargo-features)
for `fs` and `load`.

## Quick start

```rust
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct Config {
    name: String,
    port: u16,
    #[serde(with = "serde_ucl::time")]
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

fn main() -> Result<(), serde_ucl::UclError> {
    let text = r#"
# Comments are `#` and `/* ... */`.
name = "my-server"
port: 8080
# A time: 1.5 minutes, read as 90 seconds.
timeout = 1.5min
# A multiplier: 512 * 1024.
max_body = 512kb
# A repeated key keeps every value.
upstream = a.example.org
upstream = b.example.org
tls {
    enabled = yes
    cert = "/etc/ssl/server.pem"
}
"#;
    let config: Config = serde_ucl::from_str(text)?;
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

## UCL as libucl reads it

libucl's reading of UCL differs in places from what some UCL guides describe. The main rules
(`docs/spec/` has all of them):

- An entry is a key, an optional `=` or `:`, and a value. It ends at a line break, `;` or `,`.
  `key value` works as in nginx.
- Comments are `#` and `/* … */`, which nest. `//` does not start a comment.
- An unquoted value runs to the end of the line, `;`, `,`, `#` or `/*`, spaces included:
  `k = 1 2 3` is the string `"1 2 3"`.
- `true`, `yes`, `on` and `false`, `no`, `off` are booleans in any letter case. `null`, `nan` and
  `inf` are recognised in lowercase only.
- Numbers take the multipliers `k`, `m`, `g` (powers of 1000) and `kb`, `mb`, `gb` (powers of
  1024), and the time suffixes `ms`, `s`, `min`, `h`, `d`, `w` and `y`. `m` means mega. A time is
  a value type of its own, a number of seconds.
- A suffix must be followed directly by a line break, `;`, `,`, `#`, `}`, `]` or the end of input.
  Otherwise the value is a string: in `timeout = 30s # comment` the value is `"30s"`.
- Double-quoted strings take escapes and expand variables. Single-quoted strings are literal.
  Heredocs are written `<<EOF` … `EOF`, with an uppercase terminator.
- `key name { … }` nests: `server web { port = 80 }` is `server { web { port = 80 } }`.
- A repeated key keeps every value (libucl's *implicit array*). serde reads the values as a
  sequence, so the field must be a `Vec`, or the parser must keep one value
  (`DuplicateStrategy::Rewrite` or priorities).
- Variable expansion always gives a string. There is no `${NAME:-default}` form.

## Compatibility with libucl

The conformance suite in `tests/conformance/` holds 1,653 cases: libucl's own test corpus, the
cases the behaviour spec cites, and documents libucl rejects. Its golden files are libucl's
results at a pinned commit. Three tests compare the crate with them:

- **Parse results.** Every case's value, with key order, value types, priorities and, where a
  case saves them, comments; or the fact that libucl rejects it. 1,649 cases match. The 4 listed
  exceptions are deliberate differences: two signature checks, one macro-argument nesting limit
  and one non-UTF-8 document.
- **Output.** The 1,216 cases that libucl and the crate both parse are written in the config
  format, JSON, compact JSON and YAML, and each output is compared byte for byte with libucl's.
  The comparison also covers config output with saved comments (67 cases) and libucl's own
  `.res` files (24).
- **Read-back.** The crate parses every output again, which must give the same value, apart from
  the losses the spec lists (for example, floats in libucl's formats keep six decimals).

The deliberate differences, in brief:

- Keys and strings must be valid UTF-8.
- Documents given as text read no files, and relative paths never resolve against the process's
  working directory.
- Signatures are never checked (`.includes` and `sign=true` are errors) and URLs are never
  fetched. `.load` needs the Cargo feature `load`.
- Where libucl stops without a message, the crate returns an error of its own kind that holds the
  partial result.
- Limits where libucl has none: 64 levels of nested macro arguments, and 1024 levels of nesting
  in the objects `.inherit` copies, a limit the application can set up to 2048.
- A registered macro can fail with a message.
- serde's JSON output is valid JSON.
- Where libucl crashes or its result is undefined, the crate reports an error or gives a defined
  result.

A differential fuzzer compares the crate with libucl on generated documents. The spec answers
every difference it has found, with a rule the crate follows or as behaviour it leaves open,
such as what depends on the operating system. libucl's schema validation and MessagePack support
are not implemented.
[docs/COMPATIBILITY.md](docs/COMPATIBILITY.md) lists every difference with its case, and the
libucl quirks the crate reproduces.

## Reading

| Function | Reads | Files |
| --- | --- | --- |
| `from_str`, `from_slice`, `from_reader` | text, bytes, an `io::Read` | none |
| `from_str_with_variables`, `from_str_with_map` | text, with registered variables | none |
| `from_str_with_env` | text, with a handler that reads environment variables for `${NAME}` | none |
| `from_file` (feature `fs`) | a file | the file and its includes |
| `UclDeserializer::new`, `from_slice`, `from_parser` | text or bytes, as a `serde::Deserializer` | through the parser's loader |
| `from_value` | a `UclValue` | none |

How values map onto serde (the `de` module documentation has the full table):

- An entry with several values is a sequence. A single value read into a sequence is a
  one-element sequence.
- A time is an `f64` number of seconds. `#[serde(with = "serde_ucl::time")]` reads and writes a
  `std::time::Duration`, and accepts any non-negative finite number as seconds.
- `null` into an `Option` is `None`. Enums are externally tagged: a string names a unit variant,
  and an object with one key a data variant.
- Keys can be read into integer, `bool`, `char` and float map keys.
- Values are owned: a borrowed `&str` field fails. Use `String` or `Cow<str>`.

## The value tree

`UclValue` is libucl's object model: `Object(UclObject)`, `Array(Vec<UclValue>)`,
`Integer(i64)`, `Float(f64)`, `Time(f64)` (seconds), `String`, `Boolean` and `Null`. A
`UclObject` keeps its keys in order. Each key holds an `Entry` of one or more values, and each
value is a `Slot` with its priority and whether `.inherit` copied it. Indexing an object, or
`get`, gives the first value of a key, and `get_all` gives every value. `from_str::<UclValue>`
and `parse::parse` give the tree. `from_value` and `to_value` convert it to and from other types,
and keep a `UclValue` exactly as it is, priorities included.

```rust
use serde_ucl::UclValue;

fn main() -> Result<(), serde_ucl::UclError> {
    let text = "port = 80\nport = 8080\ntimeout = 30s\nservers = [a, b]\n";
    let value: UclValue = serde_ucl::from_str(text)?;
    let root = value.as_object().unwrap();
    assert_eq!(root["port"].as_integer(), Some(80));
    let ports: Vec<i64> = root.get_all("port").filter_map(UclValue::as_integer).collect();
    assert_eq!(ports, [80, 8080]);
    assert!(root.entry("port").unwrap().is_multi());
    assert_eq!(root["timeout"], UclValue::Time(30.0));
    assert_eq!(root["servers"].as_array().unwrap().len(), 2);
    Ok(())
}
```

`UclObject` also offers insertion and removal, iteration, and `insert_with_strategy`, which
applies libucl's duplicate rules. Cloning and serde conversion take the same stack at any depth
of nesting.

## Writing

`to_string` writes any `Serialize` value in the UCL config format. `to_json_string`,
`to_json_string_compact` and `to_yaml_string` write the other formats, and `to_writer` writes the
config format to an `io::Write`. `to_value` gives a `UclValue` instead of text.

The output reads back, in this crate and in libucl, as exactly the value written, floats
included. That holds for a reader with the default flags and the `append` strategy. For JSON and
YAML, the reader must also register none of the variables the strings refer to, since double
quotes expand them. The config format single-quotes every string that contains `$`. An entry with
several values is written as repeated keys.

The JSON functions write valid JSON (RFC 8259). A time is written as its number of seconds and
reads back as a float, or as the same `Duration` through `serde_ucl::time`. A NaN or infinite
float or time is an error there. Values that have no form that reads back, such as an integer
above `i64::MAX` or a root that is not a map, struct or sequence, fail with
`SerdeError::Unrepresentable`. The `ser` module documentation lists them.

```rust
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Upstream {
    name: String,
    servers: Vec<String>,
    weight: f64,
    #[serde(with = "serde_ucl::time")]
    timeout: Duration,
}

fn main() -> Result<(), serde_ucl::UclError> {
    let upstream = Upstream {
        name: "backend".into(),
        servers: vec!["10.0.0.1:80".into(), "10.0.0.2:80".into()],
        weight: 0.1,
        timeout: Duration::from_millis(1500),
    };

    let text = serde_ucl::to_string(&upstream)?;
    let expected = r#"name = "backend";
servers [
    "10.0.0.1:80",
    "10.0.0.2:80",
]
weight = 0.1;
timeout = 1.5s;
"#;
    assert_eq!(text, expected);
    assert_eq!(serde_ucl::from_str::<Upstream>(&text)?, upstream);

    let json = serde_ucl::to_json_string_compact(&upstream)?;
    assert_eq!(
        json,
        r#"{"name":"backend","servers":["10.0.0.1:80","10.0.0.2:80"],"weight":0.1,"timeout":1.5}"#
    );
    assert_eq!(serde_ucl::from_str::<Upstream>(&json)?, upstream);
    Ok(())
}
```

## Parser settings

The functions above parse with default settings. Other settings belong to `parse::Parser`. Set
them with its setters, or with `parse::ParserBuilder` (also `Parser::builder()`). Then parse to a
`UclValue` with `Parser::parse` and deserialize it with `from_value`, or pass the parser to
`UclDeserializer::from_parser`, which keeps error positions. `UclDeserializer::parser_mut` changes
the settings of a deserializer's parser before it runs.

```rust
use serde::Deserialize;
use serde_ucl::parse::ParserBuilder;
use serde_ucl::{DuplicateStrategy, ParserFlags};

#[derive(Debug, Deserialize)]
struct Config {
    url: String,
    workers: u32,
    timeout: String,
}

fn main() -> Result<(), serde_ucl::UclError> {
    let mut parser = ParserBuilder::new()
        .with_flags(ParserFlags::NO_TIME)
        .with_strategy(DuplicateStrategy::Rewrite)
        .with_variable("HOST", "example.org")
        .build();
    let text = b"url = \"https://$HOST/\"\nworkers = 2\nworkers = 8\ntimeout = 30s\n";
    let value = parser.parse(text)?;
    let config: Config = serde_ucl::from_value(value)?;
    assert_eq!(config.url, "https://example.org/");
    // Rewrite: a repeated key replaces the value before it.
    assert_eq!(config.workers, 8);
    // NO_TIME: `s` is not read as a time suffix, so the value is a string.
    assert_eq!(config.timeout, "30s");
    Ok(())
}
```

### Flags

`ParserFlags` are libucl's parser flags, combined with `|`:

| Flag | Effect |
| --- | --- |
| `KEY_LOWERCASE` | Keys are lowercased (ASCII letters only), so keys that differ in case are one key. |
| `NO_TIME` | `s`, `min`, `h`, `d`, `w` and `y` are not time suffixes, so such values are strings. `ms`, `ks` and `gs` still give times. |
| `NO_IMPLICIT_ARRAYS` | A repeated key collects its values into an explicit array. |
| `SAVE_COMMENTS` | Comments are saved (`Parser::comments`) and attached to values (`Parser::attached_comments`). |
| `DISABLE_MACRO` | Every macro is a syntax error, and variables are not expanded. |
| `NO_FILEVARS` | `$FILENAME` and `$CURDIR` are not defined for a document given as text. |
| `ZEROCOPY` | No effect on the result; accepted for compatibility. |

### Duplicate keys and priorities

A repeated key is resolved by the `DuplicateStrategy` of the input it is in:

- `Append`, the default, compares priorities. A value with the same priority as the entry's is
  added to it, a higher priority replaces the entry, and a lower one is dropped.
- `Merge` merges an object into an object and an array into an array. A scalar replaces a
  container, and other values follow `Append`.
- `Rewrite` replaces the entry whatever the priorities.
- `Error` rejects the document.

Priorities run from 0 to 15 (`MAX_PRIORITY`) and are taken modulo 16. They come from
`with_priority` or `set_priority`, the `.priority` macro, an include's `priority` parameter, and
`Input::with_priority`. An include's `duplicate` parameter sets the strategy of the included file.
Spec §8 has the details.

### Variables

`$NAME` and `${NAME}` in double-quoted strings, unquoted values and heredocs expand to registered
variables (`with_variable`, `with_variables`, `Parser::register_variable`). Single-quoted strings
and keys are never expanded. `FILENAME` and `CURDIR` are defined for every document unless
`NO_FILEVARS` is set (see [Includes and file access](#includes-and-file-access)). A variable
handler is asked for braced references `${NAME}` whose name is not registered, never for `$NAME`.
If it returns `None`, or there is no handler, the reference stays as written.

```rust
use serde::Deserialize;
use std::collections::HashMap;
use serde_ucl::UclDeserializer;
use serde_ucl::parse::ParserBuilder;

fn main() -> Result<(), serde_ucl::UclError> {
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
    let deserializer = UclDeserializer::from_parser(parser, text);
    let config = HashMap::<String, String>::deserialize(deserializer)?;
    assert_eq!(config["url"], "https://example.org:8443/");
    assert_eq!(config["literal"], "$HOST");
    assert_eq!(config["secret"], "<DB>");
    assert_eq!(config["unbraced"], "$SECRET_DB");
    assert_eq!(config["other"], "${OTHER}");
    Ok(())
}
```

### Macros

Macros stand where a key could start:

- `.include "path"` reads another file in place. A missing file is an error, or with `try=true`
  is skipped. `.try_include` reads a file too, but a missing file stops the parse silently, as in
  libucl (see [Errors](#errors)). Parameters in parentheses change what both do: `try`,
  `priority`, `duplicate`, `glob`, `path` (search directories), `key`, `prefix` and `target`
  (nest the file under a key), `url` and `sign`.
- `.priority N` sets the priority of the values after it.
- `.inherit "name"` copies the entries of the root object `name` into the current object. The
  copies may nest a value at most 1024 containers deep, or as deep as
  `ParserBuilder::with_inherit_depth_limit` allows ([Limits](#limits)).
- `.load(key="k") "path"` reads a file into a value under the key `k`, with the feature `load`.
- `.includes` (signed includes) is an "unsupported" error. Any other name is an error unless the
  application registered it ([Registered macros](#registered-macros)).

```rust
fn main() -> Result<(), serde_ucl::parse::Error> {
    let text = br#"
defaults { timeout = 30s; retries = 3 }
api {
    .inherit "defaults"
    retries = 5
}
workers = 4
.priority 2
workers = 16
.priority 1
workers = 8
"#;
    let value = serde_ucl::parse::parse(text)?;
    let root = value.as_object().unwrap();
    // A copied value gives way to one written in the object.
    let api = root["api"].as_object().unwrap();
    assert_eq!(api["timeout"].as_time(), Some(30.0));
    assert_eq!(api["retries"].as_integer(), Some(5));
    // Priority 2 replaced the value at priority 0, and the value at priority 1 was dropped.
    let workers = root.entry("workers").unwrap();
    assert_eq!(workers.len(), 1);
    assert_eq!(workers.first().as_integer(), Some(16));
    assert_eq!(workers.slots()[0].priority(), 2);
    Ok(())
}
```

## Includes and file access

**Text input reads no files.** `from_str`, `from_slice`, `from_reader`, `parse::parse` and any
`Parser` that keeps its default loader, an empty `MemoryLoader`, have no file access. In such a
document, `.include` fails with `ErrorKind::FileNotFound`, `.try_include` stops the parse, and
`.load` fails as for a missing file. Only `from_file` reads the filesystem without being asked
to.

A parser reads files through its `parse::Loader`:

- `FsLoader` (feature `fs`) reads the filesystem.
- `MemoryLoader` serves files added with `add_file` (also `add_dir`, `set_current_dir`).
- An application can implement `Loader`: `current_dir`, `canonicalize`, `kind` (a `FileKind`),
  `read`, `read_dir` for glob patterns, and optionally `read_limited` for the input limit.

Relative paths resolve against the parser's base directory (`with_base_dir`, `set_base_dir`).
Without one, a file given to `from_file` or `Parser::parse_file` resolves them against its own
directory, also inside the files it includes, and a document given as text against the loader's
current directory: `/` for a `MemoryLoader`, the working directory for an `FsLoader`. libucl uses
the working directory in every case. `from_file` resolves its own relative path against the
working directory. For a file, `$FILENAME` and `$CURDIR` are its canonical path and directory. For
text, `FILENAME` is `undef` and `CURDIR` is the base directory or the loader's current directory.

```rust
use serde::Deserialize;
use serde_ucl::parse::{FsLoader, ParserBuilder};

#[derive(Debug, Deserialize)]
struct Config {
    name: String,
    workers: u32,
}

fn main() -> Result<(), serde_ucl::UclError> {
    let dir = std::env::temp_dir().join(format!("ucl-readme-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("app.conf"), "name = app\n.include \"workers.conf\"\n")?;
    std::fs::write(dir.join("workers.conf"), "workers = 4\n")?;

    // A file: its relative includes resolve against its directory.
    let config: Config = serde_ucl::from_file(dir.join("app.conf"))?;
    assert_eq!((config.name.as_str(), config.workers), ("app", 4));

    // The same document as text finds no file...
    let text = std::fs::read(dir.join("app.conf"))?;
    assert!(serde_ucl::from_slice::<Config>(&text).is_err());
    // ...unless its parser has a filesystem loader and a base directory.
    let mut parser = ParserBuilder::new()
        .with_loader(FsLoader::new())
        .with_base_dir(&dir)
        .build();
    let config: Config = serde_ucl::from_value(parser.parse(&text)?)?;
    assert_eq!(config.workers, 4);

    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
```

**Search directories.** `with_search_path` (or `set_search_path`) puts a list of directories in
effect from the start of every parse, as a `path` parameter of an earlier include would be.
`.include "x.conf"` then reads `DIR/x.conf` from the first directory, and `.try_include
"x.conf"` from the first directory that has the file. A `path` parameter in the document
replaces the list. `.load` does not use it.

**Input limit.** There is no limit on input size by default, as in libucl. For untrusted input,
`with_max_input_bytes(n)` (or `set_max_input_bytes`) caps the bytes one parse reads: the document
and every file its macros read, together. Going over it fails with `ErrorKind::InputTooLarge`,
also under `try=true` and in `.try_include`, and a large file is read no further than needed to
tell.

```rust
use serde_ucl::UclValue;
use serde_ucl::parse::{ErrorKind, MemoryLoader, ParserBuilder};

fn main() -> Result<(), serde_ucl::parse::Error> {
    let mut files = MemoryLoader::new();
    files.add_file("/usr/share/app/defaults.conf", "workers = 4\n");
    files.add_file("/etc/app/local.conf", "workers = 8\n");
    files.add_file("/etc/app/big.conf", format!("x = \"{}\"\n", "a".repeat(100_000)));
    let mut parser = ParserBuilder::new()
        .with_loader(files)
        .with_search_path(["/etc/app", "/usr/share/app"])
        .with_max_input_bytes(64 * 1024)
        .build();

    let value = parser.parse(b".try_include \"defaults.conf\"\n.try_include \"local.conf\"\n")?;
    let root = value.as_object().unwrap();
    let workers: Vec<i64> = root.get_all("workers").filter_map(UclValue::as_integer).collect();
    assert_eq!(workers, [4, 8]);

    let err = parser.parse(b".include \"big.conf\"\n").unwrap_err();
    assert!(matches!(err.kind(), ErrorKind::InputTooLarge { .. }));
    Ok(())
}
```

## Several inputs

A parser can read several inputs into one result, as libucl's parser does. `Parser::inputs`
starts the parse, `Inputs::add` reads each `Input` (`Input::bytes` or `Input::file`), and
`Inputs::finish` returns the result. Each input has its own priority and strategy
(`Input::with_priority`, `Input::with_strategy`; by default the parser's). A user's file can so
be layered over the defaults:

```rust
use serde_ucl::parse::{Input, MemoryLoader, ParserBuilder};
use serde_ucl::{DuplicateStrategy, UclError};

fn main() -> Result<(), UclError> {
    let mut files = MemoryLoader::new();
    files.add_file(
        "/usr/share/app/app.conf",
        "workers = 4\nlog { level = info; file = /var/log/app.log }\n",
    );
    files.add_file("/etc/app/app.conf", "workers = 16\nlog { level = debug }\n");
    let mut parser = ParserBuilder::new().with_loader(files).build();

    let mut inputs = parser.inputs();
    inputs.add(Input::file("/usr/share/app/app.conf"))?;
    // A higher priority replaces the defaults' values, and `merge` merges objects.
    inputs.add(
        Input::file("/etc/app/app.conf")
            .with_priority(1)
            .with_strategy(DuplicateStrategy::Merge),
    )?;
    let value = inputs.finish()?;

    let root = value.as_object().unwrap();
    assert_eq!(root["workers"].as_integer(), Some(16));
    let log = root["log"].as_object().unwrap();
    assert_eq!(log["level"].as_str(), Some("debug"));
    assert_eq!(log["file"].as_str(), Some("/var/log/app.log"));
    Ok(())
}
```

Each input goes on where the one before it ended, and libucl's quirks at the joins are kept:

- A parser takes at most 16 inputs (`ErrorKind::TooManyInputs`), and they count towards the
  include nesting limit.
- A zero-byte first input gives an empty root that later inputs cannot add to
  (`ErrorKind::AfterRoot`).
- The end of an input is not a separator. An input that ends right after a value, as `x = 1`
  does, must be followed by one that starts with a line break, `;`, `,` or a comment
  (`ErrorKind::UnseparatedInput`).

A silent stop ends only its own input: `Inputs::add` reports it, and the next input goes on. Any
other error fails the whole parse. `Parser::parse` and `Parser::parse_file` are parses of one
input.

## Registered macros

An application can register macros of its own with `ParserBuilder::with_macro` or
`Parser::register_macro`. A handler gets a `MacroCall`: the macro's `name`, its `value` (text
with variables expanded; `value_str` as `&str`) and its `arguments` (the text in parentheses,
parsed as a document). The handler can:

- add entries where the macro stands (`add`, `add_with_priority`);
- have text parsed in place of the macro (`parse`);
- stop the parse silently, as libucl's failing handlers do (`MacroError::stop`, reported as
  `ErrorKind::MacroStopped` with the partial result);
- fail with a message (`MacroError::new`, reported as `ErrorKind::MacroFailed`), which libucl
  cannot do.

```rust
use serde_ucl::UclValue;
use serde_ucl::parse::{ErrorKind, MacroError, ParserBuilder};

fn main() {
    let mut parser = ParserBuilder::new()
        // `.version {}` adds an entry where the macro stands.
        .with_macro("version", |call| call.add("version", UclValue::String("1.4.2".into())))
        // `.feature NAME` has text parsed in place of the macro.
        .with_macro("feature", |call| match call.value_str() {
            Some("tls") => call.parse("tls { enabled = true; port = 443 }"),
            Some("off") => Err(MacroError::stop()),
            _ => Err(MacroError::new("unknown feature")),
        })
        .build();

    let value = parser.parse(b".version {}\nserver {\n  .feature tls\n}\n").unwrap();
    let root = value.as_object().unwrap();
    assert_eq!(root["version"].as_str(), Some("1.4.2"));
    let tls = root["server"].as_object().unwrap()["tls"].as_object().unwrap();
    assert_eq!(tls["port"].as_integer(), Some(443));

    let err = parser.parse(b".feature gzip\n").unwrap_err();
    assert!(matches!(err.kind(), ErrorKind::MacroFailed { name, .. } if name == "feature"));

    // A silent stop keeps what was parsed before the macro.
    let err = parser.parse(b"a = 1\n.feature off\nb = 2\n").unwrap_err();
    assert!(err.is_stopped());
    let partial = err.partial().unwrap().as_object().unwrap();
    assert!(partial.contains_key("a") && !partial.contains_key("b"));
}
```

`with_context_macro` (or `register_context_macro`) registers a macro whose handler also gets a
copy of the root built so far (`MacroCall::root`) and the root's priority (`root_priority`).
Handlers are `Fn`; one that keeps state uses a `Cell` or `RefCell`. A registered name replaces a
built-in macro of the same name. Registered macros work in every input and included file, but not
inside macro arguments, and `DISABLE_MACRO` disables them too. A deserialization error from a
document whose parse ran a registered macro has no position, because the document is not parsed
again to find it.

## Emitters and saved comments

The `emit` module writes a `UclValue` in libucl's output formats, byte for byte as libucl does:
`Format::Config`, `Format::Json`, `Format::JsonCompact` and `Format::Yaml`. Some of that output
depends on how the document was written, such as which strings were single-quoted.
`Parser::emitter` gives an emitter that uses these facts from the last parse
(`Parser::output_facts`; `Emitter::with_facts` takes them too). `Emitter::new(format)` and the
functions `emit::to_config`, `emit::to_json`, `emit::to_json_compact` and `emit::to_yaml` write a
value without them: a `UclValue` does not record how it was written, so strings come out in the
JSON form and keys as they are stored, quoted in the config and YAML formats where
`emit::key_needs_quoting` says. To write a parsed document as libucl would, use
`Parser::emitter` after the parse. The facts are found by the positions of values in the parsed
document: after removing an entry from an object, the entries after it are written as if they
had no facts.

```rust
use serde_ucl::emit::Format;
use serde_ucl::parse::Parser;

fn main() -> Result<(), serde_ucl::parse::Error> {
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
example). The serde functions in [Writing](#writing) do.

Under `ParserFlags::SAVE_COMMENTS` the parser saves comments (`Parser::comments`, each with its
text and position) and attaches them to values (`Parser::attached_comments`, by path, before or
after the value). The config format writes them only when asked with `Emitter::with_comments`.

```rust
use serde_ucl::ParserFlags;
use serde_ucl::emit::Format;
use serde_ucl::parse::Parser;

fn main() -> Result<(), serde_ucl::parse::Error> {
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

The serde functions return `UclError`, which has these variants. The parser's methods return
`parse::Error`, which `?` converts into `UclError::Syntax` or `UclError::Stopped`.

| Variant | Meaning |
| --- | --- |
| `Syntax(parse::Error)` | The parser rejected the document, or a macro failed. |
| `Stopped(parse::Error)` | A silent stop (see below). |
| `Deserialize(DeserializeError)` | The document does not fit the target type. |
| `Io(io::Error)` | The input could not be read. |
| `Serde(SerdeError)` | Serialization failed: `Custom`, `Unrepresentable`, or `TooDeep` (see [Limits](#limits)). |

A `parse::Error` has a `kind()` (`parse::ErrorKind`, which is `#[non_exhaustive]`), a
`position()` and, for an error inside an included file, that `file()`. A `Position` has a 1-based
line and column (counted in characters) and a 0-based byte offset. `is_unsupported()` marks
signatures, `.includes` and `.load` without its feature. The kinds, by group (a `match` on them
needs a `_` arm):

- syntax: `UnterminatedString` and the other `Unterminated*` kinds, `InvalidKey`, `MissingValue`,
  `DuplicateKey` (under the `Error` strategy), `InvalidUtf8`, `NumberOutOfRange` and others;
- macros and files: `UnknownMacro`, `MacrosDisabled`, `InvalidPriority`, `InheritSourceMissing`,
  `FileNotFound`, `NotAFile`, `IncludeSelf`, `LoadKeyMissing`, `UrlNotSupported`, `Unsupported`
  and others;
- limits: `NestingTooDeep`, `IncludeTooDeep`, `ArgumentsTooDeep`, `InputTooLarge`;
- several inputs: `TooManyInputs`, `AfterRoot`, `UnseparatedInput`;
- registered macros: `MacroFailed`, `MacroStopped`;
- silent stops: `Stopped` and `MacroStopped`;
- `Io`, for an input file that could not be read.

A `DeserializeError` has serde's error (`error()`), the path of the value it is about (`path()`,
as `parse::PathSegment`s; `at_key()` when it is about the key, such as an unknown field), and
where the value was written (`position()`, and `file()` for an included file). The position
follows values that the duplicate rules merged or moved and values that `.inherit` copied.
`UclError::position`, `UclError::file` and `UclError::parse_error` work for every variant that
has them.

```rust
use serde::Deserialize;
use serde_ucl::UclError;
use serde_ucl::parse::{ErrorKind, PathSegment};

#[derive(Debug, Deserialize)]
struct Config {
    #[allow(dead_code)]
    port: u16,
}

fn main() {
    match serde_ucl::from_str::<Config>("port = 80\nname = \"open") {
        Err(UclError::Syntax(e)) => {
            assert_eq!(e.kind(), &ErrorKind::UnterminatedString);
            assert_eq!((e.position().line, e.position().column), (2, 8));
        }
        other => panic!("{other:?}"),
    }

    match serde_ucl::from_str::<Config>("# ports\nport = 70000") {
        Err(UclError::Deserialize(e)) => {
            assert_eq!(e.path(), [PathSegment::Key { key: "port".into(), index: 0 }]);
            let position = e.position().unwrap();
            assert_eq!((position.line, position.column), (2, 8));
            // invalid value: integer `70000`, expected u16 at port (line 2, column 8)
            eprintln!("{e}");
        }
        other => panic!("{other:?}"),
    }
}
```

The text functions (`from_str`, `from_slice`, `from_reader`, `from_file`, `from_str_with_*`) find
the path and position only when deserialization fails: they then parse the document again,
recording where values were written, and deserialize a second time, recording the path. They cost
nothing while deserialization succeeds. The second parse asks the loader for included files again,
and gives the variable handler's first answers instead of calling it. `UclDeserializer` records
paths as it goes, at some cost when nothing fails. `from_value` records nothing, so its errors
have neither a path nor a position.

**Silent stops.** libucl ends a parse without an error message at a `.try_include` that finds no
usable file, at an `.include` of a glob pattern that matches nothing, and at a registered macro
that fails. The crate reports such a stop as `UclError::Stopped` from the serde functions, and as
a `parse::Error` for which `is_stopped()` is true from `Parser::parse`. `partial()` (or
`into_partial()`) returns what was parsed before the stop:

```rust
use serde_ucl::{UclError, UclValue};

fn main() {
    let text = "a = 1\n.try_include \"extra.conf\"\nb = 2\n";
    let err = serde_ucl::from_str::<UclValue>(text).unwrap_err();
    let UclError::Stopped(stop) = err else { panic!("{err}") };
    let partial = stop.partial().unwrap().as_object().unwrap();
    assert_eq!(partial["a"].as_integer(), Some(1));
    assert!(partial.get("b").is_none());
}
```

## Limits

| Limit | Value | Error |
| --- | --- | --- |
| Containers open inside one another, the root included (`parse::MAX_NESTING`) | 1024 | `ErrorKind::NestingTooDeep` |
| Containers a copy made by `.inherit` nests a value in, the root included (`with_inherit_depth_limit`, `Parser::set_inherit_depth_limit`) | 1024 by default (`parse::DEFAULT_INHERIT_DEPTH_LIMIT`), at most 2048 (`parse::MAX_INHERIT_DEPTH_LIMIT`) | `ErrorKind::NestingTooDeep` |
| Input units open at once: inputs, included files and text parsed in place by macros (`parse::MAX_INCLUDE_DEPTH`) | 16 | `ErrorKind::IncludeTooDeep` |
| Macro argument lists nested inside one another (`parse::MAX_ARGUMENT_DEPTH`; libucl has none) | 64 | `ErrorKind::ArgumentsTooDeep` |
| Maps and sequences that serde enters in types other than `UclValue` and `UclObject` (`MAX_SERDE_NESTING`, as in `serde_json`) | 128 | `SerdeError::TooDeep` |
| Bytes read by one parse (`with_max_input_bytes`) | none by default | `ErrorKind::InputTooLarge` |

Parsing, cloning, comparing and dropping a value, the emitters and serde handle every document
the parser accepts on a 2 MiB thread stack in a debug build, with the `.inherit` limit at any
setting up to 2048; that is how the largest setting was chosen. A value nested more than 1024 deep
has no text that could be parsed again, so the serde text functions (`to_string` and the others)
reject it, while the emitters write it. `Debug` formatting of such a value needs a larger stack.
A `UclValue` or `UclObject`, also as a field of another type, is converted in one step at any
depth. Other types are read and written by recursion through their serde impls,
hence `MAX_SERDE_NESTING`. A value that the target skips, such as an unknown field, does not
count.

## Cargo features

| Feature | Default | Enables |
| --- | --- | --- |
| `fs` | yes | `parse::FsLoader` and `from_file` |
| `load` | no | the `.load` macro; without it `.load` is an "unsupported" error |

`default-features = false` turns off `fs`. Without it, files can still come from a
`MemoryLoader` or a loader of the application's own.

## Examples

The [examples](examples/) are programs that check their results with assertions
([examples/README.md](examples/README.md)). Run one with `cargo run --example basic_usage`.

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

## Benchmarks

Three [criterion](https://docs.rs/criterion) benchmarks cover parsing, the emitters and serde
([benches/README.md](benches/README.md)). Run them with `cargo bench`; criterion's options go
after `--`, as in `cargo bench -- --noplot`. One run on an Apple M4 Max
with rustc 1.98.1, median times:

| Group | Document | Time | Throughput |
| --- | --- | --- | --- |
| `parse/config/1000` | configuration, 510 KB | 5.49 ms | 88.7 MiB/s |
| `parse/json/1000` | JSON, 1000 records | 3.13 ms | 44.2 MiB/s |
| `parse/nested/1000` | objects nested 1000 deep | 308 µs | 27.5 MiB/s |
| `emit/config-1000/config` | the configuration, config format | 1.25 ms | 371 MiB/s of output |
| `serde/deserialize-1000/from_str` | the configuration into a struct | 6.09 ms | 80.0 MiB/s |
| `serde/deserialize-1000/from_value` | the parsed tree into a struct | 775 µs | 628 MiB/s |
| `serde/deserialize-error-1000/from_str` | as above, failing on the last value | 15.0 ms | 32.5 MiB/s |
| `serde/serialize-1000/to_string` | the struct in the config format | 2.59 ms | 182 MiB/s of output |

## Repository layout

| Path | Contents |
| --- | --- |
| `src/parse/` | the parser: `Parser` and settings, document structure, numbers, strings, variables, comments, macros, includes and globs, loaders, several inputs, registered macros, errors |
| `src/emit/` | the output formats |
| `src/de.rs`, `src/de/`, `src/ser/` | serde deserialization and serialization |
| `src/value.rs`, `src/error.rs`, `src/time.rs` | the value model, `UclError` and `Position`, the `Duration` helper |
| `docs/spec/` | the behaviour spec, one file per format feature |
| `docs/COMPATIBILITY.md` | deliberate differences from libucl and reproduced quirks |
| `docs/clean-room/` | the clean-room protocol, work list, log and spec questions |
| `tests/` | integration tests, the conformance suite and the serde corpus |
| `tools/ucl-dump/`, `scripts/` | the oracle that dumps libucl's results; CI and golden-file scripts |
| `fuzz/` | the differential fuzzer |
| `examples/`, `benches/` | examples and benchmarks |

Changes to `src/` follow the clean-room rules in
[docs/clean-room/PROTOCOL.md](docs/clean-room/PROTOCOL.md): they are made from the spec and the
conformance suite, never from libucl's source code.

## Tests

```sh
cargo test                                     # unit, integration, conformance and doc tests
cargo test --test conformance                  # the conformance suite
UCL_CONFORMANCE_REPORT=1 cargo test --test conformance -- --nocapture   # per-case detail
cargo test --test serde_roundtrip              # serde round trips and the serde corpus
scripts/ci.sh                                  # everything CI runs
```

- The README's Rust examples run as doctests.
- `tests/stack_depth.rs` checks that no entry point overflows a 2 MiB stack at the deepest input
  the parser accepts. `scripts/ci.sh` also runs it with the crate unoptimised. The test profile
  builds the crate with `opt-level = 2` for speed only.
- `tests/scaling.rs` is the only timing-based test. It checks that time grows linearly by
  comparing time ratios within one run, with no wall-clock thresholds.
- `tests/features/` is a separate package that tests the crate without the `load` feature, which
  the crate's dev-dependency on itself turns on for every other test build.
- `tests/serde_corpus/` holds serialized values and libucl's readings of them.
  `UCL_SERDE_REGEN=1 cargo test --test serde_roundtrip` regenerates it and needs the oracle
  binary `target/libucl-oracle/ucl-dump`.

## Conformance suite and golden files

`tests/conformance/` (see its [README](tests/conformance/README.md)) holds the cases:
`libucl/basic/` (libucl's own tests), `cases/spec/NN-topic/` (the cases each spec section cites),
and `cases/review/`, `cases/additions/`, `cases/errors/` and `cases/migrated/`. Next to each case,
`<case>.golden.json` is libucl's typed dump of the parse, and
`<case>.{config,json,json-compact,yaml}.golden` are libucl's output in each format. Optional
`<case>.flags` and `<case>.inputs` files give parser settings and further inputs.
`tests/conformance.rs` runs three tests: parse results, emitter output and read-back. Known
failures are listed in `xfail-new.txt` and `xfail-emit.txt` with a reason. They may only shrink:
a listed case that passes fails the run.

Golden files come only from libucl and are never edited by hand. `scripts/regen-golden.sh` clones
libucl at the pinned commit under `target/`, builds it and `tools/ucl-dump/`, and runs every case.
It needs git, CMake and a C compiler, and runs on Linux and macOS. `LIBUCL_DIR` reuses an
existing checkout, and `LIBUCL_COMMIT` builds another commit.

Three cases, listed in `tests/conformance/platform-dependent.txt`, have a libucl result that
depends on the platform's C library: glob `[^…]` negation, glob character classes, and the key
taken from the first matched file. On platforms other than macOS their golden files are written
under `tests/conformance/platform/<platform>/`, and the Linux ones are committed. The files next
to each case stay the one expectation the crate is tested against.

```sh
scripts/regen-golden.sh           # regenerate every golden file
scripts/ci.sh golden              # regenerate, and fail if any golden file changed
scripts/ci.sh pin <commit>        # golden files at another libucl commit, in target/pin-move/
```

## Differential fuzzer

`fuzz/` holds `ucl-differential`, a package of its own outside `cargo test`
([fuzz/README.md](fuzz/README.md)). It mutates the conformance cases, parses each input with the
crate and with the oracle binary, compares the results as the conformance suite does, and saves
reduced differences under `target/fuzz-differential/findings/`. `scripts/ci.sh fuzz 600` builds the
oracle if needed and runs it for ten minutes.

## CI workflows

`scripts/ci.sh` runs every check CI runs. It runs `cargo fmt --check`, clippy with `-D warnings`
over every feature set, the tests (the stack-depth tests unoptimised, and the `tests/features/`
package with and without default features), the fuzzer's unit tests, every example, the
benchmarks' build, and `cargo doc` with warnings denied. `scripts/ci.sh coverage` runs the tests
under [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) and writes the report to
`target/coverage/`. `scripts/ci.sh release vX.Y.Z` checks a release tag against the version in
`Cargo.toml` and writes that version's `CHANGELOG.md` section to `target/release-notes.md`.

| Workflow | Runs | Does |
| --- | --- | --- |
| `ci.yml` | push, pull request, manual | `scripts/ci.sh` on Linux and macOS with stable Rust |
| `coverage.yml` | push, pull request, manual | `scripts/ci.sh coverage` on Linux: the report in the job summary and an artifact, and on Codecov when the repository has a `CODECOV_TOKEN` secret; without one the upload is skipped |
| `release.yml` | a pushed tag `vX.Y.Z` | `scripts/ci.sh release` and `ci.yml`, then `cargo publish` through crates.io Trusted Publishing, skipped with a notice when crates.io has the version already, then the GitHub release with the version's `CHANGELOG.md` section as notes, unless it exists; nothing is published if a check fails |
| `golden.yml` | nightly, manual | `scripts/ci.sh golden` on Linux and macOS: fails if libucl's results drifted from the golden files |
| `pin-move.yml` | manual, with a libucl commit | `scripts/ci.sh pin` on Linux and macOS: publishes the golden-file changes as a summary and a patch, one artifact per platform, and commits nothing |
| `fuzz.yml` | manual, with a duration and seed | `scripts/ci.sh fuzz`: fails on a difference and uploads the findings |

A release is a commit that sets the version in `Cargo.toml` and adds its `CHANGELOG.md` section
(`## X.Y.Z - DATE`), and a tag `vX.Y.Z` pushed at it. `release.yml` publishes with crates.io
[Trusted Publishing](https://crates.io/docs/trusted-publishing): the repository stores no
crates.io token, and the publish job, which runs in the GitHub environment `release`, exchanges
GitHub's OIDC token for a crates.io token that lasts for the job. This needs a one-time setup on
crates.io, once the crate exists there: in the crate's settings, under Trusted Publishing, add a
GitHub publisher with owner `AnderEnder`, repository `serde_ucl`, workflow `release.yml` and
environment `release`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

The repository also holds libucl's test files under `tests/conformance/libucl/`. They are covered
by libucl's BSD-2-Clause license,
[tests/conformance/libucl/LICENSE](tests/conformance/libucl/LICENSE), and are not part of
the published package.

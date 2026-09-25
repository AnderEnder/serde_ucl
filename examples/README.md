# Examples

Each example is a program that parses UCL with this crate, checks the result with assertions,
and prints what it found. Every example exits with status 0; a non-zero exit means an assertion
failed. The UCL in the examples is read as libucl reads it (checked with the libucl oracle).

Run one with:

```bash
cargo run --example <name>
```

## The examples

| Example | What it shows |
| --- | --- |
| `basic_usage` | `from_str` into nested structs; `key value` entries and unquoted strings; a parse error with its kind and position; a serde error for a document that does not fit the struct. |
| `complete_ucl_syntax` | A tour of the format: separators, objects and arrays, named sections (`server "name" { }`), repeated keys, the four string forms, comments, keywords, numbers, variables, and `.include` and `.priority` with an in-memory loader. The document is also written in libucl's config format. |
| `number_parsing` | Integers, floats, hex, decimal and binary multipliers (`5k`, `64kb`), time suffixes (`30s`, `1.5min`), the `no-time` flag, range errors, and numbers into Rust types. |
| `web_server_config` | A web server configuration into structs with `Duration` fields (`ucl_lexer::time`); per-environment files layered with `.include(priority=1, duplicate="merge")`; registered variables and a variable handler. |
| `framework_integration` | Configuration structs of the shape Axum-, Tokio- and API-style services take; `#[serde(default)]` for partial documents; writing the effective configuration back with `to_string` and `to_json_string`. |
| `nginx_style_framework_integration` | nginx-style UCL (`key value;`, `location "/api" { }`) mapped onto service structs; how named sections and repeated keys land in serde under the `append` and `merge` strategies. |
| `real_world_configurations` | An infrastructure configuration assembled from several files (`.include`, `.include(duplicate="merge", priority=5)`, `.include(try=true)`, `.inherit`); priorities in the parsed tree; an audit snapshot that reads back as the same value; `.try_include` of a missing file as `UclError::Stopped`. |
| `configuration_management` | Layers (defaults, a site file, command-line overrides) combined by include priorities; per-environment variables and secrets from a variable handler with fallbacks; validation (parse errors, serde errors, cross-field checks); comparing a reloaded document with the running one. |
| `real_world_usage` | Four larger configurations: a microservice, a CI/CD pipeline, a game server and an IoT device, with registered variables and a variable handler. |
| `advanced_features` | Parser settings through `ParserBuilder`: variables and the `$$` rule, duplicate strategies, priorities, macros with a `MemoryLoader`, saved comments and config output with comments, the four output formats, silent stops, and errors inside included files. |

## Points the examples rely on

- **No file access from text.** `from_str`, `from_slice`, `from_reader` and a default
  `parse::Parser` read no files: an `.include` in text input finds nothing. `from_file` reads
  the file and its includes from the filesystem, resolving relative include paths against the
  file's directory. The examples that use includes give the parser a `MemoryLoader`
  (`ParserBuilder::with_loader`); `FsLoader` reads the filesystem instead.
- **Variables.** `$NAME` and `${NAME}` expand to registered variables
  (`from_str_with_variables`, `ParserBuilder::with_variable`). A variable handler
  (`ParserBuilder::with_variable_handler`, or `from_str_with_env` for the environment) is asked
  only for braced references to names that are not registered. UCL has no `${NAME:-default}`
  form, and expansion never produces a number or boolean.
- **Suffixes and comments.** A number with a suffix must be followed directly by a line break,
  `;`, `,`, `#`, `}`, `]` or the end of input (spec §5.5): in `timeout = 30s  # comment` or `{ window = 1min }` the value is
  the string `"30s"` or `"1min"`. Put such comments on their own line.
- **Comments.** `#` and `/* */` (which nest) are comments; `//` is not.
- **Repeated keys.** A repeated key holds several values; serde reads them as a sequence, and a
  single value into a `Vec` field as a one-element sequence.

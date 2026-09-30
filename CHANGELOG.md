# Changelog

All notable changes to this crate are recorded here.

## 0.6.0 - 2026-09-30

Optional includes of files that do not exist and floats parse faster, and includes follow
libucl in two more cases. A file that exists but cannot be read is now an error for an optional
include, where 0.5.0 skipped it.

### Faster optional includes and floats

Clean-room work item C14. Against 0.5.0, on an Apple M4 Max, with `scripts/bench-compare.sh`:

- An optional include (`.include(try=true)`, `.try_include`) of a path that does not exist fails
  with one filesystem call in `FsLoader`, instead of resolving the whole path first. rspamd's
  `groups.conf`, which includes 14 files and tries 42 that do not exist, parses in 0.71 ms
  instead of 1.22 ms, and `composites.conf` in 34 µs instead of 57 µs.
- A float's text is no longer checked as UTF-8 a second time: `canada.json`, mostly floats,
  parses 11% faster.
- The other benchmark documents are within 2% of 0.5.0.

### Include behaviour (spec-v15 to spec-v17)

- A `.include` with a search path whose first directory lacks the file is an error, as before,
  except when a later directory has the file and a skipped URL include
  (`.include(try=true, url=true)` or `.try_include(url=true)` with `://` in its path) follows in
  the same input: the document is then accepted with the later directory's file, as libucl
  accepts it (spec §9.4).
- A regular file that exists but cannot be read is an error for optional includes too
  (`try=true`, `.try_include`, also when a glob selects it); before, it was skipped (spec §9.4).

### API

- `parse::Loader::canonicalize_optional`: the canonical path of an optional include. Its default
  calls `canonicalize`; `FsLoader` checks that the path exists first.

### Benchmarks

- Irregular configurations from a seeded generator, whose keys, section sizes, order, depth and
  strings vary, and the JSON documents `twitter.json`, `citm_catalog.json` and `canada.json`,
  which `benches/fetch-documents.sh` fetches at a pinned commit with SHA-256 checks. New groups
  `parse/irregular`, `parse/json-corpus`, `parse/corpus`, `serde/irregular-*`,
  `serde/json-corpus-*` and `serde/corpus-*`; the corpus groups parse three rspamd
  configurations of `benches/corpus/`, one with the 14 files it includes.
  `benches/check-documents.sh` checks every document against libucl (clean-room work item C14).
- `scripts/bench-compare.sh` also times the previous release, and runs the timings on a thread
  of their own, so that the size of the environment does not move the stack and the results.

## 0.5.0 - 2026-09-28

Serde targets can borrow keys and strings from the input, and parsing and deserialization are
faster again. The value model gains a lifetime and objects their own iterator types, which
changes the value API; hand-written visitors that implement only `visit_string` no longer get
the strings that borrow.

### Serde targets that borrow

`from_str`, `from_slice` and `UclDeserializer` now fill targets that borrow from the input
without copying (clean-room work item C13): `&str` fields, `Cow<str>` fields with
`#[serde(borrow)]`, and `&str` map keys take the keys and strings that appear in the text as
they are. A key or string that does not (an escaped or expanded string, a key that
`KEY_LOWERCASE` changes, a string from an included file or from text a registered macro parses
in place) is owned in a `Cow`, and still fails for a `&str`, with its path and position. Before,
every target that borrows failed. Owned targets get the same values as before. Which visitor
method a borrowed string reaches changes, which breaks visitors that implement only
`visit_string` (see *Breaking API changes*).

`from_str`, `from_slice`, `from_reader`, `from_file` and the `from_str_with_*` functions now
parse the document at the target's first request of the deserializer, as `UclDeserializer`
does, and that request decides whether the value borrows: a `UclValue` or `UclObject` target
gets a value that owns its strings, without a copy (clean-room work item C13, owner decision of
2026-09-28). The target's `Deserialize` runs once on a document that parses and deserializes
(twice, as before, when deserialization fails, to find the error's position). On a document
that does not parse, it now runs too, and its first request fails with the parse error; the
function returns that parse error, with its kind, position and file, whatever the target made of
it.

### Breaking API changes

- Strings that borrow from the input reach visitors through `visit_borrowed_str` where a `&str`
  is asked for (clean-room work item C13, owner decision of 2026-09-28): for a string value
  through `deserialize_str`, and for a key through `deserialize_str`, `deserialize_identifier`
  and `deserialize_any`. They reached them through `visit_string`. A hand-written visitor that
  implements `visit_string` but not `visit_str` or `visit_borrowed_str` therefore no longer
  receives these strings, and fails with an invalid-type error; implement `visit_str`, which
  serde's default `visit_borrowed_str` calls, or ask with `deserialize_string`, which still
  offers `visit_string`. `serde_json` behaves the same. This applies to `from_str`,
  `from_slice`, `from_reader`, `from_file`, the `from_str_with_*` functions and
  `UclDeserializer`. Strings that do not borrow (escaped, expanded, lowercased, from an included
  file), a string value asked for with `deserialize_any`, and the strings of a parsed value
  given to `from_value` are offered as before. serde's own types, derived types and
  `serde_json::Value` are unaffected.

- The value model can borrow (clean-room work item C13). `Value<'a>` is a value whose keys and
  strings may borrow text that lives for `'a`; `UclValue` is now `Value<'static>`, and
  `UclObject` and `UclArray` are `Object<'static>` and `Array<'static>`, so code that names the
  owned types keeps compiling where it does not look inside strings and arrays. `Entry`, `Slot`
  and `Values` have a lifetime parameter. The parser and every other function that gives a value
  give an owned one; `Value::into_owned` turns a borrowed value into an owned one.
  - Strings and keys are the new `value::Str<'a>`, borrowed or owned, which derefs to `str`,
    compares and hashes as one, and converts from `&str`, `String` and `Cow<str>` and into
    `String`. `UclValue::String` holds a `Str`: build one with `"text".into()` or
    `Str::from(string)`, and read it through `as_str`, deref or `String::from`. `UclObject`'s
    keys (`get_index`, `iter`, `keys`, `into_iter`, `remove_index`) are `Str`s, and its methods
    that take a key take `impl Into<Str>`.
  - `UclArray` is a struct around the vector of values, which it derefs to, instead of a
    `Vec<UclValue>` alias: build one with `vec![...].into()` or `collect()`, and take the vector
    with `into_vec`.
  - Dropping a value no longer recurses, as cloning and comparing already did not: arrays and
    objects drop what they hold through a heap stack. `Debug` output is as before.
  - The emitters and `Serialize` take a `Value` of any lifetime.

- An object of up to 16 keys keeps them in a vector, searched in order, instead of a hash index
  (clean-room work item C13). `UclObject::iter`, `iter_mut`, `keys` and `entries`, and its
  `IntoIterator` implementations, return the crate's `value::Iter`, `IterMut`, `Keys`, `Entries`
  and `IntoIter` instead of `indexmap`'s iterators. They are opaque structs, double-ended,
  exact-size and fused, and give the same items in the same order; `indexmap` no longer appears
  in the crate's API.

### Faster parsing and deserialization

Parsing is 7% to 31% faster than in 0.4.0, deserialization 14% to 33%, and serializing the
benchmarks' typed configuration 22% to 30% (clean-room work item C13). The gains come from three
changes: objects of up to 16 keys keep their keys in a vector, a parse no longer copies its file
variables and base directory, and the serde entry points borrow keys and strings from the input.
On an Apple M4 Max, with the crate's benchmarks (0.3.0, 0.4.0 and this version measured in the
same runs, so the 0.4.0 figures differ from those in its own entry):
- `Parser::parse` of the 1000-service configuration takes 2.24 ms instead of 2.52 ms (5.04 ms in
  0.3.0), and of the 1000-record JSON document 1.38 ms instead of 1.69 ms (2.84 ms).
- `from_str` of the configuration into a typed struct takes 2.19 ms instead of 2.79 ms
  (5.72 ms), and 1.98 ms into a struct that borrows its keys and strings.
- A three-entry document parses in 0.49 µs instead of 0.72 µs, with 6 allocations instead of
  13; `from_str` of it into a typed struct takes about 0.52 µs instead of 0.78 µs, with 3.

Values, errors with their positions, and emitter output do not change.

## 0.4.0 - 2026-09-27

Parsing is about twice as fast as in 0.3.0, and small documents much faster. Output facts
are found by position, which changes the parse API; the serde interface is unchanged.

### Breaking API changes

- Output facts are found by the position of a value in the parsed document: the position of its
  entry in its object and of its element in its array (clean-room work item C13). An emitter
  uses the facts of an entry's value only when the entry at that position still has the key it
  had in the parse, so a value changed after the parse so that another key is at its position is
  written as if it had no facts. For example, after `remove` of an entry, the entries after it
  lose their single quotes and key spellings in config output; before, facts followed the keys
  by name, and a key removed and added again took the old value's facts. Values at their own
  positions, and every unchanged document, are written as before.
  `OutputFacts::insert(path, facts)` becomes `OutputFacts::insert(root, path, facts)`: it
  takes the value the facts are for, to find the positions of `path`, and returns false when
  `root` has no value there. `OutputFacts::get` and `OutputFacts::iter` still take and give key
  paths.

### Faster parsing

Parsing is about 1.6 to 2 times as fast as in 0.3.0 (clean-room work item C11). On an Apple
M4 Max, with the crate's benchmarks: `Parser::parse` of the 1000-service configuration takes
2.8 ms instead of 5.1 ms, of the 1000-record JSON document 1.8 ms instead of 2.8 ms, and
`from_str` of the configuration into a typed struct 2.9 ms instead of 5.7 ms. A three-entry
document parses in 0.83 µs instead of 1.19 µs. Values, errors with their positions, and emitter
output do not change. The `indexmap` requirement is now 2.2.2 (was 2), whose raw entry API the
parser uses.

The fixed cost of a parse is lower (clean-room work item C12): `Parser::new` no longer
allocates, and a parse of the three-entry document allocates 13 times, instead of 19 with a
reused parser and 25 with a new one. It takes 0.70 µs with a reused parser instead of 0.83 µs,
`parse::parse` of it 0.72 µs instead of 0.98 µs, and `from_str` into a typed struct 0.76 µs
instead of 0.99 µs. Large documents parse as fast as after C11.

## 0.3.0 - 2026-09-26

The crate is renamed `serde_ucl`. This release also gives deserialization errors the position of
the offending value, adds several inputs into one parser, registered macros, search directories,
an input limit and a configurable `.inherit` depth limit, and follows libucl in more edge cases
(spec-v12 and spec-v13). It is not compatible with 0.2.0: read *Renamed to `serde_ucl`* and
*Breaking API changes* before upgrading.

### Renamed to `serde_ucl`

**Breaking.** The package `ucl-rust-lexer` and the library `ucl_lexer` are renamed `serde_ucl`,
and the GitHub repository `AnderEnder/ucl-rust-lexer` is renamed `AnderEnder/serde_ucl`
(<https://github.com/AnderEnder/serde_ucl>). From this release the crate is published on
crates.io, with its documentation on docs.rs. To upgrade, depend on the new name:

```toml
[dependencies]
serde_ucl = "0.3"
```

and change every `ucl_lexer::` path to `serde_ucl::`, attributes included:
`use ucl_lexer::parse::Parser;` becomes `use serde_ucl::parse::Parser;`, and
`#[serde(with = "ucl_lexer::time")]` becomes `#[serde(with = "serde_ucl::time")]`. The rename
changes nothing else.

### Breaking API changes

- A document that parses but does not fit the target type is the new
  `UclError::Deserialize(error::DeserializeError)`, not `UclError::Serde`, which is left for
  serialization. The error has serde's error (`DeserializeError::error`), the path of the value
  it is about (`path`, as `parse::PathSegment`s; `at_key` for an error about a key, such as an
  unknown field) and, from `from_str`, `from_slice`, `from_reader`, `from_file`, the
  `from_str_with_*` functions and `UclDeserializer`, where the value was written (`position`, and
  `file` for a value from an included file), also for values merged by the duplicate rules or
  copied by `.inherit`. `UclError::position` returns it. `from_value` gives neither a path nor a
  position. `SerdeError::TooDeep` of deserialization comes inside `UclError::Deserialize`. The
  text functions record nothing while deserialization succeeds: when it fails, they parse the
  document again, recording where values were written, and deserialize the target a second time,
  recording the path; the second parse asks the loader for included files again but gives the
  variable handler's answers from the first parse instead of asking it. `UclDeserializer`, which
  cannot run its visitor twice, records paths as it deserializes.

### Added

- Search directories for the include macros: `ParserBuilder::with_search_path`,
  `Parser::set_search_path`, `Parser::clear_search_path` and `Parser::search_path`. The list is
  in effect from the start of every parse, as a `path` list given to an earlier include would be
  (spec §9.4), and in macro argument documents too.
- An input limit: `ParserBuilder::with_max_input_bytes`, `Parser::set_max_input_bytes` and
  `Parser::max_input_bytes` cap the bytes one parse reads, the document and the files that
  `.include`, `.try_include` and `.load` read for it together. Going over it is the new
  `parse::ErrorKind::InputTooLarge`, which `try=true` and `.try_include` do not soften. The
  default is no limit, as in libucl. `Loader::read_limited`, with a default that calls
  `Loader::read`, lets a loader stop reading a file once it is over the limit; `FsLoader` and
  `MemoryLoader` do.
- Several inputs into one parser (spec §13.1): `Parser::inputs` starts a parse that takes inputs
  in turn, `Inputs::add` reads each `Input` (`Input::bytes`, `Input::file`, with
  `Input::with_priority` and `Input::with_strategy`), and `Inputs::finish` returns the result. A
  silent stop ends only its own input; any other error fails the parse. libucl's quirks at the
  joins are kept: at most 16 inputs (`ErrorKind::TooManyInputs`), which count towards the include
  nesting limit; nothing can follow a closed root or a zero-byte first input
  (`ErrorKind::AfterRoot`); and the end of an input is not a separator
  (`ErrorKind::UnseparatedInput`). `Parser::parse` and `Parser::parse_file` are parses of one
  input.
- Registered macros (spec §13.2): `Parser::register_macro`, `Parser::register_context_macro`,
  `ParserBuilder::with_macro` and `ParserBuilder::with_context_macro` register a handler
  (`MacroHandler`) by name. It gets a `MacroCall`, with the macro's value, arguments and, for a
  context macro, the root built so far; it can add entries (`MacroCall::add`), have text parsed
  in place (`MacroCall::parse`), stop the parse silently (`MacroError::stop`, reported as
  `ErrorKind::MacroStopped`, a silent stop) or fail with a message (`MacroError::new`,
  `ErrorKind::MacroFailed`; libucl has no such error). A registered name replaces a built-in
  macro of the same name. A deserialization error of a document whose parse ran a registered
  macro has no position, since the document is not parsed again. A context macro also gets the
  root's priority, that of the first input (`MacroCall::root_priority`), which a copy of the root
  keeps in libucl; `MacroCall::add_with_priority` adds a value at a priority other than 0.
- A configurable limit on the copies `.inherit` makes: `ParserBuilder::with_inherit_depth_limit`,
  `Parser::set_inherit_depth_limit` and `Parser::inherit_depth_limit`. A copy that would nest a
  value more than the limit deep, the root included, fails with `ErrorKind::NestingTooDeep`,
  whose `limit` is the setting. The default, `parse::DEFAULT_INHERIT_DEPTH_LIMIT`, is 1024; the
  largest setting, `parse::MAX_INHERIT_DEPTH_LIMIT`, is 2048, the largest round depth at which
  every entry point handles the parser's values on a 2 MiB thread stack in an unoptimised build.
  A larger setting panics. The serde text functions reject a value nested more than 1024 deep,
  which could not be parsed again; `emit` writes it. libucl sets no limit.
- `UclError::file`: the included file that `UclError::position` is in, if any.
- `MAX_PRIORITY` (15) is re-exported from the crate root.

### Behaviour changes

- Edge cases follow libucl (spec-v12): after an entry, a VT or FF makes a `#` that is the last
  byte of the input an error unless a comment comes directly before it (§2.2); `<<` followed by
  two or more uppercase letters and then the end of the document, an included file or a macro
  argument list is an unterminated heredoc, `ErrorKind::UnterminatedHeredoc` (§6.3), where it was a
  string; in macro arguments a `"` after a `\` outside quotes begins a quoted part, and a `(`
  that is the last byte is the macro's value (§9.2); a macro inside macro arguments whose own
  arguments are rejected runs without them instead of failing the document (§9.2); a NUL byte in
  a braced macro value ends an include or `.load` path and a `.priority` value, where an empty
  priority is 0 (§9.2, §9.5); `.load` looks its path up as written, so a regular file followed
  by `/` is not found (§9.6); after a fraction or exponent, the number before an `x` does not
  count toward the 127-character limit (§5.2, §5.3); and an included file, or text a registered macro parses in place,
  that holds only whitespace and then a `{` or `[` as its last byte adds nothing (§9.4, §13.2),
  where it was an error.
- Edge cases follow libucl (spec-v13): with a leading `-`, a number with an `x` after a fraction
  or exponent whose hex digits begin with a letter is a string (`-1.5xd`), where it was `0`
  (§5.2); the string parameters of the include macros and of `.load` (`key`, `target`,
  `duplicate` and the entries of `path`) end at their first NUL byte (§9.2); `.load(try=true)`
  with a braced value that starts with a NUL byte is skipped as a missing file, where it was an
  error (§9.6); with `glob=true`, a `*` or `?` after a NUL byte in the value makes the part before
  the NUL a pattern (§9.4); an included file or text in place that holds only a `{` or `[` after a
  leading comment group adds nothing (§9.4, §13.2); `.inherit` keeps only the first value of an
  entry whose first value is an object or an array at every level of a copy, not only in the
  copied object itself (§9.7); and text that a registered macro parses in place keeps the section
  object it stands in open for the rest of the parse (§13.2).
- Comparing (`==`) a `UclValue`, `UclObject`, `Entry` or `Slot` takes the same stack at any
  depth, as cloning them does, where it recursed into nested values.

### Packaging

- The crate targets the latest stable Rust (1.98 at this release; `rust-version` in
  `Cargo.toml`), where 0.2.0 required 1.88.
- `Cargo.toml` sets `homepage` (the repository) and `documentation` (docs.rs).
- `cargo bench` runs the benches in `benches/` only, so criterion's options work
  (`cargo bench -- --noplot`); the library has no benchmarks (`bench = false`).
- Release builds of this package, its benches and examples, use fat LTO and one codegen unit.
  Crates that depend on it build it with their own release profile.

### Repository

- libucl's license moved from the repository root to `tests/conformance/libucl/LICENSE`, next to
  the libucl test files it covers. It was never in the package.
- `fuzz/` holds a differential fuzzer that compares the crate's parse results with libucl's on
  generated inputs (`scripts/ci.sh fuzz`, the manual workflow `fuzz.yml`).
- New workflows: `pin-move.yml` (manual: the golden files at another libucl commit, for review),
  `coverage.yml` (cargo-llvm-cov on Linux; Codecov with a `CODECOV_TOKEN` secret) and
  `release.yml` (on a `vX.Y.Z` tag: the checks, then `cargo publish` through crates.io Trusted
  Publishing and the GitHub release). CI runs on stable Rust only.

## 0.2.0

This release replaces the parser with one that reads UCL as libucl does, adds output in libucl's
formats and serde serialization, and removes the old lexer, parser and extension API. It is not
compatible with 0.1.0: read the sections below before upgrading. The full format rules are in
`docs/spec/` in the repository.

### Text input never reads files

**Breaking.** A document given as text has no file access by default. This applies to
`from_str`, `from_slice`, `from_reader`, `UclDeserializer::new`, `UclDeserializer::from_slice`,
`parse::parse`, and any `parse::Parser` that keeps its default loader: in every build, that loader
is an empty `parse::MemoryLoader`, which holds no files. In such a document:

- `.include` finds no file: it fails with `ErrorKind::FileNotFound`, or with `try=true` does
  nothing;
- `.try_include` stops the parse: the serde entry points return `UclError::Stopped`, and
  `Parser::parse` an error for which `parse::Error::is_stopped` is true. Its
  `parse::Error::partial` holds the entries parsed before the macro;
- `.load` (feature `load`) fails with `ErrorKind::FileNotFound`, and with `try=true` does
  nothing;
- `Parser::parse_file` fails as well, because the default loader holds no file to read.

Only `from_file` (feature `fs`, on by default) reads the filesystem without being asked to. It
resolves relative include paths against the directory of the file it was given, also inside the
files that one includes; a base directory set on the parser takes precedence. libucl resolves them
against the process's working directory instead.

To let a document given as text read files, give its parser a filesystem loader, and preferably a
base directory:

```rust
use ucl_lexer::parse::{FsLoader, ParserBuilder};

let mut parser = ParserBuilder::new()
    .with_loader(FsLoader::new()) // or Parser::set_loader
    .with_base_dir("/etc/myapp")
    .build();
let config: Config = ucl_lexer::from_value(parser.parse(text.as_bytes())?)?;
```

With `FsLoader` and no base directory, relative paths in text input, and `$CURDIR`, resolve
against the process's working directory.

### Breaking API changes

Removed:

- The modules `ucl_lexer::lexer` and `ucl_lexer::parser`, with everything in them, and their
  re-exports from the crate root: `UclLexer`, `LexerConfig`, `StreamingUclLexer`,
  `StringFormat`, `Token`, `streaming_lexer_from_file`, `streaming_lexer_from_reader`,
  `UclParser`, `UclParserBuilder`, `ParserConfig`, `DuplicateKeyBehavior`, the
  `VariableHandler` trait, `VariableContext`, `MapVariableHandler`,
  `EnvironmentVariableHandler`, `ChainedVariableHandler`, `ParsingHooks`, `NumberSuffixHandler`,
  `StringPostProcessor`, `ValidationHook`, `UclPlugin`, `PluginConfig`, `PluginRegistry`,
  `ConfigValidationPlugin`, `CssUnitsPlugin`, `PathProcessingPlugin`, `CustomUnitSuffixHandler`,
  `PathNormalizationProcessor` and `SchemaValidationHook`.
- In `error`: `LexError`, `ParseError`, `Span`, `ErrorContext` and `EnhancedError` (and their
  root re-exports); the variants `UclError::Lex` and `UclError::Parse`; the methods
  `UclError::with_source_context`, `UclError::format_with_context`, `Position::advance` and
  `Position::advance_by`; the variants `SerdeError::TypeMismatch`, `SerdeError::MissingField` and
  `SerdeError::UnknownField`, which were never produced.
- In `de`: `from_str_with_config`, `from_str_with_config_and_variables`,
  `UclDeserializer::with_lexer_config` and `UclDeserializer::with_variable_handler`. Parser
  settings are now made with `parse::ParserBuilder`; pass the parser to
  `UclDeserializer::from_parser`, or parse to a `UclValue` and call `from_value`.

Changed:

- `from_str_with_variables(text, variables)` takes `(name, value)` pairs (any
  `IntoIterator<Item = (K, V)>` with `K, V: Into<String>`), registered in order, instead of a
  boxed handler.
- `from_str_with_map` registers the map's names in descending byte order, which puts every name
  before the names that are prefixes of it, so an unbraced `$NAME` takes the longest matching
  name.
- `from_str_with_env` asks the environment only for braced references: `${HOME}` is expanded,
  `$HOME` is left as written.
- A variable handler is a closure `FnMut(&str) -> Option<String>`, installed with
  `ParserBuilder::with_variable_handler` or `Parser::set_variable_handler`. It is asked only for
  braced references `${NAME}` whose name is not registered, never for `$NAME`. If it returns
  `None`, the reference stays as written.
- `UclDeserializer::from_parser(parser, input)` takes a `parse::Parser` and the input bytes;
  `UclDeserializer::parser` and `parser_mut` return that `parse::Parser`.
- A document the parser rejects is `UclError::Syntax(parse::Error)`, with its `kind()` (a
  `parse::ErrorKind`), its `position()` and, for an error inside an included file, that
  `file()`. `UclError::Stopped` is new (see above). Error messages have new wording.
- A `Position` has a 1-based line and column, the column counted in characters, and a 0-based
  byte offset. Only a line feed starts a new line; a carriage return is counted as a character.

Added:

- `parse`: the parser. `Parser` and `ParserBuilder` (also `Parser::builder`) set parser flags
  (`ParserFlags`), the duplicate-key strategy (`DuplicateStrategy`) and priority, registered
  variables, a variable handler, the loader (`Loader`, `MemoryLoader`, and `FsLoader` with
  feature `fs`) and a base directory. The parser can save comments and attach them to values.
  `parse::parse` parses bytes with default settings.
- Macros: `.include` and `.try_include` with their parameters and glob patterns, `.priority`,
  `.inherit`, and `.load` behind the `load` feature (off by default). An unknown macro is an
  error.
- `emit`: output in libucl's four formats, the config format, JSON, compact JSON and YAML, byte
  for byte as libucl writes them. Saved comments appear in config output only when asked for
  (`Emitter::with_comments`).
- serde serialization: `to_string` (config format), `to_json_string`, `to_json_string_compact`,
  `to_yaml_string`, `to_writer` and `to_value`, and `Serialize` and `Deserialize` for `UclValue`
  and `UclObject`. The output reads back, in this crate and in libucl, as exactly the value
  written, floats included. JSON output is valid JSON: a time is written as its number of
  seconds, and a NaN or infinite float or time cannot be written as JSON
  (`SerdeError::Unrepresentable`, also used for other values that have no form in a format).
- The crate's serde functions move a `UclValue` or `UclObject` as it is, also as a field of
  another type, with the same stack at any depth: `from_value::<UclValue>(v)` and `to_value(&v)`
  give `v` back, priorities and the marks of `.inherit` copies included. Other types are read
  and written by recursion, and there nesting is limited to `MAX_SERDE_NESTING` (128) maps and
  sequences; deeper nesting fails with the new `SerdeError::TooDeep`. Cloning a `UclValue`
  takes the same stack at any depth as well.
- `time` serializes `Duration` as well as deserializing it.
- `from_slice`, `from_reader`, `from_file` (feature `fs`) and `from_value`; `from_value`,
  `from_str_with_env` and `from_str_with_map` are also re-exported from the crate root.
  `UclDeserializer::from_slice`.
- In the value model: `Placement` (also at the crate root), `UclObject::insert_slot_placed`,
  `UclObject::get_index`, `UclObject::get_index_mut`, `UclObject::index_of`,
  `UclObject::rename_key` and `Entry::value_at_mut`.
- `UclError::parse_error` and `UclError::position`.
- Cargo features `fs` (default) and `load`.

### Behaviour changes

Parsing now follows libucl. Documents that the old parser read may parse differently or fail:

- `//` does not start a comment: `// text` is the key `//` with the value `text`. Comments are
  `#` and `/* … */`.
- An unquoted value runs to the end of the line, a `,`, a `;` or a comment, spaces included:
  `k = 1 2 3` is the string `"1 2 3"`.
- A number with a suffix followed by a space is a string: in `timeout = 30s # comment` the value
  is `"30s"`, while `timeout = 30s` is a time.
- `null` is recognised in lowercase only; `NULL` and `Null` are strings. Likewise `inf` and
  `nan`: `Inf` and `NaN` are strings.
- `\u` takes exactly four hex digits; `\u{…}` is an error. An escape that is not defined drops
  the backslash (`"\q"` is `"q"`). A raw TAB inside a double-quoted string is an error.
- A heredoc's terminator name consists of uppercase letters (`<<EOF`); `<<eof` does not start a
  heredoc.
- `key name { … }` nests: `a b { c = 1 }` is `a { b { c = 1 } }`.
- A repeated key keeps every value. Deserializing such a key into a field that is not a sequence
  fails with "invalid type: sequence"; use a `Vec` field, `DuplicateStrategy::Rewrite`, or
  priorities to keep one value.
- Variable expansion always produces a string: with `PORT` set to `8080`, `port = $PORT` is the
  string `"8080"`. There is no `${NAME:-default}` form; it stays as written.
- `$FILENAME` and `$CURDIR` are defined. For text input, `FILENAME` is `undef` and `CURDIR` is the
  base directory, or without one the loader's current directory: `/` for the default loader, the
  process's working directory for `FsLoader`. For a file, they are its canonical path and its
  directory, and registered variables of the same names do not override them. The
  `NO_FILEVARS` flag turns them off for text input. Like other registered names, they also match
  the start of a longer unbraced reference: `$CURDIRx` is `/x` for text input.
- Keys and strings must be valid UTF-8, including strings built from `\u` escapes of surrogate
  code points: anything else is `ErrorKind::InvalidUtf8`. Comments may hold any bytes. libucl
  accepts such bytes.
- Signatures are never verified: `.includes`, and `sign=true` on any include macro, fail with an
  "unsupported" error (`parse::Error::is_unsupported`); `sign=false` is accepted.
- URLs are never fetched: `.include(url=true)` of a path containing `://` fails with
  `ErrorKind::UrlNotSupported`, and with `try=true` is skipped. Without `url=true` such a path is
  an ordinary path.
- Limits: containers nest at most 1024 deep, the root included (`parse::MAX_NESTING`), also in
  the objects that `.inherit` copies (libucl sets no limit there); included files 16 deep
  (`parse::MAX_INCLUDE_DEPTH`) and macro argument documents 64 deep
  (`parse::MAX_ARGUMENT_DEPTH`; libucl sets no limit there either). The old lexer's
  configurable limits are gone. serde deserialization into, and serialization from, types
  other than `UclValue` and `UclObject` follows at most 128 nested maps and sequences
  (`MAX_SERDE_NESTING`).
- Deserializing into a borrowed `&str` field is not supported and fails with "expected a
  borrowed string"; use `String` or `Cow<str>`.

### Removed features, examples and benches

- Features: the streaming lexer and the token API, the plugin and hook system (plugins, number
  suffix handlers, string post-processors, validation hooks), the configurable lexer limits,
  source-context error formatting, and the syntax extensions listed above (`//` comments,
  `\u{…}` escapes, `${NAME:-default}`).
- Cargo features `std`, `save-comments` and `strict-unicode`, which had no effect: the crate
  always uses the standard library, comments are saved with the parser flag
  `ParserFlags::SAVE_COMMENTS`, and keys and strings are always checked to be UTF-8. Remove them
  from `features` lists; `default-features = false` now turns off `fs` only.
- Examples: `cpp_comments_demo`, `extensibility_demo` and `performance_comparison`. The other
  examples are rewritten on the new API.
- Benches: `lexer_benchmarks`, `parser_benchmarks`, `zero_copy_benchmarks`,
  `memory_efficiency_benchmarks` and `ucl_compatibility_benchmarks`, replaced by
  `parse_benchmarks`, `emit_benchmarks` and `serde_benchmarks`.

### Packaging

- The minimum supported Rust version is 1.88 (`rust-version` in `Cargo.toml`).
- The license files `LICENSE-MIT` and `LICENSE-APACHE` are added, for the crate's license
  `MIT OR Apache-2.0`.
- The package holds only the library sources, `Cargo.toml`, `README.md`, `CHANGELOG.md` and the
  two license files. `LICENSE-libucl`, which covers libucl's test files in the repository's
  conformance suite, is not in the package, and neither are the tests, examples, benches or
  `docs/`.

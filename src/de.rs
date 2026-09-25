//! Serde deserializer.
//!
//! UCL text is parsed by the parser core ([`crate::parse`]) into a [`UclValue`](crate::UclValue)
//! tree, and one value deserializer maps that tree onto serde. [`from_str`], [`from_slice`],
//! [`from_reader`] and [`from_file`] parse with a default [`Parser`]; [`from_value`] deserializes
//! a tree that a parser with other settings produced; [`UclDeserializer`] is the document-level
//! `serde::Deserializer`. All of them follow the same rules, for the document and for every
//! nested value:
//!
//! | UCL | serde |
//! | --- | --- |
//! | entry with one value | that value |
//! | entry with several values (implicit array) | sequence |
//! | single value into a sequence target (`Vec<T>`) | one-element sequence |
//! | object into a sequence target | error; use a map (`HashMap`, `IndexMap`) |
//! | `Time` | `f64` seconds (see [`crate::time`] for `Duration` fields) |
//! | `Float` into an integer target | only if integral and in range |
//! | `Null` into `Option<T>` | `None`; anything else is `Some` |
//! | string into an enum | unit variant; an object with one key is a data variant |
//! | key into an integer, `bool`, `char` or float target | the key's text parsed as one |
//! | string, or array of integers 0–255, into bytes | the bytes |
//!
//! Values are owned: a target that borrows from the input, such as a `&str` field, is an error
//! (`invalid type: string "…", expected a borrowed string`); use `String` or `Cow<str>`.
//!
//! `Deserialize for UclValue` keeps what the table flattens: from this crate's deserializer, a
//! time stays a time and an entry with several values keeps them all, so
//! `from_value::<UclValue>(v)` gives `v` back, apart from priorities and the marks of values
//! that `.inherit` copied or `no-implicit-arrays` collected. From any other deserializer it
//! takes what that format offers; integers above `i64::MAX` are an error.
//!
//! # Parsing
//!
//! A default [`Parser`] has no flags, no registered variables and no variable handler, and runs
//! the macros of spec §9. It reads no files (WORKLIST C5 decision 1): for text given to
//! [`from_str`], [`from_slice`], [`from_reader`] or [`UclDeserializer::new`], an `.include`,
//! `.try_include` or `.load` finds no file and behaves as §9.4 and §9.6 describe for a missing
//! file. `FILENAME` is `undef` and `CURDIR` is `/` there (§7.8).
//!
//! [`from_file`] reads the file and its includes from the filesystem. Relative include paths
//! resolve against the file's directory, also inside included files; `FILENAME` and `CURDIR`
//! are the file's canonical path and its directory.
//!
//! [`from_str_with_variables`] and [`from_str_with_map`] register variables for `$NAME` and
//! `${NAME}` references (spec §7); [`from_str_with_env`] installs a variable handler that reads
//! the environment, which is asked for braced references `${NAME}` only (§7.7).
//!
//! To read files for text input, or to change any other setting, build a [`Parser`] (see
//! [`crate::parse::ParserBuilder`]) and deserialize with [`UclDeserializer::from_parser`], or
//! parse to a [`UclValue`](crate::UclValue) and use [`from_value`].

mod value;

use value::ValueDeserializer;
pub use value::from_value;

use crate::error::UclError;
use crate::parse::{Parser, ParserBuilder};
use serde::de::{self, Deserialize, DeserializeOwned, Visitor};
use std::collections::HashMap;
use std::io;
#[cfg(feature = "fs")]
use std::path::Path;

/// Deserializes a UCL document: parses it with its [`Parser`], then deserializes the value.
///
/// ```
/// use serde::Deserialize;
/// use ucl_lexer::UclDeserializer;
/// use ucl_lexer::parse::Parser;
///
/// #[derive(Deserialize)]
/// struct Config {
///     name: String,
/// }
///
/// let mut parser = Parser::new();
/// parser.register_variable("APP", "demo");
/// let config = Config::deserialize(UclDeserializer::from_parser(parser, b"name = $APP"))?;
/// assert_eq!(config.name, "demo");
/// # Ok::<(), ucl_lexer::UclError>(())
/// ```
pub struct UclDeserializer<'a> {
    parser: Parser,
    input: &'a [u8],
}

impl<'a> UclDeserializer<'a> {
    /// A deserializer for UCL text, with a default [`Parser`].
    pub fn new(input: &'a str) -> Self {
        Self::from_slice(input.as_bytes())
    }

    /// A deserializer for UCL bytes, with a default [`Parser`].
    pub fn from_slice(input: &'a [u8]) -> Self {
        Self::from_parser(Parser::new(), input)
    }

    /// A deserializer that parses `input` with `parser`.
    pub fn from_parser(parser: Parser, input: &'a [u8]) -> Self {
        Self { parser, input }
    }

    /// The parser.
    pub fn parser(&self) -> &Parser {
        &self.parser
    }

    /// The parser, for changing its settings before deserializing.
    pub fn parser_mut(&mut self) -> &mut Parser {
        &mut self.parser
    }

    /// Parses the document.
    fn into_value(mut self) -> Result<ValueDeserializer, UclError> {
        let value = self.parser.parse(self.input)?;
        Ok(ValueDeserializer::new(value))
    }
}

/// Forwards each `Deserializer` method of `UclDeserializer` to the value deserializer.
macro_rules! forward_to_value {
    ($($method:ident($($arg:ident: $ty:ty),*))*) => {
        $(
            fn $method<V>(self, $($arg: $ty,)* visitor: V) -> Result<V::Value, UclError>
            where
                V: Visitor<'de>,
            {
                self.into_value()?.$method($($arg,)* visitor)
            }
        )*
    };
}

impl<'de> de::Deserializer<'de> for UclDeserializer<'de> {
    type Error = UclError;

    forward_to_value! {
        deserialize_any()
        deserialize_bool()
        deserialize_i8()
        deserialize_i16()
        deserialize_i32()
        deserialize_i64()
        deserialize_i128()
        deserialize_u8()
        deserialize_u16()
        deserialize_u32()
        deserialize_u64()
        deserialize_u128()
        deserialize_f32()
        deserialize_f64()
        deserialize_char()
        deserialize_str()
        deserialize_string()
        deserialize_bytes()
        deserialize_byte_buf()
        deserialize_option()
        deserialize_unit()
        deserialize_unit_struct(name: &'static str)
        deserialize_newtype_struct(name: &'static str)
        deserialize_seq()
        deserialize_tuple(len: usize)
        deserialize_tuple_struct(name: &'static str, len: usize)
        deserialize_map()
        deserialize_struct(name: &'static str, fields: &'static [&'static str])
        deserialize_enum(name: &'static str, variants: &'static [&'static str])
        deserialize_identifier()
        deserialize_ignored_any()
    }
}

/// Deserializes a `T` from UCL text, parsed with a default [`Parser`].
///
/// ```
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Server {
///     port: u16,
///     timeout: f64,
/// }
///
/// let server: Server = ucl_lexer::from_str("port = 8080\ntimeout = 1.5min")?;
/// assert_eq!((server.port, server.timeout), (8080, 90.0));
/// # Ok::<(), ucl_lexer::UclError>(())
/// ```
pub fn from_str<'a, T>(s: &'a str) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    from_slice(s.as_bytes())
}

/// Deserializes a `T` from UCL bytes, parsed with a default [`Parser`]. Keys and strings must be
/// valid UTF-8, or the parse fails with [`crate::parse::ErrorKind::InvalidUtf8`]; comments may
/// hold any bytes.
pub fn from_slice<'a, T>(v: &'a [u8]) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    T::deserialize(UclDeserializer::from_slice(v))
}

/// Deserializes a `T` from the UCL document `reader` holds, read to its end and parsed with a
/// default [`Parser`]. A read failure is [`UclError::Io`].
pub fn from_reader<T>(mut reader: impl io::Read) -> Result<T, UclError>
where
    T: DeserializeOwned,
{
    let mut input = Vec::new();
    reader.read_to_end(&mut input)?;
    from_slice(&input)
}

/// Deserializes a `T` from the UCL file at `path`, read from the filesystem.
///
/// The file is parsed with a default [`Parser`] whose loader is [`crate::parse::FsLoader`], as
/// [`Parser::parse_file`] parses it: a relative `path` resolves against the process's working
/// directory; relative paths in the file's include macros, and in the files they include,
/// resolve against the file's directory (WORKLIST C5 decision 1); `FILENAME` and `CURDIR` are the
/// file's canonical path and its directory. A failure to read the file itself is
/// [`UclError::Io`]; a failure in an included file is a parse error whose
/// [`file`](crate::parse::Error::file) names it.
#[cfg(feature = "fs")]
pub fn from_file<T>(path: impl AsRef<Path>) -> Result<T, UclError>
where
    T: DeserializeOwned,
{
    let mut parser = Parser::new();
    parser.set_loader(crate::parse::FsLoader::new());
    let (canonical, input) = parser
        .read_file(path.as_ref())
        .map_err(|(path, e)| io::Error::new(e.kind(), format!("{}: {e}", path.display())))?;
    from_value(parser.parse_read_file(canonical, &input)?)
}

/// Deserializes a `T` from UCL text, parsed with a default [`Parser`] on which `variables` are
/// registered, for `$NAME` and `${NAME}` references (spec §7). Otherwise as [`from_str`].
///
/// Variables are registered in iteration order, and that order decides between names that are
/// prefixes of one another in unbraced references: `$NAME` takes the first registered name that
/// is a prefix of the text after `$` (spec §7.4). Registering a name again changes its value but
/// not its place.
///
/// ```
/// let value: serde_json::Value = ucl_lexer::from_str_with_variables(
///     "url = \"https://${HOST}:$PORT/\"",
///     [("HOST", "example.org"), ("PORT", "8443")],
/// )?;
/// assert_eq!(value["url"], "https://example.org:8443/");
/// # Ok::<(), ucl_lexer::UclError>(())
/// ```
pub fn from_str_with_variables<'a, T, I, K, V>(s: &'a str, variables: I) -> Result<T, UclError>
where
    T: Deserialize<'a>,
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    let parser = ParserBuilder::new().with_variables(variables).build();
    T::deserialize(UclDeserializer::from_parser(parser, s.as_bytes()))
}

/// [`from_str_with_variables`] with the variables of a map. A `HashMap` has no order, so they
/// are registered in descending byte order of their names, which puts every name before the
/// names that are prefixes of it: an unbraced reference `$NAME` takes the longest registered
/// name that matches (spec §7.4). Use [`from_str_with_variables`] to choose the order.
pub fn from_str_with_map<'a, T>(
    s: &'a str,
    variables: HashMap<String, String>,
) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    let mut variables: Vec<(String, String)> = variables.into_iter().collect();
    variables.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    from_str_with_variables(s, variables)
}

/// Deserializes a `T` from UCL text, parsed with a default [`Parser`] whose variable handler
/// reads the process's environment. Otherwise as [`from_str`].
///
/// The handler is asked only for braced references `${NAME}` whose name is not registered
/// (spec §7.7): `"${HOME}"` becomes the value of `HOME`, while an unbraced `$HOME` stays as
/// written, as does a braced reference to a variable that is not set or not valid Unicode. The
/// file variables `FILENAME` and `CURDIR` (§7.8) are registered, so they are never read from the
/// environment.
pub fn from_str_with_env<'a, T>(s: &'a str) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    let parser = ParserBuilder::new()
        .with_variable_handler(environment_variable)
        .build();
    T::deserialize(UclDeserializer::from_parser(parser, s.as_bytes()))
}

/// The value of the environment variable `name`, if it is set, valid Unicode, and `name` can be
/// an environment variable's name at all.
fn environment_variable(name: &str) -> Option<String> {
    if name.is_empty() || name.contains(['=', '\0']) {
        return None;
    }
    std::env::var(name).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ErrorKind;
    use crate::value::UclValue;
    use serde::Deserialize;
    use std::collections::HashMap;

    /// The value deserializer over a prepared value (no parsing).
    fn with_value(value: UclValue) -> ValueDeserializer {
        ValueDeserializer::new(value)
    }

    /// The kind and the line and column of a parse error.
    fn parse_error(input: &str) -> (ErrorKind, usize, usize) {
        match from_str::<serde_json::Value>(input).unwrap_err() {
            UclError::Syntax(e) => (e.kind().clone(), e.position().line, e.position().column),
            other => panic!("expected a parse error, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_errors_keep_kind_and_position() {
        // The opening quote.
        assert_eq!(
            parse_error("\nkey = \"unterminated"),
            (ErrorKind::UnterminatedString, 2, 7)
        );
        assert_eq!(parse_error("a = [1, 2").0, ErrorKind::UnterminatedArray);
        assert_eq!(
            parse_error("a = 1\nb = 2\n}"),
            (ErrorKind::UnmatchedClose { found: '}' }, 3, 1)
        );
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct UnsignedField {
        v: u64,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct SignedField {
        v: i64,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct ByteField {
        v: u8,
    }

    #[test]
    fn test_float_to_integer_only_when_integral_and_in_range() {
        // Integral floats and times convert exactly.
        assert_eq!(from_str::<UnsignedField>("v = 30.0").unwrap().v, 30);
        assert_eq!(from_str::<UnsignedField>("v = 30s").unwrap().v, 30);
        assert_eq!(from_str::<SignedField>("v = -30.0").unwrap().v, -30);
        assert_eq!(from_str::<SignedField>("v = 30").unwrap().v, 30);

        // Anything else is an error instead of a truncated or saturated value.
        for input in [
            "v = 30.7",
            "v = 1e300",
            "v = -1.0",
            "v = nan",
            "v = inf",
            "v = -1",
        ] {
            assert!(
                from_str::<UnsignedField>(input).is_err(),
                "u64 from {input:?}"
            );
        }
        for input in ["v = 30.7", "v = 1e300", "v = -1e300", "v = nan"] {
            assert!(
                from_str::<SignedField>(input).is_err(),
                "i64 from {input:?}"
            );
        }
        // Narrower targets are range-checked after the conversion.
        assert!(from_str::<ByteField>("v = 300.0").is_err());
        assert_eq!(from_str::<ByteField>("v = 255.0").unwrap().v, 255);
    }

    #[test]
    fn test_document_and_nested_values_follow_the_same_rules() {
        // The document-level deserializer hands its value to the value deserializer, so a
        // top-level value behaves exactly like a nested one.
        assert_eq!(
            u64::deserialize(with_value(UclValue::Float(30.0))).unwrap(),
            30
        );
        assert_eq!(
            i64::deserialize(with_value(UclValue::Time(-2.0))).unwrap(),
            -2
        );
        assert!(u64::deserialize(with_value(UclValue::Float(30.7))).is_err());
        assert!(u64::deserialize(with_value(UclValue::Float(1e300))).is_err());
        assert!(i64::deserialize(with_value(UclValue::Float(f64::NAN))).is_err());
        assert_eq!(
            Option::<u32>::deserialize(with_value(UclValue::Integer(7))).unwrap(),
            Some(7)
        );
        assert_eq!(
            Option::<u32>::deserialize(with_value(UclValue::Null)).unwrap(),
            None
        );
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Port(u16);

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "lowercase")]
    enum Level {
        Debug,
        Info,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Fields {
        maybe: Option<u32>,
        #[serde(default)]
        absent: Option<u32>,
        nothing: Option<u32>,
        level: Level,
        port: Port,
    }

    #[test]
    fn test_option_enum_and_newtype_fields() {
        // Each of these once failed for struct fields: the value deserializer sent
        // `option`, `enum` and `newtype_struct` to `deserialize_any`.
        let parsed: Fields =
            from_str("maybe = 30; nothing = null; level = \"info\"; port = 8080").unwrap();
        assert_eq!(
            parsed,
            Fields {
                maybe: Some(30),
                absent: None,
                nothing: None,
                level: Level::Info,
                port: Port(8080),
            }
        );
        assert!(from_str::<Fields>("maybe = 1; nothing = null; level = trace; port = 1").is_err());
        let _ = Level::Debug;
    }

    #[test]
    fn test_time_reads_as_seconds() {
        #[derive(Deserialize)]
        struct Timeouts {
            connect: f64,
            read: f32,
        }
        let parsed: Timeouts = from_str("connect = 1.5min; read = 250ms").unwrap();
        assert_eq!(parsed.connect, 90.0);
        assert_eq!(parsed.read, 0.25);
    }

    #[test]
    fn test_repeated_key_reads_as_sequence() {
        #[derive(Deserialize)]
        struct Servers {
            server: Vec<String>,
        }
        let parsed: Servers = from_str("server = a; server = b; server = c").unwrap();
        assert_eq!(parsed.server, vec!["a", "b", "c"]);

        // A value repeated after an explicit array is a second value, not an extension.
        let value: serde_json::Value = from_str("k = [1, 2]\nk = 3").unwrap();
        assert_eq!(value["k"], serde_json::json!([[1, 2], 3]));
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Repos {
        #[serde(default)]
        repo: Vec<String>,
        mirror: Option<Vec<String>>,
    }

    #[test]
    fn test_one_or_many_sequences() {
        let parse = |input: &str| from_str::<Repos>(input).unwrap();
        let strings = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();

        // Zero occurrences
        assert_eq!(
            parse(""),
            Repos {
                repo: vec![],
                mirror: None
            }
        );
        // One occurrence: a one-element sequence
        assert_eq!(
            parse("repo = a\nmirror = m1"),
            Repos {
                repo: strings(&["a"]),
                mirror: Some(strings(&["m1"]))
            }
        );
        // Two occurrences (implicit array)
        assert_eq!(
            parse("repo = a\nrepo = b\nmirror = m1\nmirror = m2"),
            Repos {
                repo: strings(&["a", "b"]),
                mirror: Some(strings(&["m1", "m2"]))
            }
        );
        // An explicit array is unchanged
        assert_eq!(parse(r#"repo = ["a", "b"]"#).repo, strings(&["a", "b"]));
        // `null` into `Option<Vec<T>>` is `None`, not a one-element sequence
        assert_eq!(parse("mirror = null").mirror, None);
        // A tuple target takes a single value as a one-element tuple
        assert_eq!(
            <(u32,)>::deserialize(with_value(UclValue::Integer(5))).unwrap(),
            (5,)
        );
    }

    #[test]
    fn test_object_is_not_a_sequence() {
        // It used to read as the sequence of its values, silently dropping the keys.
        #[derive(Debug, Deserialize)]
        struct Sections {
            #[allow(
                dead_code,
                reason = "a deserialization target; the test checks the error"
            )]
            section: Vec<u32>,
        }
        let err = from_str::<Sections>("section { a = 1; b = 2 }").unwrap_err();
        assert!(err.to_string().contains("invalid type: map"), "{err}");

        // A map target keeps the keys; repeated sections still read as a sequence of objects.
        #[derive(Debug, Deserialize)]
        struct Named {
            section: HashMap<String, u32>,
        }
        let named: Named = from_str("section { a = 1; b = 2 }").unwrap();
        assert_eq!(named.section.len(), 2);
        let repeated: Vec<HashMap<String, u32>> =
            <Vec<HashMap<String, u32>>>::deserialize(with_value(UclValue::Array(vec![
                UclValue::Object([("a", UclValue::Integer(1))].into_iter().collect()),
                UclValue::Object([("b", UclValue::Integer(2))].into_iter().collect()),
            ])))
            .unwrap();
        assert_eq!(repeated.len(), 2);
    }

    #[test]
    fn test_type_errors_name_what_was_found() {
        let err = from_str::<UnsignedField>("v = \"30\"").unwrap_err();
        assert!(
            err.to_string().contains("invalid type: string \"30\""),
            "{err}"
        );
        let err = from_str::<SignedField>("v = 30s").map(|_| ()).err();
        assert!(err.is_none(), "an integral time converts");
        #[derive(Debug, Deserialize)]
        struct Flag {
            #[allow(
                dead_code,
                reason = "a deserialization target; the test checks the error"
            )]
            f: bool,
        }
        let err = from_str::<Flag>("f = 1").unwrap_err();
        assert!(
            err.to_string().contains("invalid type: integer `1`"),
            "{err}"
        );
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestStruct {
        name: String,
        age: u32,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct NestedStruct {
        user: TestStruct,
        active: bool,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct OptionalFields {
        required: String,
        #[serde(default)]
        optional: Option<String>,
        #[serde(default = "default_number")]
        number: i32,
    }

    fn default_number() -> i32 {
        42
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    struct RenamedFields {
        first_name: String,
        last_name: String,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    enum TestEnum {
        Unit,
        Newtype(String),
        Tuple(String, i32),
        Struct { field: String },
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct EnumField {
        e: TestEnum,
    }

    #[test]
    fn test_basic_struct_deserialization() {
        let parsed: TestStruct = from_str(r#"{ name = "Alice", age = 30 }"#).unwrap();
        assert_eq!(
            parsed,
            TestStruct {
                name: "Alice".into(),
                age: 30
            }
        );
    }

    #[test]
    fn test_nested_struct_deserialization() {
        let ucl = r#"{
            user = { name = "Bob", age = 25 },
            active = true
        }"#;
        let parsed: NestedStruct = from_str(ucl).unwrap();
        assert_eq!(parsed.user.name, "Bob");
        assert_eq!(parsed.user.age, 25);
        assert!(parsed.active);
    }

    #[test]
    fn test_array_deserialization() {
        let parsed: Vec<i32> = from_str("[1, 2, 3, 4, 5]").unwrap();
        assert_eq!(parsed, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_map_deserialization() {
        let parsed: HashMap<String, String> =
            from_str(r#"{ key1 = "value1", key2 = "value2" }"#).unwrap();
        assert_eq!(parsed.get("key1"), Some(&"value1".to_string()));
        assert_eq!(parsed.get("key2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_optional_fields() {
        let parsed: OptionalFields = from_str(r#"{ required = "test" }"#).unwrap();
        assert_eq!(parsed.required, "test");
        assert_eq!(parsed.optional, None);
        assert_eq!(parsed.number, 42);

        let parsed: OptionalFields = from_str(r#"{ required = "a", optional = "b" }"#).unwrap();
        assert_eq!(parsed.optional.as_deref(), Some("b"));
    }

    #[test]
    fn test_renamed_fields() {
        let parsed: RenamedFields =
            from_str(r#"{ first_name = "John", last_name = "Doe" }"#).unwrap();
        assert_eq!(parsed.first_name, "John");
        assert_eq!(parsed.last_name, "Doe");
    }

    #[test]
    fn test_enum_variants() {
        let parse = |input: &str| from_str::<EnumField>(input).map(|f| f.e);
        assert_eq!(parse(r#"e = "Unit""#).unwrap(), TestEnum::Unit);
        assert_eq!(
            parse(r#"e = { Newtype = "test_value" }"#).unwrap(),
            TestEnum::Newtype("test_value".into())
        );
        assert_eq!(
            parse(r#"e = { Tuple = ["a", 1] }"#).unwrap(),
            TestEnum::Tuple("a".into(), 1)
        );
        assert_eq!(
            parse(r#"e = { Struct = { field = "test" } }"#).unwrap(),
            TestEnum::Struct {
                field: "test".into()
            }
        );
        // The document itself can be a data variant
        assert_eq!(
            from_str::<TestEnum>(r#"{ Newtype = "x" }"#).unwrap(),
            TestEnum::Newtype("x".into())
        );
        assert!(parse(r#"e = { Unit = 1, Newtype = "x" }"#).is_err());
        assert!(parse("e = 1").is_err());
    }

    #[test]
    fn test_variable_expansion() {
        // Registered variables expand in braced and unbraced references (spec §7.3, §7.4).
        let mut variables = HashMap::new();
        variables.insert("name".to_string(), "World".to_string());
        let parsed: HashMap<String, String> =
            from_str_with_map(r#"{ greeting = "Hello ${name}! $name." }"#, variables).unwrap();
        assert_eq!(parsed["greeting"], "Hello World! World.");

        // Registration order decides between names that are prefixes of one another (§7.4).
        let parse = |variables: Vec<(&str, &str)>| {
            from_str_with_variables::<HashMap<String, String>, _, _, _>("v = \"$ABC\"", variables)
                .unwrap()
                .remove("v")
                .unwrap()
        };
        assert_eq!(parse(vec![("AB", "1"), ("ABC", "2")]), "1C");
        assert_eq!(parse(vec![("ABC", "2"), ("AB", "1")]), "2");
        // From a map, the longest matching name wins whatever the map's order.
        let map: HashMap<String, String> = [("AB", "1"), ("ABC", "2"), ("A", "0")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let parsed: HashMap<String, String> =
            from_str_with_map("v = \"$ABC $ABD $AX\"", map).unwrap();
        assert_eq!(parsed["v"], "2 1D 0X");
    }

    #[test]
    fn test_environment_variables() {
        // The handler reads the environment for braced references only (spec §7.7).
        unsafe {
            std::env::set_var("TEST_UCL_VAR", "test_value");
        }
        let result: Result<HashMap<String, String>, _> = from_str_with_env(
            r#"{ braced = "${TEST_UCL_VAR}", unbraced = "$TEST_UCL_VAR", unset = "${TEST_UCL_UNSET_VAR}", odd = "${} ${A=B}" }"#,
        );
        unsafe {
            std::env::remove_var("TEST_UCL_VAR");
        }
        let result = result.unwrap();
        assert_eq!(result["braced"], "test_value");
        assert_eq!(result["unbraced"], "$TEST_UCL_VAR");
        assert_eq!(result["unset"], "${TEST_UCL_UNSET_VAR}");
        assert_eq!(result["odd"], "${} ${A=B}");
        // The file variables are registered, so the environment cannot override them (§7.8).
        let result: HashMap<String, String> = from_str_with_env("f = \"${FILENAME}\"").unwrap();
        assert_eq!(result["f"], "undef");
    }

    #[test]
    fn test_type_coercion() {
        #[derive(Debug, Deserialize)]
        struct TypeCoercion {
            as_i32: i32,
            as_f64: f64,
            as_string: String,
            as_bool: bool,
        }
        let ucl = r#"{
            as_i32 = 42,
            as_f64 = 42,
            as_string = "42",
            as_bool = true
        }"#;
        let parsed: TypeCoercion = from_str(ucl).unwrap();
        assert_eq!(parsed.as_i32, 42);
        assert_eq!(parsed.as_f64, 42.0);
        assert_eq!(parsed.as_string, "42");
        assert!(parsed.as_bool);
    }

    #[test]
    fn test_error_propagation() {
        let result: Result<TestStruct, _> = from_str(r#"{ invalid syntax }"#);
        let error = result.unwrap_err();
        assert!(!error.to_string().is_empty());
    }

    #[test]
    fn test_convenience_functions() {
        let ucl = r#"{ name = "test", age = 25 }"#;
        let expected = TestStruct {
            name: "test".into(),
            age: 25,
        };
        assert_eq!(from_str::<TestStruct>(ucl).unwrap(), expected);
        assert_eq!(from_slice::<TestStruct>(ucl.as_bytes()).unwrap(), expected);
        assert_eq!(from_reader::<TestStruct>(ucl.as_bytes()).unwrap(), expected);
        assert_eq!(
            from_str_with_variables::<TestStruct, _, &str, &str>(ucl, []).unwrap(),
            expected
        );
        assert_eq!(
            from_str_with_map::<TestStruct>(ucl, HashMap::new()).unwrap(),
            expected
        );
        assert_eq!(from_str_with_env::<TestStruct>(ucl).unwrap(), expected);
    }

    #[test]
    fn test_parser_settings_are_honoured() {
        // A parser's settings apply through the document-level deserializer: with `no-time`
        // a time suffix leaves a string (spec §12.3).
        let parse = |parser: Parser| {
            serde_json::Value::deserialize(UclDeserializer::from_parser(
                parser,
                b"t = 10s\nv = \"${N}\"",
            ))
            .unwrap()
        };
        let with_default = parse(ParserBuilder::new().with_variable("N", "x").build());
        assert_eq!(with_default["t"], 10.0);
        assert_eq!(with_default["v"], "x");
        let with_flags = parse(
            ParserBuilder::new()
                .with_flags(crate::ParserFlags::NO_TIME)
                .with_variable("N", "x")
                .build(),
        );
        assert_eq!(with_flags["t"], "10s");
        assert_eq!(with_flags["v"], "x");
    }

    #[test]
    fn test_deserializer_methods() {
        let mut deserializer = UclDeserializer::new("test");
        assert!(deserializer.parser().flags().is_empty());
        deserializer
            .parser_mut()
            .set_flags(crate::ParserFlags::KEY_LOWERCASE);
        assert_eq!(
            deserializer.parser().flags(),
            crate::ParserFlags::KEY_LOWERCASE
        );
    }
}

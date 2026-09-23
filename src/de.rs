//! Serde deserializer.
//!
//! UCL text is parsed into a [`UclValue`] tree, and one value deserializer maps that tree onto
//! serde. [`from_value`] deserializes a tree, such as one from the new parser core
//! ([`crate::parse`]); the document-level [`UclDeserializer`] and [`from_str`] parse with the
//! existing parser and hand the result to the same value deserializer, so the document and
//! every nested value follow the same rules:
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
//! `Deserialize for UclValue` keeps what the table flattens: from this crate's deserializer, a
//! time stays a time and an entry with several values keeps them all, so
//! `from_value::<UclValue>(v)` gives `v` back, apart from priorities and the marks of values
//! that `.inherit` copied or `no-implicit-arrays` collected. From any other deserializer it
//! takes what that format offers; integers above `i64::MAX` are an error.

mod value;

use value::ValueDeserializer;
pub use value::from_value;

use crate::error::UclError;
use crate::lexer::LexerConfig;
use crate::parser::{
    EnvironmentVariableHandler, MapVariableHandler, UclParser, UclParserBuilder, VariableHandler,
};
use crate::value::UclValue;
use serde::de::{self, Deserialize, Visitor};

/// Deserializes UCL text: parses the document, then deserializes the resulting value.
pub struct UclDeserializer<'a> {
    parser: UclParser<'a>,
    value: Option<UclValue>,
}

impl<'a> UclDeserializer<'a> {
    /// Creates a new deserializer from UCL text
    pub fn new(input: &'a str) -> Self {
        Self::from_parser(UclParser::new(input))
    }

    /// Creates a deserializer with custom lexer configuration
    pub fn with_lexer_config(input: &'a str, config: LexerConfig) -> Self {
        Self::from_parser(UclParser::with_lexer_config(input, config))
    }

    /// Creates a deserializer with a variable handler
    pub fn with_variable_handler(input: &'a str, handler: Box<dyn VariableHandler>) -> Self {
        Self::from_parser(UclParser::with_variable_handler(input, handler))
    }

    /// Creates a deserializer from an existing parser
    pub fn from_parser(parser: UclParser<'a>) -> Self {
        Self {
            parser,
            value: None,
        }
    }

    /// Returns a reference to the underlying parser
    pub fn parser(&self) -> &UclParser<'a> {
        &self.parser
    }

    /// Returns a mutable reference to the underlying parser
    pub fn parser_mut(&mut self) -> &mut UclParser<'a> {
        &mut self.parser
    }

    /// Parses the document unless a value is already present.
    fn into_value(mut self) -> Result<ValueDeserializer, UclError> {
        let value = match self.value.take() {
            Some(value) => value,
            None => self.parser.parse_document()?,
        };
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

/// Convenience function to deserialize UCL text into a Rust type
pub fn from_str<'a, T>(s: &'a str) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    T::deserialize(UclDeserializer::new(s))
}

/// Convenience function to deserialize UCL text with variable expansion
pub fn from_str_with_variables<'a, T>(
    s: &'a str,
    handler: Box<dyn VariableHandler>,
) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    T::deserialize(UclDeserializer::with_variable_handler(s, handler))
}

/// Convenience function to deserialize UCL text with custom lexer configuration
pub fn from_str_with_config<'a, T>(s: &'a str, config: LexerConfig) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    T::deserialize(UclDeserializer::with_lexer_config(s, config))
}

/// Convenience function to deserialize UCL text with both custom config and variables
pub fn from_str_with_config_and_variables<'a, T>(
    s: &'a str,
    config: LexerConfig,
    handler: Box<dyn VariableHandler>,
) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    let parser = UclParserBuilder::new(s)
        .with_lexer_config(config)
        .with_variable_handler(handler)
        .build()?;
    T::deserialize(UclDeserializer::from_parser(parser))
}

/// Convenience function to deserialize UCL text using environment variables
pub fn from_str_with_env<'a, T>(s: &'a str) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    from_str_with_variables(s, Box::new(EnvironmentVariableHandler))
}

/// Convenience function to deserialize UCL text using a map of variables
pub fn from_str_with_map<'a, T>(
    s: &'a str,
    variables: std::collections::HashMap<String, String>,
) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    from_str_with_variables(s, Box::new(MapVariableHandler::from_map(variables)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ParseError;
    use serde::Deserialize;
    use std::collections::HashMap;

    /// A document-level deserializer over a prepared value (no parsing).
    fn with_value(value: UclValue) -> UclDeserializer<'static> {
        UclDeserializer {
            parser: UclParser::new(""),
            value: Some(value),
        }
    }

    #[test]
    fn test_lex_errors_surface_as_lex_with_their_position() {
        // Lex errors used to reach callers as `SerdeError::Custom` strings, or as
        // `ParseError::InvalidObject` with the position reset to 1:1 (REVIEW.md §5.4).
        let err = from_str::<serde_json::Value>("\nkey = \"unterminated").unwrap_err();
        match err {
            UclError::Lex(crate::error::LexError::UnterminatedString { position }) => {
                // The opening quote
                assert_eq!((position.line, position.column), (2, 7));
            }
            other => panic!("expected UnterminatedString, got {other:?}"),
        }

        let err = from_str::<serde_json::Value>("a = 1\nb = 0xZZ").unwrap_err();
        match err {
            UclError::Lex(crate::error::LexError::InvalidNumber { position, .. }) => {
                assert_eq!(position.line, 2);
            }
            other => panic!("expected InvalidNumber, got {other:?}"),
        }

        let err = from_str::<serde_json::Value>("key = @@@").unwrap_err();
        match err {
            UclError::Lex(crate::error::LexError::UnexpectedCharacter {
                character,
                position,
            }) => {
                assert_eq!(character, '@');
                assert_eq!((position.line, position.column), (1, 7));
            }
            other => panic!("expected UnexpectedCharacter, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_errors_stay_typed() {
        let err = from_str::<serde_json::Value>("a = [1, 2").unwrap_err();
        match err {
            UclError::Parse(ParseError::UnexpectedToken { position, .. }) => {
                assert_eq!((position.line, position.column), (1, 10));
            }
            other => panic!("expected ParseError::UnexpectedToken, got {other:?}"),
        }
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

        // Anything else is an error instead of a truncated or saturated value (REVIEW.md §5.5).
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
        // Each of these failed for struct fields before P2.2: the value deserializer sent
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
            #[allow(dead_code)]
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
            #[allow(dead_code)]
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
        let mut variables = HashMap::new();
        variables.insert("name".to_string(), "World".to_string());

        let parsed: HashMap<String, String> =
            from_str_with_map(r#"{ greeting = "Hello ${name}!" }"#, variables).unwrap();
        assert_eq!(parsed.get("greeting"), Some(&"Hello World!".to_string()));
    }

    #[test]
    fn test_environment_variables() {
        unsafe {
            std::env::set_var("TEST_UCL_VAR", "test_value");
        }
        let result: Result<HashMap<String, String>, _> =
            from_str_with_env(r#"{ env_value = "${TEST_UCL_VAR}" }"#);
        unsafe {
            std::env::remove_var("TEST_UCL_VAR");
        }
        assert_eq!(
            result.unwrap().get("env_value"),
            Some(&"test_value".to_string())
        );
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
        assert_eq!(
            from_str_with_config::<TestStruct>(ucl, LexerConfig::default()).unwrap(),
            expected
        );
        assert_eq!(
            from_str_with_map::<TestStruct>(ucl, HashMap::new()).unwrap(),
            expected
        );
        assert_eq!(from_str_with_env::<TestStruct>(ucl).unwrap(), expected);
    }

    #[test]
    fn test_config_and_variables_honours_the_lexer_config() {
        // The `LexerConfig` argument used to be ignored.
        let vars = || {
            let mut vars = MapVariableHandler::new();
            vars.insert("N".into(), "x".into());
            Box::new(vars)
        };
        let config = LexerConfig {
            allow_time_suffixes: false,
            ..LexerConfig::default()
        };
        let with_default: Result<serde_json::Value, _> = from_str_with_config_and_variables(
            "t = 10s\nv = \"${N}\"",
            LexerConfig::default(),
            vars(),
        );
        let with_config: Result<serde_json::Value, _> =
            from_str_with_config_and_variables("t = 10s\nv = \"${N}\"", config, vars());
        let with_default = with_default.unwrap();
        assert_eq!(with_default["t"], 10.0);
        assert_eq!(with_default["v"], "x");
        assert_ne!(
            with_config.ok().map(|v| v["t"].clone()),
            Some(serde_json::json!(10.0))
        );
    }

    #[test]
    fn test_deserializer_methods() {
        let mut deserializer = UclDeserializer::new("test");
        let _parser_ref = deserializer.parser();
        let _parser_mut = deserializer.parser_mut();
    }
}

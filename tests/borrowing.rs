//! Serde targets that borrow from the input (clean-room work item C13, decision 2): `&str`
//! fields, `Cow` fields with `#[serde(borrow)]` and borrowed map keys take the keys and strings
//! that appear in the text as they are without a copy; a string that does not appear as it is
//! (escaped, expanded, lowercased, or from an included file) becomes an owned `Cow`, and fails
//! for a `&str`, as it did before borrowing. Owned targets are unchanged.

use serde::Deserialize;
use serde_ucl::parse::{MemoryLoader, ParserBuilder};
use serde_ucl::{ParserFlags, UclDeserializer, UclError, UclValue, from_slice, from_str};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};

/// Whether `s` lies inside `text`: it was borrowed from it.
fn inside(s: &str, text: &str) -> bool {
    let range = text.as_bytes().as_ptr_range();
    let bytes = s.as_bytes().as_ptr_range();
    range.start <= bytes.start && bytes.end <= range.end
}

/// Whether `cow` holds text borrowed from `text`.
#[allow(
    clippy::ptr_arg,
    reason = "the test is about which variant the `Cow` is"
)]
fn borrowed(cow: &Cow<'_, str>, text: &str) -> bool {
    matches!(cow, Cow::Borrowed(s) if inside(s, text))
}

#[test]
fn str_fields_borrow_from_the_text() {
    #[derive(Debug, Deserialize)]
    struct Server<'a> {
        name: &'a str,
        host: &'a str,
        path: &'a str,
        motd: &'a str,
        tags: Vec<&'a str>,
        nested: Nested<'a>,
    }
    #[derive(Debug, Deserialize)]
    struct Nested<'a> {
        user: &'a str,
    }
    // Unquoted, double-quoted and single-quoted strings, a heredoc, array elements, a nested
    // object and a string with a `$` that names no variable, all as written.
    let text = "name = web\nhost = \"example.org\"\npath = '/srv/$app'\n\
                motd = <<EOD\nhello\nEOD\ntags [a, \"b\", 'c']\nnested { user = deploy }\n";
    let server: Server = from_str(text).unwrap();
    assert_eq!(
        (server.name, server.host, server.path, server.motd),
        ("web", "example.org", "/srv/$app", "hello")
    );
    assert_eq!(server.tags, ["a", "b", "c"]);
    assert_eq!(server.nested.user, "deploy");
    for s in [
        server.name,
        server.host,
        server.path,
        server.motd,
        server.nested.user,
    ] {
        assert!(inside(s, text), "{s:?} is a copy");
    }
    assert!(server.tags.iter().all(|s| inside(s, text)));

    // from_slice and the document-level deserializer borrow too.
    let bytes = text.as_bytes();
    let server: Server = from_slice(bytes).unwrap();
    assert!(inside(server.name, text));
    let server = Server::deserialize(UclDeserializer::new(text)).unwrap();
    assert!(inside(server.host, text) && inside(server.nested.user, text));
}

#[test]
fn cow_fields_borrow_what_appears_as_it_is_and_own_the_rest() {
    #[derive(Debug, Deserialize)]
    struct Fields<'a> {
        #[serde(borrow)]
        plain: Cow<'a, str>,
        #[serde(borrow)]
        escaped: Cow<'a, str>,
        #[serde(borrow)]
        single_escaped: Cow<'a, str>,
        #[serde(borrow)]
        expanded: Cow<'a, str>,
        #[serde(borrow)]
        not_a_variable: Cow<'a, str>,
        // Owned targets are filled as before.
        list: Vec<Cow<'a, str>>,
        owned: String,
        also_owned: Cow<'a, str>,
    }
    let text = "plain = \"as it is\"\nescaped = \"a\\tb\"\nsingle_escaped = 'it\\'s'\n\
                expanded = \"${HOST}/x\"\nnot_a_variable = \"$NOPE\"\n\
                list = [x, \"y\\u0041\"]\nowned = z\nalso_owned = w\n";
    let fields: Fields =
        serde_ucl::from_str_with_variables(text, [("HOST", "example.org")]).unwrap();
    assert!(borrowed(&fields.plain, text));
    assert_eq!(fields.plain, "as it is");
    assert!(matches!(&fields.escaped, Cow::Owned(s) if s == "a\tb"));
    assert!(matches!(&fields.single_escaped, Cow::Owned(s) if s == "it's"));
    assert!(matches!(&fields.expanded, Cow::Owned(s) if s == "example.org/x"));
    // A `$` that names no variable leaves the string as written.
    assert!(borrowed(&fields.not_a_variable, text));
    assert_eq!(fields.not_a_variable, "$NOPE");
    assert_eq!(fields.owned, "z");
    // A `Cow` without `#[serde(borrow)]`, also inside a vector, deserializes as a `String`
    // (serde's rule).
    assert!(matches!(&fields.also_owned, Cow::Owned(s) if s == "w"));
    assert!(matches!(&fields.list[..], [Cow::Owned(x), Cow::Owned(y)] if x == "x" && y == "yA"));
}

#[test]
fn map_keys_borrow_from_the_text() {
    let text = "alpha = 1\n\"beta\" = 2\ngamma { delta = 3 }\n\"e\\u0041\" = 4\n";
    #[derive(Debug, Deserialize)]
    struct Doc<'a> {
        #[serde(borrow)]
        gamma: HashMap<&'a str, i64>,
    }
    let doc: Doc = from_str(text).unwrap();
    let (key, value) = doc.gamma.iter().next().unwrap();
    assert_eq!((*key, *value), ("delta", 3));
    assert!(inside(key, text));

    // Keys that appear as they are borrow; an escaped key has no `&str` form.
    let plain = "alpha = 1\n\"beta\" = 2\n";
    let keys: BTreeMap<&str, UclValue> = from_str(plain).unwrap();
    assert_eq!(keys.keys().copied().collect::<Vec<_>>(), ["alpha", "beta"]);
    assert!(keys.keys().all(|k| inside(k, plain)));
    let err = from_str::<HashMap<&str, UclValue>>(text).unwrap_err();
    assert!(err.to_string().contains("borrowed string"), "{err}");
    // Keys read as owned strings, and `Cow` keys, which serde reads as owned, are unchanged.
    let owned: BTreeMap<Cow<str>, UclValue> = from_str(text).unwrap();
    assert_eq!(
        owned.keys().map(|k| k.as_ref()).collect::<Vec<_>>(),
        ["alpha", "beta", "eA", "gamma"]
    );

    // With the paths of the document-level deserializer, too.
    let small = "a = 1\nb = 2";
    let map = HashMap::<&str, UclValue>::deserialize(UclDeserializer::new(small)).unwrap();
    assert_eq!(map.len(), 2);
    assert!(map.keys().all(|k| inside(k, small)));
}

#[test]
fn lowercased_and_included_strings_are_owned() {
    #[derive(Debug, Deserialize)]
    struct Doc<'a> {
        keys: Vec<&'a str>,
        #[serde(borrow)]
        from_file: Cow<'a, str>,
        #[serde(borrow)]
        here: Cow<'a, str>,
    }
    let mut files = MemoryLoader::new();
    files.add_file("/inc.conf", "from_file = included\n");
    let parser = ParserBuilder::new()
        .with_loader(files)
        .with_flags(ParserFlags::KEY_LOWERCASE)
        .build();
    let text = "KEYS = one\nkeys = two\n.include \"/inc.conf\"\nhere = kept\n";
    let doc = Doc::deserialize(UclDeserializer::from_parser(parser, text.as_bytes())).unwrap();
    // The lowercased key `KEYS` is the entry's key; the values appear as they are.
    assert_eq!(doc.keys, ["one", "two"]);
    assert!(doc.keys.iter().all(|k| inside(k, text)));
    assert!(matches!(&doc.from_file, Cow::Owned(s) if s == "included"));
    assert!(borrowed(&doc.here, text));

    // A key that lowercasing changes is owned, so it has no `&str` form; one that lowercasing
    // leaves as it is borrows.
    let lowercase = || {
        ParserBuilder::new()
            .with_flags(ParserFlags::KEY_LOWERCASE)
            .build()
    };
    let text = "lower = 1\nalso_lower = 2\n";
    let keys = BTreeMap::<&str, i64>::deserialize(UclDeserializer::from_parser(
        lowercase(),
        text.as_bytes(),
    ))
    .unwrap();
    assert!(keys.keys().all(|k| inside(k, text)));
    let text = "lower = 1\nUpper = 2\n";
    let err = BTreeMap::<&str, i64>::deserialize(UclDeserializer::from_parser(
        lowercase(),
        text.as_bytes(),
    ))
    .unwrap_err();
    assert!(err.to_string().contains("borrowed string"), "{err}");
}

#[test]
fn str_fails_where_the_text_cannot_be_borrowed() {
    #[derive(Debug, Deserialize)]
    #[allow(
        dead_code,
        reason = "a deserialization target; the test checks the error"
    )]
    struct Borrowed<'a> {
        ok: &'a str,
        name: &'a str,
    }
    // An escaped string has no borrowed form; the error has its path and position.
    let err = from_str::<Borrowed>("ok = fine\nname = \"a\\nb\"").unwrap_err();
    let UclError::Deserialize(e) = &err else {
        panic!("{err:?}")
    };
    assert!(
        e.error().to_string().contains("expected a borrowed string"),
        "{err}"
    );
    assert!(err.to_string().contains("at name"), "{err}");
    let position = err.position().unwrap();
    assert_eq!((position.line, position.column), (2, 8));
    // So does an expanded one.
    let err = serde_ucl::from_str_with_variables::<Borrowed, _, _, _>(
        "ok = fine\nname = $USER",
        [("USER", "deploy")],
    )
    .unwrap_err();
    assert!(err.to_string().contains("borrowed string"), "{err}");
    // As a string from an included file.
    let mut files = MemoryLoader::new();
    files.add_file("/inc.conf", "name = x\n");
    let parser = ParserBuilder::new().with_loader(files).build();
    let err = Borrowed::deserialize(UclDeserializer::from_parser(
        parser,
        b"ok = y\n.include \"/inc.conf\"",
    ))
    .unwrap_err();
    assert!(err.to_string().contains("borrowed string"), "{err}");
    assert_eq!(
        err.file().map(|f| f.to_string_lossy().into_owned()),
        Some("/inc.conf".into())
    );
}

#[test]
fn owned_targets_are_unchanged() {
    // The same text read into owned and borrowing targets gives the same values, and a
    // `UclValue` target gets an owned tree, whatever the target borrows elsewhere.
    #[derive(Debug, Deserialize, PartialEq)]
    struct Owned {
        name: String,
        list: Vec<String>,
        extra: UclValue,
    }
    #[derive(Debug, Deserialize)]
    struct Borrowing<'a> {
        name: &'a str,
        list: Vec<&'a str>,
        extra: UclValue,
    }
    let text = String::from("name = web\nlist = [a, b]\nextra { k = v }\n");
    let owned: Owned = from_str(&text).unwrap();
    let borrowing: Borrowing = from_str(&text).unwrap();
    assert_eq!(owned.name, borrowing.name);
    assert_eq!(owned.list, borrowing.list);
    assert_eq!(owned.extra, borrowing.extra);
    let extra = borrowing.extra;
    drop(text);
    // The `UclValue` does not borrow from the text, which is gone.
    assert_eq!(extra.as_object().unwrap()["k"].as_str(), Some("v"));
    let whole: UclValue = from_str("a = 'x'\nb = [1, \"y\"]").unwrap();
    assert_eq!(
        whole,
        serde_ucl::parse::parse(b"a = 'x'\nb = [1, \"y\"]").unwrap()
    );
}

/// Decision 5: where a `&str` is asked for, a string that borrows from the input reaches
/// `visit_borrowed_str`, so a visitor that implements only `visit_string` no longer receives it.
/// `deserialize_string`, a string that does not borrow, and `from_value` of a parsed value offer
/// `visit_string` as before.
#[test]
fn visitors_with_only_visit_string_miss_borrowed_strings() {
    use serde::de::{self, Deserializer, Visitor};

    /// Implements `visit_string` only: what clippy's `serde_api_misuse` warns of, and the case
    /// decision 5 accepts breaking.
    struct OnlyString;
    #[allow(clippy::serde_api_misuse)]
    impl<'de> Visitor<'de> for OnlyString {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a string")
        }
        fn visit_string<E: de::Error>(self, v: String) -> Result<String, E> {
            Ok(v)
        }
    }

    /// Asks with `deserialize_str`, or with `deserialize_string` when `STRING`.
    #[derive(Debug)]
    struct Text<const STRING: bool>(String);
    impl<'de, const STRING: bool> Deserialize<'de> for Text<STRING> {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            if STRING {
                deserializer.deserialize_string(OnlyString).map(Text)
            } else {
                deserializer.deserialize_str(OnlyString).map(Text)
            }
        }
    }

    /// A map key that asks with `deserialize_identifier`.
    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct Key(String);
    impl<'de> Deserialize<'de> for Key {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer.deserialize_identifier(OnlyString).map(Key)
        }
    }

    type Values<const STRING: bool> = BTreeMap<String, Text<STRING>>;
    let plain = "v = plain";
    let err = from_str::<Values<false>>(plain).unwrap_err().to_string();
    assert!(err.contains("invalid type: string \"plain\""), "{err}");
    let err = Values::<false>::deserialize(UclDeserializer::new(plain)).unwrap_err();
    assert!(err.to_string().contains("invalid type: string"), "{err}");
    assert_eq!(from_str::<Values<true>>(plain).unwrap()["v"].0, "plain");
    assert_eq!(
        from_str::<Values<false>>("v = \"pl\\u0061in\"").unwrap()["v"].0,
        "plain"
    );
    let parsed = serde_ucl::parse::parse(plain.as_bytes()).unwrap();
    assert_eq!(
        serde_ucl::from_value::<Values<false>>(parsed).unwrap()["v"].0,
        "plain"
    );

    assert!(from_str::<BTreeMap<Key, i64>>("k = 1").is_err());
    assert!(BTreeMap::<Key, i64>::deserialize(UclDeserializer::new("k = 1")).is_err());
    let escaped = from_str::<BTreeMap<Key, i64>>("\"k\\u0031\" = 1").unwrap();
    assert_eq!(escaped.keys().next().map(|k| k.0.as_str()), Some("k1"));
    let parsed = serde_ucl::parse::parse(b"k = 1").unwrap();
    assert_eq!(
        serde_ucl::from_value::<BTreeMap<Key, i64>>(parsed)
            .unwrap()
            .len(),
        1
    );
}

//! [`Str`], the string of a [`super::Value`]: a key or a string value, borrowed or owned.

use std::borrow::{Borrow, Cow};
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;

/// A key or a string value of a [`Value<'a>`](super::Value): text borrowed for `'a`, or owned.
///
/// The parser gives owned strings ([`super::UclValue`]); the serde entry points parse into
/// strings that borrow from the caller's text where a key or string appears in it as it is
/// (clean-room work item C13), so that serde targets that borrow can take them without a copy.
/// A `Str` derefs to [`str`], and compares, orders and hashes as the `str` it holds, borrowed or
/// not, so an object is looked up by `&str`. `Debug` and `Display` are those of the `str`.
///
/// ```
/// use serde_ucl::value::Str;
///
/// let owned = Str::from("port");
/// let text = String::from("port");
/// let borrowed = Str::borrowed(&text);
/// assert_eq!(owned, borrowed);
/// assert!(borrowed.is_borrowed() && !owned.is_borrowed());
/// assert_eq!(owned.len(), 4);
/// assert_eq!(String::from(owned), "port");
/// ```
#[derive(Clone)]
pub struct Str<'a>(Repr<'a>);

#[derive(Clone)]
enum Repr<'a> {
    Borrowed(&'a str),
    Owned(String),
}

const _: () = assert!(std::mem::size_of::<Str<'_>>() == std::mem::size_of::<String>());

impl<'a> Str<'a> {
    /// The empty string.
    pub const fn new() -> Self {
        Str(Repr::Borrowed(""))
    }

    /// A string that borrows `text`.
    pub const fn borrowed(text: &'a str) -> Self {
        Str(Repr::Borrowed(text))
    }

    /// The string as a `&str`.
    pub fn as_str(&self) -> &str {
        match &self.0 {
            Repr::Borrowed(s) => s,
            Repr::Owned(s) => s,
        }
    }

    /// The text, when it is borrowed: for as long as `'a`, beyond the life of the `Str`.
    pub fn as_borrowed(&self) -> Option<&'a str> {
        match self.0 {
            Repr::Borrowed(s) => Some(s),
            Repr::Owned(_) => None,
        }
    }

    /// Whether the string borrows its text.
    pub fn is_borrowed(&self) -> bool {
        matches!(self.0, Repr::Borrowed(_))
    }

    /// The string as a `String`: its own, or a copy of the text it borrows.
    pub fn into_string(self) -> String {
        match self.0 {
            Repr::Borrowed(s) => s.to_owned(),
            Repr::Owned(s) => s,
        }
    }

    /// The string as a `Cow`, borrowed or owned as it is.
    pub fn into_cow(self) -> Cow<'a, str> {
        match self.0 {
            Repr::Borrowed(s) => Cow::Borrowed(s),
            Repr::Owned(s) => Cow::Owned(s),
        }
    }

    /// The string with the text it borrows copied.
    pub fn into_owned(self) -> Str<'static> {
        match self.0 {
            Repr::Borrowed(s) => Str(Repr::Owned(s.to_owned())),
            Repr::Owned(s) => Str(Repr::Owned(s)),
        }
    }
}

impl Default for Str<'_> {
    fn default() -> Self {
        Str::new()
    }
}

impl From<&str> for Str<'_> {
    /// An owned copy of `s`; [`Str::borrowed`] borrows it instead.
    fn from(s: &str) -> Self {
        Str(Repr::Owned(s.to_owned()))
    }
}

impl From<&String> for Str<'_> {
    fn from(s: &String) -> Self {
        Str(Repr::Owned(s.clone()))
    }
}

impl From<String> for Str<'_> {
    fn from(s: String) -> Self {
        Str(Repr::Owned(s))
    }
}

impl From<Box<str>> for Str<'_> {
    fn from(s: Box<str>) -> Self {
        Str(Repr::Owned(s.into_string()))
    }
}

impl From<char> for Str<'_> {
    fn from(c: char) -> Self {
        Str(Repr::Owned(c.to_string()))
    }
}

impl<'a> From<Cow<'a, str>> for Str<'a> {
    fn from(s: Cow<'a, str>) -> Self {
        match s {
            Cow::Borrowed(s) => Str(Repr::Borrowed(s)),
            Cow::Owned(s) => Str(Repr::Owned(s)),
        }
    }
}

impl<'a> From<&Str<'a>> for Str<'a> {
    fn from(s: &Str<'a>) -> Self {
        s.clone()
    }
}

impl From<Str<'_>> for String {
    fn from(s: Str<'_>) -> Self {
        s.into_string()
    }
}

impl<'a> From<Str<'a>> for Cow<'a, str> {
    fn from(s: Str<'a>) -> Self {
        s.into_cow()
    }
}

impl Deref for Str<'_> {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for Str<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<[u8]> for Str<'_> {
    fn as_ref(&self) -> &[u8] {
        self.as_str().as_bytes()
    }
}

impl Borrow<str> for Str<'_> {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl<'b> PartialEq<Str<'b>> for Str<'_> {
    fn eq(&self, other: &Str<'b>) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for Str<'_> {}

impl PartialEq<str> for Str<'_> {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Str<'_> {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<String> for Str<'_> {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<Str<'_>> for str {
    fn eq(&self, other: &Str<'_>) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<Str<'_>> for &str {
    fn eq(&self, other: &Str<'_>) -> bool {
        *self == other.as_str()
    }
}

impl PartialEq<Str<'_>> for String {
    fn eq(&self, other: &Str<'_>) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialOrd for Str<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Str<'_> {
    /// As `str` orders.
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Hash for Str<'_> {
    /// As the `str` it holds hashes, so that a `&str` finds it in a map.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl fmt::Debug for Str<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for Str<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

impl serde::Serialize for Str<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::BuildHasher;

    #[test]
    fn borrowed_and_owned_strings_are_their_text() {
        let text = String::from("a key");
        let hasher = std::collections::hash_map::RandomState::new();
        for s in [Str::borrowed(&text), Str::from(text.as_str())] {
            assert_eq!(s, "a key");
            assert_eq!(s, Str::borrowed("a key"));
            assert_eq!(hasher.hash_one(&s), hasher.hash_one("a key"));
            assert_eq!(format!("{s:?}"), format!("{:?}", "a key".to_string()));
            assert_eq!(s.to_string(), "a key");
            assert_eq!(s.clone().into_string(), "a key");
            assert_eq!(s.clone().into_owned(), "a key");
            assert!(!s.clone().into_owned().is_borrowed());
        }
        assert_eq!(Str::borrowed(&text).as_borrowed(), Some("a key"));
        assert_eq!(Str::from(text.as_str()).as_borrowed(), None);
        assert!(matches!(
            Str::borrowed(&text).into_cow(),
            Cow::Borrowed("a key")
        ));
        let (a, b) = (Str::from("a"), Str::borrowed("b"));
        assert!(a < b);
    }
}

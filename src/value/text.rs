//! [`Str`], the string type of the parser's copies of keys.

use std::fmt;

/// The longest string kept inline, without a heap allocation: 22 bytes, which with the length
/// and the variant's tag make a [`Str`] as large as a `String`, 24 bytes.
const INLINE: usize = 22;

/// A copy of a key that the parser keeps besides the object's own: the steps to the open
/// containers and the keys of the path trees (output facts, saved comments). It is kept inline
/// when it is at most 22 bytes long and on the heap otherwise, so that such copies need no
/// allocation each (clean-room work item C13; the C11 research notes on small-string keys,
/// after compact_str and kstring).
///
/// The copies are compared as bytes, so an inline copy's UTF-8 is checked only when its text is
/// asked for ([`Str::as_str`]), which the hot paths do not do. `Str` does not implement `Hash`: a
/// map that holds copies hashes their bytes on both sides, and a lookup among an object's `String`
/// keys goes through [`Str::as_str`].
#[derive(Clone)]
pub(crate) struct Str(Repr);

#[derive(Clone)]
enum Repr {
    /// The first `len` bytes of `bytes`, valid UTF-8.
    Inline {
        len: u8,
        bytes: [u8; INLINE],
    },
    Heap(Box<str>),
}

const _: () = assert!(std::mem::size_of::<Str>() == std::mem::size_of::<String>());

impl Str {
    /// The string as a `&str`.
    pub(crate) fn as_str(&self) -> &str {
        match &self.0 {
            Repr::Inline { len, bytes } => std::str::from_utf8(&bytes[..usize::from(*len)])
                .expect("an inline string is built from a str"),
            Repr::Heap(s) => s,
        }
    }

    /// The string's bytes. Unlike [`Str::as_str`], this does not check that the inline bytes are
    /// UTF-8, which they always are.
    pub(crate) fn as_bytes(&self) -> &[u8] {
        match &self.0 {
            Repr::Inline { len, bytes } => &bytes[..usize::from(*len)],
            Repr::Heap(s) => s.as_bytes(),
        }
    }

    /// Whether the string is kept inline, without a heap allocation.
    #[cfg(test)]
    fn is_inline(&self) -> bool {
        matches!(self.0, Repr::Inline { .. })
    }
}

impl From<&str> for Str {
    fn from(s: &str) -> Self {
        if s.len() <= INLINE {
            let mut bytes = [0; INLINE];
            bytes[..s.len()].copy_from_slice(s.as_bytes());
            Str(Repr::Inline {
                len: s.len() as u8,
                bytes,
            })
        } else {
            Str(Repr::Heap(s.into()))
        }
    }
}

impl From<&String> for Str {
    fn from(s: &String) -> Self {
        Str::from(s.as_str())
    }
}

impl From<String> for Str {
    /// A short string is copied inline and `s` dropped; a long one keeps `s`'s allocation, which
    /// is shrunk to its length.
    fn from(s: String) -> Self {
        if s.len() <= INLINE {
            Str::from(s.as_str())
        } else {
            Str(Repr::Heap(s.into_boxed_str()))
        }
    }
}

impl PartialEq for Str {
    fn eq(&self, other: &Str) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl Eq for Str {}

impl fmt::Debug for Str {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for Str {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_up_to_22_bytes() {
        for len in [0, 1, 21, 22, 23, 64] {
            let s = "é".repeat(len / 2) + &"a".repeat(len % 2);
            let from_str = Str::from(s.as_str());
            let from_string = Str::from(s.clone());
            for key in [&from_str, &from_string] {
                assert_eq!(key.as_str(), s);
                assert_eq!(key.as_bytes(), s.as_bytes());
                assert_eq!(key.is_inline(), s.len() <= INLINE, "{len}");
            }
            assert!(from_str == from_string);
        }
    }

    #[test]
    fn formats_as_str() {
        for s in [
            "",
            "k",
            "a key of exactly 22 by",
            "a key longer than twenty-two bytes",
        ] {
            let key = Str::from(s);
            assert_eq!(format!("{key:?}"), format!("{s:?}"));
            assert_eq!(key.to_string(), s);
        }
    }
}

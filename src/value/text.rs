//! [`KeyCopy`], the parser's copies of keys.

use std::fmt;

/// The longest string kept inline, without a heap allocation: 22 bytes, which with the length
/// and the variant's tag make a [`KeyCopy`] as large as a `String`, 24 bytes.
const INLINE: usize = 22;

/// A copy of a key that the parser keeps besides the object's own: the steps to the open
/// containers and the keys of the path trees (output facts, saved comments). It is kept inline
/// when it is at most 22 bytes long and on the heap otherwise, so that such copies need no
/// allocation each (clean-room work item C13; the C11 research notes on small-string keys,
/// after compact_str and kstring).
///
/// The copies are compared as bytes, so an inline copy's UTF-8 is checked only when its text is
/// asked for ([`KeyCopy::as_str`]), which the hot paths do not do. `KeyCopy` does not implement
/// `Hash`: a map that holds copies hashes their bytes on both sides, and a lookup among an
/// object's keys goes through [`KeyCopy::as_str`].
#[derive(Clone)]
pub(crate) struct KeyCopy(Repr);

#[derive(Clone)]
enum Repr {
    /// The first `len` bytes of `bytes`, valid UTF-8.
    Inline {
        len: u8,
        bytes: [u8; INLINE],
    },
    Heap(Box<str>),
}

const _: () = assert!(std::mem::size_of::<KeyCopy>() == std::mem::size_of::<String>());

impl KeyCopy {
    /// The string as a `&str`.
    pub(crate) fn as_str(&self) -> &str {
        match &self.0 {
            Repr::Inline { len, bytes } => std::str::from_utf8(&bytes[..usize::from(*len)])
                .expect("an inline string is built from a str"),
            Repr::Heap(s) => s,
        }
    }

    /// The string's bytes. Unlike [`KeyCopy::as_str`], this does not check that the inline bytes are
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

impl From<&str> for KeyCopy {
    fn from(s: &str) -> Self {
        if s.len() <= INLINE {
            let mut bytes = [0; INLINE];
            bytes[..s.len()].copy_from_slice(s.as_bytes());
            KeyCopy(Repr::Inline {
                len: s.len() as u8,
                bytes,
            })
        } else {
            KeyCopy(Repr::Heap(s.into()))
        }
    }
}

impl From<&super::Str<'_>> for KeyCopy {
    fn from(s: &super::Str<'_>) -> Self {
        KeyCopy::from(s.as_str())
    }
}

impl From<&String> for KeyCopy {
    fn from(s: &String) -> Self {
        KeyCopy::from(s.as_str())
    }
}

impl From<String> for KeyCopy {
    /// A short string is copied inline and `s` dropped; a long one keeps `s`'s allocation, which
    /// is shrunk to its length.
    fn from(s: String) -> Self {
        if s.len() <= INLINE {
            KeyCopy::from(s.as_str())
        } else {
            KeyCopy(Repr::Heap(s.into_boxed_str()))
        }
    }
}

impl PartialEq for KeyCopy {
    fn eq(&self, other: &KeyCopy) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl Eq for KeyCopy {}

impl fmt::Debug for KeyCopy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for KeyCopy {
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
            let from_str = KeyCopy::from(s.as_str());
            let from_string = KeyCopy::from(s.clone());
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
            let key = KeyCopy::from(s);
            assert_eq!(format!("{key:?}"), format!("{s:?}"));
            assert_eq!(key.to_string(), s);
        }
    }
}

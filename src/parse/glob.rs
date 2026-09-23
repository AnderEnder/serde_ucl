//! Glob patterns for `.include(glob=true)` (spec §9.4, *Globs*).
//!
//! `*`, `?` and bracket expressions (`[ab]`, `[a-z]`, `[!a]`), with a backslash quoting the next
//! character. A `*`, `?` or bracket expression never matches a leading `.` of a name, while a
//! written `.` does, `.` and `..` included. There is no brace or `~` expansion. As the oracle
//! does, `[^…]` is not a negation and `[[:alpha:]]` is not a character class (QUESTIONS.md #29).
//! Matches are sorted by byte value.

use super::loader::{FileKind, Loader};
use std::path::{Path, PathBuf};

/// Whether `path` is a pattern when `glob=true`: it contains `*` or `?` (spec §9.4).
pub(crate) fn has_wildcard(path: &str) -> bool {
    path.contains(['*', '?'])
}

/// The paths that `pattern` matches, sorted by byte value. A relative pattern is matched below
/// `base`, which is not itself a pattern.
pub(crate) fn expand(loader: &dyn Loader, base: &Path, pattern: &str) -> Vec<PathBuf> {
    let (start, rest) = match pattern.strip_prefix('/') {
        Some(rest) => (PathBuf::from("/"), rest),
        None => (base.to_path_buf(), pattern),
    };
    let mut parts: Vec<&str> = rest.split('/').collect();
    // A pattern that ends in `/` matches directories only.
    let dirs_only = parts.len() > 1 && parts.last() == Some(&"");
    if dirs_only {
        parts.pop();
    }
    let mut paths = vec![start];
    for part in parts {
        if paths.is_empty() {
            break;
        }
        let pattern: Vec<char> = part.chars().collect();
        if !is_pattern(&pattern) {
            let literal = unescape(&pattern);
            for path in &mut paths {
                path.push(&literal);
            }
            continue;
        }
        let mut next = Vec::new();
        for dir in &paths {
            let Ok(mut names) = loader.read_dir(dir) else {
                continue;
            };
            names.push(".".to_string());
            names.push("..".to_string());
            for name in names {
                let chars: Vec<char> = name.chars().collect();
                if matches(&pattern, &chars) {
                    next.push(dir.join(&name));
                }
            }
        }
        paths = next;
    }
    paths.retain(|p| match loader.kind(p) {
        Some(FileKind::Directory) => true,
        Some(_) => !dirs_only,
        None => false,
    });
    if dirs_only {
        for path in &mut paths {
            path.push("");
        }
    }
    paths.sort_by(|a, b| {
        a.as_os_str()
            .as_encoded_bytes()
            .cmp(b.as_os_str().as_encoded_bytes())
    });
    paths.dedup();
    paths
}

/// Whether a path component holds a wildcard or a bracket that is not quoted.
fn is_pattern(part: &[char]) -> bool {
    let mut i = 0;
    while i < part.len() {
        match part[i] {
            '\\' => i += 2,
            '*' | '?' | '[' => return true,
            _ => i += 1,
        }
    }
    false
}

/// A component without wildcards, with quoting backslashes removed.
fn unescape(part: &[char]) -> String {
    let mut out = String::new();
    let mut chars = part.iter();
    while let Some(&c) = chars.next() {
        match c {
            '\\' => out.push(*chars.next().unwrap_or(&'\\')),
            c => out.push(c),
        }
    }
    out
}

/// Whether the name `name` matches the component pattern `pattern`.
pub(crate) fn matches(pattern: &[char], name: &[char]) -> bool {
    if name.first() == Some(&'.') && !matches!(pattern, ['.', ..] | ['\\', '.', ..]) {
        return false;
    }
    let (mut p, mut n) = (0, 0);
    // Where to resume after the last `*`: the pattern after it, and the name position it was
    // tried at.
    let mut resume: Option<(usize, usize)> = None;
    loop {
        if p < pattern.len() {
            let step = match pattern[p] {
                '*' => {
                    resume = Some((p + 1, n));
                    p += 1;
                    continue;
                }
                '?' => (n < name.len()).then_some(1),
                '[' => match bracket(&pattern[p..], name.get(n).copied()) {
                    Some((true, len)) => Some(len),
                    Some((false, _)) => None,
                    None => (name.get(n) == Some(&'[')).then_some(1),
                },
                '\\' if p + 1 < pattern.len() => {
                    (name.get(n) == Some(&pattern[p + 1])).then_some(2)
                }
                c => (name.get(n) == Some(&c)).then_some(1),
            };
            if let Some(len) = step {
                p += len;
                n += 1;
                continue;
            }
        } else if n == name.len() {
            return true;
        }
        match resume {
            Some((after_star, tried)) if tried < name.len() => {
                resume = Some((after_star, tried + 1));
                p = after_star;
                n = tried + 1;
            }
            _ => return false,
        }
    }
}

/// The bracket expression at the start of `pattern`, tested against `c`: whether it matches and
/// how many pattern characters it takes. `None` if the bracket is not closed, which makes the
/// `[` an ordinary character.
fn bracket(pattern: &[char], c: Option<char>) -> Option<(bool, usize)> {
    let mut i = 1;
    let negate = pattern.get(1) == Some(&'!');
    if negate {
        i += 1;
    }
    let mut matched = false;
    let mut first = true;
    loop {
        let &b = pattern.get(i)?;
        if b == ']' && !first {
            i += 1;
            break;
        }
        first = false;
        let mut low = b;
        if b == '\\' && i + 1 < pattern.len() {
            i += 1;
            low = pattern[i];
        }
        i += 1;
        let mut high = low;
        if pattern.get(i) == Some(&'-') && pattern.get(i + 1).is_some_and(|&h| h != ']') {
            high = pattern[i + 1];
            i += 2;
            if high == '\\' && i < pattern.len() {
                high = pattern[i];
                i += 1;
            }
        }
        if c.is_some_and(|c| low <= c && c <= high) {
            matched = true;
        }
    }
    Some((c.is_some() && matched != negate, i))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::MemoryLoader;

    fn m(pattern: &str, name: &str) -> bool {
        let p: Vec<char> = pattern.chars().collect();
        let n: Vec<char> = name.chars().collect();
        matches(&p, &n)
    }

    #[test]
    fn component_matching() {
        // Oracle runs (spec §9.4; QUESTIONS.md #29).
        assert!(m("*.inc", "a.inc"));
        assert!(!m("*.inc", ".hidden.inc"));
        assert!(m(".*", ".hidden.inc"));
        assert!(m(".*", "."));
        assert!(m(".*", ".."));
        assert!(!m("?hidden.inc", ".hidden.inc"));
        assert!(!m("[.]hidden.inc", ".hidden.inc"));
        assert!(m("?.inc", "a.inc"));
        assert!(!m("?.inc", "ab.inc"));
        assert!(m("[ab]*", "b.conf"));
        assert!(m("[a-b]*", "b.conf"));
        assert!(!m("[!a]*", "a.inc"));
        assert!(m("[!a]*", "b.conf"));
        assert!(m("[^a]*", "a.inc"), "^ is an ordinary member");
        assert!(m("[]a]*", "a.inc"));
        assert!(m("[]a]*", "]"));
        assert!(!m("[[:alpha:]]*", "a.inc"));
        assert!(m("[[:alpha:]]*", "a]"));
        assert!(m("\\a*", "a.inc"));
        assert!(!m("\\*.inc", "a.inc"));
        assert!(m("\\*.inc", "*.inc"));
        assert!(m("[a", "[a"));
        assert!(m("[a*", "[abc"));
        assert!(m("{a,b}*", "{a,b}.inc"));
        assert!(!m("{a,b}*", "a.inc"));
        assert!(m("a*b*c", "aXbYbZc"));
        assert!(!m("a*b*c", "aXbYbZ"));
        assert!(m("*", "é"));
        assert!(m("?", "é"));
    }

    #[test]
    fn expansion_walks_directories_and_sorts_by_bytes() {
        let mut fs = MemoryLoader::new();
        for f in [
            "/c/o/10.inc",
            "/c/o/9.inc",
            "/c/o/B.inc",
            "/c/o/_u.inc",
            "/c/o/a.inc",
        ] {
            fs.add_file(f, "");
        }
        fs.add_file("/c/g/.hidden.inc", "")
            .add_file("/c/g/a.inc", "")
            .add_file("/c/g/sub/x.inc", "");
        let base = Path::new("/c");
        let names = |pattern: &str| -> Vec<String> {
            expand(&fs, base, pattern)
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect()
        };
        assert_eq!(
            names("o/*.inc"),
            [
                "/c/o/10.inc",
                "/c/o/9.inc",
                "/c/o/B.inc",
                "/c/o/_u.inc",
                "/c/o/a.inc"
            ]
        );
        assert_eq!(names("g/*"), ["/c/g/a.inc", "/c/g/sub"]);
        assert_eq!(names("g/.*"), ["/c/g/.", "/c/g/..", "/c/g/.hidden.inc"]);
        assert_eq!(names("*/a.inc"), ["/c/g/a.inc", "/c/o/a.inc"]);
        assert_eq!(names("g/*/*"), ["/c/g/sub/x.inc"]);
        assert_eq!(names("g/*/"), ["/c/g/sub/"]);
        assert_eq!(names("/c/g/a*"), ["/c/g/a.inc"]);
        assert_eq!(names("g/a.inc/*"), Vec::<String>::new());
        assert_eq!(names("none/*"), Vec::<String>::new());
        assert!(has_wildcard("a*") && has_wildcard("a?") && !has_wildcard("[ab].inc"));
    }
}

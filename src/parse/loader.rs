//! Loaders: where the include macros and `.load` read files from (spec §9.3, §9.4, §9.6).
//!
//! The parser never touches the filesystem itself. It resolves relative paths against its base
//! directory ([`super::Parser::set_base_dir`]; without one, the directory of the file given to
//! [`super::Parser::parse_file`], or [`Loader::current_dir`] for a document given as bytes),
//! and asks the loader to make a path canonical, to say what a path names, to read a file and
//! to list a directory for glob patterns.
//!
//! - [`FsLoader`] (Cargo feature `fs`, on by default) reads the real filesystem.
//! - [`MemoryLoader`] serves files registered in memory.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Component, Path, PathBuf};

/// What a path names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
    /// Anything else, such as a device, a socket or a FIFO.
    Other,
}

/// A source of files for `.include`, `.try_include` and `.load`.
///
/// Every path a parser passes to a loader is absolute.
pub trait Loader {
    /// The directory that relative paths in a document given as bytes, and a relative path
    /// given to [`super::Parser::parse_file`], resolve against when the parser has no base
    /// directory of its own. It is then also `CURDIR` for a document given as bytes (spec §7.8)
    /// and for macro argument lists in it (spec §9.2).
    fn current_dir(&self) -> io::Result<PathBuf>;

    /// `path` made canonical: absolute, with `.` and `..` components and symbolic links resolved
    /// (spec §9.3). An error when nothing exists at `path`.
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;

    /// What `path` names, following symbolic links, or `None` if nothing exists there.
    fn kind(&self, path: &Path) -> Option<FileKind>;

    /// The contents of the regular file at `path`.
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;

    /// The contents of the regular file at `path` if it holds at most `limit` bytes; if it holds
    /// more, any part of it longer than `limit` bytes, such as its first `limit + 1` bytes. A
    /// parser with an input limit ([`super::Parser::set_max_input_bytes`]) reads files with it,
    /// so that a large file is not read whole only to be rejected.
    ///
    /// The default reads the whole file with [`Loader::read`]; [`FsLoader`] and
    /// [`MemoryLoader`] stop after `limit + 1` bytes.
    fn read_limited(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        let _ = limit;
        self.read(path)
    }

    /// The names of the entries of the directory at `path`, in any order, without `.` and `..`.
    /// Names that are not valid UTF-8 may be left out. Used to expand glob patterns (spec §9.4).
    fn read_dir(&self, path: &Path) -> io::Result<Vec<String>>;
}

/// Reads files from the filesystem with `std::fs`. Its current directory is the process's
/// working directory: without a base directory, relative paths in a document given as bytes
/// resolve against it, while those in a file given to [`super::Parser::parse_file`] resolve
/// against that file's directory.
#[cfg(feature = "fs")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FsLoader;

#[cfg(feature = "fs")]
impl FsLoader {
    /// The filesystem loader.
    pub fn new() -> Self {
        Self
    }
}

#[cfg(feature = "fs")]
impl Loader for FsLoader {
    fn current_dir(&self) -> io::Result<PathBuf> {
        std::env::current_dir()
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }

    fn kind(&self, path: &Path) -> Option<FileKind> {
        let meta = std::fs::metadata(path).ok()?;
        Some(if meta.is_file() {
            FileKind::File
        } else if meta.is_dir() {
            FileKind::Directory
        } else {
            FileKind::Other
        })
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn read_limited(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(limit.saturating_add(1))
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in std::fs::read_dir(path)? {
            if let Ok(name) = entry?.file_name().into_string() {
                names.push(name);
            }
        }
        Ok(names)
    }
}

/// Serves files held in memory, for tests and for applications that embed their configuration.
///
/// Paths are kept in a normalised absolute form: a relative path is taken relative to the
/// loader's current directory (`/` unless [`MemoryLoader::set_current_dir`] changes it), and
/// `.` and `..` components are resolved by name. The directories above every file exist
/// implicitly. There are no symbolic links.
///
/// ```
/// use serde_ucl::parse::{MemoryLoader, Parser};
///
/// let mut loader = MemoryLoader::new();
/// loader.add_file("/etc/app/main.conf", "port = 80\n.include \"${CURDIR}/extra.conf\"\n");
/// loader.add_file("/etc/app/extra.conf", "host = example.org\n");
/// let mut parser = Parser::new();
/// parser.set_loader(loader);
/// let value = parser.parse_file("/etc/app/main.conf").unwrap();
/// assert_eq!(value.as_object().unwrap()["host"].as_str(), Some("example.org"));
/// ```
#[derive(Debug, Clone)]
pub struct MemoryLoader {
    files: BTreeMap<PathBuf, Vec<u8>>,
    dirs: BTreeSet<PathBuf>,
    current_dir: PathBuf,
}

impl Default for MemoryLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryLoader {
    /// An empty loader whose current directory is `/`.
    pub fn new() -> Self {
        let root = root();
        let mut dirs = BTreeSet::new();
        dirs.insert(root.clone());
        Self {
            files: BTreeMap::new(),
            dirs,
            current_dir: root,
        }
    }

    /// Adds or replaces the file at `path`, and the directories above it.
    pub fn add_file(&mut self, path: impl AsRef<Path>, contents: impl Into<Vec<u8>>) -> &mut Self {
        let path = self.normalise(path.as_ref());
        if let Some(parent) = path.parent() {
            self.add_dirs(parent.to_path_buf());
        }
        self.files.insert(path, contents.into());
        self
    }

    /// Adds the directory at `path`, and the directories above it.
    pub fn add_dir(&mut self, path: impl AsRef<Path>) -> &mut Self {
        let path = self.normalise(path.as_ref());
        self.add_dirs(path);
        self
    }

    /// Sets the directory that relative paths are taken relative to, and adds it.
    pub fn set_current_dir(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        let dir = self.normalise(dir.as_ref());
        self.add_dirs(dir.clone());
        self.current_dir = dir;
        self
    }

    fn add_dirs(&mut self, mut dir: PathBuf) {
        loop {
            if !self.dirs.insert(dir.clone()) {
                return;
            }
            if !dir.pop() {
                return;
            }
        }
    }

    /// `path` as an absolute path with `.` and `..` resolved by name.
    fn normalise(&self, path: &Path) -> PathBuf {
        let mut out = if path.is_absolute() {
            PathBuf::new()
        } else {
            self.current_dir.clone()
        };
        for component in path.components() {
            match component {
                Component::Prefix(p) => out.push(p.as_os_str()),
                Component::RootDir => out.push(Component::RootDir.as_os_str()),
                Component::CurDir => {}
                Component::ParentDir => {
                    out.pop();
                }
                Component::Normal(name) => out.push(name),
            }
        }
        if out.as_os_str().is_empty() {
            out = root();
        }
        out
    }
}

fn root() -> PathBuf {
    PathBuf::from(Component::RootDir.as_os_str())
}

fn not_found(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{}: no such file or directory", path.display()),
    )
}

impl Loader for MemoryLoader {
    fn current_dir(&self) -> io::Result<PathBuf> {
        Ok(self.current_dir.clone())
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        let path = self.normalise(path);
        if self.files.contains_key(&path) || self.dirs.contains(&path) {
            Ok(path)
        } else {
            Err(not_found(&path))
        }
    }

    fn kind(&self, path: &Path) -> Option<FileKind> {
        let path = self.normalise(path);
        if self.files.contains_key(&path) {
            Some(FileKind::File)
        } else if self.dirs.contains(&path) {
            Some(FileKind::Directory)
        } else {
            None
        }
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.read_limited(path, u64::MAX)
    }

    fn read_limited(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        let path = self.normalise(path);
        match self.files.get(&path) {
            Some(bytes) => {
                let len = usize::try_from(limit.saturating_add(1)).unwrap_or(usize::MAX);
                Ok(bytes[..bytes.len().min(len)].to_vec())
            }
            None if self.dirs.contains(&path) => Err(io::Error::other(format!(
                "{}: is a directory",
                path.display()
            ))),
            None => Err(not_found(&path)),
        }
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<String>> {
        let dir = self.normalise(path);
        if !self.dirs.contains(&dir) {
            return Err(not_found(&dir));
        }
        let children = self
            .files
            .keys()
            .chain(self.dirs.iter())
            .filter(|p| p.parent() == Some(dir.as_path()))
            .filter_map(|p| p.file_name()?.to_str().map(str::to_owned))
            .collect();
        Ok(children)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_loader_paths() {
        let mut m = MemoryLoader::new();
        m.add_file("/a/b/c.conf", "x = 1").add_dir("/a/empty");
        m.set_current_dir("/a");
        assert_eq!(m.kind(Path::new("/a/b/c.conf")), Some(FileKind::File));
        assert_eq!(m.kind(Path::new("b")), Some(FileKind::Directory));
        assert_eq!(
            m.kind(Path::new("/a/b/../b/./c.conf")),
            Some(FileKind::File)
        );
        assert_eq!(m.kind(Path::new("/nope")), None);
        assert_eq!(
            m.canonicalize(Path::new("b/../b/c.conf")).unwrap(),
            PathBuf::from("/a/b/c.conf")
        );
        assert!(m.canonicalize(Path::new("/a/x")).is_err());
        assert_eq!(m.read(Path::new("/a/b/c.conf")).unwrap(), b"x = 1");
        let limited = |limit| m.read_limited(Path::new("/a/b/c.conf"), limit).unwrap();
        assert_eq!(limited(5), b"x = 1");
        assert_eq!(limited(2), b"x =");
        assert_eq!(limited(u64::MAX), b"x = 1");
        assert!(m.read_limited(Path::new("/a/b"), 2).is_err());
        assert!(m.read(Path::new("/a/b")).is_err());
        let mut names = m.read_dir(Path::new("/a")).unwrap();
        names.sort();
        assert_eq!(names, ["b", "empty"]);
        assert!(m.read_dir(Path::new("/a/b/c.conf")).is_err());
        assert_eq!(m.current_dir().unwrap(), PathBuf::from("/a"));
    }

    #[cfg(feature = "fs")]
    #[test]
    fn fs_loader_kinds() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let fs = FsLoader::new();
        assert_eq!(fs.kind(&here.join("Cargo.toml")), Some(FileKind::File));
        assert_eq!(fs.kind(&here.join("src")), Some(FileKind::Directory));
        assert_eq!(fs.kind(&here.join("no such file")), None);
        assert!(fs.read_dir(here).unwrap().iter().any(|n| n == "Cargo.toml"));
        let cargo = fs.read(&here.join("Cargo.toml")).unwrap();
        assert_eq!(
            fs.read_limited(&here.join("Cargo.toml"), 9).unwrap(),
            cargo[..10]
        );
        assert_eq!(
            fs.read_limited(&here.join("Cargo.toml"), cargo.len() as u64)
                .unwrap(),
            cargo
        );
        assert!(fs.read_limited(&here.join("no such file"), 9).is_err());
    }
}

//! Benchmark documents read from files (clean-room work item C14): the JSON documents that
//! `benches/fetch-documents.sh` fetches into `target/bench-corpus/`, and the configurations in
//! `benches/corpus/` (or in the directory `UCL_BENCH_CORPUS` names). A missing document is
//! skipped with a message, so that the benchmarks build and run without the network.

use std::path::{Path, PathBuf};

/// The JSON documents that serde_json and simd-json publish figures for, by name, in
/// `target/bench-corpus/<name>.json`.
pub const JSON_DOCUMENTS: [&str; 3] = ["twitter", "citm_catalog", "canada"];

/// A document read from a file.
pub struct Document {
    /// The benchmark name: the file name without `.json` for the JSON documents, the file name
    /// for the corpus.
    pub name: String,
    pub path: PathBuf,
    pub text: String,
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The JSON documents that are present, with a message for each one that is missing.
pub fn json_documents(bench: &str) -> Vec<Document> {
    let dir = root().join("target/bench-corpus");
    let mut documents = Vec::new();
    for name in JSON_DOCUMENTS {
        let path = dir.join(format!("{name}.json"));
        match std::fs::read_to_string(&path) {
            Ok(text) => documents.push(Document {
                name: name.to_string(),
                path,
                text,
            }),
            Err(e) => eprintln!(
                "{bench}: skipping {name}: cannot read {} ({e}); benches/fetch-documents.sh \
                 fetches it",
                path.display()
            ),
        }
    }
    documents
}

/// True for the files of `benches/corpus/` that describe the documents rather than being one:
/// hidden files, Markdown and plain-text notes, and licence files.
fn is_metadata(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    name.starts_with('.')
        || upper.ends_with(".MD")
        || upper.ends_with(".TXT")
        || upper.starts_with("LICENSE")
        || upper.starts_with("LICENCE")
        || upper.starts_with("NOTICE")
        || upper.starts_with("COPYING")
}

/// The configurations in `benches/corpus/`, or in the directory `UCL_BENCH_CORPUS` names, in
/// the order of their file names, with a message if there are none.
pub fn corpus_documents(bench: &str) -> Vec<Document> {
    let dir = match std::env::var_os("UCL_BENCH_CORPUS") {
        Some(dir) => PathBuf::from(dir),
        None => root().join("benches/corpus"),
    };
    let mut paths: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| !is_metadata(name))
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    paths.sort();
    let mut documents = Vec::new();
    for path in paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        match std::fs::read_to_string(&path) {
            Ok(text) => documents.push(Document { name, path, text }),
            Err(e) => eprintln!("{bench}: skipping {}: {e}", path.display()),
        }
    }
    if documents.is_empty() {
        eprintln!(
            "{bench}: skipping the corpus: no documents in {}",
            dir.display()
        );
    }
    documents
}

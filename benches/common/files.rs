//! Benchmark documents read from files (clean-room work item C14): the JSON documents that
//! `benches/fetch-documents.sh` fetches into `target/bench-corpus/`, and the rspamd
//! configurations in `benches/corpus/`. A missing document is skipped with a message, so that
//! the benchmarks build and run without the network.

use serde_ucl::parse::{FsLoader, Parser};
use std::path::{Path, PathBuf};

/// The JSON documents that serde_json and simd-json publish figures for, by name, in
/// `target/bench-corpus/<name>.json`.
pub const JSON_DOCUMENTS: [&str; 3] = ["twitter", "citm_catalog", "canada"];

/// A JSON document read from its file.
pub struct Document {
    /// The benchmark name: the file name without `.json`.
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

/// A document of `benches/corpus/`, with the settings its check uses (`benches/corpus/README.md`,
/// `benches/check-documents.sh`).
pub struct CorpusDocument {
    /// The benchmark name.
    pub name: &'static str,
    /// The file, relative to `benches/corpus/`.
    pub file: &'static str,
    /// The directory that includes resolve against, relative to `benches/corpus/`: the check's
    /// `--dir`.
    pub dir: &'static str,
    /// The variables of the check's `var:` flags.
    pub variables: &'static [(&'static str, &'static str)],
    /// The files the document includes, relative to `dir`. Their bytes count toward the
    /// throughput, since a parse reads them.
    pub includes: &'static [&'static str],
}

/// The documents of the corpus's table (`benches/corpus/README.md`).
pub const CORPUS: [CorpusDocument; 3] = [
    CorpusDocument {
        name: "rspamd-groups",
        file: "rspamd/groups.conf",
        dir: "rspamd",
        variables: &[("CONFDIR", ".")],
        includes: &[
            "scores.d/headers_group.conf",
            "scores.d/subject_group.conf",
            "scores.d/mua_group.conf",
            "scores.d/rbl_group.conf",
            "scores.d/statistics_group.conf",
            "scores.d/fuzzy_group.conf",
            "scores.d/policies_group.conf",
            "scores.d/whitelist_group.conf",
            "scores.d/surbl_group.conf",
            "scores.d/phishing_group.conf",
            "scores.d/url_suspect_group.conf",
            "scores.d/hfilter_group.conf",
            "scores.d/mime_types_group.conf",
            "scores.d/content_group.conf",
        ],
    },
    CorpusDocument {
        name: "rspamd-composites",
        file: "rspamd/composites.conf",
        dir: "rspamd",
        variables: &[],
        includes: &[],
    },
    CorpusDocument {
        name: "rspamd-rbl_group",
        file: "rspamd/scores.d/rbl_group.conf",
        dir: "rspamd",
        variables: &[],
        includes: &[],
    },
];

/// A corpus document read from its file.
pub struct Corpus {
    pub document: &'static CorpusDocument,
    pub path: PathBuf,
    pub text: String,
    /// The bytes a parse reads: the document's and those of the files it includes.
    pub bytes_read: u64,
}

impl Corpus {
    /// A parser set up as the check sets one up (`tests/common/oracle.rs`, `configure`): the
    /// filesystem loader, the document's directory as the base directory, the variable `ABI`
    /// = `unknown`, and the document's variables.
    pub fn parser(&self) -> Parser {
        let mut parser = Parser::new();
        parser.set_loader(FsLoader::new());
        parser.set_base_dir(corpus_dir().join(self.document.dir));
        parser.register_variable("ABI", "unknown");
        for &(name, value) in self.document.variables {
            parser.register_variable(name, value);
        }
        parser
    }
}

fn corpus_dir() -> PathBuf {
    root().join("benches/corpus")
}

/// The documents of [`CORPUS`], with a message for each one that cannot be read.
pub fn corpus_documents(bench: &str) -> Vec<Corpus> {
    let mut documents = Vec::new();
    'documents: for document in &CORPUS {
        let path = corpus_dir().join(document.file);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) => {
                eprintln!("{bench}: skipping {}: {e}", path.display());
                continue;
            }
        };
        let mut bytes_read = text.len() as u64;
        for include in document.includes {
            let include = corpus_dir().join(document.dir).join(include);
            match std::fs::metadata(&include) {
                Ok(metadata) => bytes_read += metadata.len(),
                Err(e) => {
                    eprintln!(
                        "{bench}: skipping {}: {}: {e}",
                        path.display(),
                        include.display()
                    );
                    continue 'documents;
                }
            }
        }
        documents.push(Corpus {
            document,
            path,
            text,
            bytes_read,
        });
    }
    documents
}

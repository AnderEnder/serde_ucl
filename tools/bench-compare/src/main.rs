//! Times serde_ucl and serde_json on the benchmarks' documents, for the README's comparison with
//! libucl and serde_json (`scripts/bench-compare.sh` runs it in turns with `lucl.c`).
//!
//! ```text
//! bench-compare write DIR                  write the generated documents into DIR
//! bench-compare run DIR [CORPUS]           time serde_ucl and serde_json on them, and on CORPUS
//! bench-compare summarize RESULTS...       median tables from the lines of the runs
//! ```
//!
//! Every line of `run` is `tool|kind|document|seconds`: `kind` is `parse` (into the library's
//! value tree, freed each time), `parse-nofree` (the values kept and freed after the timing),
//! `typed` and `typed-borrowed` (`from_str` into a struct whose strings are owned or borrowed).

#[path = "../../../benches/common/mod.rs"]
mod common;

use serde::Deserialize;
use std::collections::BTreeMap;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Deserialize)]
#[allow(dead_code)]
struct Items {
    items: Vec<Item>,
}
#[derive(Deserialize)]
#[allow(dead_code)]
struct Item {
    id: u64,
    name: String,
    price: f64,
    active: bool,
    tags: Vec<String>,
    dims: Dims,
    note: Option<String>,
}
#[derive(Deserialize)]
#[allow(dead_code)]
struct Dims {
    w: u64,
    h: u64,
    d: f64,
}
#[derive(Deserialize)]
#[allow(dead_code)]
struct ItemsBorrowed<'a> {
    #[serde(borrow)]
    items: Vec<ItemBorrowed<'a>>,
}
#[derive(Deserialize)]
#[allow(dead_code)]
struct ItemBorrowed<'a> {
    id: u64,
    name: &'a str,
    price: f64,
    active: bool,
    #[serde(borrow)]
    tags: Vec<&'a str>,
    dims: Dims,
    note: Option<&'a str>,
}
#[derive(Deserialize)]
#[allow(dead_code)]
struct Small {
    name: String,
    port: u16,
    debug: bool,
}
#[derive(Deserialize)]
#[allow(dead_code)]
struct SmallBorrowed<'a> {
    name: &'a str,
    port: u16,
    debug: bool,
}

/// The generated documents: file name, contents. JSON documents have a typed target.
fn documents() -> Vec<(&'static str, String)> {
    vec![
        ("json-10000.json", common::json(10000)),
        ("json-1000.json", common::json(1000)),
        ("config-1000.ucl", common::config(1000)),
        ("small.ucl", common::SMALL.to_string()),
        (
            "small.json",
            r#"{"name": "svc", "port": 8080, "debug": true}"#.to_string(),
        ),
    ]
}

/// The corpus documents, relative to the corpus directory (`benches/corpus/rspamd`), and
/// whether they include files of it, through the variable `CONFDIR` (benches/corpus/README.md).
const CORPUS: [(&str, bool); 3] = [
    ("groups.conf", true),
    ("composites.conf", false),
    ("scores.d/rbl_group.conf", false),
];

/// Median time per call over 31 samples, each at least about 5 ms of repetitions.
fn time<F: FnMut() -> bool>(mut f: F) -> f64 {
    assert!(f(), "the document failed");
    let start = Instant::now();
    black_box(f());
    let one = start.elapsed().as_secs_f64();
    let reps = ((0.005 / one.max(1e-9)) as usize).max(1);
    let mut samples: Vec<f64> = (0..31)
        .map(|_| {
            let start = Instant::now();
            for _ in 0..reps {
                black_box(f());
            }
            start.elapsed().as_secs_f64() / reps as f64
        })
        .collect();
    samples.sort_by(f64::total_cmp);
    samples[15]
}

/// As [`time`], but the results are kept and dropped after each sample's timing.
fn time_keep<T, F: FnMut() -> T>(mut f: F) -> f64 {
    let start = Instant::now();
    let first = f();
    let one = start.elapsed().as_secs_f64();
    drop(first);
    let reps = ((0.005 / one.max(1e-9)) as usize).max(1);
    let mut keep = Vec::with_capacity(reps);
    let mut samples: Vec<f64> = (0..31)
        .map(|_| {
            let start = Instant::now();
            for _ in 0..reps {
                keep.push(f());
            }
            let sample = start.elapsed().as_secs_f64() / reps as f64;
            keep.clear();
            sample
        })
        .collect();
    samples.sort_by(f64::total_cmp);
    samples[15]
}

fn write(dir: &Path) {
    std::fs::create_dir_all(dir).expect("create the document directory");
    for (name, doc) in documents() {
        std::fs::write(dir.join(name), doc).expect("write a document");
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn run(dir: &Path, corpus: Option<&Path>) {
    for (name, _) in documents() {
        let doc = read(&dir.join(name));
        let t = time(|| serde_ucl::parse::parse(black_box(doc.as_bytes())).is_ok());
        println!("serde_ucl|parse|{name}|{t:.9}");
        let t = time_keep(|| serde_ucl::parse::parse(black_box(doc.as_bytes())).unwrap());
        println!("serde_ucl|parse-nofree|{name}|{t:.9}");
        if name.ends_with(".json") {
            let t = time(|| serde_json::from_str::<serde_json::Value>(black_box(&doc)).is_ok());
            println!("serde_json|parse|{name}|{t:.9}");
        }
        if name.starts_with("json") {
            let t = time(|| serde_ucl::from_str::<Items>(black_box(&doc)).is_ok());
            println!("serde_ucl|typed|{name}|{t:.9}");
            let t = time(|| serde_ucl::from_str::<ItemsBorrowed>(black_box(&doc)).is_ok());
            println!("serde_ucl|typed-borrowed|{name}|{t:.9}");
            let t = time(|| serde_json::from_str::<Items>(black_box(&doc)).is_ok());
            println!("serde_json|typed|{name}|{t:.9}");
            let t = time(|| serde_json::from_str::<ItemsBorrowed>(black_box(&doc)).is_ok());
            println!("serde_json|typed-borrowed|{name}|{t:.9}");
        }
        if name.starts_with("small") {
            let t = time(|| serde_ucl::from_str::<Small>(black_box(&doc)).is_ok());
            println!("serde_ucl|typed|{name}|{t:.9}");
            let t = time(|| serde_ucl::from_str::<SmallBorrowed>(black_box(&doc)).is_ok());
            println!("serde_ucl|typed-borrowed|{name}|{t:.9}");
            if name.ends_with(".json") {
                let t = time(|| serde_json::from_str::<Small>(black_box(&doc)).is_ok());
                println!("serde_json|typed|{name}|{t:.9}");
                let t = time(|| serde_json::from_str::<SmallBorrowed>(black_box(&doc)).is_ok());
                println!("serde_json|typed-borrowed|{name}|{t:.9}");
            }
        }
    }
    let Some(corpus) = corpus else { return };
    let confdir = std::fs::canonicalize(corpus).expect("the corpus directory");
    for (path, includes) in CORPUS {
        let doc = read(&confdir.join(path));
        let name = Path::new(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        // A parser as `parse::parse` makes one, with a file loader and `CONFDIR` for includes.
        let parse = || {
            let mut parser = serde_ucl::parse::Parser::new();
            if includes {
                parser.set_loader(serde_ucl::parse::FsLoader::new());
                parser.register_variable("CONFDIR", confdir.to_string_lossy().into_owned());
            }
            parser.parse(black_box(doc.as_bytes()))
        };
        let t = time(|| parse().is_ok());
        println!("serde_ucl|parse|{name}|{t:.9}");
        let t = time_keep(|| parse().unwrap());
        println!("serde_ucl|parse-nofree|{name}|{t:.9}");
    }
}

/// The README's label of a document.
fn label(name: &str) -> String {
    match name {
        "json-10000.json" => "JSON, 10,000 records".into(),
        "json-1000.json" => "JSON, 1,000 records".into(),
        "config-1000.ucl" => "configuration, 1,000 services".into(),
        "small.ucl" => "three entries, UCL".into(),
        "small.json" => "three entries, JSON".into(),
        "groups.conf" => "rspamd `groups.conf`, with its includes".into(),
        other => format!("rspamd `{other}`"),
    }
}

/// A time as the README writes it: milliseconds from 0.1 ms, microseconds below.
fn format_time(seconds: f64) -> String {
    let (value, unit) = if seconds >= 1e-4 {
        (seconds * 1e3, "ms")
    } else {
        (seconds * 1e6, "µs")
    };
    let digits = if value >= 100.0 {
        0
    } else if value >= 10.0 {
        1
    } else {
        2
    };
    format!("{value:.digits$} {unit}")
}

fn summarize(files: &[PathBuf]) {
    // (tool, kind, document) -> the round's values
    let mut values: BTreeMap<(String, String, String), Vec<f64>> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for file in files {
        for line in read(file).lines() {
            let fields: Vec<&str> = line.split('|').collect();
            // An optional round number first.
            let fields = if fields.len() == 5 {
                &fields[1..]
            } else {
                &fields[..]
            };
            let [tool, kind, doc, seconds] = fields else {
                continue;
            };
            let Ok(seconds) = seconds.parse::<f64>() else {
                continue;
            };
            if !order.iter().any(|d| d == doc) {
                order.push(doc.to_string());
            }
            values
                .entry((tool.to_string(), kind.to_string(), doc.to_string()))
                .or_default()
                .push(seconds);
        }
    }
    let median = |tool: &str, kind: &str, doc: &str| -> String {
        match values.get(&(tool.into(), kind.into(), doc.into())) {
            Some(v) => {
                let mut v = v.clone();
                v.sort_by(f64::total_cmp);
                let m = if v.len() % 2 == 1 {
                    v[v.len() / 2]
                } else {
                    (v[v.len() / 2 - 1] + v[v.len() / 2]) / 2.0
                };
                format_time(m)
            }
            None => "–".into(),
        }
    };
    let rounds = values.values().map(Vec::len).max().unwrap_or(0);
    let plural = if rounds == 1 { "" } else { "s" };
    println!("Medians over {rounds} round{plural}; each time includes freeing the result.\n");
    println!("| Document | serde_ucl | libucl | serde_json |");
    println!("| --- | ---: | ---: | ---: |");
    for doc in &order {
        println!(
            "| {} | {} | {} | {} |",
            label(doc),
            median("serde_ucl", "parse", doc),
            median("libucl", "parse", doc),
            median("serde_json", "parse", doc)
        );
    }
    println!("\nThe same without freeing the value in the timing (freed after each sample):\n");
    println!("| Document | serde_ucl | libucl |");
    println!("| --- | ---: | ---: |");
    for doc in &order {
        println!(
            "| {} | {} | {} |",
            label(doc),
            median("serde_ucl", "parse-nofree", doc),
            median("libucl", "parse-nofree", doc)
        );
    }
    println!();
    println!(
        "| Document | serde_ucl, owned | serde_ucl, borrowed | serde_json, owned | serde_json, borrowed |"
    );
    println!("| --- | ---: | ---: | ---: | ---: |");
    for doc in &order {
        if !values.contains_key(&("serde_json".into(), "typed".into(), doc.clone())) {
            continue;
        }
        println!(
            "| {} | {} | {} | {} | {} |",
            label(doc),
            median("serde_ucl", "typed", doc),
            median("serde_ucl", "typed-borrowed", doc),
            median("serde_json", "typed", doc),
            median("serde_json", "typed-borrowed", doc)
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("write") if args.len() == 2 => write(Path::new(&args[1])),
        Some("run") if (2..=3).contains(&args.len()) => {
            run(Path::new(&args[1]), args.get(2).map(Path::new))
        }
        Some("summarize") if args.len() >= 2 => {
            summarize(&args[1..].iter().map(PathBuf::from).collect::<Vec<_>>())
        }
        _ => {
            eprintln!("usage: bench-compare write DIR | run DIR [CORPUS] | summarize RESULTS...");
            std::process::exit(2);
        }
    }
}

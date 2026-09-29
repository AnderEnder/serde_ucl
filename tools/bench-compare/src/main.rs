//! Times serde_ucl and serde_json on the benchmarks' documents, for the README's comparison with
//! libucl and serde_json (`scripts/bench-compare.sh` runs it in turns with `lucl.c`).
//!
//! ```text
//! bench-compare write DIR                  write the generated documents into DIR
//! bench-compare run DIR [CORPUS] [JSON_CORPUS]
//!                                         time the generated, rspamd and available JSON documents
//! bench-compare summarize RESULTS...       median tables from the lines of the runs
//! ```
//!
//! Every line of `run` is `tool|kind|document|seconds`: `kind` is `parse` (into the library's
//! value tree, freed each time), `parse-nofree` (the values kept and freed after the timing),
//! `typed` and `typed-borrowed` (`from_str` into a struct whose strings are owned or borrowed).
//!
//! `BENCH_COMPARE_LABEL` names the serde_ucl this binary was built with (default `serde_ucl`).
//! `scripts/bench-compare.sh` builds a second copy against the previous release and runs it as
//! `serde_ucl@VERSION`; that copy leaves serde_json out, which the first one times.

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
    let mut docs = vec![
        ("json-10000.json", common::json(10000)),
        ("json-1000.json", common::json(1000)),
        ("config-1000.ucl", common::config(1000)),
        ("small.ucl", common::SMALL.to_string()),
        (
            "small.json",
            r#"{"name": "svc", "port": 8080, "debug": true}"#.to_string(),
        ),
    ];
    for (name, seed, size) in common::IRREGULAR {
        docs.push((
            match name {
                "60k" => "irregular-60k.ucl",
                "600k" => "irregular-600k.ucl",
                _ => unreachable!("benchmark irregular document"),
            },
            common::irregular(seed, size),
        ));
    }
    docs
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

fn run_value(ucl: &str, json: bool, name: &str, doc: &str) {
    let t = time(|| serde_ucl::parse::parse(black_box(doc.as_bytes())).is_ok());
    println!("{ucl}|parse|{name}|{t:.9}");
    let t = time_keep(|| serde_ucl::parse::parse(black_box(doc.as_bytes())).unwrap());
    println!("{ucl}|parse-nofree|{name}|{t:.9}");
    if json && name.ends_with(".json") {
        let t = time(|| serde_json::from_str::<serde_json::Value>(black_box(doc)).is_ok());
        println!("serde_json|parse|{name}|{t:.9}");
    }
}

fn run(dir: &Path, corpus: Option<&Path>, json_corpus: Option<&Path>) {
    let ucl = std::env::var("BENCH_COMPARE_LABEL").unwrap_or_else(|_| "serde_ucl".into());
    // A copy built against an earlier serde_ucl times only that serde_ucl.
    let json = !ucl.contains('@');
    for (name, _) in documents() {
        let doc = read(&dir.join(name));
        run_value(&ucl, json, name, &doc);
        if name.starts_with("json") {
            let t = time(|| serde_ucl::from_str::<Items>(black_box(&doc)).is_ok());
            println!("{ucl}|typed|{name}|{t:.9}");
            let t = time(|| serde_ucl::from_str::<ItemsBorrowed>(black_box(&doc)).is_ok());
            println!("{ucl}|typed-borrowed|{name}|{t:.9}");
            if json {
                let t = time(|| serde_json::from_str::<Items>(black_box(&doc)).is_ok());
                println!("serde_json|typed|{name}|{t:.9}");
                let t = time(|| serde_json::from_str::<ItemsBorrowed>(black_box(&doc)).is_ok());
                println!("serde_json|typed-borrowed|{name}|{t:.9}");
            }
        }
        if name.starts_with("small") {
            let t = time(|| serde_ucl::from_str::<Small>(black_box(&doc)).is_ok());
            println!("{ucl}|typed|{name}|{t:.9}");
            let t = time(|| serde_ucl::from_str::<SmallBorrowed>(black_box(&doc)).is_ok());
            println!("{ucl}|typed-borrowed|{name}|{t:.9}");
            if json && name.ends_with(".json") {
                let t = time(|| serde_json::from_str::<Small>(black_box(&doc)).is_ok());
                println!("serde_json|typed|{name}|{t:.9}");
                let t = time(|| serde_json::from_str::<SmallBorrowed>(black_box(&doc)).is_ok());
                println!("serde_json|typed-borrowed|{name}|{t:.9}");
            }
        }
    }
    if let Some(json_corpus) = json_corpus {
        for name in common::JSON_DOCUMENTS {
            let filename = format!("{name}.json");
            let path = json_corpus.join(&filename);
            match std::fs::read_to_string(&path) {
                Ok(doc) => run_value(&ucl, json, &filename, &doc),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    eprintln!("bench-compare: skipping missing {}", path.display());
                }
                Err(e) => panic!("{}: {e}", path.display()),
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
        println!("{ucl}|parse|{name}|{t:.9}");
        let t = time_keep(|| parse().unwrap());
        println!("{ucl}|parse-nofree|{name}|{t:.9}");
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
        "irregular-60k.ucl" => "irregular configuration, 78,466 bytes".into(),
        "irregular-600k.ucl" => "irregular configuration, 639,049 bytes".into(),
        "twitter.json" => "twitter.json".into(),
        "citm_catalog.json" => "citm_catalog.json".into(),
        "canada.json" => "canada.json".into(),
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
    let median = |tool: &str, kind: &str, doc: &str| -> Option<f64> {
        let mut v = values.get(&(tool.into(), kind.into(), doc.into()))?.clone();
        v.sort_by(f64::total_cmp);
        Some(if v.len() % 2 == 1 {
            v[v.len() / 2]
        } else {
            (v[v.len() / 2 - 1] + v[v.len() / 2]) / 2.0
        })
    };
    let time = |tool: &str, kind: &str, doc: &str| -> String {
        median(tool, kind, doc).map_or_else(|| "–".into(), format_time)
    };
    // This serde_ucl against the previous one: the change in time.
    let change = |base: &str, kind: &str, doc: &str| -> String {
        match (median("serde_ucl", kind, doc), median(base, kind, doc)) {
            (Some(now), Some(before)) => {
                let percent = (now / before - 1.0) * 100.0;
                let sign = if percent < 0.0 { "−" } else { "+" };
                format!("{sign}{:.0}%", percent.abs())
            }
            _ => "–".into(),
        }
    };
    // The previous release, when the script ran one: tools named `serde_ucl@VERSION`.
    let base = values
        .keys()
        .map(|(tool, _, _)| tool.as_str())
        .find(|tool| tool.starts_with("serde_ucl@"))
        .map(str::to_owned);
    let rounds = values.values().map(Vec::len).max().unwrap_or(0);
    let plural = if rounds == 1 { "" } else { "s" };
    println!("Medians over {rounds} round{plural}; each time includes freeing the result.\n");
    match &base {
        Some(base) => {
            let version = &base["serde_ucl@".len()..];
            println!("| Document | serde_ucl | {version} | change | libucl | serde_json |");
            println!("| --- | ---: | ---: | ---: | ---: | ---: |");
            for doc in &order {
                println!(
                    "| {} | {} | {} | {} | {} | {} |",
                    label(doc),
                    time("serde_ucl", "parse", doc),
                    time(base, "parse", doc),
                    change(base, "parse", doc),
                    time("libucl", "parse", doc),
                    time("serde_json", "parse", doc)
                );
            }
        }
        None => {
            println!("| Document | serde_ucl | libucl | serde_json |");
            println!("| --- | ---: | ---: | ---: |");
            for doc in &order {
                println!(
                    "| {} | {} | {} | {} |",
                    label(doc),
                    time("serde_ucl", "parse", doc),
                    time("libucl", "parse", doc),
                    time("serde_json", "parse", doc)
                );
            }
        }
    }
    println!("\nThe same without freeing the value in the timing (freed after each sample):\n");
    match &base {
        Some(base) => {
            let version = &base["serde_ucl@".len()..];
            println!("| Document | serde_ucl | {version} | change | libucl |");
            println!("| --- | ---: | ---: | ---: | ---: |");
            for doc in &order {
                println!(
                    "| {} | {} | {} | {} | {} |",
                    label(doc),
                    time("serde_ucl", "parse-nofree", doc),
                    time(base, "parse-nofree", doc),
                    change(base, "parse-nofree", doc),
                    time("libucl", "parse-nofree", doc)
                );
            }
        }
        None => {
            println!("| Document | serde_ucl | libucl |");
            println!("| --- | ---: | ---: |");
            for doc in &order {
                println!(
                    "| {} | {} | {} |",
                    label(doc),
                    time("serde_ucl", "parse-nofree", doc),
                    time("libucl", "parse-nofree", doc)
                );
            }
        }
    }
    println!();
    let typed: Vec<&String> = order
        .iter()
        .filter(|doc| values.contains_key(&("serde_json".into(), "typed".into(), (*doc).clone())))
        .collect();
    match &base {
        Some(base) => {
            let version = &base["serde_ucl@".len()..];
            println!(
                "| Document | serde_ucl, owned | {version}, owned | change | serde_ucl, borrowed | {version}, borrowed | change | serde_json, owned | serde_json, borrowed |"
            );
            println!("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
            for doc in typed {
                println!(
                    "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                    label(doc),
                    time("serde_ucl", "typed", doc),
                    time(base, "typed", doc),
                    change(base, "typed", doc),
                    time("serde_ucl", "typed-borrowed", doc),
                    time(base, "typed-borrowed", doc),
                    change(base, "typed-borrowed", doc),
                    time("serde_json", "typed", doc),
                    time("serde_json", "typed-borrowed", doc)
                );
            }
        }
        None => {
            println!(
                "| Document | serde_ucl, owned | serde_ucl, borrowed | serde_json, owned | serde_json, borrowed |"
            );
            println!("| --- | ---: | ---: | ---: | ---: |");
            for doc in typed {
                println!(
                    "| {} | {} | {} | {} | {} |",
                    label(doc),
                    time("serde_ucl", "typed", doc),
                    time("serde_ucl", "typed-borrowed", doc),
                    time("serde_json", "typed", doc),
                    time("serde_json", "typed-borrowed", doc)
                );
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("write") if args.len() == 2 => write(Path::new(&args[1])),
        Some("run") if (2..=4).contains(&args.len()) => run(
            Path::new(&args[1]),
            args.get(2).map(Path::new),
            args.get(3).map(Path::new),
        ),
        Some("summarize") if args.len() >= 2 => {
            summarize(&args[1..].iter().map(PathBuf::from).collect::<Vec<_>>())
        }
        _ => {
            eprintln!(
                "usage: bench-compare write DIR | run DIR [CORPUS] [JSON_CORPUS] | summarize RESULTS..."
            );
            std::process::exit(2);
        }
    }
}

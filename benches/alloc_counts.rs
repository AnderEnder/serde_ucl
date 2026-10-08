//! Allocation counts (clean-room work item C16, task P1). Not a timing benchmark: for each
//! document and entry point it runs one call, the drop of its result included, under a counting
//! global allocator, and prints a Markdown table of:
//!
//! - `allocs`, `reallocs`, `frees`: calls of the allocator;
//! - `bytes`: bytes requested, the sizes of allocations and the new sizes of reallocations;
//! - `peak`: the most bytes live at once during the call, above those live before it;
//! - `kept`: the change in live bytes over the call: 0 when the call leaves as much allocated
//!   as it found, as a reused parser does once it holds what one parse leaves it;
//! - `allocs/KB`: allocations per 1000 input bytes.
//!
//! The counts are deterministic: they do not depend on the machine's load.
//!
//! ```text
//! cargo bench --bench alloc_counts               # every document
//! cargo bench --bench alloc_counts -- twitter    # the documents whose name contains `twitter`
//! ```
//!
//! The entry points: `parse`, a second parse with the same `Parser` (`parse::Parser::parse`);
//! `parse-new`, making a parser and parsing; `parser holds`, whose `kept` is what a parser holds
//! after one parse whose result was dropped (its output facts, for example), and whose other
//! columns are those of making the parser and that parse; and `from_str` into `UclValue`, the
//! document's typed
//! form where it has one (`typed`), and `IgnoredAny`. The rspamd documents go through
//! `UclDeserializer::from_parser` with the parser their check uses, made in the call. The
//! documents are those of WORKLIST C16 P1, then the compact JSON documents and the workloads
//! (`parse` only).

mod common;

use serde::Deserialize;
use serde::de::{DeserializeOwned, IgnoredAny};
use serde_ucl::parse::Parser;
use serde_ucl::{ParserFlags, UclDeserializer, UclValue};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// The system allocator, counting.
struct Counting;

static ALLOCS: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
static FREES: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE: AtomicU64 = AtomicU64::new(0);
static PEAK: AtomicU64 = AtomicU64::new(0);

fn grow(by: usize) {
    let live = LIVE.fetch_add(by as u64, Relaxed) + by as u64;
    PEAK.fetch_max(live, Relaxed);
}

// SAFETY: every method forwards to `System` with the arguments it was given, so the contract of
// `GlobalAlloc` holds as it does for `System`; the counters do not allocate.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(layout.size() as u64, Relaxed);
        grow(layout.size());
        // SAFETY: forwarded as it is.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(layout.size() as u64, Relaxed);
        grow(layout.size());
        // SAFETY: forwarded as it is.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        FREES.fetch_add(1, Relaxed);
        LIVE.fetch_sub(layout.size() as u64, Relaxed);
        // SAFETY: forwarded as it is.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        REALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(new_size as u64, Relaxed);
        LIVE.fetch_sub(layout.size() as u64, Relaxed);
        grow(new_size);
        // SAFETY: forwarded as it is.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// What one call did.
struct Counts {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    bytes: u64,
    peak: u64,
    kept: i64,
}

/// Runs `f` and drops its result, counting.
fn count<R>(f: impl FnOnce() -> R) -> Counts {
    let (allocs, reallocs, frees, bytes) = (
        ALLOCS.load(Relaxed),
        REALLOCS.load(Relaxed),
        FREES.load(Relaxed),
        BYTES.load(Relaxed),
    );
    let live = LIVE.load(Relaxed);
    PEAK.store(live, Relaxed);
    drop(black_box(f()));
    Counts {
        allocs: ALLOCS.load(Relaxed) - allocs,
        reallocs: REALLOCS.load(Relaxed) - reallocs,
        frees: FREES.load(Relaxed) - frees,
        bytes: BYTES.load(Relaxed) - bytes,
        peak: PEAK.load(Relaxed) - live,
        kept: LIVE.load(Relaxed) as i64 - live as i64,
    }
}

fn row(document: &str, size: usize, entry: &str, counts: &Counts) {
    println!(
        "| {document} | {size} | {entry} | {} | {} | {} | {} | {} | {} | {:.2} |",
        counts.allocs,
        counts.reallocs,
        counts.frees,
        counts.bytes,
        counts.peak,
        counts.kept,
        counts.allocs as f64 * 1000.0 / size as f64
    );
}

/// The parse rows of a document parsed with `parser`, which `new` makes.
fn parse_rows(name: &str, text: &str, size: usize, new: &dyn Fn() -> Parser) {
    let mut parser = new();
    parser.parse(text.as_bytes()).unwrap();
    let counts = count(|| parser.parse(text.as_bytes()).unwrap());
    row(name, size, "parse", &counts);
    let counts = count(|| new().parse(text.as_bytes()).unwrap());
    row(name, size, "parse-new", &counts);
    let mut held = None;
    let counts = count(|| {
        let mut parser = new();
        parser.parse(text.as_bytes()).unwrap();
        held = Some(parser);
    });
    row(name, size, "parser holds", &counts);
    drop(held);
}

/// The `from_str` rows of a document, with its typed form `T` if it has one.
fn from_str_rows<T: DeserializeOwned>(name: &str, text: &str, typed: bool) {
    let size = text.len();
    let counts = count(|| serde_ucl::from_str::<UclValue>(text).unwrap());
    row(name, size, "from_str UclValue", &counts);
    if typed {
        let counts = count(|| serde_ucl::from_str::<T>(text).unwrap());
        row(name, size, "from_str typed", &counts);
    }
    let counts = count(|| serde_ucl::from_str::<IgnoredAny>(text).unwrap());
    row(name, size, "from_str IgnoredAny", &counts);
}

/// A document of WORKLIST C16 P1: its parse and `from_str` rows.
fn document<T: DeserializeOwned>(name: &str, text: &str, typed: bool) {
    parse_rows(name, text, text.len(), &Parser::new);
    from_str_rows::<T>(name, text, typed);
}

/// The options of criterion's command line that take a value, which `cargo bench -- …` passes to
/// every bench, this one included.
const VALUE_OPTIONS: [&str; 19] = [
    "-c",
    "--color",
    "-s",
    "--save-baseline",
    "-b",
    "--baseline",
    "--baseline-lenient",
    "--format",
    "--profile-time",
    "--load-baseline",
    "--sample-size",
    "--warm-up-time",
    "--measurement-time",
    "--nresamples",
    "--noise-threshold",
    "--confidence-level",
    "--significance-level",
    "--plotting-backend",
    "--output-format",
];

/// The filters among the arguments: those that are neither an option (`--bench`, which `cargo
/// bench` passes, or one of criterion's) nor the value of one. A criterion filter is a filter
/// here too: it selects the documents whose name contains it.
fn filters(mut args: impl Iterator<Item = String>) -> Vec<String> {
    let mut filters = Vec::new();
    while let Some(arg) = args.next() {
        if !arg.starts_with('-') {
            filters.push(arg);
        } else if !arg.contains('=') && VALUE_OPTIONS.contains(&arg.as_str()) {
            args.next();
        }
    }
    filters
}

fn main() {
    let filters = filters(std::env::args().skip(1));
    let matched = std::cell::Cell::new(false);
    let wanted = |name: &str| {
        let wanted = filters.is_empty() || filters.iter().any(|f| name.contains(f));
        matched.set(matched.get() || wanted);
        wanted
    };
    println!(
        "| document | input bytes | entry point | allocs | reallocs | frees | bytes | peak | \
         kept | allocs/KB |"
    );
    println!("| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");

    if wanted("small") {
        document::<common::Small>("small", common::SMALL, true);
    }
    if wanted("config-1000") {
        document::<common::Config>("config-1000", &common::config(1000), true);
    }
    if wanted("json-1000") {
        document::<common::typed::JsonItems>("json-1000", &common::json(1000), true);
    }
    for (name, seed, size) in common::IRREGULAR {
        let name = format!("irregular-{name}");
        if wanted(&name) {
            document::<IgnoredAny>(&name, &common::irregular(seed, size), false);
        }
    }
    let json_documents = common::json_documents("alloc_counts");
    for doc in &json_documents {
        if !wanted(&doc.name) {
            continue;
        }
        match doc.name.as_str() {
            "twitter" => document::<common::typed::Twitter>(&doc.name, &doc.text, true),
            "citm_catalog" => document::<common::typed::CitmCatalog>(&doc.name, &doc.text, true),
            "canada" => document::<common::typed::Canada>(&doc.name, &doc.text, true),
            _ => document::<IgnoredAny>(&doc.name, &doc.text, false),
        }
    }
    for corpus in common::corpus_documents("alloc_counts") {
        let name = corpus.document.name;
        if !wanted(name) {
            continue;
        }
        let size = corpus.bytes_read as usize;
        parse_rows(name, &corpus.text, size, &|| corpus.parser());
        let input = corpus.text.as_bytes();
        let counts = count(|| {
            UclValue::deserialize(UclDeserializer::from_parser(corpus.parser(), input)).unwrap()
        });
        row(name, size, "UclDeserializer UclValue", &counts);
        let counts = count(|| {
            IgnoredAny::deserialize(UclDeserializer::from_parser(corpus.parser(), input)).unwrap()
        });
        row(name, size, "UclDeserializer IgnoredAny", &counts);
    }

    // The compact JSON documents and the workloads, parse only.
    let mut compact = vec![("json-1000".to_string(), common::json(1000))];
    compact.extend(json_documents.into_iter().map(|d| (d.name, d.text)));
    for (name, text) in compact {
        let name = format!("parse/json-compact/{name}");
        if wanted(&name) {
            let text = common::compact_json(&text);
            parse_rows(&name, &text, text.len(), &Parser::new);
        }
    }
    for workload in common::workloads() {
        let name = workload.id();
        if wanted(&name) {
            let flags: ParserFlags = workload.flags;
            parse_rows(&name, &workload.text, workload.text.len(), &|| {
                Parser::with_flags(flags)
            });
        }
    }
    if !matched.get() {
        eprintln!("alloc_counts: no document's name contains any of {filters:?}");
    }
}

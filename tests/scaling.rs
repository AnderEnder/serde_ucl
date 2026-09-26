//! Parse, emit and serde time grow linearly with the input and with nesting depth, up to the
//! nesting limit (`parse::MAX_NESTING`).
//!
//! This is the only timing-based test (WORKLIST C6 and C8c; the benches in `benches/` measure
//! time but check nothing). What it checks, that no input shape makes the work grow faster than
//! the input, can be observed only through time: every other test checks results, which a
//! quadratic implementation gets right too. Everything else, limits and stack use
//! (`tests/stack_depth.rs`) included, is tested without a clock.
//!
//! Each check times one operation on two documents of the same shape, one about eight times the
//! size of the other (for nested documents, nested about eight times as deep), in the same test
//! run, and compares the growth of the time with the growth of the size. Linear growth gives a
//! time ratio near the size ratio, quadratic growth near its square; a check fails above three
//! times the size ratio. There is no fixed time limit, so a slow machine passes as long as it is
//! slow for both documents. For output in the indented formats, whose size grows with depth
//! times lines, the size is the output's.
//!
//! Shared CI runners are noisy: other jobs take CPU time at random moments. So that noise cannot
//! decide a check:
//!
//! - each time is the fastest of `SAMPLES` samples, the two documents taking turns, and each
//!   sample repeats the operation for at least `SAMPLE_TIME`, so a burst of load slows some
//!   samples and the fastest one is kept;
//! - a check that fails is measured again, and fails the test only if all `ATTEMPTS` fail. A
//!   quadratic slowdown gives a ratio near 64 against a limit near 24 and fails every attempt;
//!   noise rarely triples a ratio three times in a row;
//! - the tests of this file run one at a time (`timed`), since `cargo test` runs the tests of a
//!   binary in parallel and they would take CPU time from one another's samples. They share no
//!   state and pass in any order.
//!
//! `timed` also runs each test on a thread with a large stack. The nested documents are nearly
//! 1000 levels deep, and stack use is not what this file measures: `tests/stack_depth.rs` checks
//! that every entry point fits in a 2 MiB stack, and a large one here keeps a stack limit from
//! failing a test about time.

use serde_ucl::emit::Format;
use serde_ucl::parse::Parser;
use serde_ucl::{DuplicateStrategy, ParserFlags, UclValue};
use std::fmt::Write;
use std::hint::black_box;
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

const SMALL: usize = 120;
const LARGE: usize = 960;
/// A time ratio above this many times the size ratio fails an attempt.
const MARGIN: f64 = 3.0;
/// Samples per document and attempt; the fastest counts.
const SAMPLES: usize = 7;
/// The least time one sample repeats the operation for.
const SAMPLE_TIME: Duration = Duration::from_millis(5);
/// Attempts per check; the check fails only if every attempt does.
const ATTEMPTS: usize = 3;
/// The stack of the thread each test runs on.
const STACK_SIZE: usize = 64 << 20;

/// Held by the test that is running, so that the tests of this file run one at a time.
static RUNNING: Mutex<()> = Mutex::new(());

/// Runs `test` while no other test of this file runs, on a thread with a `STACK_SIZE` stack.
fn timed(test: impl FnOnce() + Send) {
    // A test that failed leaves the lock poisoned; the others still run.
    let _running = RUNNING.lock().unwrap_or_else(PoisonError::into_inner);
    thread::scope(|scope| {
        let handle = thread::Builder::new()
            .stack_size(STACK_SIZE)
            .spawn_scoped(scope, test)
            .unwrap();
        if let Err(panic) = handle.join() {
            std::panic::resume_unwind(panic);
        }
    });
}

/// The time of one run of `f`: runs it until at least `SAMPLE_TIME` has passed.
fn sample(f: &mut dyn FnMut()) -> Duration {
    let start = Instant::now();
    let mut runs = 0u32;
    loop {
        f();
        runs += 1;
        let elapsed = start.elapsed();
        if elapsed >= SAMPLE_TIME {
            return elapsed / runs;
        }
    }
}

/// The fastest samples of `small` and `large`, taken in turns.
fn fastest(small: &mut dyn FnMut(), large: &mut dyn FnMut()) -> (Duration, Duration) {
    let (mut best_small, mut best_large) = (Duration::MAX, Duration::MAX);
    for _ in 0..SAMPLES {
        best_small = best_small.min(sample(small));
        best_large = best_large.min(sample(large));
    }
    (best_small, best_large)
}

/// Fails if, in every one of `ATTEMPTS` attempts, `large` takes more than `MARGIN` times
/// `size_ratio` as long as `small`.
fn check_growth(what: &str, size_ratio: f64, mut small: impl FnMut(), mut large: impl FnMut()) {
    // Warm up both.
    small();
    large();
    let mut failed = Vec::new();
    for _ in 0..ATTEMPTS {
        let (best_small, best_large) = fastest(&mut small, &mut large);
        let ratio = best_large.as_secs_f64() / best_small.as_secs_f64();
        if ratio < MARGIN * size_ratio {
            return;
        }
        failed.push(format!("{ratio:.1} ({best_small:?} and {best_large:?})"));
    }
    panic!(
        "{what}: {size_ratio:.1} times the size took more than {:.1} times as long in each of \
         {ATTEMPTS} attempts: {}",
        MARGIN * size_ratio,
        failed.join(", ")
    );
}

fn ratio(small: usize, large: usize) -> f64 {
    large as f64 / small as f64
}

/// Objects nested `depth` deep, one per line. Each has a comment, a number, a single-quoted
/// string, a key that needs quoting and a heredoc (output facts, spec §10.1), and inherits a
/// small object (spec §9.7).
fn nested_objects(depth: usize) -> String {
    let mut out = String::from("base { shared = 1 }\n");
    for i in 0..depth {
        writeln!(
            out,
            "l{i} {{ # level {i}\nn = {i}; s = 'q{i}'; \"k {i}\" = 1; .inherit \"base\"\nh = <<EOD\nx\nEOD"
        )
        .unwrap();
    }
    out.push_str("leaf = 1;\n");
    for _ in 0..depth {
        out.push_str("}\n");
    }
    out
}

/// Arrays nested `depth` deep.
fn nested_arrays(depth: usize) -> String {
    format!("a = {}1{}\n", "[".repeat(depth), "]".repeat(depth))
}

/// A section path of `depth` names on one line (spec §3.4).
fn section_path(depth: usize) -> String {
    let mut out = String::new();
    for i in 0..depth {
        write!(out, "n{i} ").unwrap();
    }
    out.push_str("{ x = 1 }\n");
    out
}

fn parse(flags: ParserFlags, input: &str) -> (Parser, UclValue) {
    let mut parser = Parser::with_flags(flags);
    let value = parser.parse(input.as_bytes()).unwrap();
    (parser, value)
}

fn check_parse(what: &str, flags: ParserFlags, small: &str, large: &str) {
    check_parse_with(what, || Parser::with_flags(flags), small, large);
}

fn check_parse_with(what: &str, parser: impl Fn() -> Parser, small: &str, large: &str) {
    let (mut p, mut q) = (parser(), parser());
    check_growth(
        what,
        ratio(small.len(), large.len()),
        || drop(black_box(p.parse(small.as_bytes()).unwrap())),
        || drop(black_box(q.parse(large.as_bytes()).unwrap())),
    );
}

#[test]
fn parse_time_grows_linearly() {
    timed(|| {
        let objects = (nested_objects(SMALL), nested_objects(LARGE));
        check_parse(
            "parse nested objects",
            ParserFlags::empty(),
            &objects.0,
            &objects.1,
        );
        check_parse(
            "parse nested objects, saving comments",
            ParserFlags::SAVE_COMMENTS,
            &objects.0,
            &objects.1,
        );
        let arrays = (nested_arrays(SMALL), nested_arrays(LARGE));
        check_parse(
            "parse nested arrays",
            ParserFlags::empty(),
            &arrays.0,
            &arrays.1,
        );
        let sections = (section_path(SMALL), section_path(LARGE));
        check_parse(
            "parse a section path",
            ParserFlags::empty(),
            &sections.0,
            &sections.1,
        );
    });
}

/// `keys` keys in one object, after a quoted key whose escape gives an uppercase letter, so that
/// under `KEY_LOWERCASE` every key is compared regardless of case (spec §12.1).
fn keys_after_uppercase_escape(keys: usize) -> String {
    let mut out = String::from("\"\\u0041\" = 1\n");
    for i in 0..keys {
        writeln!(out, "k{i} = {i}").unwrap();
    }
    out
}

/// `values` values of one key, each a single-quoted string (an output fact, spec §10.1) with a
/// comment.
fn many_values(values: usize) -> String {
    let mut out = String::new();
    for i in 0..values {
        writeln!(out, "# value {i}\nr = 'q{i}'").unwrap();
    }
    out
}

/// `keys` keys with a comment each, then `keys` repeats of one key with a comment each.
fn commented_repeats(keys: usize) -> String {
    let mut out = String::new();
    for i in 0..keys {
        writeln!(out, "# key {i}\nk{i} = {i}").unwrap();
    }
    for i in 0..keys {
        writeln!(out, "# repeat {i}\nr = {i}").unwrap();
    }
    out
}

#[test]
fn parse_time_grows_linearly_with_width() {
    timed(|| {
        let (small, large) = (
            keys_after_uppercase_escape(SMALL * 8),
            keys_after_uppercase_escape(LARGE * 8),
        );
        check_parse(
            "parse keys compared regardless of case",
            ParserFlags::KEY_LOWERCASE,
            &small,
            &large,
        );
        // Each repeat replaces the value before it, and the comments of that value go with it.
        let (small, large) = (commented_repeats(SMALL * 8), commented_repeats(LARGE * 8));
        let parser = || {
            let mut parser = Parser::with_flags(ParserFlags::SAVE_COMMENTS);
            parser.set_strategy(DuplicateStrategy::Rewrite);
            parser
        };
        check_parse_with(
            "parse replaced values with saved comments",
            parser,
            &small,
            &large,
        );
        let (small, large) = (many_values(SMALL * 8), many_values(LARGE * 8));
        check_parse(
            "parse many values of one key, saving comments",
            ParserFlags::SAVE_COMMENTS,
            &small,
            &large,
        );
        let (p, v) = parse(ParserFlags::SAVE_COMMENTS, &small);
        let (q, w) = parse(ParserFlags::SAVE_COMMENTS, &large);
        let e = p
            .emitter(Format::Config)
            .with_comments(p.comments(), p.attached_comments());
        let f = q
            .emitter(Format::Config)
            .with_comments(q.comments(), q.attached_comments());
        check_growth(
            "emit many values of one key with comments",
            ratio(e.emit(&v).len(), f.emit(&w).len()),
            || drop(black_box(e.emit(&v))),
            || drop(black_box(f.emit(&w))),
        );
    });
}

#[test]
fn emit_time_grows_linearly() {
    timed(|| {
        let (small, large) = (nested_objects(SMALL), nested_objects(LARGE));
        for flags in [ParserFlags::empty(), ParserFlags::SAVE_COMMENTS] {
            let (p, v) = parse(flags, &small);
            let (q, w) = parse(flags, &large);
            for format in [
                Format::JsonCompact,
                Format::Json,
                Format::Config,
                Format::Yaml,
            ] {
                // With the output facts of the parse, and with its saved comments if any.
                let (e, f) = (p.emitter(format), q.emitter(format));
                let (e, f) = if flags.is_empty() {
                    (e, f)
                } else {
                    (
                        e.with_comments(p.comments(), p.attached_comments()),
                        f.with_comments(q.comments(), q.attached_comments()),
                    )
                };
                let size_ratio = ratio(e.emit(&v).len(), f.emit(&w).len());
                check_growth(
                    &format!("emit {format:?} ({flags:?})"),
                    size_ratio,
                    || drop(black_box(e.emit(&v))),
                    || drop(black_box(f.emit(&w))),
                );
            }
        }
    });
}

#[test]
fn serde_time_grows_linearly() {
    timed(|| {
        let (small, large) = (nested_objects(SMALL), nested_objects(LARGE));
        let size_ratio = ratio(small.len(), large.len());
        check_growth(
            "from_str",
            size_ratio,
            || drop(black_box(serde_ucl::from_str::<UclValue>(&small).unwrap())),
            || drop(black_box(serde_ucl::from_str::<UclValue>(&large).unwrap())),
        );
        let (v, w) = (
            parse(ParserFlags::empty(), &small).1,
            parse(ParserFlags::empty(), &large).1,
        );
        check_growth(
            "from_value",
            size_ratio,
            || {
                drop(black_box(
                    serde_ucl::from_value::<UclValue>(v.clone()).unwrap(),
                ))
            },
            || {
                drop(black_box(
                    serde_ucl::from_value::<UclValue>(w.clone()).unwrap(),
                ))
            },
        );
        check_growth(
            "to_value",
            size_ratio,
            || drop(black_box(serde_ucl::to_value(&v).unwrap())),
            || drop(black_box(serde_ucl::to_value(&w).unwrap())),
        );
        let compact = ratio(
            serde_ucl::to_json_string_compact(&v).unwrap().len(),
            serde_ucl::to_json_string_compact(&w).unwrap().len(),
        );
        check_growth(
            "to_json_string_compact",
            compact,
            || drop(black_box(serde_ucl::to_json_string_compact(&v).unwrap())),
            || drop(black_box(serde_ucl::to_json_string_compact(&w).unwrap())),
        );
        let config = ratio(
            serde_ucl::to_string(&v).unwrap().len(),
            serde_ucl::to_string(&w).unwrap().len(),
        );
        check_growth(
            "to_string",
            config,
            || drop(black_box(serde_ucl::to_string(&v).unwrap())),
            || drop(black_box(serde_ucl::to_string(&w).unwrap())),
        );
    });
}

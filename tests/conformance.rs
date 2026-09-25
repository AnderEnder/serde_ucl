//! libucl conformance suite.
//!
//! Every case under `tests/conformance/` is parsed by this crate, and the result is compared with
//! libucl's typed dump of the same input (`<case>.golden.json`, produced by
//! `scripts/regen-golden.sh`). Expected values are never written by hand.
//!
//! The parser `ucl_lexer::parse` runs every case (`libucl_conformance_new_core`), with its known
//! failures in `tests/conformance/xfail-new.txt`. The runner applies each case's `.flags` file,
//! and gives the parser the case's directory as its base directory, so relative include paths
//! resolve as the oracle's do (spec §9.3) without changing the process's working directory. A
//! case is parsed as a file, which defines `FILENAME` and `CURDIR` from its path, unless its
//! `.flags` say `string-input` or `no-filevars`: then it is parsed as bytes, as the oracle parses
//! it.
//!
//! Each xfail file lists cases one per line: `<case-id> <reason>`. The run fails when an unlisted
//! case fails, and also when a listed case passes, so the lists can only shrink.
//!
//! A case whose golden file is an error passes only if the parser rejects the input. For the new
//! core, a rejection because the input uses something the project does not support
//! (`Error::is_unsupported`: `.includes`, `sign=true`) never counts as a pass. A case listed with
//! the reason `divergence:signature` must fail with exactly that error. A silent stop (spec §9.4,
//! `Error::is_stopped`) is not a rejection: its partial result is compared with the golden file.
//!
//! A case with a `.inputs` file is parsed as several inputs into one parser (spec §13.1): the
//! case's own file first, then each further input with its own priority and strategy, through
//! `Parser::inputs`. A silent stop in one input does not end the parse; an error in any input
//! makes the result an error. The flags `registered-macros` and `registered-priority-override`
//! register macros equivalent to the oracle's test macros (spec §13.2, *The test macros*).
//!
//! For cases with `dump-comments` in their `.flags`, the new core's dump also records the saved
//! comments attached to each value, as the oracle's does (`tests/conformance/README.md`): `"c"` for
//! comments attached before the value, `"ca"` for those attached after it.
//!
//! A second test (`libucl_conformance_emitters`) compares the new core's output formats (spec §10,
//! `ucl_lexer::emit`) byte for byte with libucl's: for every case that parses, the config, JSON,
//! compact JSON and YAML output with the golden files `<case>.<format>.golden`, the config output
//! with saved comments with `<case>.config-comments.golden` for cases that save comments, and for
//! upstream cases the config output of the two passes of spec §10.9 with the `.res` file. Its
//! known failures are in `tests/conformance/xfail-emit.txt`, one entry per case.
//!
//! A third test (`libucl_conformance_readback`) reads the new core's output back: for every case
//! that parses, the output in each of the four formats must parse again to the same value, apart
//! from the losses spec §10.8 lists. Differences that §10.8 does not list are held in
//! `READBACK_PENDING`, each with the question in `docs/clean-room/QUESTIONS.md` that asks about it.
//!
//! Set `UCL_CONFORMANCE_REPORT=1` (with `-- --nocapture`) to print every failing case with the
//! first point where the two dumps, or the two outputs, differ, and a suggested xfail reason; the
//! readback test also prints every difference and every output it lets through as unreadable.

#[path = "common/oracle.rs"]
mod oracle;

use oracle::{
    Setup, comment_map, configure, dump, find_diff, quietly, read_flags, strategy_named,
    strip_unobservable_priorities,
};
use serde_json::Value as J;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use ucl_lexer::emit::Format;
use ucl_lexer::parse::{FsLoader, Input, Parser as CoreParser, PathSegment};
use ucl_lexer::{DuplicateStrategy, ParserFlags, UclValue};

const CONFORMANCE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/conformance");

struct Case {
    id: String,
    input: PathBuf,
    golden: PathBuf,
    flags: Vec<String>,
    /// The further inputs of `<case>.inputs` (spec §13.1), in order.
    inputs: Vec<FurtherInput>,
}

/// One line of a `<case>.inputs` file: `MODE PRIORITY STRATEGY PATH`
/// (`tests/conformance/README.md`).
struct FurtherInput {
    /// `file`: added by its path; `chunk`: its bytes added as a document given as text.
    file: bool,
    priority: u8,
    strategy: DuplicateStrategy,
    path: PathBuf,
}

/// The further inputs listed in `path`, a `<case>.inputs` file, with paths relative to `dir`.
fn further_inputs(path: &Path, dir: &Path) -> Vec<FurtherInput> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .map(|line| line.split('#').next().unwrap().trim())
        .filter(|line| !line.is_empty())
        .map(|line| {
            let bad = || panic!("{}: bad input line '{line}'", path.display());
            let fields: Vec<&str> = line.split_whitespace().collect();
            let [mode, priority, strategy, file] = fields[..] else {
                bad()
            };
            let priority: u32 = priority.parse().unwrap_or_else(|_| bad());
            FurtherInput {
                file: match mode {
                    "file" => true,
                    "chunk" => false,
                    _ => bad(),
                },
                priority: (priority & 0x0f) as u8,
                strategy: strategy_named(strategy).unwrap_or_else(|| bad()),
                path: dir.join(file),
            }
        })
        .collect()
}

enum Outcome {
    Pass,
    Fail { hint: &'static str, detail: String },
}

fn discover() -> Vec<Case> {
    let root = Path::new(CONFORMANCE_DIR);
    let mut inputs = Vec::new();
    collect(&root.join("libucl/basic"), "in", false, &mut inputs);
    collect(&root.join("cases"), "ucl", true, &mut inputs);
    inputs.sort();
    inputs
        .into_iter()
        .map(|input| {
            let stem = input.file_stem().unwrap().to_string_lossy().to_string();
            let dir = input.parent().unwrap();
            let rel = dir
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let flags = read_flags(&dir.join(format!("{stem}.flags")));
            Case {
                id: format!("{rel}/{stem}"),
                golden: dir.join(format!("{stem}.golden.json")),
                inputs: further_inputs(&dir.join(format!("{stem}.inputs")), dir),
                input,
                flags,
            }
        })
        .collect()
}

fn collect(dir: &Path, ext: &str, recursive: bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if recursive {
                collect(&path, ext, recursive, out);
            }
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
}

/// What a parser made of a case: a dump, a rejection, or a reason why the case could not be run.
enum Parsed {
    Value(J),
    Rejected(String),
    /// The parser recognised the input but does not support it; never a pass.
    Unsupported(String),
}

/// The settings the case's `.flags` file asks for.
fn setup(case: &Case) -> Setup {
    Setup::from_flags(&case.flags).unwrap_or_else(|e| panic!("{}: {e}", case.id))
}

/// Parses the case's own file and then its further inputs with one parser (spec §13.1). A silent
/// stop ends only its input; the first error ends the parse.
fn run_inputs(
    parser: &mut CoreParser,
    case: &Case,
    as_bytes: bool,
) -> Result<UclValue, ucl_lexer::parse::Error> {
    let own = fs::read(&case.input).unwrap();
    let further: Vec<Vec<u8>> = case
        .inputs
        .iter()
        .map(|input| {
            if input.file {
                Vec::new()
            } else {
                fs::read(&input.path).unwrap()
            }
        })
        .collect();
    let mut inputs = parser.inputs();
    let first = if as_bytes {
        Input::bytes(&own)
    } else {
        Input::file(&case.input)
    };
    let mut all = vec![first];
    for (spec, bytes) in case.inputs.iter().zip(&further) {
        let input = if spec.file {
            Input::file(&spec.path)
        } else {
            Input::bytes(bytes)
        };
        all.push(
            input
                .with_priority(spec.priority)
                .with_strategy(spec.strategy),
        );
    }
    for input in all {
        match inputs.add(input) {
            Err(e) if !e.is_stopped() => return Err(e),
            _ => {}
        }
    }
    inputs.finish()
}

/// Runs the new core on a case, applying its `.flags` file as the oracle does
/// (`docs/spec/README.md`, "How the conformance oracle runs every case"), and its `.inputs`. A
/// silent stop gives its partial result. Returns the parser, for its saved comments and output
/// facts, and the result.
fn run_new_core(
    case: &Case,
    setup: &Setup,
) -> (CoreParser, Result<UclValue, ucl_lexer::parse::Error>) {
    let mut parser = configure(
        setup,
        case.input.parent().expect("a case is in a directory"),
    );
    // The oracle parses every case as a string, then defines FILENAME and CURDIR from the
    // case's path unless the case has `no-filevars` (spec §12.7). The core defines them
    // whenever it parses a file (project decision 5).
    let no_filevars = setup.flags.contains(ParserFlags::NO_FILEVARS);
    let as_bytes = setup.string_input || no_filevars;
    let result = if !case.inputs.is_empty() {
        run_inputs(&mut parser, case, as_bytes)
    } else if as_bytes {
        let bytes = fs::read(&case.input).unwrap();
        parser.parse(&bytes)
    } else {
        parser.parse_file(&case.input)
    };
    let result = match result {
        Err(e) if e.is_stopped() => Ok(e.into_partial().expect("a stop keeps its result")),
        other => other,
    };
    (parser, result)
}

/// Parses a case with the new core and dumps the result.
fn parse_with_new_core(case: &Case) -> Result<Parsed, &'static str> {
    let setup = setup(case);
    let result = quietly(|| {
        let (parser, result) = run_new_core(case, &setup);
        match result {
            Ok(v) => {
                let comments = setup.dump_comments.then(|| comment_map(&parser));
                Parsed::Value(dump(&v, comments.as_ref()))
            }
            Err(e) if e.is_unsupported() => Parsed::Unsupported(e.to_string()),
            Err(e) => Parsed::Rejected(e.to_string()),
        }
    });
    result.map_err(|_| "panic")
}

fn uses_macros(case: &Case) -> bool {
    let bytes = fs::read(&case.input).unwrap();
    String::from_utf8_lossy(&bytes).lines().any(|line| {
        let line = line.trim_start();
        [
            "include",
            "try_include",
            "includes",
            "priority",
            "load",
            "inherit",
            "emit",
            "seen",
            "fail",
            "ctx",
        ]
        .iter()
        .any(|m| {
            line.strip_prefix('.')
                .and_then(|rest| rest.strip_prefix(m))
                .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
        })
    })
}

fn evaluate(case: &Case, parse: fn(&Case) -> Result<Parsed, &'static str>) -> Outcome {
    let golden_text = match fs::read_to_string(&case.golden) {
        Ok(t) => t,
        Err(_) => {
            return Outcome::Fail {
                hint: "missing-golden",
                detail: "no golden file; run scripts/regen-golden.sh".into(),
            };
        }
    };
    let mut golden: J = serde_json::from_str(&golden_text)
        .unwrap_or_else(|e| panic!("{}: the golden file is not valid JSON: {e}", case.id));
    strip_unobservable_priorities(&mut golden, true);
    let golden_is_error = golden.get("error").is_some();

    let actual = match parse(case) {
        Err(hint) => {
            return Outcome::Fail {
                hint,
                detail: hint.to_string(),
            };
        }
        Ok(r) => r,
    };
    let macro_hint = if uses_macros(case) { "macro" } else { "parser" };

    match (golden_is_error, actual) {
        (_, Parsed::Unsupported(e)) => Outcome::Fail {
            hint: "unsupported",
            detail: format!("not supported: {e}"),
        },
        (true, Parsed::Rejected(_)) => Outcome::Pass,
        (true, Parsed::Value(value)) => Outcome::Fail {
            hint: "parser",
            detail: format!("libucl rejects the input; crate accepted it as {value}"),
        },
        (false, Parsed::Rejected(e)) => Outcome::Fail {
            hint: macro_hint,
            detail: format!("crate error: {e}"),
        },
        (false, Parsed::Value(value)) => match find_diff(&golden, &value, "$") {
            None => Outcome::Pass,
            Some(detail) => Outcome::Fail {
                hint: macro_hint,
                detail,
            },
        },
    }
}

fn load_xfail(file: &str) -> BTreeMap<String, String> {
    let text = fs::read_to_string(Path::new(CONFORMANCE_DIR).join(file))
        .unwrap_or_else(|e| panic!("cannot read {file}: {e}"));
    let mut map = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (id, reason) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let reason = reason.trim();
        assert!(
            !reason.is_empty(),
            "{file} line {}: entry '{id}' has no reason",
            n + 1
        );
        assert!(
            map.insert(id.to_string(), reason.to_string()).is_none(),
            "{file}: duplicate '{id}'"
        );
    }
    map
}

#[test]
fn libucl_conformance_new_core() {
    run_suite("new core", "xfail-new.txt", parse_with_new_core);
}

fn run_suite(label: &str, xfail_file: &str, parse: fn(&Case) -> Result<Parsed, &'static str>) {
    let cases = discover();
    assert!(
        !cases.is_empty(),
        "no conformance cases found under {CONFORMANCE_DIR}"
    );
    let xfail = load_xfail(xfail_file);
    let report = std::env::var_os("UCL_CONFORMANCE_REPORT").is_some();

    // Parser panics are caught per case, without their messages (`quietly`).
    let outcomes: Vec<(&Case, Outcome)> = cases.iter().map(|c| (c, evaluate(c, parse))).collect();

    let known: BTreeSet<&str> = cases.iter().map(|c| c.id.as_str()).collect();
    let mut problems = Vec::new();
    for id in xfail.keys() {
        if !known.contains(id.as_str()) {
            problems.push(format!("{xfail_file} lists unknown case '{id}'"));
        }
    }

    let mut passed = 0;
    let mut xfailed: BTreeMap<&str, usize> = BTreeMap::new();
    for (case, outcome) in &outcomes {
        let reason = xfail
            .get(&case.id)
            .map(|r| r.split([',', '#']).next().unwrap().trim());
        if reason == Some("divergence:signature")
            && !matches!(
                outcome,
                Outcome::Fail {
                    hint: "unsupported",
                    ..
                }
            )
        {
            // Signatures are never verified (project decision): these cases must fail with
            // the "unsupported" error, not with any other outcome.
            problems.push(format!(
                "{} is listed as divergence:signature but does not fail with the unsupported error",
                case.id
            ));
        }
        match (outcome, xfail.get(&case.id)) {
            (Outcome::Pass, None) => passed += 1,
            (Outcome::Pass, Some(_)) => problems.push(format!(
                "{} now passes: remove it from {xfail_file}",
                case.id
            )),
            (Outcome::Fail { hint, detail }, None) => problems.push(format!(
                "{} fails and is not in {xfail_file} (suggested reason: {hint})\n    {detail}",
                case.id
            )),
            (Outcome::Fail { .. }, Some(reason)) => {
                let key = reason.split([',', '#']).next().unwrap().trim();
                *xfailed.entry(key).or_default() += 1;
            }
        }
        if report && let Outcome::Fail { hint, detail } = outcome {
            println!("FAIL({label}) {} [{hint}]\n    {detail}", case.id);
        }
    }

    let total = outcomes.len();
    let xfail_total: usize = xfailed.values().sum();
    println!(
        "conformance ({label}): {total} cases, {passed} pass, {xfail_total} expected failures {xfailed:?}"
    );
    assert!(
        problems.is_empty(),
        "conformance problems ({label}):\n{}",
        problems.join("\n")
    );
}

// ----- output formats (spec §10) ------------------------------------------------------------

/// The output golden files of spec §10: the file suffix and the format.
const OUTPUT_FORMATS: [(&str, Format); 4] = [
    ("config", Format::Config),
    ("json", Format::Json),
    ("json-compact", Format::JsonCompact),
    ("yaml", Format::Yaml),
];

/// The label of the comparison with an upstream `.res` file (spec §10.9).
const RES: &str = "res";

/// Every output comparison of one case: the label, and `None` if the bytes match or the first
/// difference.
type OutputOutcome = Result<Vec<(&'static str, Option<String>)>, String>;

fn golden_path(case: &Case, suffix: &str) -> PathBuf {
    let stem = case.input.file_stem().unwrap().to_string_lossy();
    case.input.with_file_name(format!("{stem}.{suffix}.golden"))
}

/// Where two byte strings first differ, with some context from each.
fn first_difference(golden: &[u8], actual: &[u8]) -> String {
    let at = golden
        .iter()
        .zip(actual)
        .position(|(g, a)| g != a)
        .unwrap_or(golden.len().min(actual.len()));
    let context = |bytes: &[u8]| {
        let start = at.saturating_sub(40);
        let end = (at + 40).min(bytes.len());
        format!("{:?}", String::from_utf8_lossy(&bytes[start..end]))
    };
    format!(
        "offset {at} (lengths libucl {} crate {}): libucl {} crate {}",
        golden.len(),
        actual.len(),
        context(golden),
        context(actual)
    )
}

fn compare_bytes(golden: &[u8], actual: &[u8]) -> Option<String> {
    (golden != actual).then(|| first_difference(golden, actual))
}

/// The upstream `.res` file of a case, if it has one.
fn res_path(case: &Case) -> Option<PathBuf> {
    let path = case.input.with_extension("res");
    (case.input.extension().is_some_and(|e| e == "in") && path.exists()).then_some(path)
}

/// Repeats the two passes that produced an upstream `.res` file (spec §10.9) and returns the
/// second pass's config output, with its extra final line break.
fn two_pass_config(case: &Case) -> Result<String, String> {
    let dir = case.input.parent().expect("a case is in a directory");
    // `libucl/basic/14` includes `./1.in` with `try=true`; its `.res` was produced where that
    // path does not exist (spec §10.9).
    let base = if case.id == "libucl/basic/14" {
        Path::new(CONFORMANCE_DIR).to_path_buf()
    } else {
        dir.to_path_buf()
    };
    assert!(
        case.id != "libucl/basic/14" || !base.join("1.in").exists(),
        "the directory for case 14 must not hold 1.in"
    );
    let mut first = CoreParser::with_flags(ParserFlags::KEY_LOWERCASE);
    first.set_loader(FsLoader::new()).set_base_dir(&base);
    first.register_variable("ABI", "unknown");
    let value = match first.parse_file(&case.input) {
        Err(e) if e.is_stopped() => e.into_partial().expect("a stop keeps its result"),
        other => other.map_err(|e| format!("first pass: {e}"))?,
    };
    let text = first.emitter(Format::Config).emit(&value);
    let mut second = CoreParser::with_flags(ParserFlags::KEY_LOWERCASE);
    second.set_loader(FsLoader::new()).set_base_dir(&base);
    let value = second
        .parse(text.as_bytes())
        .map_err(|e| format!("second pass: {e}"))?;
    let mut out = second.emitter(Format::Config).emit(&value);
    out.push('\n');
    Ok(out)
}

/// Runs a case through the new core and compares its output in every format that has a golden
/// file, and its `.res` file if it is an upstream case.
fn evaluate_outputs(case: &Case) -> Option<OutputOutcome> {
    let golden_text = fs::read_to_string(&case.golden).ok()?;
    let golden: J = serde_json::from_str(&golden_text)
        .unwrap_or_else(|e| panic!("{}: the golden file is not valid JSON: {e}", case.id));
    let setup = setup(case);
    let mut expected: Vec<(&'static str, PathBuf)> = OUTPUT_FORMATS
        .iter()
        .map(|(suffix, _)| (*suffix, golden_path(case, suffix)))
        .collect();
    if setup.save_comments {
        expected.push(("config-comments", golden_path(case, "config-comments")));
    }
    if golden.get("error").is_some() {
        for (_, path) in &expected {
            assert!(
                !path.exists(),
                "{}: an error case has output golden file {}",
                case.id,
                path.display()
            );
        }
        return None;
    }
    for (_, path) in &expected {
        assert!(
            path.exists(),
            "{}: missing output golden file {}; run scripts/regen-golden.sh",
            case.id,
            path.display()
        );
    }
    let result = quietly(|| -> OutputOutcome {
        let (parser, result) = run_new_core(case, &setup);
        let value = result.map_err(|e| format!("crate error: {e}"))?;
        let mut outcomes = Vec::new();
        for (label, path) in &expected {
            let golden = fs::read(path).unwrap();
            let emitter = match *label {
                "config-comments" => parser
                    .emitter(Format::Config)
                    .with_comments(parser.comments(), parser.attached_comments()),
                _ => {
                    let format = OUTPUT_FORMATS
                        .iter()
                        .find(|(suffix, _)| suffix == label)
                        .expect("a known format")
                        .1;
                    parser.emitter(format)
                }
            };
            let actual = emitter.emit(&value);
            outcomes.push((*label, compare_bytes(&golden, actual.as_bytes())));
        }
        if let Some(res) = res_path(case) {
            let golden = fs::read(res).unwrap();
            let outcome = match two_pass_config(case) {
                Ok(actual) => compare_bytes(&golden, actual.as_bytes()),
                Err(e) => Some(e),
            };
            outcomes.push((RES, outcome));
        }
        Ok(outcomes)
    });
    Some(result.unwrap_or_else(|_| Err("panic".to_string())))
}

/// Every case that parses has libucl's output in each format of spec §10 (`<case>.<format>.golden`,
/// and `<case>.config-comments.golden` for cases that save comments). The new core's emitters must
/// write the same bytes, and an upstream case's config output must reproduce its `.res` file by
/// the two-pass procedure of spec §10.9. Known failures are in `tests/conformance/xfail-emit.txt`.
#[test]
fn libucl_conformance_emitters() {
    let cases = discover();
    let xfail = load_xfail("xfail-emit.txt");
    let report = std::env::var_os("UCL_CONFORMANCE_REPORT").is_some();
    let outcomes: Vec<(&Case, OutputOutcome)> = cases
        .iter()
        .filter_map(|c| evaluate_outputs(c).map(|o| (c, o)))
        .collect();

    let known: BTreeSet<&str> = cases.iter().map(|c| c.id.as_str()).collect();
    let parse_failures = load_xfail("xfail-new.txt");
    let mut problems = Vec::new();
    for id in xfail.keys() {
        if !known.contains(id.as_str()) {
            problems.push(format!("xfail-emit.txt lists unknown case '{id}'"));
        }
        // Only a case the core does not parse may be listed, so an emitter difference cannot
        // hide behind a divergence label.
        if !parse_failures.contains_key(id) {
            problems.push(format!(
                "xfail-emit.txt lists '{id}', which is not a known failure of the new core"
            ));
        }
    }
    let mut matched: BTreeMap<&str, usize> = BTreeMap::new();
    let mut compared: BTreeMap<&str, usize> = BTreeMap::new();
    let mut passed = 0;
    let mut xfailed = 0;
    for (case, outcome) in &outcomes {
        let failures: Vec<String> = match outcome {
            Err(e) => vec![format!("all formats: {e}")],
            Ok(results) => {
                for (label, diff) in results {
                    *compared.entry(label).or_default() += 1;
                    if diff.is_none() {
                        *matched.entry(label).or_default() += 1;
                    }
                }
                results
                    .iter()
                    .filter_map(|(label, diff)| diff.as_ref().map(|d| format!("{label}: {d}")))
                    .collect()
            }
        };
        if report {
            for failure in &failures {
                println!("FAIL(emit) {}\n    {failure}", case.id);
            }
        }
        match (failures.is_empty(), xfail.contains_key(&case.id)) {
            (true, false) => passed += 1,
            (true, true) => problems.push(format!(
                "{} now matches libucl in every format: remove it from xfail-emit.txt",
                case.id
            )),
            (false, false) => problems.push(format!(
                "{} output differs and is not in xfail-emit.txt\n    {}",
                case.id,
                failures.join("\n    ")
            )),
            (false, true) => xfailed += 1,
        }
    }
    println!(
        "conformance (emitters): {} cases with output, {passed} match, {xfailed} expected \
         failures; matching outputs per format {matched:?} of {compared:?}",
        outcomes.len()
    );
    assert!(
        problems.is_empty(),
        "conformance problems (emitters):\n{}",
        problems.join("\n")
    );
}

// ----- reading output back (spec §10.8) -----------------------------------------------------

/// Differences found by `libucl_conformance_readback` that §10.8 does not list, each with the
/// question that asks about it: `(case, formats, reason)`. libucl reads its own golden output
/// files the same way (checked with the oracle). Like the xfail files, the list may only shrink:
/// an entry whose difference is gone fails the test.
/// Empty since spec-v10 §10.8 answered QUESTIONS.md #57 and #58.
const READBACK_PENDING: &[(&str, &[&str], &str)] = &[];

/// The labels of the four formats of spec §10.
const ALL_FORMATS: [&str; 4] = ["config", "json", "json-compact", "yaml"];

/// What reading a format's output back must give for one value, apart from the losses of §10.8.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Written {
    Config,
    /// JSON and compact JSON, which differ only in whitespace.
    Json,
    Yaml,
}

impl Written {
    fn of(format: Format) -> Written {
        match format {
            Format::Config => Written::Config,
            Format::Yaml => Written::Yaml,
            _ => Written::Json,
        }
    }
}

/// How a float or a time reads back from its output (spec §10.3, §10.8).
enum FloatBack {
    /// A float with these bits, or NaN.
    Float(f64),
    /// Output that reads back as this string (§10.8): `-inf`, and a `%f` output whose digits and
    /// `.` take 127 or more characters, a leading `-` not counted
    /// (`readback_percent_f_length_limit`, `cases/spec/05-numbers/float_max`).
    Text(String),
    /// A `%.15g` output below the normal range, which reading rejects together with the whole
    /// document (§10.8): the smallest normal float and every subnormal time
    /// (`readback_fifteen_digit_min_normal_error`, `readback_fifteen_digit_subnormal_error`).
    BelowNormal,
}

/// The float that the output of `v` reads back as. Rust's `{:.6}` and `{:.14e}` round the exact
/// decimal value of the double, ties to even, as the C conversions of §10.3 do.
fn float_back(v: f64) -> FloatBack {
    if v == f64::NEG_INFINITY {
        // Written `-inf` (§10.3), which reads as a string (§4, §10.8).
        return FloatBack::Text("-inf".to_string());
    }
    if !v.is_finite() {
        return FloatBack::Float(v);
    }
    let t = v.trunc();
    let int_range = -2_147_483_648.0..=2_147_483_647.0;
    if v == t && int_range.contains(&v) {
        // `%.1f`: exact.
        FloatBack::Float(v)
    } else if int_range.contains(&t) && (v - t).abs() < 1e-7 {
        // `%.15g`: 15 significant digits.
        let x: f64 = format!("{v:.14e}").parse().expect("a float");
        if x.abs() < f64::MIN_POSITIVE {
            FloatBack::BelowNormal
        } else {
            FloatBack::Float(x)
        }
    } else {
        // `%f`: 6 decimals.
        let text = format!("{v:.6}");
        if text.strip_prefix('-').unwrap_or(&text).len() >= 127 {
            FloatBack::Text(text)
        } else {
            FloatBack::Float(text.parse().expect("a float"))
        }
    }
}

/// `s` with each byte that the JSON form writes as `�` replaced by U+FFFD (spec §10.2,
/// §10.8): 0x00–0x1F other than LF, CR, TAB, BS, FF and VT, and DEL.
fn replacement_form(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' | '\u{b}' => c,
            '\0'..='\u{1f}' | '\u{7f}' => '\u{FFFD}',
            c => c,
        })
        .collect()
}

/// A byte that may start a bare key (spec §3.1).
fn bare_key_start(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'/' || b == b'_' || b >= 0x80
}

/// A key that reads back as itself when written bare (spec §3.1).
fn reads_bare(key: &str) -> bool {
    let mut bytes = key.bytes();
    bytes.next().is_some_and(bare_key_start)
        && bytes.all(|b| bare_key_start(b) || b == b'-' || b == b'.')
}

/// The keys of an output that make it unreadable, each with the §10.8 bullet that lists it or,
/// if none does, `None`.
type UnreadableKeys = Vec<(String, Option<&'static str>)>;

/// A value expected where the output is read back, and its path in the value written.
type Expected<'v> = (Cow<'v, UclValue>, Vec<PathSegment>);

/// The outcome of each format's readback for one case, or why there is none.
type CaseReadback = Result<Vec<(&'static str, ReadbackOutcome)>, String>;

/// Compares a parsed value with what its output in one format read back as, allowing only the
/// losses §10.8 lists.
struct Readback<'a> {
    written: Written,
    facts: &'a ucl_lexer::parse::OutputFacts,
    /// Differences that §10.8 does not list.
    diffs: Vec<String>,
    /// Keys written bare that cannot be read bare (§10.8), found while walking the value.
    unreadable: UnreadableKeys,
    /// The value holds a float or time whose `%.15g` output lies below the normal range, which
    /// reading rejects (§10.8).
    below_normal: bool,
}

impl<'a> Readback<'a> {
    fn new(written: Written, facts: &'a ucl_lexer::parse::OutputFacts) -> Self {
        Self {
            written,
            facts,
            diffs: Vec::new(),
            unreadable: Vec::new(),
            below_normal: false,
        }
    }

    /// The key of the value at `path` as the output writes it: its spelling (§10.1), and
    /// whether it is quoted.
    fn written_key(&self, key: &str, path: &[PathSegment]) -> (String, bool) {
        let facts = self.facts.get(path);
        let spelling = facts
            .and_then(|f| f.key_spelling.clone())
            .unwrap_or_else(|| key.to_string());
        let quoted = facts
            .and_then(|f| f.key_quoted)
            .unwrap_or_else(|| ucl_lexer::emit::key_needs_quoting(&spelling));
        (spelling, quoted)
    }

    /// Records the key of the value at `path` if the output writes it bare but it cannot be read
    /// bare, with the §10.8 bullet that lists it.
    fn check_key(&mut self, spelling: &str, quoted: bool, path: &[PathSegment]) {
        if quoted || reads_bare(spelling) || self.written == Written::Json {
            return;
        }
        let listed = if spelling.is_empty() {
            if self.written == Written::Yaml {
                // Written `null`, which reads back as the key "null" (§10.1, §10.8).
                return;
            }
            Some("the empty key")
        } else if spelling.contains([';', '}', '#', ',']) {
            Some("a quirk key of §10.1, with `;`, `}`, `#` or `,`")
        } else if !bare_key_start(spelling.as_bytes()[0]) {
            Some("a key that does not begin with a byte a bare key may begin with")
        } else if self.facts.get(path).and_then(|f| f.key_quoted) == Some(false) {
            // Only macros and `no-implicit-arrays` collections make a key bare that fact 3
            // would quote (spec §10.1).
            Some(
                "a key created by `.include` with `key` or `prefix`, or of an array built from repeated keys",
            )
        } else {
            None
        };
        self.unreadable.push((spelling.to_string(), listed));
    }

    /// Finds, in the value at `path`, the keys that make the output unreadable and the floats
    /// that reading rejects.
    fn scan(&mut self, value: &UclValue, path: &mut Vec<PathSegment>) {
        match value {
            UclValue::Object(object) => {
                for (key, entry) in object {
                    for (index, value) in entry.values().enumerate() {
                        path.push(PathSegment::Key {
                            key: key.clone(),
                            index,
                        });
                        if index == 0 || self.written == Written::Config {
                            let (spelling, quoted) = self.written_key(key, path);
                            self.check_key(&spelling, quoted, path);
                        }
                        self.scan(value, path);
                        path.pop();
                    }
                }
            }
            UclValue::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    path.push(PathSegment::Index(index));
                    self.scan(item, path);
                    path.pop();
                }
            }
            UclValue::Float(v) | UclValue::Time(v)
                if matches!(float_back(*v), FloatBack::BelowNormal) =>
            {
                self.below_normal = true;
            }
            _ => {}
        }
    }

    /// The key that a key written as `spelling` reads back as.
    fn key_back(&self, spelling: &str) -> String {
        if spelling.is_empty() && self.written != Written::Config {
            "null".to_string()
        } else {
            replacement_form(spelling)
        }
    }

    fn diff(&mut self, path: &[PathSegment], what: String) {
        self.diffs.push(format!("at {}: {what}", path_text(path)));
    }

    fn compare(&mut self, original: &UclValue, back: &UclValue, path: &mut Vec<PathSegment>) {
        match (original, back) {
            (UclValue::Object(o), UclValue::Object(b)) => self.compare_objects(o, b, path),
            (UclValue::Array(o), UclValue::Array(b)) => self.compare_arrays(o, b, path),
            (UclValue::Integer(o), UclValue::Integer(b)) if o == b => {}
            (UclValue::Boolean(o), UclValue::Boolean(b)) if o == b => {}
            (UclValue::Null, UclValue::Null) => {}
            (UclValue::String(o), UclValue::String(b)) if o == b || replacement_form(o) == *b => {}
            (UclValue::Float(v) | UclValue::Time(v), _) => match (float_back(*v), back) {
                (FloatBack::Float(x), UclValue::Float(y))
                    if (x.is_nan() && y.is_nan()) || x.to_bits() == y.to_bits() => {}
                (FloatBack::Text(t), UclValue::String(s)) if t == *s => {}
                _ => self.diff(path, format!("{original:?} read back as {back:?}")),
            },
            _ => self.diff(path, format!("{original:?} read back as {back:?}")),
        }
    }

    fn compare_arrays(&mut self, o: &[UclValue], b: &[UclValue], path: &mut Vec<PathSegment>) {
        if o.len() != b.len() {
            self.diff(
                path,
                format!("{} elements read back as {}", o.len(), b.len()),
            );
        }
        for (index, (o, b)) in o.iter().zip(b).enumerate() {
            path.push(PathSegment::Index(index));
            self.compare(o, b, path);
            path.pop();
        }
    }

    fn compare_objects(
        &mut self,
        o: &ucl_lexer::UclObject,
        b: &ucl_lexer::UclObject,
        path: &[PathSegment],
    ) {
        // The keys values are read back under, in order, each with the values expected there
        // and their paths.
        let mut expected: Vec<(String, Vec<Expected<'_>>)> = Vec::new();
        for (key, entry) in o {
            let at = |index: usize| {
                let mut at = path.to_vec();
                at.push(PathSegment::Key {
                    key: key.clone(),
                    index,
                });
                at
            };
            let mut wanted = Vec::new();
            match self.written {
                // One line per value, each with its own key: values written with different
                // spellings read back under separate keys (§10.1, §10.8).
                Written::Config => {
                    for (index, value) in entry.values().enumerate() {
                        let (spelling, _) = self.written_key(key, &at(index));
                        wanted.push((self.key_back(&spelling), Cow::Borrowed(value), at(index)));
                    }
                }
                // The entry once, with the key of its first value (§10.1). Several values are an
                // explicit array, of which only a first value that is an array remains (§10.7).
                Written::Json | Written::Yaml => {
                    let (spelling, _) = self.written_key(key, &at(0));
                    let value = if entry.len() == 1 || entry.first().is_array() {
                        Cow::Borrowed(entry.first())
                    } else {
                        Cow::Owned(UclValue::Array(entry.values().cloned().collect()))
                    };
                    wanted.push((self.key_back(&spelling), value, at(0)));
                }
            }
            for (key_back, value, at) in wanted {
                match expected.iter_mut().find(|(k, _)| *k == key_back) {
                    Some((_, values)) => values.push((value, at)),
                    None => expected.push((key_back, vec![(value, at)])),
                }
            }
        }
        for (key, values) in &expected {
            let Some(entry) = b.entry(key) else {
                self.diff(path, format!("key {key:?} is missing"));
                continue;
            };
            if entry.len() != values.len() {
                self.diff(
                    path,
                    format!(
                        "key {key:?}: {} values read back as {}",
                        values.len(),
                        entry.len()
                    ),
                );
            }
            for ((value, at), back) in values.iter().zip(entry.values()) {
                self.compare(value, back, &mut at.clone());
            }
        }
        for key in b.keys() {
            if !expected.iter().any(|(k, _)| k == key) {
                self.diff(path, format!("key {key:?} was not written"));
            }
        }
    }
}

/// A value path for messages: keys in the JSON form joined by `.`, `#n` for value `n` of a
/// multi-value entry, `[n]` for array elements.
fn path_text(path: &[PathSegment]) -> String {
    if path.is_empty() {
        return "the root".to_string();
    }
    let mut text = String::new();
    for segment in path {
        match segment {
            PathSegment::Key { key, index } => {
                if !text.is_empty() {
                    text.push('.');
                }
                text.push_str(&format!("{key:?}"));
                if *index > 0 {
                    text.push_str(&format!("#{index}"));
                }
            }
            PathSegment::Index(index) => text.push_str(&format!("[{index}]")),
        }
    }
    text
}

/// How the output of one case in one format read back.
enum ReadbackOutcome {
    /// The same value, apart from the losses §10.8 lists for values.
    Same,
    /// Unreadable, because of keys that §10.8 lists as making the output unreadable.
    UnreadableKeys(Vec<String>),
    /// Rejected, because of a float or time whose `%.15g` output lies below the normal range
    /// (§10.8).
    BelowNormal,
    /// Differences that §10.8 does not list.
    Differs(Vec<String>),
}

/// Emits a case's value in `format` and reads the output back with a parser that expands
/// nothing: no registered variables, no handler, and `no-filevars` (spec §7.5: without a
/// replacement, `$$` stays as written).
fn read_back(parser: &CoreParser, value: &UclValue, format: Format) -> ReadbackOutcome {
    let text = parser.emitter(format).emit(value);
    let back = CoreParser::with_flags(ParserFlags::NO_FILEVARS).parse(text.as_bytes());
    let mut readback = Readback::new(Written::of(format), parser.output_facts());
    readback.scan(value, &mut Vec::new());
    let unlisted: Vec<String> = readback
        .unreadable
        .iter()
        .filter(|(_, listed)| listed.is_none())
        .map(|(key, _)| format!("key {key:?} is written bare and cannot be read bare"))
        .collect();
    let listed: Vec<String> = readback
        .unreadable
        .iter()
        .filter_map(|(key, listed)| listed.map(|why| format!("{key:?}: {why}")))
        .collect();
    if !unlisted.is_empty() {
        return ReadbackOutcome::Differs(unlisted);
    }
    match back {
        // Keys listed in §10.8 make the output unreadable: whatever reading gives is allowed.
        Err(_) if !listed.is_empty() => ReadbackOutcome::UnreadableKeys(listed),
        Err(_) if readback.below_normal => ReadbackOutcome::BelowNormal,
        Err(e) => ReadbackOutcome::Differs(vec![format!("reading the output fails: {e}")]),
        Ok(back) => {
            readback.compare(value, &back, &mut Vec::new());
            if readback.diffs.is_empty() {
                ReadbackOutcome::Same
            } else if !listed.is_empty() {
                ReadbackOutcome::UnreadableKeys(listed)
            } else {
                ReadbackOutcome::Differs(readback.diffs)
            }
        }
    }
}

/// For every case that parses, the output in each format of spec §10 reads back as the same
/// value, apart from the losses §10.8 lists: times become floats, bytes written `\uFFFD` become
/// U+FFFD, floats keep the precision of their conversion (§10.3), a `%f` output whose digits and
/// `.` take 127 or more characters becomes a string, −∞ becomes the string `"-inf"`, a `%.15g`
/// output below the normal range is rejected, keys written bare that cannot be read bare make the
/// output unreadable, values of a multi-value entry written with different spellings read back
/// under separate keys in the config format, JSON and YAML write a multi-value entry as an
/// explicit array (only a first value that is an array, §10.7) and the empty key as `null`, and
/// priorities, saved comments and marks are not written. The output is read with nothing
/// registered, so no string expands. Other differences would go in `READBACK_PENDING`, each with
/// the question that asks about it.
#[test]
fn libucl_conformance_readback() {
    let cases = discover();
    let report = std::env::var_os("UCL_CONFORMANCE_REPORT").is_some();
    let outcomes: Vec<(&Case, CaseReadback)> = cases
        .iter()
        .filter_map(|case| {
            let setup = setup(case);
            let result = quietly(|| {
                let (parser, result) = run_new_core(case, &setup);
                let value = result.ok()?;
                Some(
                    OUTPUT_FORMATS
                        .iter()
                        .map(|(label, format)| (*label, read_back(&parser, &value, *format)))
                        .collect(),
                )
            });
            match result {
                Ok(None) => None,
                Ok(Some(outcomes)) => Some((case, Ok(outcomes))),
                Err(_) => Some((case, Err("panic".to_string()))),
            }
        })
        .collect();

    assert_eq!(ALL_FORMATS, OUTPUT_FORMATS.map(|(label, _)| label));
    let known: BTreeSet<&str> = cases.iter().map(|c| c.id.as_str()).collect();
    let mut pending: BTreeMap<(&str, &str), &str> = BTreeMap::new();
    let mut problems = Vec::new();
    for (id, labels, reason) in READBACK_PENDING {
        if !known.contains(id) {
            problems.push(format!("READBACK_PENDING lists unknown case '{id}'"));
        }
        for label in *labels {
            if pending.insert((id, label), reason).is_some() {
                problems.push(format!("READBACK_PENDING lists '{id}' {label} twice"));
            }
        }
    }
    let mut counts: BTreeMap<&str, [usize; 4]> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for (case, outcome) in &outcomes {
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(e) => {
                problems.push(format!("{}: {e}", case.id));
                continue;
            }
        };
        for (label, outcome) in outcome {
            let count = counts.entry(label).or_default();
            let diffs = match outcome {
                ReadbackOutcome::Same => {
                    count[0] += 1;
                    continue;
                }
                ReadbackOutcome::UnreadableKeys(keys) => {
                    count[1] += 1;
                    if report {
                        println!(
                            "UNREADABLE(readback) {} {label}: {}",
                            case.id,
                            keys.join("; ")
                        );
                    }
                    continue;
                }
                ReadbackOutcome::BelowNormal => {
                    count[2] += 1;
                    if report {
                        println!("REJECTED(readback) {} {label}", case.id);
                    }
                    continue;
                }
                ReadbackOutcome::Differs(diffs) => {
                    count[3] += 1;
                    diffs
                }
            };
            if report {
                println!(
                    "FAIL(readback) {} {label}\n    {}",
                    case.id,
                    diffs.join("\n    ")
                );
            }
            let key = (case.id.as_str(), *label);
            seen.insert(key);
            if !pending.contains_key(&key) {
                problems.push(format!(
                    "{} {label}: the output reads back differently, in a way §10.8 does not list\n    {}",
                    case.id,
                    diffs.join("\n    ")
                ));
            }
        }
    }
    for key in pending.keys() {
        if !seen.contains(key) {
            problems.push(format!(
                "{} {} now reads back as §10.8 says: remove it from READBACK_PENDING",
                key.0, key.1
            ));
        }
    }
    println!(
        "conformance (readback): {} cases parse; per format [same value, unreadable keys (§10.8), \
         rejected float (§10.8), other differences]: {counts:?}",
        outcomes.len()
    );
    assert!(
        problems.is_empty(),
        "conformance problems (readback):\n{}",
        problems.join("\n")
    );
}

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
//! Set `UCL_CONFORMANCE_REPORT=1` (with `-- --nocapture`) to print every failing case with the
//! first point where the two dumps, or the two outputs, differ, and a suggested xfail reason.

use serde_json::{Value as J, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use ucl_lexer::emit::Format;
use ucl_lexer::parse::{CommentPlacement, FsLoader, Parser as CoreParser, PathSegment};
use ucl_lexer::{DuplicateStrategy, ParserFlags, UclValue};

const CONFORMANCE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/conformance");

struct Case {
    id: String,
    input: PathBuf,
    golden: PathBuf,
    flags: Vec<String>,
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
            let flags_path = dir.join(format!("{stem}.flags"));
            let flags = fs::read_to_string(&flags_path)
                .map(|text| {
                    text.lines()
                        .map(|l| l.split('#').next().unwrap().trim().to_string())
                        .filter(|l| !l.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            Case {
                id: format!("{rel}/{stem}"),
                golden: dir.join(format!("{stem}.golden.json")),
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

/// Dumps a crate value in the same schema as `tools/ucl-dump`: every value of a multi-value
/// entry, times as `"time"`, and `"pri"` on any entry value whose priority is non-zero. The model
/// keeps priorities only on entry values, which is where libucl's dumps have them. With
/// `comments`, also the saved comments attached to each value.
fn dump(value: &UclValue, comments: Option<&CommentMap>) -> J {
    dump_node(value, 0, &mut Vec::new(), comments)
}

/// Saved comments by value path: the dump key (`"c"` or `"ca"`) and the comment texts.
type CommentMap = HashMap<Vec<PathSegment>, (&'static str, Vec<String>)>;

/// The new core's attached comments of the last parse, keyed by value path.
fn comment_map(parser: &CoreParser) -> CommentMap {
    let comments = parser.comments();
    parser
        .attached_comments()
        .iter()
        .map(|group| {
            let key = match group.placement {
                CommentPlacement::Before => "c",
                CommentPlacement::After => "ca",
            };
            let texts = group
                .comments
                .iter()
                .map(|&i| comments[i].text.clone())
                .collect();
            (group.path.clone(), (key, texts))
        })
        .collect()
}

fn dump_node(
    value: &UclValue,
    priority: u8,
    path: &mut Vec<PathSegment>,
    comments: Option<&CommentMap>,
) -> J {
    let mut node = match value {
        UclValue::Object(obj) => {
            let mut entries = Vec::new();
            for (k, entry) in obj.iter() {
                let mut values = Vec::new();
                for (index, slot) in entry.slots().iter().enumerate() {
                    path.push(PathSegment::Key {
                        key: k.clone(),
                        index,
                    });
                    values.push(dump_node(slot.value(), slot.priority(), path, comments));
                    path.pop();
                }
                entries.push(json!({"k": k, "v": values}));
            }
            json!({"t": "object", "entries": entries})
        }
        UclValue::Array(arr) => {
            let mut items = Vec::new();
            for (index, item) in arr.iter().enumerate() {
                path.push(PathSegment::Index(index));
                items.push(dump_node(item, 0, path, comments));
                path.pop();
            }
            json!({"t": "array", "v": items})
        }
        UclValue::Integer(i) => json!({"t": "int", "v": i.to_string()}),
        UclValue::Float(f) => json!({"t": "float", "v": format!("{f:?}")}),
        UclValue::Time(t) => json!({"t": "time", "v": format!("{t:?}")}),
        UclValue::String(s) => json!({"t": "string", "v": s}),
        UclValue::Boolean(b) => json!({"t": "bool", "v": b}),
        UclValue::Null => json!({"t": "null"}),
    };
    if priority != 0 {
        node["pri"] = J::from(priority);
    }
    if let Some((key, texts)) = comments.and_then(|map| map.get(path.as_slice())) {
        node[*key] = J::from(texts.clone());
    }
    node
}

/// Removes the priorities the spec says are not observable (`docs/spec/08-duplicates.md` §8.7):
/// that of the root object and those of array elements. libucl's dumps record them, but they never
/// affect a parse result or any output format.
fn strip_unobservable_priorities(node: &mut J, is_root: bool) {
    if is_root {
        if let Some(map) = node.as_object_mut() {
            map.remove("pri");
        }
    }
    match node.get("t").and_then(J::as_str) {
        Some("array") => {
            if let Some(items) = node.get_mut("v").and_then(J::as_array_mut) {
                for item in items {
                    if let Some(map) = item.as_object_mut() {
                        map.remove("pri");
                    }
                    strip_unobservable_priorities(item, false);
                }
            }
        }
        Some("object") => {
            if let Some(entries) = node.get_mut("entries").and_then(J::as_array_mut) {
                for entry in entries {
                    if let Some(values) = entry.get_mut("v").and_then(J::as_array_mut) {
                        for value in values {
                            strip_unobservable_priorities(value, false);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// What a parser made of a case: a dump, a rejection, or a reason why the case could not be run.
enum Parsed {
    Value(J),
    Rejected(String),
    /// The parser recognised the input but does not support it; never a pass.
    Unsupported(String),
}

/// The settings a case's `.flags` file asks for (`tests/conformance/README.md`).
#[derive(Default)]
struct Setup {
    flags: ParserFlags,
    vars: Vec<(String, String)>,
    handler: bool,
    priority: Option<u8>,
    strategy: Option<DuplicateStrategy>,
    string_input: bool,
    dump_comments: bool,
    /// `save-comments` or `dump-comments`: the case has config output with comments.
    save_comments: bool,
}

fn setup(case: &Case) -> Setup {
    let mut setup = Setup::default();
    for flag in &case.flags {
        let flag = flag.as_str();
        let parser_flag = match flag {
            "key-lowercase" => Some(ParserFlags::KEY_LOWERCASE),
            "zerocopy" => Some(ParserFlags::ZEROCOPY),
            "no-time" => Some(ParserFlags::NO_TIME),
            "no-implicit-arrays" => Some(ParserFlags::NO_IMPLICIT_ARRAYS),
            "save-comments" => {
                setup.save_comments = true;
                Some(ParserFlags::SAVE_COMMENTS)
            }
            // Also records the attached comments in the dump ("c"/"ca" keys,
            // tests/conformance/README.md).
            "dump-comments" => {
                setup.dump_comments = true;
                setup.save_comments = true;
                Some(ParserFlags::SAVE_COMMENTS)
            }
            "disable-macro" => Some(ParserFlags::DISABLE_MACRO),
            "no-filevars" => Some(ParserFlags::NO_FILEVARS),
            _ => None,
        };
        if let Some(f) = parser_flag {
            setup.flags |= f;
        } else if flag == "variable-handler" {
            setup.handler = true;
        } else if flag == "string-input" {
            setup.string_input = true;
        } else if let Some(var) = flag.strip_prefix("var:") {
            let (name, value) = var
                .split_once('=')
                .unwrap_or_else(|| panic!("{}: bad flag '{flag}'", case.id));
            setup.vars.push((name.to_string(), value.to_string()));
        } else if let Some(n) = flag.strip_prefix("priority:") {
            let n: u32 = n
                .parse()
                .unwrap_or_else(|_| panic!("{}: bad flag '{flag}'", case.id));
            setup.priority = Some((n & 0x0f) as u8);
        } else if let Some(name) = flag.strip_prefix("strategy:") {
            setup.strategy = Some(match name {
                "append" => DuplicateStrategy::Append,
                "merge" => DuplicateStrategy::Merge,
                "rewrite" => DuplicateStrategy::Rewrite,
                "error" => DuplicateStrategy::Error,
                _ => panic!("{}: unknown strategy in '{flag}'", case.id),
            });
        } else {
            panic!("{}: unknown .flags entry '{flag}'", case.id);
        }
    }
    setup
}

/// Runs the new core on a case, applying its `.flags` file as the oracle does
/// (`docs/spec/README.md`, "How the conformance oracle runs every case"). A silent stop gives its
/// partial result. Returns the parser, for its saved comments and output facts, and the result.
fn run_new_core(
    case: &Case,
    setup: &Setup,
) -> (CoreParser, Result<UclValue, ucl_lexer::parse::Error>) {
    let mut parser = CoreParser::with_flags(setup.flags);
    // The oracle reads the filesystem; a parser's default loader holds no files.
    parser.set_loader(FsLoader::new());
    parser.set_base_dir(case.input.parent().expect("a case is in a directory"));
    parser.register_variable("ABI", "unknown");
    for (name, value) in &setup.vars {
        parser.register_variable(name.as_str(), value.as_str());
    }
    if setup.handler {
        parser.set_variable_handler(|name| name.starts_with("H_").then(|| "[handled]".to_string()));
    }
    if let Some(priority) = setup.priority {
        parser.set_priority(priority);
    }
    if let Some(strategy) = setup.strategy {
        parser.set_strategy(strategy);
    }
    // The oracle parses every case as a string, then defines FILENAME and CURDIR from the
    // case's path unless the case has `no-filevars` (spec §12.7). The core defines them
    // whenever it parses a file (project decision 5).
    let no_filevars = setup.flags.contains(ParserFlags::NO_FILEVARS);
    let result = if setup.string_input || no_filevars {
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
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let (parser, result) = run_new_core(case, &setup);
        match result {
            Ok(v) => {
                let comments = setup.dump_comments.then(|| comment_map(&parser));
                Parsed::Value(dump(&v, comments.as_ref()))
            }
            Err(e) if e.is_unsupported() => Parsed::Unsupported(e.to_string()),
            Err(e) => Parsed::Rejected(e.to_string()),
        }
    }));
    result.map_err(|_| "panic")
}

/// Compares two dumps. Floats and times are compared as numbers (NaN equals NaN).
fn find_diff(golden: &J, actual: &J, path: &str) -> Option<String> {
    if let (Some(gt), Some(at)) = (golden.get("t"), actual.get("t"))
        && gt == at
        && (gt == "float" || gt == "time")
    {
        let g = golden["v"].as_str().and_then(|s| s.parse::<f64>().ok());
        let a = actual["v"].as_str().and_then(|s| s.parse::<f64>().ok());
        let same = match (g, a) {
            (Some(g), Some(a)) => g.to_bits() == a.to_bits() || (g.is_nan() && a.is_nan()),
            _ => false,
        };
        let rest_same = strip(golden, &["v"]) == strip(actual, &["v"]);
        return if same && rest_same {
            None
        } else {
            Some(format!("{path}: libucl {golden} crate {actual}"))
        };
    }
    match (golden, actual) {
        (J::Object(g), J::Object(a)) => {
            let keys: BTreeSet<_> = g.keys().chain(a.keys()).collect();
            for k in keys {
                match (g.get(k), a.get(k)) {
                    (Some(gv), Some(av)) => {
                        if let Some(d) = find_diff(gv, av, &format!("{path}.{k}")) {
                            return Some(d);
                        }
                    }
                    _ => return Some(format!("{path}: libucl {golden} crate {actual}")),
                }
            }
            None
        }
        (J::Array(g), J::Array(a)) => {
            for (i, (gv, av)) in g.iter().zip(a.iter()).enumerate() {
                if let Some(d) = find_diff(gv, av, &format!("{path}[{i}]")) {
                    return Some(d);
                }
            }
            if g.len() != a.len() {
                return Some(format!(
                    "{path}: length libucl {} crate {}",
                    g.len(),
                    a.len()
                ));
            }
            None
        }
        _ if golden == actual => None,
        _ => Some(format!("{path}: libucl {golden} crate {actual}")),
    }
}

fn strip(value: &J, fields: &[&str]) -> J {
    let mut v = value.clone();
    if let J::Object(map) = &mut v {
        for f in fields {
            map.remove(*f);
        }
    }
    v
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
    let mut golden: J = serde_json::from_str(&golden_text).expect("golden files are valid JSON");
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

/// Serialises the panic-hook swap of the two tests, which cargo runs in parallel.
static PANIC_HOOK: Mutex<()> = Mutex::new(());

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

    // Parser panics are caught per case; keep their messages out of the output.
    let outcomes: Vec<(&Case, Outcome)> = {
        let _guard = PANIC_HOOK.lock().unwrap_or_else(|e| e.into_inner());
        let hook = panic::take_hook();
        panic::set_hook(Box::new(|_| {}));
        let outcomes = cases.iter().map(|c| (c, evaluate(c, parse))).collect();
        panic::set_hook(hook);
        outcomes
    };

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
    let golden: J = serde_json::from_str(&golden_text).expect("golden files are valid JSON");
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
    let result = panic::catch_unwind(AssertUnwindSafe(|| -> OutputOutcome {
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
    }));
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
    let outcomes: Vec<(&Case, OutputOutcome)> = {
        let _guard = PANIC_HOOK.lock().unwrap_or_else(|e| e.into_inner());
        let hook = panic::take_hook();
        panic::set_hook(Box::new(|_| {}));
        let outcomes = cases
            .iter()
            .filter_map(|c| evaluate_outputs(c).map(|o| (c, o)))
            .collect();
        panic::set_hook(hook);
        outcomes
    };

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

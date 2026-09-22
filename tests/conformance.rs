//! libucl conformance suite (PLAN.md P0.5).
//!
//! Every case under `tests/conformance/` is parsed by this crate, and the result is compared with
//! libucl's typed dump of the same input (`<case>.golden.json`, produced by
//! `scripts/regen-golden.sh`). Expected values are never written by hand.
//!
//! Two parsers run every case:
//!
//! - the existing parser (`libucl_conformance`), with its known failures in
//!   `tests/conformance/xfail.txt`. It cannot apply `.flags` files, so cases that have one fail;
//! - the new parser core `ucl_lexer::parse` (`libucl_conformance_new_core`), with its known
//!   failures in `tests/conformance/xfail-new.txt`. It applies each case's `.flags` file.
//!
//! Each xfail file lists cases one per line: `<case-id> <reason>`. The run fails when an unlisted
//! case fails, and also when a listed case passes, so the lists can only shrink.
//!
//! A case whose golden file is an error passes only if the parser rejects the input. For the new
//! core, a rejection because the input uses something not supported yet (`Error::is_unsupported`,
//! such as a macro before C3) does not count: the case fails with the reason `macro`.
//!
//! Set `UCL_CONFORMANCE_REPORT=1` (with `-- --nocapture`) to print every failing case with the
//! first point where the two dumps differ, and a suggested xfail reason.

use serde_json::{Value as J, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use ucl_lexer::parse::Parser as CoreParser;
use ucl_lexer::{DuplicateStrategy, MapVariableHandler, ParserFlags, UclParser, UclValue};

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
/// keeps priorities only on entry values, which is where libucl's dumps have them.
fn dump(value: &UclValue) -> J {
    dump_node(value, 0)
}

fn dump_node(value: &UclValue, priority: u8) -> J {
    let mut node = match value {
        UclValue::Object(obj) => json!({
            "t": "object",
            "entries": obj
                .iter()
                .map(|(k, entry)| {
                    let values: Vec<J> = entry
                        .slots()
                        .iter()
                        .map(|slot| dump_node(slot.value(), slot.priority()))
                        .collect();
                    json!({"k": k, "v": values})
                })
                .collect::<Vec<_>>(),
        }),
        UclValue::Array(arr) => {
            json!({"t": "array", "v": arr.iter().map(dump).collect::<Vec<_>>()})
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
    /// The parser recognised the input but does not support it yet; never a pass.
    Unsupported(String),
}

fn parse_with_crate(case: &Case) -> Result<Parsed, &'static str> {
    let bytes = fs::read(&case.input).unwrap();
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return Err("non-utf8");
    };
    if !case.flags.is_empty() {
        // No parser flags exist yet (PLAN.md P3.11 adds ParserFlags).
        return Err("flags");
    }
    let mut vars = MapVariableHandler::new();
    vars.insert("ABI".to_string(), "unknown".to_string());
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let mut parser = UclParser::with_variable_handler(text, Box::new(vars));
        match parser.parse_document() {
            Ok(v) => Parsed::Value(dump(&v)),
            Err(e) => Parsed::Rejected(e.to_string()),
        }
    }));
    result.map_err(|_| "panic")
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
            "save-comments" => Some(ParserFlags::SAVE_COMMENTS),
            // Also asks the oracle to record the attached comments in the dump ("c"/"ca" keys,
            // tests/conformance/README.md); the crate's dump has none yet, so such cases fail.
            "dump-comments" => Some(ParserFlags::SAVE_COMMENTS),
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

/// Parses a case with the new core, applying its `.flags` file as the oracle does
/// (`docs/spec/README.md`, "How the conformance oracle runs every case").
fn parse_with_new_core(case: &Case) -> Result<Parsed, &'static str> {
    let setup = setup(case);
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let mut parser = CoreParser::with_flags(setup.flags);
        parser.register_variable("ABI", "unknown");
        for (name, value) in &setup.vars {
            parser.register_variable(name.as_str(), value.as_str());
        }
        if setup.handler {
            parser.set_variable_handler(|name| {
                name.starts_with("H_").then(|| "[handled]".to_string())
            });
        }
        if let Some(priority) = setup.priority {
            parser.set_priority(priority);
        }
        if let Some(strategy) = setup.strategy {
            parser.set_strategy(strategy);
        }
        let result = if setup.string_input {
            let bytes = fs::read(&case.input).unwrap();
            parser.parse(&bytes)
        } else {
            parser.parse_file(&case.input)
        };
        match result {
            Ok(v) => Parsed::Value(dump(&v)),
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
            let hint = if hint == "non-utf8" {
                "non-utf8, D2"
            } else {
                hint
            };
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
            hint: "macro",
            detail: format!("not supported yet: {e}"),
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
fn libucl_conformance() {
    run_suite("existing parser", "xfail.txt", parse_with_crate);
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

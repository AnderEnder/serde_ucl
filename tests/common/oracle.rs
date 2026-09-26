//! How the conformance runner runs the crate as the oracle runs a case, and the oracle's dump
//! schema. `tests/conformance.rs` and the differential fuzzer in `fuzz/` both use it, so that the
//! fuzzer compares the crate with the oracle exactly as the conformance suite does.
//!
//! - [`Setup::from_flags`]: the settings a `.flags` file asks for (`tests/conformance/README.md`);
//! - [`configure`]: a parser set up as the oracle parses a case, with the test macros of spec
//!   §13.2 when the flags ask for them;
//! - [`dump`]: a value in the schema of `tools/ucl-dump`'s typed dump (the golden files);
//! - [`strip_unobservable_priorities`] and [`find_diff`]: the comparison of two dumps.

use serde_json::{Value as J, json};
use serde_ucl::parse::{CommentPlacement, FsLoader, MacroCall, MacroError, Parser, PathSegment};
use serde_ucl::{DuplicateStrategy, ParserFlags, UclObject, UclValue};
use std::cell::Cell;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::Once;

thread_local! {
    /// Set while this thread runs [`quietly`].
    static QUIET: Cell<bool> = const { Cell::new(false) };
}

/// Runs `f`, catching a panic, and keeps the panic's message out of the output. Only the thread
/// that runs `f` is quiet: a panic on any other thread, such as a failing assertion of a test
/// running in parallel, is reported as usual.
pub fn quietly<R>(f: impl FnOnce() -> R) -> std::thread::Result<R> {
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let report = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if !QUIET.with(Cell::get) {
                report(info);
            }
        }));
    });
    let was_quiet = QUIET.with(|quiet| quiet.replace(true));
    let result = panic::catch_unwind(AssertUnwindSafe(f));
    QUIET.with(|quiet| quiet.set(was_quiet));
    result
}

/// The entries of a `.flags` file, without comments and blank lines; none if there is no file.
pub fn read_flags(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .map(|text| {
            text.lines()
                .map(|l| l.split('#').next().unwrap().trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// The duplicate strategy of a `strategy:NAME` flag or a `.inputs` line.
pub fn strategy_named(name: &str) -> Option<DuplicateStrategy> {
    Some(match name {
        "append" => DuplicateStrategy::Append,
        "merge" => DuplicateStrategy::Merge,
        "rewrite" => DuplicateStrategy::Rewrite,
        "error" => DuplicateStrategy::Error,
        _ => return None,
    })
}

/// The settings a case's `.flags` file asks for (`tests/conformance/README.md`).
#[derive(Default)]
pub struct Setup {
    pub flags: ParserFlags,
    pub vars: Vec<(String, String)>,
    pub handler: bool,
    pub priority: Option<u8>,
    pub strategy: Option<DuplicateStrategy>,
    pub string_input: bool,
    pub dump_comments: bool,
    /// `save-comments` or `dump-comments`: the case has config output with comments.
    pub save_comments: bool,
    /// `registered-macros`: the test macros `.emit`, `.seen`, `.fail` and `.ctx` (spec §13.2).
    pub registered_macros: bool,
    /// `registered-priority-override`: the handler of `.seen` registered as `priority`.
    pub priority_override: bool,
}

impl Setup {
    /// The settings that `flags`, the entries of a `.flags` file, ask for.
    pub fn from_flags(flags: &[String]) -> Result<Setup, String> {
        let mut setup = Setup::default();
        for flag in flags {
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
            } else if flag == "registered-macros" {
                setup.registered_macros = true;
            } else if flag == "registered-priority-override" {
                setup.priority_override = true;
            } else if let Some(var) = flag.strip_prefix("var:") {
                let (name, value) = var
                    .split_once('=')
                    .ok_or_else(|| format!("bad flag '{flag}'"))?;
                setup.vars.push((name.to_string(), value.to_string()));
            } else if let Some(n) = flag.strip_prefix("priority:") {
                let n: u32 = n.parse().map_err(|_| format!("bad flag '{flag}'"))?;
                setup.priority = Some((n & 0x0f) as u8);
            } else if let Some(name) = flag.strip_prefix("strategy:") {
                setup.strategy = Some(
                    strategy_named(name).ok_or_else(|| format!("unknown strategy in '{flag}'"))?,
                );
            } else {
                return Err(format!("unknown .flags entry '{flag}'"));
            }
        }
        Ok(setup)
    }
}

// ----- the test macros (spec §13.2, *The test macros*) ------------------------------------

/// `.emit`: has its VALUE text parsed in place, and fails if that fails or stops.
fn emit_macro(call: &mut MacroCall<'_>) -> Result<(), MacroError> {
    let text = call.value().to_vec();
    call.parse(text).map_err(|_| MacroError::stop())
}

/// A copy of `value` as the oracle's test macros make it: an entry whose first value is an object
/// or an array is copied as that value only, as `.inherit` copies (spec §9.7; oracle runs,
/// QUESTIONS.md #24 and #59).
fn test_macro_copy(value: &UclValue) -> UclValue {
    match value {
        UclValue::Object(object) => {
            let mut copy = UclObject::new();
            for (key, entry) in object {
                let count = match entry.first() {
                    UclValue::Object(_) | UclValue::Array(_) => 1,
                    _ => entry.len(),
                };
                let mut slots = entry.slots()[..count].iter().map(|slot| {
                    serde_ucl::Slot::new(test_macro_copy(slot.value()), slot.priority())
                });
                let mut copied = serde_ucl::Entry::from_slot(slots.next().expect("a value"));
                slots.for_each(|slot| copied.push_slot(slot));
                copy.insert_entry(key.clone(), copied);
            }
            UclValue::Object(copy)
        }
        UclValue::Array(items) => UclValue::Array(items.iter().map(test_macro_copy).collect()),
        scalar => scalar.clone(),
    }
}

/// `.seen`: adds `seen = { data: <VALUE text>, args: <a copy of the ARGUMENTS, or null> }` to the
/// innermost open object, and fails if there is none.
fn seen_macro(call: &mut MacroCall<'_>) -> Result<(), MacroError> {
    let mut seen = UclObject::new();
    seen.insert(
        "data",
        UclValue::String(String::from_utf8_lossy(call.value()).into_owned()),
    );
    let args = call.arguments().map_or(UclValue::Null, test_macro_copy);
    seen.insert("args", args);
    call.add("seen", UclValue::Object(seen))
        .map_err(|_| MacroError::stop())
}

/// `.fail`: fails and does nothing else.
fn fail_macro(_: &mut MacroCall<'_>) -> Result<(), MacroError> {
    Err(MacroError::stop())
}

/// `.ctx`, a context macro: adds `ctx` with a copy of the root it received, which keeps the
/// root's priority, that of the first input (spec §13.2, *The root's priority*), and fails like
/// `.seen`.
fn ctx_macro(call: &mut MacroCall<'_>) -> Result<(), MacroError> {
    let root = call.root().map_or(UclValue::Null, test_macro_copy);
    let priority = call.root_priority().unwrap_or(0);
    call.add_with_priority("ctx", root, priority)
        .map_err(|_| MacroError::stop())
}

/// Registers the test macros a case's `.flags` ask for.
fn register_test_macros(parser: &mut Parser, setup: &Setup) {
    if setup.registered_macros {
        parser
            .register_macro("emit", emit_macro)
            .register_macro("seen", seen_macro)
            .register_macro("fail", fail_macro)
            .register_context_macro("ctx", ctx_macro);
    }
    if setup.priority_override {
        parser.register_macro("priority", seen_macro);
    }
}

/// A parser set up as the oracle parses a case with `setup`: the filesystem loader, with
/// `base_dir` standing in for the working directory the oracle runs in (spec §9.3), the variable
/// `ABI` = `unknown` and those of `var:` flags, the test variable handler, the priority and
/// strategy of the input, and the test macros (spec §13.2).
pub fn configure(setup: &Setup, base_dir: &Path) -> Parser {
    let mut parser = Parser::with_flags(setup.flags);
    // The oracle reads the filesystem; a parser's default loader holds no files.
    parser.set_loader(FsLoader::new());
    parser.set_base_dir(base_dir);
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
    register_test_macros(&mut parser, setup);
    parser
}

/// Dumps a crate value in the same schema as `tools/ucl-dump`: every value of a multi-value
/// entry, times as `"time"`, and `"pri"` on any entry value whose priority is non-zero. The model
/// keeps priorities only on entry values, which is where libucl's dumps have them. With
/// `comments`, also the saved comments attached to each value.
pub fn dump(value: &UclValue, comments: Option<&CommentMap>) -> J {
    dump_node(value, 0, &mut Vec::new(), comments)
}

/// Saved comments by value path: the dump key (`"c"` or `"ca"`) and the comment texts.
pub type CommentMap = HashMap<Vec<PathSegment>, (&'static str, Vec<String>)>;

/// The new core's attached comments of the last parse, keyed by value path.
pub fn comment_map(parser: &Parser) -> CommentMap {
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
pub fn strip_unobservable_priorities(node: &mut J, is_root: bool) {
    if is_root && let Some(map) = node.as_object_mut() {
        map.remove("pri");
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

/// Compares two dumps. Floats and times are compared as numbers (NaN equals NaN).
pub fn find_diff(golden: &J, actual: &J, path: &str) -> Option<String> {
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

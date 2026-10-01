//! One input through the oracle and through the crate, and the comparison of the two results as
//! the conformance runner compares a case with its golden file (`tests/common/oracle.rs`).

use crate::oracle::{self, Setup};
use crate::uncertain::{self, Context, SavedComment};
use serde_json::Value as J;
use serde_ucl::parse::{ErrorKind, FileKind, FsLoader, Loader, Parser, Uncertain};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// The flags the oracle runs with to give the expected result of an input with `flags`: all of
/// them but `zerocopy`. Spec §12.2 gives a document with `zerocopy` the result the spec gives it
/// without `zerocopy`, all other settings unchanged, while libucl's own result with the flag is
/// undefined in places (the text of an expanded `.emit` and what depends on it, and documents
/// that include a file with entries). The crate still parses with every flag.
pub fn expectation_flags(flags: &[String]) -> Vec<String> {
    flags
        .iter()
        .filter(|flag| *flag != "zerocopy")
        .cloned()
        .collect()
}

/// The options the oracle runs with for an input with `flags`: those of the
/// [`expectation_flags`]. Running it ([`check`]) and telling how to run it again (the fuzzer's
/// reports) both use this.
pub fn expectation_options(flags: &[String]) -> Result<Vec<String>, String> {
    oracle_options(&expectation_flags(flags))
}

/// The oracle's options for the entries of a `.flags` file, mapped as `scripts/regen-golden.sh`
/// maps them.
pub fn oracle_options(flags: &[String]) -> Result<Vec<String>, String> {
    let mut options = Vec::new();
    for flag in flags {
        let (option, argument) = match flag.as_str() {
            "key-lowercase" => ("-l", None),
            "zerocopy" => ("-z", None),
            "no-time" => ("-T", None),
            "no-implicit-arrays" => ("-I", None),
            "save-comments" => ("-C", None),
            "dump-comments" => ("-c", None),
            "disable-macro" => ("-M", None),
            "no-filevars" => ("-F", None),
            "variable-handler" => ("-H", None),
            "string-input" => ("-S", None),
            "registered-macros" => ("-R", None),
            "registered-priority-override" => ("-O", None),
            other => {
                if let Some(var) = other.strip_prefix("var:") {
                    ("-v", Some(var))
                } else if let Some(n) = other.strip_prefix("priority:") {
                    ("-p", Some(n))
                } else if let Some(name) = other.strip_prefix("strategy:") {
                    ("-s", Some(name))
                } else {
                    return Err(format!("unknown flag '{other}'"));
                }
            }
        };
        options.push(option.to_string());
        options.extend(argument.map(str::to_string));
    }
    Ok(options)
}

/// What the oracle made of an input.
pub enum OracleResult {
    /// The typed dump of the value.
    Dump(J),
    /// libucl rejected the input.
    Error,
    /// The oracle ended on a signal, which the spec calls undefined behaviour.
    Crashed(String),
    TimedOut,
    /// Output that is not a dump: `too deep` for a dump nested deeper than `serde_json` reads,
    /// as for the conformance runner.
    BadOutput(String),
}

/// Runs the oracle on the file `input` from the working directory `cwd`.
pub fn run_oracle(
    binary: &Path,
    options: &[String],
    input: &Path,
    cwd: &Path,
    timeout: Duration,
) -> OracleResult {
    let mut child = match Command::new(binary)
        .args(options)
        .arg(input)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => return OracleResult::BadOutput(format!("cannot run {}: {e}", binary.display())),
    };
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        let _ = sender.send(bytes);
    });
    let Ok(bytes) = receiver.recv_timeout(timeout) else {
        let _ = child.kill();
        let _ = child.wait();
        return OracleResult::TimedOut;
    };
    let status = match child.wait() {
        Ok(status) => status,
        Err(e) => return OracleResult::BadOutput(format!("wait: {e}")),
    };
    if !status.success() {
        return match status.code() {
            None => OracleResult::Crashed(format!("{status}")),
            Some(_) => OracleResult::BadOutput(format!("{status}")),
        };
    }
    match serde_json::from_slice::<J>(&bytes) {
        Ok(dump) if dump.get("error").is_some() => OracleResult::Error,
        Ok(dump) => OracleResult::Dump(dump),
        Err(e) if e.to_string().contains("recursion limit") => {
            OracleResult::BadOutput("too deep".to_string())
        }
        Err(e) => OracleResult::BadOutput(format!("not JSON: {e}")),
    }
}

/// What the crate made of an input.
pub enum CrateResult {
    /// The dump of the value, or of the partial value of a silent stop (spec §9.4).
    Dump(J),
    Rejected {
        kind: ErrorKind,
        message: String,
    },
    Panicked(String),
}

/// Parses `bytes` as a document given as text, set up as the oracle runs with `setup` from the
/// working directory `base_dir`. With `dump-comments`, the dump has the saved comments, as the
/// oracle's does. Also returns what [`uncertain`] needs from the parse: the comments the crate
/// saved and the values they are attached to (spec §12.5), the uncertain rules the parse reached
/// that its result does not show ([`Parser::uncertain_reached`]), and the document's other units.
pub fn run_crate(setup: &Setup, base_dir: &Path, bytes: &[u8]) -> (CrateResult, CrateNotes) {
    let result = oracle::quietly(|| {
        let units = Rc::new(RefCell::new(Vec::new()));
        let mut parser = oracle::configure(setup, base_dir);
        parser.set_loader(RecordingLoader {
            units: Rc::clone(&units),
        });
        if setup.registered_macros {
            let units = Rc::clone(&units);
            parser.register_macro("emit", move |call| {
                units.borrow_mut().push(call.value().to_vec());
                oracle::emit_macro(call)
            });
        }
        let parsed = match parser.parse(bytes) {
            Ok(value) => Ok(value),
            Err(e) if e.is_stopped() => Ok(e.into_partial().expect("a stop keeps its result")),
            Err(e) => Err(e),
        };
        let mut notes = CrateNotes {
            comments: Vec::new(),
            uncertain: parser.uncertain_reached(),
            units: units.take(),
        };
        let value = match parsed {
            Ok(value) => value,
            Err(e) => return (Err(e), notes),
        };
        let comments = setup.dump_comments.then(|| oracle::comment_map(&parser));
        notes.comments = saved_comments(&parser);
        (Ok(oracle::dump(&value, comments.as_ref())), notes)
    });
    match result {
        Ok((Ok(dump), notes)) => (CrateResult::Dump(dump), notes),
        Ok((Err(e), notes)) => (
            CrateResult::Rejected {
                kind: e.kind().clone(),
                message: e.to_string(),
            },
            notes,
        ),
        Err(panic) => (
            CrateResult::Panicked(
                panic
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "a panic".to_string()),
            ),
            CrateNotes::default(),
        ),
    }
}

/// What the crate's parse tells [`uncertain`] besides its result.
#[derive(Default)]
pub struct CrateNotes {
    /// The comments the crate saved, in the order it read them ([`Parser::comments`]).
    pub comments: Vec<SavedComment>,
    pub uncertain: Vec<Uncertain>,
    /// The bytes of the document's units other than the input: every file the loader read, and
    /// every text the test macro `.emit` had parsed in place (spec §9.4, §13.2). A file that
    /// `.load` reads is among them, since the loader is not told why it reads (§9.6).
    pub units: Vec<Vec<u8>>,
}

/// The filesystem loader of `oracle::configure`, which also keeps the bytes of every file it
/// reads. Each method is the filesystem loader's own.
struct RecordingLoader {
    units: Rc<RefCell<Vec<Vec<u8>>>>,
}

impl RecordingLoader {
    fn keep(&self, read: io::Result<Vec<u8>>) -> io::Result<Vec<u8>> {
        if let Ok(bytes) = &read {
            self.units.borrow_mut().push(bytes.clone());
        }
        read
    }
}

impl Loader for RecordingLoader {
    fn current_dir(&self) -> io::Result<PathBuf> {
        FsLoader.current_dir()
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        FsLoader.canonicalize(path)
    }

    fn canonicalize_optional(&self, path: &Path) -> io::Result<PathBuf> {
        FsLoader.canonicalize_optional(path)
    }

    fn kind(&self, path: &Path) -> Option<FileKind> {
        FsLoader.kind(path)
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.keep(FsLoader.read(path))
    }

    fn read_limited(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        self.keep(FsLoader.read_limited(path, limit))
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<String>> {
        FsLoader.read_dir(path)
    }
}

/// The saved comments of the last parse, in the order read, each with the path of the value it
/// is attached to, or none when §8 replaced or discarded that value (spec §12.5).
fn saved_comments(parser: &Parser) -> Vec<SavedComment> {
    let mut values = vec![None; parser.comments().len()];
    for group in parser.attached_comments() {
        for &index in &group.comments {
            values[index] = Some(group.path.clone());
        }
    }
    parser
        .comments()
        .iter()
        .zip(values)
        .map(|(comment, value)| SavedComment {
            text: comment.text.clone(),
            value,
        })
        .collect()
}

/// How the crate differs from the oracle.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum Kind {
    /// libucl rejects the input; the crate accepts it.
    CrateAccepts,
    /// libucl accepts the input; the crate rejects it.
    CrateRejects,
    /// Both accept it, with different values.
    ValuesDiffer,
    /// The crate panicked.
    CratePanics,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::CrateAccepts => "crate-accepts",
            Kind::CrateRejects => "crate-rejects",
            Kind::ValuesDiffer => "values-differ",
            Kind::CratePanics => "crate-panics",
        }
    }
}

/// The comparison of the two results.
pub enum Verdict {
    Agree,
    /// Not compared, for the reason given: the oracle crashed, timed out or wrote no dump, or
    /// the crate rejected the input for a documented project divergence.
    Skipped(&'static str),
    Differs {
        kind: Kind,
        detail: String,
    },
}

/// Compares the oracle's result with the crate's, as `tests/conformance.rs` compares a case.
/// Only the project's divergences (spec README, *Divergences decided by the project*), what
/// libucl does not define (a crash) and the behaviour the spec leaves uncertain
/// ([`uncertain`]) are skipped.
pub fn compare(oracle: &OracleResult, krate: &CrateResult, ctx: &Context<'_>) -> Verdict {
    match compare_results(oracle, krate, ctx) {
        // The parse reached a rule the spec leaves uncertain, where libucl's result is undefined.
        Verdict::Differs { .. } if !ctx.uncertain.is_empty() => {
            Verdict::Skipped(uncertain::reached(ctx.uncertain[0]))
        }
        verdict => verdict,
    }
}

fn compare_results(oracle: &OracleResult, krate: &CrateResult, ctx: &Context<'_>) -> Verdict {
    if let CrateResult::Panicked(message) = krate {
        return Verdict::Differs {
            kind: Kind::CratePanics,
            detail: message.clone(),
        };
    }
    match (oracle, krate) {
        (OracleResult::Crashed(_), _) => Verdict::Skipped("oracle crashed (undefined in the spec)"),
        (OracleResult::TimedOut, _) => Verdict::Skipped("oracle timed out"),
        (OracleResult::BadOutput(what), _) if what == "too deep" => {
            Verdict::Skipped("dump deeper than serde_json reads")
        }
        (OracleResult::BadOutput(_), _) => Verdict::Skipped("oracle wrote no dump"),
        (OracleResult::Error, CrateResult::Rejected { .. }) => Verdict::Agree,
        (OracleResult::Error, CrateResult::Dump(value)) => {
            if uncertain::handler_in_single_include_path(ctx, value) {
                Verdict::Skipped(uncertain::HANDLER)
            } else {
                Verdict::Differs {
                    kind: Kind::CrateAccepts,
                    detail: format!("libucl rejects the input; crate accepted it as {value}"),
                }
            }
        }
        (
            OracleResult::Dump(golden),
            CrateResult::Rejected {
                kind: ErrorKind::MissingValue,
                ..
            },
        ) if uncertain::handler_in_single_emit_value(ctx, golden) => {
            Verdict::Skipped(uncertain::HANDLER)
        }
        (OracleResult::Dump(_), CrateResult::Rejected { kind, message }) => match kind {
            // Project divergences (spec README, *Divergences decided by the project*).
            ErrorKind::InvalidUtf8 => Verdict::Skipped("divergence: non-UTF-8"),
            ErrorKind::Unsupported { .. } => Verdict::Skipped("divergence: unsupported"),
            ErrorKind::ArgumentsTooDeep { .. } => Verdict::Skipped("divergence: argument depth"),
            ErrorKind::NestingTooDeep { .. } => Verdict::Skipped("divergence: nesting depth"),
            // Spec §9.4 and §13.2 leave libucl's reading of such a unit uncertain; the crate
            // reports the error at the `[`.
            ErrorKind::IncludeArrayRoot => Verdict::Skipped(uncertain::ARRAY_TEXT),
            _ => Verdict::Differs {
                kind: Kind::CrateRejects,
                detail: format!("crate error: {message}"),
            },
        },
        // libucl keeps bytes that are not UTF-8, which the dump writes as `hex`; the crate
        // never holds them (a project divergence). Only a registered macro's VALUE, which the
        // test macros copy lossily, gets them through.
        (OracleResult::Dump(golden), CrateResult::Dump(_)) if holds_hex(golden) => {
            Verdict::Skipped("divergence: non-UTF-8")
        }
        (OracleResult::Dump(golden), CrateResult::Dump(value)) => {
            let mut golden = golden.clone();
            oracle::strip_unobservable_priorities(&mut golden, true);
            let mut reasons = BTreeSet::new();
            uncertain::excuse(&mut golden, value, ctx, &mut reasons);
            match oracle::find_diff(&golden, value, "$") {
                None => reasons
                    .first()
                    .map_or(Verdict::Agree, |reason| Verdict::Skipped(reason)),
                Some(detail) => Verdict::Differs {
                    kind: Kind::ValuesDiffer,
                    detail,
                },
            }
        }
        (_, CrateResult::Panicked(_)) => unreachable!("handled above"),
    }
}

/// Whether a dump holds a string or key that is not UTF-8.
fn holds_hex(dump: &J) -> bool {
    match dump {
        J::Object(map) => map.contains_key("hex") || map.values().any(holds_hex),
        J::Array(items) => items.iter().any(holds_hex),
        _ => false,
    }
}

/// Everything one comparison needs besides the input.
pub struct Target<'a> {
    pub oracle: &'a Path,
    pub timeout: Duration,
    /// The file the input is written to for the oracle.
    pub file: &'a Path,
}

/// The outcome of one input, with both results for a report.
pub struct Checked {
    pub oracle: OracleResult,
    pub krate: CrateResult,
    pub verdict: Verdict,
}

/// Runs `bytes`, with the flags `flags` and the working directory `dir`, through both: the
/// oracle with the [`expectation_options`], the crate with `flags`.
pub fn check(target: &Target<'_>, bytes: &[u8], flags: &[String], dir: &Path) -> Checked {
    let options = expectation_options(flags).expect("the fuzzer's flags are known");
    fs::write(target.file, bytes).expect("the work file can be written");
    let oracle = run_oracle(target.oracle, &options, target.file, dir, target.timeout);
    check_against(oracle, bytes, flags, dir)
}

/// Runs `bytes` through the crate with `flags` from the working directory `dir` and compares its
/// result with `oracle`, the oracle's result with the [`expectation_flags`]. The recognisers of
/// uncertain behaviour see those flags too: the result expected with `zerocopy` is uncertain
/// where the result without it is (§12.2).
pub fn check_against(oracle: OracleResult, bytes: &[u8], flags: &[String], dir: &Path) -> Checked {
    let setup = Setup::from_flags(flags).expect("the fuzzer's flags are known");
    let (krate, notes) = run_crate(&setup, dir, bytes);
    let expected_flags = expectation_flags(flags);
    let ctx = Context {
        input: bytes,
        flags: &expected_flags,
        comments: &notes.comments,
        uncertain: &notes.uncertain,
        units: &notes.units,
    };
    let verdict = compare(&oracle, &krate, &ctx);
    Checked {
        oracle,
        krate,
        verdict,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The texts of the comments the crate attached to no value.
    fn dropped(comments: &[SavedComment]) -> Vec<&str> {
        comments
            .iter()
            .filter(|comment| comment.value.is_none())
            .map(|comment| comment.text.as_str())
            .collect()
    }

    fn flags(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn options_follow_the_golden_script() {
        let options = oracle_options(&flags(&[
            "string-input",
            "var:X=y",
            "priority:3",
            "strategy:merge",
            "no-implicit-arrays",
        ]))
        .unwrap();
        assert_eq!(options, ["-S", "-v", "X=y", "-p", "3", "-s", "merge", "-I"]);
        assert!(oracle_options(&flags(&["nonsense"])).is_err());
    }

    #[test]
    fn verdicts() {
        let dump = |text: &str| {
            let setup = Setup::from_flags(&flags(&["string-input"])).unwrap();
            run_crate(&setup, Path::new("."), text.as_bytes()).0
        };
        let ctx = Context {
            input: b"",
            flags: &[],
            comments: &[],
            uncertain: &[],
            units: &[],
        };
        let compare = |oracle: &OracleResult, krate: &CrateResult| compare(oracle, krate, &ctx);
        let value = |text: &str| match dump(text) {
            CrateResult::Dump(value) => OracleResult::Dump(value),
            _ => panic!("{text} parses"),
        };
        assert!(matches!(
            compare(&value("a = 1"), &dump("a = 1")),
            Verdict::Agree
        ));
        assert!(matches!(
            compare(&value("a = 1"), &dump("a = 2")),
            Verdict::Differs {
                kind: Kind::ValuesDiffer,
                ..
            }
        ));
        assert!(matches!(
            compare(&OracleResult::Error, &dump("a = 1")),
            Verdict::Differs {
                kind: Kind::CrateAccepts,
                ..
            }
        ));
        assert!(matches!(
            compare(&OracleResult::Error, &dump("a = {")),
            Verdict::Agree
        ));
        assert!(matches!(
            compare(&value("a = 1"), &dump("a = {")),
            Verdict::Differs {
                kind: Kind::CrateRejects,
                ..
            }
        ));
        assert!(matches!(
            compare(&OracleResult::Crashed("signal 11".into()), &dump("a = 1")),
            Verdict::Skipped(_)
        ));
    }

    #[test]
    fn mixed_handler_include_path_skips_only_the_undefined_empty_result() {
        let input = b".include(g=true,y=e)\"${H_}.*\"";
        let make_verdict = |input: &[u8], flags: &[String]| {
            let setup = Setup::from_flags(flags).unwrap();
            let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../tests/conformance/cases/spec/09-macros");
            let (krate, notes) = run_crate(&setup, &dir, input);
            let ctx = Context {
                input,
                flags,
                comments: &notes.comments,
                uncertain: &notes.uncertain,
                units: &notes.units,
            };
            compare(&OracleResult::Error, &krate, &ctx)
        };
        let handler = flags(&["string-input", "variable-handler"]);
        assert!(matches!(
            make_verdict(input, &handler),
            Verdict::Skipped(uncertain::HANDLER)
        ));
        assert!(matches!(
            make_verdict(input, &flags(&["string-input"])),
            Verdict::Differs {
                kind: Kind::CrateAccepts,
                ..
            }
        ));
        assert!(matches!(
            make_verdict(
                input,
                &flags(&["string-input", "variable-handler", "var:H_=x"])
            ),
            Verdict::Differs {
                kind: Kind::CrateAccepts,
                ..
            }
        ));
        assert!(matches!(
            make_verdict(b".include(g=true,y=e)\"${H_}.*\"\nx=1", &handler),
            Verdict::Differs {
                kind: Kind::CrateAccepts,
                ..
            }
        ));
    }

    #[test]
    fn mixed_handler_emit_text_rejection_skips_only_its_undefined_value() {
        let input = b".emit \"x=${H_}c\"";
        let flags = flags(&[
            "registered-macros",
            "dump-comments",
            "string-input",
            "variable-handler",
        ]);
        let setup = Setup::from_flags(&flags).unwrap();
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/conformance/cases/spec/13-inputs");
        let (krate, notes) = run_crate(&setup, &dir, input);
        assert!(matches!(
            &krate,
            CrateResult::Rejected {
                kind: ErrorKind::MissingValue,
                ..
            }
        ));
        let ctx = Context {
            input,
            flags: &flags,
            comments: &notes.comments,
            uncertain: &notes.uncertain,
            units: &notes.units,
        };
        let oracle = |key: &str| {
            OracleResult::Dump(serde_json::json!({
                "t":"object","entries":[{"k":key,"v":[{"t":"string","v":"[handled]\0"}]}]
            }))
        };
        assert!(matches!(
            compare(&oracle("x"), &krate, &ctx),
            Verdict::Skipped(uncertain::HANDLER)
        ));
        assert!(matches!(
            compare(&oracle("y"), &krate, &ctx),
            Verdict::Differs {
                kind: Kind::CrateRejects,
                ..
            }
        ));

        let extra = b".emit \"x=${H_}c\"\nother=1";
        let extra_ctx = Context {
            input: extra,
            flags: ctx.flags,
            comments: ctx.comments,
            uncertain: ctx.uncertain,
            units: ctx.units,
        };
        assert!(matches!(
            compare(&oracle("x"), &krate, &extra_ctx),
            Verdict::Differs {
                kind: Kind::CrateRejects,
                ..
            }
        ));
        let wrong_error = CrateResult::Rejected {
            kind: ErrorKind::UnexpectedTerminator,
            message: "other error".into(),
        };
        assert!(matches!(
            compare(&oracle("x"), &wrong_error, &ctx),
            Verdict::Differs {
                kind: Kind::CrateRejects,
                ..
            }
        ));
    }

    #[test]
    fn duplicate_reappearing_rewrite_comment_only_on_later_value() {
        let input = b"# c\na d\na 2\nk d# c";
        let run_flags = flags(&[
            "dump-comments",
            "strategy:rewrite",
            "string-input",
            "no-filevars",
        ]);
        let setup = Setup::from_flags(&run_flags).unwrap();
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/conformance/cases/spec/12-flags");
        let (krate, notes) = run_crate(&setup, &dir, input);
        assert!(matches!(&krate, CrateResult::Dump(_)));
        assert_eq!(dropped(&notes.comments), ["# c"]);
        let ctx = Context {
            input,
            flags: &run_flags,
            comments: &notes.comments,
            uncertain: &notes.uncertain,
            units: &notes.units,
        };
        let golden = serde_json::json!({"t":"object","entries":[
            {"k":"a","v":[{"t":"int","v":"2"}]},
            {"k":"k","v":[{"t":"string","v":"d","c":["# c","# c"]}]}
        ]});
        let verdict = |dump| compare(&OracleResult::Dump(dump), &krate, &ctx);
        assert!(matches!(
            verdict(golden.clone()),
            Verdict::Skipped(uncertain::REPLACED_COMMENTS)
        ));

        let mut changed_value = golden.clone();
        changed_value["entries"][1]["v"][0]["v"] = serde_json::json!("q");
        assert!(matches!(verdict(changed_value), Verdict::Differs { .. }));
        let mut unrelated_comment = golden.clone();
        unrelated_comment["entries"][1]["v"][0]["c"][0] = serde_json::json!("# x");
        assert!(matches!(
            verdict(unrelated_comment),
            Verdict::Differs { .. }
        ));
        let mut extra_entry = golden.clone();
        extra_entry["entries"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"k":"z","v":[{"t":"int","v":"1"}]}));
        assert!(matches!(verdict(extra_entry), Verdict::Differs { .. }));

        let mut changed_first_value = golden;
        changed_first_value["entries"][0]["v"][0]["v"] = serde_json::json!("3");
        assert!(matches!(
            verdict(changed_first_value),
            Verdict::Differs { .. }
        ));
    }

    #[test]
    fn the_expectation_drops_only_zerocopy() {
        let with = flags(&[
            "zerocopy",
            "registered-macros",
            "string-input",
            "priority:3",
        ]);
        let expected = expectation_flags(&with);
        assert_eq!(
            expected,
            flags(&["registered-macros", "string-input", "priority:3"])
        );
        let options = oracle_options(&expected).unwrap();
        assert!(!options.iter().any(|option| option == "-z"));
        assert_eq!(options, ["-R", "-S", "-p", "3"]);
        let without = flags(&["no-time", "string-input", "var:zerocopy=1"]);
        assert_eq!(expectation_flags(&without), without);
    }

    /// The crate's verdict with `run_flags` in `cases/spec/12-flags` against the oracle's `dump`,
    /// through [`check_against`], as the fuzzer compares.
    fn verdict_12_flags(input: &str, run_flags: &[String], dump: &J) -> Verdict {
        verdict_in("12-flags", input, run_flags, dump)
    }

    /// The same in `cases/spec/<case_dir>`.
    fn verdict_in(case_dir: &str, input: &str, run_flags: &[String], dump: &J) -> Verdict {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/conformance/cases/spec")
            .join(case_dir);
        let checked = check_against(
            OracleResult::Dump(dump.clone()),
            input.as_bytes(),
            run_flags,
            &dir,
        );
        assert!(matches!(&checked.krate, CrateResult::Dump(_)), "{input:?}");
        checked.verdict
    }

    fn values_differ(verdict: &Verdict) -> bool {
        matches!(
            verdict,
            Verdict::Differs {
                kind: Kind::ValuesDiffer,
                ..
            }
        )
    }

    /// C14 finding `values-differ-79ff7be944c9f2ba`. Under `zerocopy` the expected result is the
    /// oracle's without it (§12.2), which defines the emitted text's bytes, so every difference
    /// from it is reported, the bytes libucl changes with `zerocopy` included.
    #[test]
    fn zerocopy_is_compared_with_the_result_without_it() {
        let run_flags = flags(&["zerocopy", "registered-macros", "string-input"]);
        let input = "t I\n.emit n $ABI\nl=t";
        let entry = |k: &str, v: &str| serde_json::json!({"k": k, "v": [{"t": "string", "v": v}]});
        let dump = |entries: Vec<J>| serde_json::json!({"t": "object", "entries": entries});
        let expected = dump(vec![
            entry("t", "I"),
            entry("n", "unknown"),
            entry("l", "t"),
        ]);
        assert!(matches!(
            verdict_12_flags(input, &run_flags, &expected),
            Verdict::Agree
        ));
        for changed in [
            dump(vec![
                entry("t", "I"),
                entry("m", "unknown"),
                entry("l", "t"),
            ]),
            dump(vec![
                entry("t", "I"),
                entry("n", "unknowx"),
                entry("l", "t"),
            ]),
            dump(vec![
                entry("t", "J"),
                entry("n", "unknown"),
                entry("l", "t"),
            ]),
            dump(vec![
                entry("t", "I"),
                entry("n", "unknown"),
                entry("l", "u"),
            ]),
            dump(vec![
                entry("t", "I"),
                entry("n", "unknown"),
                entry("l", "t"),
                entry("x", "1"),
            ]),
            dump(vec![entry("t", "I"), entry("n", "unknown")]),
            // The oracle's result with `zerocopy`: nothing excuses these bytes any more.
            dump(vec![
                entry("t", "I"),
                entry("\u{0}", "\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}"),
                entry("l", "t"),
            ]),
        ] {
            assert!(
                values_differ(&verdict_12_flags(input, &run_flags, &changed)),
                "{changed}"
            );
        }
    }

    /// QUESTIONS #84: with `zerocopy`, libucl gives a separate entry to a later key; §12.2 makes
    /// that undefined, and the expected result is the one entry without `zerocopy`.
    #[test]
    fn zerocopy_later_key_expects_the_entry_without_zerocopy() {
        let run_flags = flags(&["zerocopy", "registered-macros", "string-input"]);
        let input = ".emit l $ABI\nl = s";
        let one_entry = serde_json::json!({"t": "object", "entries": [
            {"k": "l", "v": [{"t": "string", "v": "unknown"}, {"t": "string", "v": "s"}]}
        ]});
        assert!(matches!(
            verdict_12_flags(input, &run_flags, &one_entry),
            Verdict::Agree
        ));
        let two_entries = serde_json::json!({"t": "object", "entries": [
            {"k": "\u{0}", "v": [{"t": "string", "v": "\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}"}]},
            {"k": "l", "v": [{"t": "string", "v": "s"}]}
        ]});
        assert!(values_differ(&verdict_12_flags(
            input,
            &run_flags,
            &two_entries
        )));
    }

    /// C14 finding `values-differ-0bdbfbd65e9562d3`, with the oracle's result without
    /// `zerocopy`: the root's priority has no effect (§8.7) and is not compared, the entries'
    /// priorities are.
    #[test]
    fn zerocopy_under_priority_compares_the_entries_priorities() {
        let run_flags = flags(&[
            "zerocopy",
            "registered-macros",
            "string-input",
            "priority:3",
        ]);
        let input = "n I\n.emit l=$ABI\n.s";
        let expected = serde_json::json!({"t": "object", "pri": 3, "entries": [
            {"k": "n", "v": [{"t": "string", "v": "I", "pri": 3}]},
            {"k": "l", "v": [{"t": "string", "v": "unknown", "pri": 3}]}
        ]});
        assert!(matches!(
            verdict_12_flags(input, &run_flags, &expected),
            Verdict::Agree
        ));
        for (index, priority) in [(0, 0), (1, 0), (1, 5)] {
            let mut changed = expected.clone();
            changed["entries"][index]["v"][0]["pri"] = serde_json::json!(priority);
            assert!(
                values_differ(&verdict_12_flags(input, &run_flags, &changed)),
                "{index} {priority}"
            );
        }
    }

    /// The uncertain rules apply to the expected result with `zerocopy` as without it (§12.2):
    /// the §12.5 recogniser, which needs its exact flags, still sees them.
    #[test]
    fn zerocopy_keeps_the_uncertain_rules_of_the_result_without_it() {
        let input = "# c\na d\na 2\nk d# c";
        let golden = serde_json::json!({"t":"object","entries":[
            {"k":"a","v":[{"t":"int","v":"2"}]},
            {"k":"k","v":[{"t":"string","v":"d","c":["# c","# c"]}]}
        ]});
        let mut changed = golden.clone();
        changed["entries"][1]["v"][0]["v"] = serde_json::json!("q");
        for extra in [None, Some("zerocopy")] {
            let mut run_flags = flags(&[
                "dump-comments",
                "strategy:rewrite",
                "string-input",
                "no-filevars",
            ]);
            run_flags.extend(extra.map(str::to_string));
            assert!(
                matches!(
                    verdict_12_flags(input, &run_flags, &golden),
                    Verdict::Skipped(uncertain::REPLACED_COMMENTS)
                ),
                "{run_flags:?}"
            );
            assert!(values_differ(&verdict_12_flags(
                input, &run_flags, &changed
            )));
        }
    }

    /// C14 finding `values-differ-244706530c4e7da6`: `.ctx` copies a string with a NUL byte in an
    /// included file, whose bytes after the NUL libucl leaves undefined (§9.7, §13.2). The crate
    /// parse records the file, so the copy counts; the oracle's dump is that of the finding.
    #[test]
    fn nul_bytes_in_a_copy_made_in_an_included_file() {
        let run_flags = flags(&["registered-macros", "string-input"]);
        let with = |file: &str| format!("a \\u000x\"\no {{.include \"files/{file}\"}}");
        let input = with("macro_ctx.inc");
        let oracle = |copied: &str| {
            serde_json::json!({"t":"object","entries":[
                {"k":"a","v":[{"t":"string","v":"\u{0}\""}]},
                {"k":"o","v":[{"t":"object","entries":[
                    {"k":"a","v":[{"t":"int","v":"1"}]},
                    {"k":"ctx","v":[{"t":"object","entries":[
                        {"k":"a","v":[{"t":"string","v":copied}]},
                        {"k":"o","v":[{"t":"object","entries":[
                            {"k":"a","v":[{"t":"int","v":"1"}]}
                        ]}]}
                    ]}]}
                ]}]}
            ]})
        };
        assert!(matches!(
            verdict_in("13-inputs", &input, &run_flags, &oracle("\u{0}\u{0}")),
            Verdict::Skipped(uncertain::NUL_IN_COPY)
        ));
        for copied in ["x\u{0}", "\u{0}\u{0}\u{0}", "\u{0}"] {
            assert!(
                values_differ(&verdict_in(
                    "13-inputs",
                    &input,
                    &run_flags,
                    &oracle(copied)
                )),
                "{copied:?}"
            );
        }
        let mut changed = oracle("\u{0}\u{0}");
        changed["entries"][1]["v"][0]["entries"][0]["v"][0]["v"] = serde_json::json!("2");
        assert!(values_differ(&verdict_in(
            "13-inputs",
            &input,
            &run_flags,
            &changed
        )));
        // An included file without a copying macro: the NUL string is compared as it is.
        let plain = with("macro_x1.inc");
        let dump = |a: &str| {
            serde_json::json!({"t":"object","entries":[
                {"k":"a","v":[{"t":"string","v":a}]},
                {"k":"o","v":[{"t":"object","entries":[{"k":"x","v":[{"t":"int","v":"1"}]}]}]}
            ]})
        };
        assert!(matches!(
            verdict_in("13-inputs", &plain, &run_flags, &dump("\u{0}\"")),
            Verdict::Agree
        ));
        assert!(values_differ(&verdict_in(
            "13-inputs",
            &plain,
            &run_flags,
            &dump("\u{0}\u{0}")
        )));
    }

    /// Text that `.emit` parses in place counts as a unit: the macro name comes only from a
    /// variable here, and the oracle's dump is its result for this document.
    #[test]
    fn nul_bytes_in_a_copy_made_in_text_parsed_in_place() {
        let run_flags = flags(&["registered-macros", "string-input", "var:I=.inherit"]);
        let input = "d { a = \"\\u0000x\" }\ne { .emit \"$I d\" }";
        assert!(!input.contains(".inherit"));
        let oracle = |copied: &str| {
            serde_json::json!({"t":"object","entries":[
                {"k":"d","v":[{"t":"object","entries":[{"k":"a","v":[{"t":"string","v":"\u{0}x"}]}]}]},
                {"k":"e","v":[{"t":"object","entries":[{"k":"a","v":[{"t":"string","v":copied}]}]}]}
            ]})
        };
        assert!(matches!(
            verdict_12_flags(input, &run_flags, &oracle("\u{0}\u{0}")),
            Verdict::Skipped(uncertain::NUL_IN_COPY)
        ));
        assert!(values_differ(&verdict_12_flags(
            input,
            &run_flags,
            &oracle("y\u{0}")
        )));
    }

    /// C14 finding `values-differ-6dc7de42c47fd96a` (§12.5, QUESTIONS #81): the comment `# c` of
    /// the value `d` that `rewrite` replaced appears before the last value's own `# c`. The
    /// oracle's dumps here are its results, or nearby differences built on them.
    #[test]
    fn replaced_comment_of_the_same_text_before_a_later_values_own() {
        let rewrite = flags(&["dump-comments", "strategy:rewrite", "string-input"]);
        let input = "c p\na=\n# c\nv\na d\na p\na=\n# c\nv";
        let dump = |c: J, a: J| {
            serde_json::json!({"t":"object","entries":[
                {"k":"c","v":[c]},
                {"k":"a","v":[a]}
            ]})
        };
        let c = serde_json::json!({"t":"string","v":"p"});
        let a = |key: &str, texts: &[&str], v: &str| serde_json::json!({"t":"string","v":v, key: texts});
        let oracle = dump(c.clone(), a("c", &["# c", "# c"], "v"));
        assert!(matches!(
            verdict_12_flags(input, &rewrite, &oracle),
            Verdict::Skipped(uncertain::REPLACED_COMMENTS)
        ));
        for changed in [
            // A value changed, or another value's comments. (A value without comments of its
            // own in the crate that gets a dropped comment is excused whenever it was created:
            // nothing orders it, see `uncertain::replaced_comments_first`.)
            dump(c.clone(), a("c", &["# c", "# c"], "w")),
            dump(
                serde_json::json!({"t":"string","v":"p","c":["# z"]}),
                a("c", &["# c", "# c"], "v"),
            ),
            // More comments than the crate dropped, or one after the value's own.
            dump(c.clone(), a("c", &["# c", "# c", "# c"], "v")),
            dump(c.clone(), a("ca", &["# c", "# d"], "v")),
            dump(c.clone(), a("c", &["# d", "# c"], "v")),
        ] {
            assert!(
                values_differ(&verdict_12_flags(input, &rewrite, &changed)),
                "{changed}"
            );
        }
        // Under `append` nothing is replaced, so nothing is dropped.
        let append = flags(&["dump-comments", "strategy:append", "string-input"]);
        let appended = |last: J| {
            serde_json::json!({"t":"object","entries":[
                {"k":"c","v":[{"t":"string","v":"p"}]},
                {"k":"a","v":[
                    {"t":"string","v":"v"},
                    {"t":"string","v":"d","c":["# c"]},
                    {"t":"string","v":"p"},
                    last
                ]}
            ]})
        };
        assert!(matches!(
            verdict_12_flags(input, &append, &appended(a("ca", &["# c"], "v"))),
            Verdict::Agree
        ));
        assert!(values_differ(&verdict_12_flags(
            input,
            &append,
            &appended(a("c", &["# c", "# c"], "v"))
        )));
        // With a distinct first comment: before the own one it is excused, after it not.
        let distinct = "c p\na=\n# x\nv\na d\na p\na=\n# c\nv";
        assert!(matches!(
            verdict_12_flags(
                distinct,
                &rewrite,
                &dump(c.clone(), a("c", &["# x", "# c"], "v"))
            ),
            Verdict::Skipped(uncertain::REPLACED_COMMENTS)
        ));
        assert!(values_differ(&verdict_12_flags(
            distinct,
            &rewrite,
            &dump(c, a("ca", &["# c", "# x"], "v"))
        )));
    }

    /// The outermost object of a section path counts as the most recent value when its bracket
    /// closes (§12.5), though it was created before the replaced value: a dropped comment before
    /// its own `# z` is not excused there.
    #[test]
    fn replaced_comment_not_excused_on_an_earlier_section_object() {
        let rewrite = flags(&["dump-comments", "strategy:rewrite", "string-input"]);
        let input = "a b {\n# c\nk 1\nk 2\n} # z";
        let dump = |key: &str, texts: &[&str]| {
            serde_json::json!({"t":"object","entries":[{"k":"a","v":[{
                "t":"object",
                key: texts,
                "entries":[{"k":"b","v":[{"t":"object","entries":[
                    {"k":"k","v":[{"t":"int","v":"2"}]}
                ]}]}]
            }]}]})
        };
        assert!(matches!(
            verdict_12_flags(input, &rewrite, &dump("ca", &["# z"])),
            Verdict::Agree
        ));
        assert!(values_differ(&verdict_12_flags(
            input,
            &rewrite,
            &dump("c", &["# c", "# z"])
        )));
    }

    #[test]
    fn dropped_comment_cannot_reappear_on_an_earlier_value() {
        let input = b"p { v 1 # c\n}\n# c\na 1\na 2\n";
        let run_flags = flags(&["dump-comments", "strategy:rewrite", "string-input"]);
        let setup = Setup::from_flags(&run_flags).unwrap();
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/conformance/cases/spec/12-flags");
        let (krate, notes) = run_crate(&setup, &dir, input);
        assert!(matches!(&krate, CrateResult::Dump(_)));
        assert_eq!(dropped(&notes.comments), ["# c"]);
        let ctx = Context {
            input,
            flags: &run_flags,
            comments: &notes.comments,
            uncertain: &notes.uncertain,
            units: &notes.units,
        };
        let wrong_earlier_comment = serde_json::json!({"t":"object","entries":[
            {"k":"p","v":[{"t":"object","entries":[
                {"k":"v","v":[{"t":"int","v":"1","c":["# c","# c"]}]}
            ]}]},
            {"k":"a","v":[{"t":"int","v":"2"}]}
        ]});
        assert!(matches!(
            compare(&OracleResult::Dump(wrong_earlier_comment), &krate, &ctx),
            Verdict::Differs {
                kind: Kind::ValuesDiffer,
                ..
            }
        ));
    }
}

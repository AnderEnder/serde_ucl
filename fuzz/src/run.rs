//! One input through the oracle and through the crate, and the comparison of the two results as
//! the conformance runner compares a case with its golden file (`tests/common/oracle.rs`).

use crate::oracle::{self, Setup};
use crate::uncertain::{self, Context};
use serde_json::Value as J;
use serde_ucl::parse::{ErrorKind, Parser, Uncertain};
use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

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
/// oracle's does. Also returns what [`uncertain`] needs from the parse: the texts of the comments
/// the crate saved but attached to no value (spec §12.5), and the uncertain rules the parse
/// reached that its result does not show ([`Parser::uncertain_reached`]).
pub fn run_crate(setup: &Setup, base_dir: &Path, bytes: &[u8]) -> (CrateResult, CrateNotes) {
    let result = oracle::quietly(|| {
        let mut parser = oracle::configure(setup, base_dir);
        let parsed = match parser.parse(bytes) {
            Ok(value) => Ok(value),
            Err(e) if e.is_stopped() => Ok(e.into_partial().expect("a stop keeps its result")),
            Err(e) => Err(e),
        };
        let mut notes = CrateNotes {
            dropped_comments: Vec::new(),
            uncertain: parser.uncertain_reached(),
        };
        let value = match parsed {
            Ok(value) => value,
            Err(e) => return (Err(e), notes),
        };
        let comments = setup.dump_comments.then(|| oracle::comment_map(&parser));
        notes.dropped_comments = dropped_comments(&parser);
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
    pub dropped_comments: Vec<String>,
    pub uncertain: Vec<Uncertain>,
}

/// The texts of the saved comments that are attached to no value of the result.
fn dropped_comments(parser: &Parser) -> Vec<String> {
    let attached: BTreeSet<usize> = parser
        .attached_comments()
        .iter()
        .flat_map(|group| group.comments.iter().copied())
        .collect();
    parser
        .comments()
        .iter()
        .enumerate()
        .filter(|(i, _)| !attached.contains(i))
        .map(|(_, comment)| comment.text.clone())
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
        (OracleResult::Error, CrateResult::Dump(value)) => Verdict::Differs {
            kind: Kind::CrateAccepts,
            detail: format!("libucl rejects the input; crate accepted it as {value}"),
        },
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

/// Runs `bytes`, with the flags `flags` and the working directory `dir`, through both.
pub fn check(target: &Target<'_>, bytes: &[u8], flags: &[String], dir: &Path) -> Checked {
    let setup = Setup::from_flags(flags).expect("the fuzzer's flags are known");
    let options = oracle_options(flags).expect("the fuzzer's flags are known");
    fs::write(target.file, bytes).expect("the work file can be written");
    let oracle = run_oracle(target.oracle, &options, target.file, dir, target.timeout);
    let (krate, notes) = run_crate(&setup, dir, bytes);
    let ctx = Context {
        input: bytes,
        flags,
        dropped_comments: &notes.dropped_comments,
        uncertain: &notes.uncertain,
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
            dropped_comments: &[],
            uncertain: &[],
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
}

//! The differential fuzzer: parses generated inputs with the crate and with libucl (the oracle,
//! `target/libucl-oracle/ucl-dump`, run as a black box) and reports where the results differ.
//! The inputs are mutations of the conformance cases and documents from a small grammar. Each
//! difference is reduced to a small input and saved with a report. `fuzz/README.md` says how to
//! run it and how to read what it finds.

// The conformance runner's code for running the crate as the oracle runs a case. This crate uses
// part of it.
#[allow(dead_code)]
#[path = "../../tests/common/oracle.rs"]
mod oracle;

mod generate;
mod run;
mod uncertain;

use generate::Rng;
use run::{Checked, CrateResult, Kind, OracleResult, Target, Verdict};
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const USAGE: &str = "\
usage: ucl-differential [OPTIONS]
       ucl-differential --replay FINDING...
       ucl-differential --check FILE [--flag FLAG]... [--dir DIR]

Options:
  --seconds N    stop after N seconds (default 60; 0 for no limit)
  --runs N       stop after N inputs in all (default: no limit)
  --seed N       seed of the generator (default: from the clock; the run prints it)
  --jobs N       worker threads (default: the available parallelism)
  --max-len N    the longest input, in bytes (default 4096)
  --timeout N    seconds the oracle may take for one input (default 10)
  --oracle PATH  the oracle (default: target/libucl-oracle/ucl-dump of the repository)
  --out DIR      where findings and work files go (default: target/fuzz-differential)
  --replay       run the saved findings given as directories again and print their verdicts
  --check FILE   run one file through both and print both results and the verdict; --flag
                 adds a .flags entry (string-input is always added), --dir sets the oracle's
                 working directory (default: the current one)";

struct Options {
    seconds: u64,
    runs: Option<u64>,
    seed: u64,
    jobs: usize,
    max_len: usize,
    timeout: Duration,
    oracle: PathBuf,
    out: PathBuf,
    replay: Vec<PathBuf>,
    check: Option<PathBuf>,
    check_flags: Vec<String>,
    check_dir: Option<PathBuf>,
}

/// The repository's root directory.
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("fuzz/ is in the repository")
        .to_path_buf()
}

fn parse_options() -> Result<Options, String> {
    let root = repository();
    let mut options = Options {
        seconds: 60,
        runs: None,
        seed: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64),
        jobs: thread::available_parallelism().map_or(1, |n| n.get()),
        max_len: 4096,
        timeout: Duration::from_secs(10),
        oracle: root.join("target/libucl-oracle/ucl-dump"),
        out: root.join("target/fuzz-differential"),
        replay: Vec::new(),
        check: None,
        check_flags: Vec::new(),
        check_dir: None,
    };
    let mut args = std::env::args().skip(1);
    let mut replay = false;
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        let number = |name: &str, text: String| {
            text.parse::<u64>()
                .map_err(|_| format!("{name}: not a number: {text}"))
        };
        match arg.as_str() {
            "--seconds" => options.seconds = number("--seconds", value("--seconds")?)?,
            "--runs" => options.runs = Some(number("--runs", value("--runs")?)?),
            "--seed" => options.seed = number("--seed", value("--seed")?)?,
            "--jobs" => options.jobs = number("--jobs", value("--jobs")?)?.max(1) as usize,
            "--max-len" => options.max_len = number("--max-len", value("--max-len")?)? as usize,
            "--timeout" => {
                options.timeout = Duration::from_secs(number("--timeout", value("--timeout")?)?)
            }
            "--oracle" => options.oracle = PathBuf::from(value("--oracle")?),
            "--out" => options.out = PathBuf::from(value("--out")?),
            "--replay" => replay = true,
            "--check" => options.check = Some(PathBuf::from(value("--check")?)),
            "--flag" => options.check_flags.push(value("--flag")?),
            "--dir" => options.check_dir = Some(PathBuf::from(value("--dir")?)),
            "-h" | "--help" => return Err(String::new()),
            other if replay && !other.starts_with('-') => options.replay.push(other.into()),
            other => return Err(format!("unknown argument '{other}'")),
        }
    }
    if replay && options.replay.is_empty() {
        return Err("--replay needs at least one finding directory".to_string());
    }
    // The oracle runs in the seeds' directories, so every path it is given must be absolute.
    let here = std::env::current_dir().map_err(|e| format!("no working directory: {e}"))?;
    options.oracle = here.join(&options.oracle);
    options.out = here.join(&options.out);
    options.check = options.check.map(|file| here.join(file));
    options.check_dir = Some(here.join(options.check_dir.unwrap_or_default()));
    Ok(options)
}

/// A conformance case as a seed: its bytes, its directory (the oracle's working directory, so
/// that its relative include paths resolve, spec §9.3) and its flags.
struct Seed {
    id: String,
    bytes: Vec<u8>,
    dir: PathBuf,
    flags: Vec<String>,
}

/// Flags the fuzzer drops from a seed's: `string-input`, which it adds to every input. The
/// others stay, `dump-comments` (comments are compared, as the conformance runner compares them)
/// and `variable-handler` included; the behaviour of theirs that the spec leaves uncertain is
/// recognised in `uncertain.rs`.
const DROPPED_FLAGS: &[&str] = &["string-input"];

/// Flags the fuzzer adds to a seed's now and then.
const EXTRA_FLAGS: &[&str] = &[
    "key-lowercase",
    "no-time",
    "no-implicit-arrays",
    "disable-macro",
    "no-filevars",
    "zerocopy",
    "strategy:merge",
    "strategy:rewrite",
    "strategy:error",
    "strategy:append",
    "priority:3",
    "registered-macros",
    "dump-comments",
    "variable-handler",
];

/// The cases of `tests/conformance/`, as the conformance runner finds them.
fn seeds(root: &Path) -> Vec<Seed> {
    let conformance = root.join("tests/conformance");
    let mut files = Vec::new();
    collect(&conformance.join("libucl/basic"), "in", false, &mut files);
    collect(&conformance.join("cases"), "ucl", true, &mut files);
    files.sort();
    files
        .into_iter()
        .map(|file| {
            let stem = file.file_stem().unwrap().to_string_lossy().into_owned();
            let dir = file
                .parent()
                .unwrap()
                .canonicalize()
                .expect("a case directory");
            let mut flags: Vec<String> = oracle::read_flags(&dir.join(format!("{stem}.flags")))
                .into_iter()
                .filter(|flag| !DROPPED_FLAGS.contains(&flag.as_str()))
                .collect();
            // Parsed as text on both sides: the file variables then do not depend on where the
            // input is written.
            flags.push("string-input".to_string());
            let id = file
                .strip_prefix(&conformance)
                .unwrap_or(&file)
                .with_extension("")
                .to_string_lossy()
                .into_owned();
            Seed {
                id,
                bytes: fs::read(&file).expect("a case can be read"),
                dir,
                flags,
            }
        })
        .collect()
}

fn collect(dir: &Path, extension: &str, recursive: bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            if recursive {
                collect(&path, extension, recursive, out);
            }
        } else if path.extension().is_some_and(|e| e == extension) {
            out.push(path);
        }
    }
}

/// One generated input: its bytes, flags, working directory, and the seed it came from.
struct Input {
    bytes: Vec<u8>,
    flags: Vec<String>,
    dir: PathBuf,
    origin: String,
}

fn next_input(rng: &mut Rng, seeds: &[Seed], max_len: usize) -> Input {
    let seed = rng.pick(seeds);
    let (bytes, mut flags, origin) = if rng.chance(20) {
        (
            generate::generate(rng),
            vec!["string-input".to_string()],
            format!("generated, in the directory of {}", seed.id),
        )
    } else {
        let other = rng.pick(seeds);
        (
            generate::mutate(rng, &seed.bytes, &other.bytes, max_len),
            seed.flags.clone(),
            format!("a mutation of {}", seed.id),
        )
    };
    if rng.chance(25) {
        let extra = *rng.pick(EXTRA_FLAGS);
        let prefix = extra.split(':').next().unwrap();
        flags.retain(|flag| flag.split(':').next() != Some(prefix) || !extra.contains(':'));
        if !flags.iter().any(|flag| flag == extra) {
            flags.push(extra.to_string());
        }
    }
    Input {
        bytes,
        flags,
        dir: seed.dir.clone(),
        origin,
    }
}

/// What a run has found so far.
#[derive(Default)]
struct Tally {
    agree: u64,
    skipped: BTreeMap<&'static str, u64>,
    differs: BTreeMap<Kind, u64>,
    /// Differences by [`signature`]; only the first `REDUCED_PER_SIGNATURE` of each are
    /// reduced and saved, since the same difference tends to come back many times.
    signatures: BTreeMap<String, u64>,
    /// The saved findings, by directory name.
    saved: Vec<String>,
    seen: HashSet<String>,
}

/// How many inputs with the same [`signature`] are reduced and saved.
const REDUCED_PER_SIGNATURE: u64 = 3;

/// A rough class of a difference, so that one bug found many times is reduced only a few times:
/// the crate's error kind for a rejection, the two value types at the first difference, and for
/// an input the crate accepts, the conformance directory of its seed.
fn signature(kind: Kind, detail: &str, krate: &CrateResult, seed: &str) -> String {
    let class = match (kind, krate) {
        (Kind::CrateRejects, CrateResult::Rejected { kind, .. }) => {
            let name = format!("{kind:?}");
            name.split([' ', '{', '('])
                .next()
                .unwrap_or_default()
                .to_string()
        }
        (Kind::ValuesDiffer, _) => value_types(detail),
        (Kind::CratePanics, _) => detail.chars().take(60).collect(),
        _ => seed
            .rsplit_once('/')
            .map_or(seed, |(dir, _)| dir)
            .to_string(),
    };
    format!("{}: {class}", kind.name())
}

/// The types of the two values in a `find_diff` message (`PATH: libucl VALUE crate VALUE`).
fn value_types(detail: &str) -> String {
    if detail.contains(": length libucl ") {
        return "array length".to_string();
    }
    let first_type = |text: &str| {
        serde_json::Deserializer::from_str(text)
            .into_iter::<serde_json::Value>()
            .next()
            .and_then(Result::ok)
            .and_then(|v| v.get("t").and_then(|t| t.as_str()).map(str::to_string))
            .unwrap_or_else(|| "?".to_string())
    };
    let Some((_, rest)) = detail.split_once(": libucl ") else {
        return "?".to_string();
    };
    let golden = first_type(rest);
    let actual = rest
        .rsplit_once(" crate ")
        .map_or_else(|| "?".to_string(), |(_, a)| first_type(a));
    format!("libucl {golden}, crate {actual}")
}

struct Shared {
    options: Options,
    seeds: Vec<Seed>,
    stop: AtomicBool,
    runs: AtomicU64,
    tally: Mutex<Tally>,
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(message) => {
            if !message.is_empty() {
                eprintln!("error: {message}");
            }
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    if !options.oracle.is_file() {
        eprintln!(
            "error: no oracle at {}; scripts/regen-golden.sh builds it",
            options.oracle.display()
        );
        return ExitCode::from(2);
    }
    if !options.replay.is_empty() {
        return replay(&options);
    }
    if let Some(file) = &options.check {
        return check_one(&options, file);
    }
    let seeds = seeds(&repository());
    if seeds.is_empty() {
        eprintln!("error: no conformance cases found");
        return ExitCode::from(2);
    }
    let started = Instant::now();
    eprintln!(
        "ucl-differential: seed {}, {} jobs, {} seeds, {} s{}",
        options.seed,
        options.jobs,
        seeds.len(),
        options.seconds,
        options
            .runs
            .map_or(String::new(), |n| format!(", at most {n} inputs"))
    );
    let shared = Arc::new(Shared {
        options,
        seeds,
        stop: AtomicBool::new(false),
        runs: AtomicU64::new(0),
        tally: Mutex::new(Tally::default()),
    });
    let workers: Vec<_> = (0..shared.options.jobs)
        .map(|n| {
            let shared = Arc::clone(&shared);
            thread::Builder::new()
                .name(format!("worker-{n}"))
                .spawn(move || work(&shared, n))
                .expect("a worker thread")
        })
        .collect();
    let deadline =
        (shared.options.seconds > 0).then(|| started + Duration::from_secs(shared.options.seconds));
    let mut last_report = Instant::now();
    while !workers.iter().all(|w| w.is_finished()) {
        thread::sleep(Duration::from_millis(100));
        if deadline.is_some_and(|d| Instant::now() >= d) {
            shared.stop.store(true, Ordering::Relaxed);
        }
        if last_report.elapsed() >= Duration::from_secs(10) {
            last_report = Instant::now();
            eprintln!("{}", progress(&shared, started));
        }
    }
    for worker in workers {
        worker.join().expect("a worker does not panic");
    }
    let summary = summary(&shared, started);
    eprintln!("{summary}");
    let _ = fs::write(shared.options.out.join("summary.txt"), &summary);
    let tally = shared.tally.lock().unwrap();
    if tally.saved.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn progress(shared: &Shared, started: Instant) -> String {
    let runs = shared.runs.load(Ordering::Relaxed);
    let seconds = started.elapsed().as_secs_f64();
    let tally = shared.tally.lock().unwrap();
    format!(
        "[{seconds:.0} s] {runs} inputs ({:.0}/s), {} agree, {} findings saved",
        runs as f64 / seconds.max(1e-9),
        tally.agree,
        tally.saved.len()
    )
}

fn summary(shared: &Shared, started: Instant) -> String {
    let tally = shared.tally.lock().unwrap();
    let options = &shared.options;
    let mut text = String::new();
    let runs = shared.runs.load(Ordering::Relaxed);
    let _ = writeln!(
        text,
        "seed {}, {} jobs, {:.0} s, {runs} inputs, max length {} bytes",
        options.seed,
        options.jobs,
        started.elapsed().as_secs_f64(),
        options.max_len
    );
    let _ = writeln!(text, "agree: {}", tally.agree);
    for (reason, count) in &tally.skipped {
        let _ = writeln!(text, "skipped, {reason}: {count}");
    }
    for (kind, count) in &tally.differs {
        let _ = writeln!(text, "differ, {}: {count}", kind.name());
    }
    for (class, count) in &tally.signatures {
        let _ = writeln!(text, "  {class}: {count}");
    }
    let _ = writeln!(
        text,
        "findings saved in {}: {}",
        options.out.join("findings").display(),
        tally.saved.len()
    );
    for name in &tally.saved {
        let _ = writeln!(text, "  {name}");
    }
    text
}

fn work(shared: &Shared, n: usize) {
    let options = &shared.options;
    let work_dir = options.out.join("work").join(n.to_string());
    fs::create_dir_all(&work_dir).expect("the work directory can be created");
    // The oracle reads the input from this file; if the fuzzer stops abnormally, it holds the
    // input it was working on.
    let file = work_dir.join("input.ucl");
    let target = Target {
        oracle: &options.oracle,
        timeout: options.timeout,
        file: &file,
    };
    let mut rng = Rng::new(options.seed ^ (n as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    while !shared.stop.load(Ordering::Relaxed) {
        let count = shared.runs.fetch_add(1, Ordering::Relaxed);
        if options.runs.is_some_and(|limit| count >= limit) {
            break;
        }
        let input = next_input(&mut rng, &shared.seeds, options.max_len);
        let checked = run::check(&target, &input.bytes, &input.flags, &input.dir);
        let kind = {
            let mut tally = shared.tally.lock().unwrap();
            match &checked.verdict {
                Verdict::Agree => {
                    tally.agree += 1;
                    None
                }
                Verdict::Skipped(reason) => {
                    *tally.skipped.entry(reason).or_default() += 1;
                    None
                }
                Verdict::Differs { kind, detail } => {
                    *tally.differs.entry(*kind).or_default() += 1;
                    let class = signature(*kind, detail, &checked.krate, &input.origin);
                    let seen = tally.signatures.entry(class.clone()).or_default();
                    *seen += 1;
                    (*seen <= REDUCED_PER_SIGNATURE).then_some((*kind, class))
                }
            }
        };
        if let Some((kind, class)) = kind {
            let small = reduce(&target, &input, &class);
            let checked = run::check(&target, &small, &input.flags, &input.dir);
            save(shared, &input, &small, kind, &checked);
        }
    }
}

/// A smaller input that differs in the same way, with the same [`signature`]: removes pieces,
/// from large to single bytes, then pairs of matching brackets, while the difference stays.
/// Stops after a fixed number of tries.
fn reduce(target: &Target<'_>, input: &Input, class: &str) -> Vec<u8> {
    const TRIES: usize = 4000;
    let differs = |bytes: &[u8]| {
        let checked = run::check(target, bytes, &input.flags, &input.dir);
        match &checked.verdict {
            Verdict::Differs { kind, detail } => {
                signature(*kind, detail, &checked.krate, &input.origin) == class
            }
            _ => false,
        }
    };
    let mut tries = 0;
    let mut current = input.bytes.clone();
    let mut chunk = current.len().div_ceil(2).max(1);
    loop {
        let mut removed = false;
        let mut at = 0;
        while at < current.len() {
            if tries == TRIES {
                return current;
            }
            tries += 1;
            let end = (at + chunk).min(current.len());
            let mut candidate = current[..at].to_vec();
            candidate.extend_from_slice(&current[end..]);
            if differs(&candidate) {
                current = candidate;
                removed = true;
            } else {
                at += chunk;
            }
        }
        if !removed {
            if chunk > 1 {
                chunk = chunk.div_ceil(2);
                continue;
            }
            let mut at = 0;
            while at < current.len() {
                if tries == TRIES {
                    return current;
                }
                match without_pair(&current, at) {
                    Some(candidate) => {
                        tries += 1;
                        if differs(&candidate) {
                            current = candidate;
                            removed = true;
                        } else {
                            at += 1;
                        }
                    }
                    None => at += 1,
                }
            }
            if !removed {
                return current;
            }
        }
    }
}

/// `bytes` without the bracket at `at` and the one that closes it, if there is one.
fn without_pair(bytes: &[u8], at: usize) -> Option<Vec<u8>> {
    let (open, close) = match bytes[at] {
        b'(' => (b'(', b')'),
        b'[' => (b'[', b']'),
        b'{' => (b'{', b'}'),
        _ => return None,
    };
    let mut depth = 0usize;
    for (i, &b) in bytes.iter().enumerate().skip(at) {
        if b == open {
            depth += 1;
        } else if b == close {
            depth -= 1;
            if depth == 0 {
                let mut out = bytes.to_vec();
                out.remove(i);
                out.remove(at);
                return Some(out);
            }
        }
    }
    None
}

/// A stable name for an input: FNV-1a.
fn fingerprint(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

fn save(shared: &Shared, input: &Input, small: &[u8], kind: Kind, checked: &Checked) {
    let name = format!("{}-{}", kind.name(), fingerprint(small));
    {
        let mut tally = shared.tally.lock().unwrap();
        if !tally.seen.insert(name.clone()) {
            return;
        }
        tally.saved.push(name.clone());
    }
    let dir = shared.options.out.join("findings").join(&name);
    fs::create_dir_all(&dir).expect("a finding directory can be created");
    fs::write(dir.join("input.ucl"), small).unwrap();
    fs::write(dir.join("original.ucl"), &input.bytes).unwrap();
    fs::write(dir.join("flags"), input.flags.join("\n") + "\n").unwrap();
    fs::write(dir.join("dir"), input.dir.to_string_lossy().as_bytes()).unwrap();
    let report = report(&shared.options, input, &dir, checked);
    fs::write(dir.join("report.txt"), report).unwrap();
    eprintln!("finding: {}", dir.display());
}

fn report(options: &Options, input: &Input, dir: &Path, checked: &Checked) -> String {
    let mut text = String::new();
    let detail = match &checked.verdict {
        Verdict::Differs { kind, detail } => format!("{}: {detail}", kind.name()),
        Verdict::Agree => "agree (the reduced input no longer differs)".to_string(),
        Verdict::Skipped(reason) => format!("skipped: {reason}"),
    };
    let _ = writeln!(text, "{detail}");
    let _ = writeln!(text, "input: {}", input.origin);
    let _ = writeln!(text, "flags: {}", input.flags.join(" "));
    let _ = writeln!(text, "working directory: {}", input.dir.display());
    if let Some(line) = oracle_flags_line(&input.flags) {
        let _ = writeln!(text, "{line}");
    }
    let _ = writeln!(text, "oracle: {}", oracle_text(&checked.oracle));
    let _ = writeln!(text, "crate:  {}", crate_text(&checked.krate));
    let options_text = run::expectation_options(&input.flags)
        .unwrap_or_default()
        .join(" ");
    let _ = writeln!(
        text,
        "reproduce: (cd {} && {} {options_text} {})",
        input.dir.display(),
        options.oracle.display(),
        dir.join("input.ucl").display()
    );
    let _ = writeln!(
        text,
        "input bytes: {:?}",
        String::from_utf8_lossy(&fs::read(dir.join("input.ucl")).unwrap_or_default())
    );
    text
}

/// For flags the oracle does not run with all of ([`run::expectation_flags`]), a line that says
/// which it runs with.
fn oracle_flags_line(flags: &[String]) -> Option<String> {
    let expected = run::expectation_flags(flags);
    (expected.len() < flags.len()).then(|| {
        format!(
            "oracle flags: {} (the expected result is the one without zerocopy, spec §12.2)",
            expected.join(" ")
        )
    })
}

fn oracle_text(result: &OracleResult) -> String {
    match result {
        OracleResult::Dump(dump) => dump.to_string(),
        OracleResult::Error => "error".to_string(),
        OracleResult::Crashed(status) => format!("crashed ({status})"),
        OracleResult::TimedOut => "timed out".to_string(),
        OracleResult::BadOutput(what) => format!("no dump ({what})"),
    }
}

fn crate_text(result: &CrateResult) -> String {
    match result {
        CrateResult::Dump(dump) => dump.to_string(),
        CrateResult::Rejected { message, .. } => format!("error: {message}"),
        CrateResult::Panicked(message) => format!("panicked: {message}"),
    }
}

/// Runs one file through both and prints both results and the verdict.
fn check_one(options: &Options, file: &Path) -> ExitCode {
    let Ok(bytes) = fs::read(file) else {
        eprintln!("error: cannot read {}", file.display());
        return ExitCode::from(2);
    };
    let mut flags = options.check_flags.clone();
    flags.retain(|flag| flag != "string-input");
    flags.push("string-input".to_string());
    if let Err(e) = oracle::Setup::from_flags(&flags).and(run::oracle_options(&flags)) {
        eprintln!("error: {e}");
        return ExitCode::from(2);
    }
    let work_dir = options.out.join("work").join("check");
    fs::create_dir_all(&work_dir).expect("the work directory can be created");
    let work_file = work_dir.join("input.ucl");
    let target = Target {
        oracle: &options.oracle,
        timeout: options.timeout,
        file: &work_file,
    };
    let dir = options.check_dir.clone().expect("set by parse_options");
    let checked = run::check(&target, &bytes, &flags, &dir);
    if let Some(line) = oracle_flags_line(&flags) {
        println!("{line}");
    }
    println!("oracle: {}", oracle_text(&checked.oracle));
    println!("crate:  {}", crate_text(&checked.krate));
    match &checked.verdict {
        Verdict::Agree => {
            println!("agree");
            ExitCode::SUCCESS
        }
        Verdict::Skipped(reason) => {
            println!("skipped: {reason}");
            ExitCode::SUCCESS
        }
        Verdict::Differs { kind, detail } => {
            println!("{}: {detail}", kind.name());
            ExitCode::from(1)
        }
    }
}

/// Runs saved findings again and prints their verdicts, to check a fix.
fn replay(options: &Options) -> ExitCode {
    let work_dir = options.out.join("work").join("replay");
    fs::create_dir_all(&work_dir).expect("the work directory can be created");
    let file = work_dir.join("input.ucl");
    let target = Target {
        oracle: &options.oracle,
        timeout: options.timeout,
        file: &file,
    };
    let mut differing = 0;
    for finding in &options.replay {
        let read = |name: &str| fs::read(finding.join(name));
        let (Ok(bytes), Ok(flags), Ok(dir)) = (read("input.ucl"), read("flags"), read("dir"))
        else {
            eprintln!("{}: not a finding directory", finding.display());
            differing += 1;
            continue;
        };
        let flags: Vec<String> = String::from_utf8_lossy(&flags)
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        let dir = PathBuf::from(String::from_utf8_lossy(&dir).into_owned());
        let checked = run::check(&target, &bytes, &flags, &dir);
        let verdict = match &checked.verdict {
            Verdict::Agree => "agree".to_string(),
            Verdict::Skipped(reason) => format!("skipped: {reason}"),
            Verdict::Differs { kind, detail } => {
                differing += 1;
                format!("{}: {detail}", kind.name())
            }
        };
        println!("{}: {verdict}", finding.display());
    }
    if differing == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// `check` runs the oracle, and a report tells how to run it again, with the options of the
    /// flags without `zerocopy` (spec §12.2, `run::expectation_options`). A stand-in oracle
    /// writes the options it was given as its dump.
    #[test]
    fn the_oracle_runs_and_is_reproduced_without_zerocopy() {
        let dir = repository().join("target/fuzz-unit-tests/oracle-options");
        fs::create_dir_all(&dir).unwrap();
        let oracle = dir.join("oracle.sh");
        let dump = r#"{"t":"object","entries":[{"k":"options","v":[{"t":"string","v":"%s"}]}]}"#;
        fs::write(&oracle, format!("#!/bin/sh\nprintf '{dump}' \"$*\"\n")).unwrap();
        fs::set_permissions(&oracle, fs::Permissions::from_mode(0o755)).unwrap();
        let file = dir.join("input.ucl");
        let target = Target {
            oracle: &oracle,
            timeout: Duration::from_secs(10),
            file: &file,
        };
        let flags: Vec<String> = [
            "zerocopy",
            "registered-macros",
            "string-input",
            "priority:3",
        ]
        .map(String::from)
        .into();
        let checked = run::check(&target, b"a = 1", &flags, &dir);
        let OracleResult::Dump(dump) = &checked.oracle else {
            panic!(
                "no dump from the stand-in: {}",
                oracle_text(&checked.oracle)
            );
        };
        assert_eq!(
            dump["entries"][0]["v"][0]["v"],
            format!("-R -S -p 3 {}", file.display())
        );

        let options = Options {
            seconds: 0,
            runs: None,
            seed: 0,
            jobs: 1,
            max_len: 0,
            timeout: target.timeout,
            oracle: oracle.clone(),
            out: dir.clone(),
            replay: Vec::new(),
            check: None,
            check_flags: Vec::new(),
            check_dir: None,
        };
        let input = Input {
            bytes: b"a = 1".to_vec(),
            flags,
            dir: dir.clone(),
            origin: "a test".to_string(),
        };
        let text = report(&options, &input, &dir, &checked);
        let reproduce = text
            .lines()
            .find(|line| line.starts_with("reproduce:"))
            .expect("a reproduce line");
        assert!(
            reproduce.contains(" -R -S -p 3 ") && !reproduce.contains("-z"),
            "{reproduce}"
        );
        assert!(
            text.contains("oracle flags: registered-macros string-input priority:3 "),
            "{text}"
        );
    }
}

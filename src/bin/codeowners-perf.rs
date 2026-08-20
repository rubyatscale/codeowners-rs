//! Benchmark harness for `validate` and `generate_and_validate`.
//!
//! This binary exists to make performance claims checkable. It runs a fixed set of
//! named cases against a configurable corpus, records wall-clock and per-phase
//! timings, and emits JSON that can be diffed across branches.
//!
//! It is a local development tool: it is deliberately not wired into CI, because
//! shared runners are too noisy for the 2-20s wall-clock comparisons we care about
//! and have no large corpus available. See `perf/README.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use codeowners::config::Config;
use codeowners::runner::{self, RunConfig, RunResult};
use serde::{Deserialize, Serialize};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;

/// Corpora smaller than this are smoke-test scale: every case completes in
/// single-digit milliseconds and the numbers are not comparable to anything.
const SMOKE_SCALE_MAX_FILES: usize = 1_000;

#[derive(Parser)]
#[command(about = "Performance harness for codeowners validate/generate", version)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run benchmark cases against a corpus.
    Run {
        /// Corpus to measure against. Falls back to $CODEOWNERS_PERF_CORPUS, then
        /// the committed test fixture.
        #[arg(long)]
        corpus: Option<PathBuf>,
        /// Only run cases whose name contains this substring.
        #[arg(long)]
        case: Option<String>,
        /// Timed runs per case; the best is reported.
        #[arg(long, default_value_t = 3)]
        runs: usize,
        /// Untimed warmup runs per case, to settle the cache and page cache.
        #[arg(long, default_value_t = 1)]
        warmup: usize,
        /// Emit JSON instead of a human-readable table.
        #[arg(long)]
        json: bool,
    },
    /// Diff two JSON reports into a markdown delta table.
    Compare { baseline: PathBuf, candidate: PathBuf },
    /// List the available case names.
    Cases,
}

// ---------------------------------------------------------------------------
// Report model
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct Report {
    tool_version: String,
    #[serde(default)]
    machine: MachineInfo,
    corpus: CorpusInfo,
    runs_per_case: usize,
    cases: Vec<CaseResult>,
}

/// Enough to notice that two reports came from different machines. Wall-clock
/// numbers are not portable across hardware, so comparing them is meaningless.
#[derive(Serialize, Deserialize, PartialEq, Eq, Default)]
struct MachineInfo {
    os: String,
    arch: String,
    cpus: usize,
}

impl MachineInfo {
    fn detect() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            cpus: std::thread::available_parallelism().map(Into::into).unwrap_or(0),
        }
    }

    fn describe(&self) -> String {
        format!("{}/{} ({} cpus)", self.os, self.arch, self.cpus)
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct CorpusInfo {
    path: String,
    git_commit: Option<String>,
    tracked_files: usize,
    owned_files: usize,
    codeowners_lines: usize,
    /// True when the corpus is too small for the numbers to mean anything.
    smoke_scale: bool,
}

#[derive(Serialize, Deserialize)]
struct CaseResult {
    name: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    skip_reason: Option<String>,
    file_count: usize,
    runs_ms: Vec<u128>,
    best_ms: u128,
    median_ms: u128,
    /// Span durations from the best run. Nested spans are inclusive of children,
    /// so these do not sum to `best_ms`.
    phases_ms: BTreeMap<String, u128>,
    validation_errors: usize,
    io_errors: usize,
}

// ---------------------------------------------------------------------------
// Phase collection
//
// The library is already instrumented with `#[instrument]` spans. We install a
// subscriber layer that accumulates span durations by name into a global map,
// clearing it between runs. Runs are strictly sequential, so a global is safe.
// ---------------------------------------------------------------------------

fn phase_totals() -> &'static Mutex<BTreeMap<String, Duration>> {
    static TOTALS: OnceLock<Mutex<BTreeMap<String, Duration>>> = OnceLock::new();
    TOTALS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

struct SpanStart(Instant);

struct PhaseLayer;

impl<S> Layer<S> for PhaseLayer
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, _attrs: &tracing::span::Attributes<'_>, id: &tracing::Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(SpanStart(Instant::now()));
        }
    }

    fn on_close(&self, id: tracing::Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(&id) else { return };
        let elapsed = {
            let ext = span.extensions();
            match ext.get::<SpanStart>() {
                Some(start) => start.0.elapsed(),
                None => return,
            }
        };
        let name = span.name().to_string();
        if let Ok(mut totals) = phase_totals().lock() {
            *totals.entry(name).or_default() += elapsed;
        }
    }
}

fn reset_phases() {
    if let Ok(mut totals) = phase_totals().lock() {
        totals.clear();
    }
}

fn take_phases() -> BTreeMap<String, u128> {
    match phase_totals().lock() {
        Ok(totals) => totals.iter().map(|(k, v)| (k.clone(), v.as_millis())).collect(),
        Err(_) => BTreeMap::new(),
    }
}

// ---------------------------------------------------------------------------
// Corpus resolution
// ---------------------------------------------------------------------------

fn resolve_corpus(flag: Option<PathBuf>) -> Result<PathBuf, String> {
    let candidate = if let Some(path) = flag {
        path
    } else if let Some(env) = std::env::var_os("CODEOWNERS_PERF_CORPUS").filter(|v| !v.is_empty()) {
        PathBuf::from(env)
    } else {
        // The committed fixture: self-contained, works on a clean clone, and keeps
        // any reference to a specific large monorepo out of the repository.
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/valid_project")
    };

    let corpus = candidate
        .canonicalize()
        .map_err(|e| format!("corpus {} is not readable: {e}", candidate.display()))?;

    let config = corpus.join("config/code_ownership.yml");
    if !config.is_file() {
        return Err(format!(
            "corpus {} has no config/code_ownership.yml — is it a codeowners project?",
            corpus.display()
        ));
    }
    Ok(corpus)
}

fn git_output(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Tracked files in the corpus, relative to its root.
fn tracked_files(corpus: &Path) -> Vec<String> {
    match git_output(corpus, &["ls-files"]) {
        Some(stdout) => stdout.lines().map(str::to_string).collect(),
        None => Vec::new(),
    }
}

fn matches_any(path: &str, globs: &[String]) -> bool {
    globs.iter().any(|glob| fast_glob::glob_match(glob, path))
}

/// The deterministic candidate pool for `validate <files>` cases: tracked files
/// that survive the same owned/unowned glob filter `validate_files` applies, in
/// sorted order so every branch measures the same paths.
fn owned_file_pool(tracked: &[String], config: &Config) -> Vec<String> {
    let mut pool: Vec<String> = tracked
        .iter()
        .filter(|path| matches_any(path, &config.owned_globs) && !matches_any(path, &config.unowned_globs))
        .cloned()
        .collect();
    pool.sort();
    pool.dedup();
    pool
}

fn codeowners_path(corpus: &Path, config: &Config) -> PathBuf {
    corpus.join(&config.codeowners_path).join("CODEOWNERS")
}

/// Restores the corpus CODEOWNERS file on drop.
///
/// `generate` and `generate-and-validate` write to the corpus. Leaving someone's
/// working repo modified because they ran a benchmark is not acceptable, so we
/// snapshot the file up front and put it back afterwards.
struct CodeownersGuard {
    path: PathBuf,
    original: Option<String>,
}

impl CodeownersGuard {
    fn acquire(corpus: &Path, config: &Config) -> Result<Self, String> {
        let path = codeowners_path(corpus, config);

        // Refuse to run against a corpus whose CODEOWNERS is already modified: we
        // could not tell our own writes apart from the user's, and restoring would
        // silently clobber their work.
        if let Some(relative) = path.strip_prefix(corpus).ok().map(|p| p.to_string_lossy().into_owned())
            && let Some(status) = git_output(corpus, &["status", "--porcelain", "--", &relative])
            && !status.trim().is_empty()
        {
            return Err(format!(
                "corpus CODEOWNERS has uncommitted changes ({}). Commit or stash it first — \
                 the harness rewrites this file and restores it afterwards.",
                relative.trim()
            ));
        }

        let original = std::fs::read_to_string(&path).ok();
        Ok(Self { path, original })
    }
}

impl Drop for CodeownersGuard {
    fn drop(&mut self) {
        if let Some(original) = &self.original
            && let Err(e) = std::fs::write(&self.path, original)
        {
            eprintln!("warning: failed to restore {}: {e}", self.path.display());
        }
    }
}

// ---------------------------------------------------------------------------
// Cases
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Kind {
    Generate,
    Validate,
    GenerateAndValidate,
}

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    kind: Kind,
    files: usize,
    no_cache: bool,
}

const CASES: &[Case] = &[
    Case {
        name: "generate",
        kind: Kind::Generate,
        files: 0,
        no_cache: false,
    },
    Case {
        name: "validate_all",
        kind: Kind::Validate,
        files: 0,
        no_cache: false,
    },
    Case {
        name: "gv",
        kind: Kind::GenerateAndValidate,
        files: 0,
        no_cache: false,
    },
    // `gv <paths>` is the likely real-world pre-commit / CI invocation, and it is
    // not interchangeable with `validate <paths>`: generate needs the project
    // build, so an optimization that bypasses that build cannot apply here.
    // Measured separately so a win on `validate <paths>` is never mistaken for a
    // win on the command people actually run.
    Case {
        name: "gv_files_100",
        kind: Kind::GenerateAndValidate,
        files: 100,
        no_cache: false,
    },
    Case {
        name: "gv_files_1000",
        kind: Kind::GenerateAndValidate,
        files: 1000,
        no_cache: false,
    },
    Case {
        name: "validate_all_cold",
        kind: Kind::Validate,
        files: 0,
        no_cache: true,
    },
    Case {
        name: "validate_files_1",
        kind: Kind::Validate,
        files: 1,
        no_cache: false,
    },
    Case {
        name: "validate_files_100",
        kind: Kind::Validate,
        files: 100,
        no_cache: false,
    },
    Case {
        name: "validate_files_1000",
        kind: Kind::Validate,
        files: 1000,
        no_cache: false,
    },
    Case {
        name: "validate_files_2000",
        kind: Kind::Validate,
        files: 2000,
        no_cache: false,
    },
];

fn run_case(case: &Case, corpus: &Path, files: &[String]) -> RunResult {
    let run_config = RunConfig {
        project_root: corpus.to_path_buf(),
        config_path: corpus.join("config/code_ownership.yml"),
        codeowners_file_path: None,
        no_cache: case.no_cache,
        executable_name: None,
    };
    let files = files.to_vec();
    match case.kind {
        Kind::Generate => runner::generate(&run_config, false),
        Kind::Validate => runner::validate(&run_config, files),
        Kind::GenerateAndValidate => runner::generate_and_validate(&run_config, files, false),
    }
}

/// Observed run-to-run spread (max - min). The harness's own precision floor for
/// a case: deltas smaller than this cannot be distinguished from noise.
fn spread(runs: &[u128]) -> u128 {
    match (runs.iter().min(), runs.iter().max()) {
        (Some(lo), Some(hi)) => hi - lo,
        _ => 0,
    }
}

fn median(sorted: &[u128]) -> u128 {
    match sorted.len() {
        0 => 0,
        n => sorted[n / 2],
    }
}

#[allow(clippy::too_many_arguments)]
fn measure(case: &Case, corpus: &Path, pool: &[String], runs: usize, warmup: usize) -> CaseResult {
    if case.files > pool.len() {
        return CaseResult {
            name: case.name.to_string(),
            status: "skipped".to_string(),
            skip_reason: Some(format!("needs {} owned files, corpus has {}", case.files, pool.len())),
            file_count: 0,
            runs_ms: vec![],
            best_ms: 0,
            median_ms: 0,
            phases_ms: BTreeMap::new(),
            validation_errors: 0,
            io_errors: 0,
        };
    }

    let files: Vec<String> = pool.iter().take(case.files).cloned().collect();

    // The filter regression that fooled me during profiling: a bad argument list
    // silently matches nothing and the per-file loop never runs, producing a
    // suspiciously fast number. Fail loudly instead.
    assert_eq!(
        files.len(),
        case.files,
        "case {} expected {} paths but built {}",
        case.name,
        case.files,
        files.len()
    );

    for _ in 0..warmup {
        run_case(case, corpus, &files);
    }

    let mut timings = Vec::with_capacity(runs);
    let mut phases = BTreeMap::new();
    let mut last = RunResult::default();
    let mut best = u128::MAX;

    for _ in 0..runs.max(1) {
        reset_phases();
        let start = Instant::now();
        let result = run_case(case, corpus, &files);
        let elapsed = start.elapsed().as_millis();
        timings.push(elapsed);
        if elapsed < best {
            best = elapsed;
            phases = take_phases();
        }
        last = result;
    }

    let mut sorted = timings.clone();
    sorted.sort_unstable();

    CaseResult {
        name: case.name.to_string(),
        status: "ok".to_string(),
        skip_reason: None,
        file_count: case.files,
        best_ms: sorted.first().copied().unwrap_or(0),
        median_ms: median(&sorted),
        runs_ms: timings,
        phases_ms: phases,
        validation_errors: last.validation_errors.len(),
        io_errors: last.io_errors.len(),
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn cmd_run(corpus: Option<PathBuf>, case_filter: Option<String>, runs: usize, warmup: usize, json: bool) -> Result<(), String> {
    let corpus = resolve_corpus(corpus)?;
    let config = Config::load_from_path(&corpus.join("config/code_ownership.yml"))?;

    let tracked = tracked_files(&corpus);
    let pool = owned_file_pool(&tracked, &config);
    let codeowners_lines = std::fs::read_to_string(codeowners_path(&corpus, &config))
        .map(|s| s.lines().count())
        .unwrap_or(0);

    let info = CorpusInfo {
        path: corpus.to_string_lossy().into_owned(),
        git_commit: git_output(&corpus, &["rev-parse", "HEAD"]).map(|s| s.trim().to_string()),
        tracked_files: tracked.len(),
        owned_files: pool.len(),
        codeowners_lines,
        smoke_scale: tracked.len() < SMOKE_SCALE_MAX_FILES,
    };

    if info.smoke_scale && !json {
        eprintln!(
            "warning: corpus is {} ({} tracked files) — smoke-test scale.\n\
             \x20        Results are NOT comparable. Set CODEOWNERS_PERF_CORPUS to a large monorepo\n\
             \x20        (see perf/README.md) for real measurements.\n",
            info.path, info.tracked_files
        );
    }

    // Held for the whole run; restores the corpus CODEOWNERS on drop.
    let _guard = CodeownersGuard::acquire(&corpus, &config)?;

    let selected: Vec<&Case> = CASES
        .iter()
        .filter(|c| case_filter.as_ref().is_none_or(|f| c.name.contains(f.as_str())))
        .collect();
    if selected.is_empty() {
        return Err("no cases matched --case".to_string());
    }

    let mut results = Vec::new();
    for case in selected {
        if !json {
            eprint!("  {} ... ", case.name);
        }
        let result = measure(case, &corpus, &pool, runs, warmup);
        if !json {
            match result.status.as_str() {
                "skipped" => eprintln!("skipped ({})", result.skip_reason.clone().unwrap_or_default()),
                _ => eprintln!("{} ms", result.best_ms),
            }
        }
        results.push(result);
    }

    let report = Report {
        tool_version: runner::version(),
        machine: MachineInfo::detect(),
        corpus: info,
        runs_per_case: runs,
        cases: results,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?);
    } else {
        print_table(&report);
    }
    Ok(())
}

fn print_table(report: &Report) {
    println!();
    println!("corpus:    {}", report.corpus.path);
    println!("machine:   {}", report.machine.describe());
    println!(
        "scale:     {} tracked files, {} owned, {} CODEOWNERS lines{}",
        report.corpus.tracked_files,
        report.corpus.owned_files,
        report.corpus.codeowners_lines,
        if report.corpus.smoke_scale { "  [SMOKE SCALE]" } else { "" }
    );
    if let Some(commit) = &report.corpus.git_commit {
        println!("commit:    {commit}");
    }
    println!("runs:      {} (best reported)", report.runs_per_case);
    println!();
    println!("{:<22} {:>10} {:>10}  notes", "case", "best", "median");
    println!("{}", "-".repeat(72));
    for case in &report.cases {
        if case.status == "skipped" {
            println!(
                "{:<22} {:>10} {:>10}  {}",
                case.name,
                "-",
                "-",
                case.skip_reason.clone().unwrap_or_default()
            );
            continue;
        }
        let notes = if case.validation_errors > 0 || case.io_errors > 0 {
            format!("{} validation, {} io errors", case.validation_errors, case.io_errors)
        } else {
            String::new()
        };
        println!("{:<22} {:>9}ms {:>9}ms  {}", case.name, case.best_ms, case.median_ms, notes);
    }

    println!();
    println!("phase breakdown (best run, nested spans are inclusive of children)");
    println!("{}", "-".repeat(72));
    for case in &report.cases {
        if case.phases_ms.is_empty() {
            continue;
        }
        println!("{}:", case.name);
        for (phase, ms) in &case.phases_ms {
            println!("    {phase:<34} {ms:>7}ms");
        }
    }
}

fn read_report(path: &Path) -> Result<Report, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn cmd_compare(baseline_path: &Path, candidate_path: &Path) -> Result<(), String> {
    let baseline = read_report(baseline_path)?;
    let candidate = read_report(candidate_path)?;

    // Comparing across corpora is the subtle version of the smoke-scale mistake:
    // a fixture-measured branch against a monorepo-measured baseline reads as a
    // spectacular speedup. Refuse rather than mislead.
    if baseline.corpus.path != candidate.corpus.path {
        return Err(format!(
            "refusing to compare: different corpora\n  baseline:  {}\n  candidate: {}",
            baseline.corpus.path, candidate.corpus.path
        ));
    }
    if baseline.corpus.git_commit != candidate.corpus.git_commit {
        return Err(format!(
            "refusing to compare: corpus moved between runs\n  baseline:  {:?}\n  candidate: {:?}\n\
             Re-measure both sides against the same corpus commit.",
            baseline.corpus.git_commit, candidate.corpus.git_commit
        ));
    }

    let candidates: BTreeMap<&str, &CaseResult> = candidate.cases.iter().map(|c| (c.name.as_str(), c)).collect();

    println!("corpus: {} ({} tracked files)", baseline.corpus.path, baseline.corpus.tracked_files);
    println!("machine: {}", baseline.machine.describe());
    if baseline.corpus.smoke_scale {
        println!();
        println!("**Smoke-scale corpus — these numbers are not meaningful for comparison.**");
    }
    // Not fatal like a corpus mismatch, but wall-clock across different hardware
    // is not a like-for-like comparison and the reader needs to know.
    if baseline.machine != candidate.machine {
        println!();
        println!(
            "**Warning: different machines ({} vs {}). Wall-clock numbers are not comparable across hardware.**",
            baseline.machine.describe(),
            candidate.machine.describe()
        );
    }
    println!();
    println!("| Case | Baseline | Candidate | Delta | Speedup | Noise | Verdict |");
    println!("|---|---:|---:|---:|---:|---:|---|");
    for base in &baseline.cases {
        let Some(cand) = candidates.get(base.name.as_str()) else {
            println!("| {} | {}ms | — | missing | — | — | — |", base.name, base.best_ms);
            continue;
        };
        if base.status == "skipped" || cand.status == "skipped" {
            println!("| {} | skipped | skipped | — | — | — | — |", base.name);
            continue;
        }
        let delta = cand.best_ms as i128 - base.best_ms as i128;
        let speedup = if cand.best_ms > 0 {
            base.best_ms as f64 / cand.best_ms as f64
        } else {
            0.0
        };
        // A delta smaller than the run-to-run spread is not a result. Reporting
        // `best` alone hides this: min-of-N is a biased estimator with no
        // dispersion attached, so a 3% "win" on a case that swings 40% between
        // runs reads exactly like a real one.
        let noise = spread(&base.runs_ms).max(spread(&cand.runs_ms));
        let verdict = if delta.unsigned_abs() <= noise { "**within noise**" } else { "" };
        println!(
            "| {} | {}ms | {}ms | {}{}ms | {:.2}x | ±{}ms | {} |",
            base.name,
            base.best_ms,
            cand.best_ms,
            if delta > 0 { "+" } else { "" },
            delta,
            speedup,
            noise,
            verdict
        );
    }
    Ok(())
}

fn main() {
    tracing_subscriber::registry()
        .with(PhaseLayer)
        .with(tracing_subscriber::EnvFilter::new("codeowners=debug"))
        .init();

    let cli = Cli::parse();
    let result = match cli.command {
        Cmd::Run {
            corpus,
            case,
            runs,
            warmup,
            json,
        } => cmd_run(corpus, case, runs, warmup, json),
        Cmd::Compare { baseline, candidate } => cmd_compare(&baseline, &candidate),
        Cmd::Cases => {
            for case in CASES {
                println!("{}", case.name);
            }
            Ok(())
        }
    };

    if let Err(err) = result {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

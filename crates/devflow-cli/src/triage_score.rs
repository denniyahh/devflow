//! `devflow __triage-score`: measure Jev on the operator's own past answers
//! (D-01, D-04, D-05, D-06, D-07, D-24).
//!
//! Every named project is mined, each question is scored leave-one-out with
//! the request the run sends, and both thresholds are chosen by the rule
//! fixed before scoring. The corpus, the per-question scores and the report
//! are written under the user data dir, private to the user; only the report,
//! counts and scores alone, is fit to copy into a repository.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::{DirBuilder, File, OpenOptions, Permissions};
use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use devflow_core::corpus::ScoredEntry;
use devflow_core::corpus::{self, CorpusEntry, MeasurementReport, MiningReport, ProjectCounts};
use devflow_core::phase_id::PhaseId;
use devflow_core::question::{QuestionKind, QuestionSpec};
use devflow_core::triage::{self, Precedent};
use serde_json::Value;

use crate::CliError;
use crate::triage_client::{self, JevEndpoint};

/// The subdirectory of the data dir the tool writes.
const TRIAGE_DIR: &str = "triage";

/// Every mined entry, one JSON object per line.
const CORPUS_FILE: &str = "corpus.jsonl";

/// One [`ScoredEntry`] per line.
const SCORES_FILE: &str = "scores.jsonl";

/// The counts-only report.
const REPORT_FILE: &str = "report.md";

/// The triage directory's mode: the user's alone.
const PRIVATE_DIR_MODE: u32 = 0o700;

/// Each written file's mode: the user's alone.
const PRIVATE_FILE_MODE: u32 = 0o600;

/// `--project <name>=<path>`.
#[derive(Debug, Clone)]
pub(crate) struct ProjectArg {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
}

impl FromStr for ProjectArg {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.split_once('=') {
            Some((name, path)) if !name.is_empty() && !path.is_empty() => Ok(Self {
                name: name.to_string(),
                path: PathBuf::from(path),
            }),
            _ => Err(format!("`{value}` is not <name>=<path>")),
        }
    }
}

/// `--exclude <name>:<phase>`: leave that project's phase's labels out (D-23).
#[derive(Debug, Clone)]
pub(crate) struct ExcludeArg {
    pub(crate) name: String,
    pub(crate) phase: PhaseId,
}

impl FromStr for ExcludeArg {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (name, phase) = value
            .rsplit_once(':')
            .filter(|(name, _)| !name.is_empty())
            .ok_or_else(|| format!("`{value}` is not <name>:<phase>"))?;
        let phase = phase
            .parse::<PhaseId>()
            .map_err(|err| format!("`{phase}` is not a phase: {err}"))?;
        Ok(Self {
            name: name.to_string(),
            phase,
        })
    }
}

/// What `__triage-score` was asked to measure.
#[derive(Debug)]
pub(crate) struct ScoreArgs {
    pub(crate) projects: Vec<ProjectArg>,
    pub(crate) excludes: Vec<ExcludeArg>,
    pub(crate) model: String,
}

/// Mine, print the mining counts, then (with a key) score every entry and
/// write the corpus, the scores and the report under the data dir.
pub(crate) fn run_score(args: &ScoreArgs) -> Result<(), CliError> {
    check_projects(args)?;
    let mined: Vec<(&str, MiningReport)> = args
        .projects
        .iter()
        .map(|project| {
            let excluded: Vec<PhaseId> = args
                .excludes
                .iter()
                .filter(|exclude| exclude.name == project.name)
                .map(|exclude| exclude.phase)
                .collect();
            let report = corpus::mine_project(&project.path, &project.name, &excluded);
            print_mining(&project.name, &report);
            (project.name.as_str(), report)
        })
        .collect();
    let timeout = Duration::from_secs(triage::DEFAULT_TIMEOUT_SECS);
    let endpoint = triage_client::endpoint_from_env(timeout)
        .map_err(|failure| CliError::Message(format!("refusing to score: {failure}")))?;
    let data = data_dir_from(
        std::env::var_os("DEVFLOW_DATA_DIR"),
        std::env::var_os("XDG_DATA_HOME"),
        std::env::var_os("HOME"),
    )
    .ok_or_else(|| {
        CliError::Message("no data dir: set DEVFLOW_DATA_DIR, XDG_DATA_HOME or HOME".to_string())
    })?;
    let dir = data.join(TRIAGE_DIR);
    private_dir(&dir)?;
    write_corpus(&dir.join(CORPUS_FILE), &mined)?;
    let scores = score_all(&endpoint, &args.model, &mined, &dir.join(SCORES_FILE))?;
    let projects = mined
        .iter()
        .map(|(name, report)| ProjectCounts::from_mining(name, report))
        .collect();
    let report = MeasurementReport::new(&args.model, projects, &scores);
    let report_path = dir.join(REPORT_FILE);
    let mut file = private_file(&report_path)?;
    file.write_all(corpus::render_report(&report).as_bytes())
        .map_err(|err| write_error(&report_path, &err))?;
    println!("report: {}", report_path.display());
    Ok(())
}

/// Refuse a repeated project name, a path that is no project, and an
/// exclusion naming no project, before any request.
fn check_projects(args: &ScoreArgs) -> Result<(), CliError> {
    let mut names = HashSet::new();
    for project in &args.projects {
        if !names.insert(project.name.as_str()) {
            return Err(CliError::Message(format!(
                "project `{}` is named twice",
                project.name
            )));
        }
        if !project.path.join(".planning").is_dir() && !project.path.join(".devflow").is_dir() {
            return Err(CliError::Message(format!(
                "project `{}` at {} has neither .planning/ nor .devflow/",
                project.name,
                project.path.display()
            )));
        }
    }
    if let Some(exclude) = args
        .excludes
        .iter()
        .find(|exclude| !names.contains(exclude.name.as_str()))
    {
        return Err(CliError::Message(format!(
            "--exclude names `{}`, which no --project names",
            exclude.name
        )));
    }
    Ok(())
}

/// One project's mining counts, by source and by exclusion reason.
fn print_mining(name: &str, report: &MiningReport) {
    let counts = ProjectCounts::from_mining(name, report);
    let excluded: Vec<String> = report
        .excluded
        .by_reason()
        .iter()
        .map(|(reason, count)| format!("{reason} {count}"))
        .collect();
    println!(
        "mined {name}: event entries {}, log entries {}; excluded: {}",
        counts.event_entries,
        counts.log_entries,
        excluded.join(", ")
    );
}

/// The user data dir: `DEVFLOW_DATA_DIR`, else `XDG_DATA_HOME/devflow`, else
/// `HOME/.local/share/devflow`. An empty value counts as unset.
fn data_dir_from(
    devflow_data_dir: Option<OsString>,
    xdg_data_home: Option<OsString>,
    home: Option<OsString>,
) -> Option<PathBuf> {
    let set = |value: Option<OsString>| value.filter(|value| !value.is_empty()).map(PathBuf::from);
    if let Some(dir) = set(devflow_data_dir) {
        return Some(dir);
    }
    if let Some(dir) = set(xdg_data_home) {
        return Some(dir.join("devflow"));
    }
    set(home).map(|home| home.join(".local").join("share").join("devflow"))
}

/// Create `dir` (and any missing parent) and leave it mode 0700.
fn private_dir(dir: &Path) -> Result<(), CliError> {
    DirBuilder::new()
        .recursive(true)
        .mode(PRIVATE_DIR_MODE)
        .create(dir)
        .and_then(|()| std::fs::set_permissions(dir, Permissions::from_mode(PRIVATE_DIR_MODE)))
        .map_err(|err| write_error(dir, &err))
}

/// `path` truncated or created, mode 0600 either way.
fn private_file(path: &Path) -> Result<File, CliError> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(PRIVATE_FILE_MODE)
        .open(path)
        .map_err(|err| write_error(path, &err))?;
    file.set_permissions(Permissions::from_mode(PRIVATE_FILE_MODE))
        .map_err(|err| write_error(path, &err))?;
    Ok(file)
}

fn write_error(path: &Path, err: &std::io::Error) -> CliError {
    CliError::Message(format!("cannot write {}: {err}", path.display()))
}

/// `line`, serialized, as one JSON line of `file`.
fn write_line(
    file: &mut File,
    path: &Path,
    line: serde_json::Result<String>,
) -> Result<(), CliError> {
    let text = line.map_err(|err| {
        CliError::Message(format!(
            "cannot serialize a line of {}: {err}",
            path.display()
        ))
    })?;
    writeln!(file, "{text}").map_err(|err| write_error(path, &err))
}

fn write_corpus(path: &Path, mined: &[(&str, MiningReport)]) -> Result<(), CliError> {
    let mut file = private_file(path)?;
    for entry in mined.iter().flat_map(|(_, report)| &report.entries) {
        write_line(&mut file, path, serde_json::to_string(entry))?;
    }
    Ok(())
}

/// Score every entry against its own project's other entries, one line of
/// `path` per entry as it is scored.
fn score_all(
    endpoint: &JevEndpoint,
    model: &str,
    mined: &[(&str, MiningReport)],
    path: &Path,
) -> Result<Vec<ScoredEntry>, CliError> {
    let mut file = private_file(path)?;
    let mut scores = Vec::new();
    for (_, report) in mined {
        for entry in &report.entries {
            let scored = score_entry(endpoint, model, &report.entries, entry);
            write_line(&mut file, path, serde_json::to_string(&scored))?;
            scores.push(scored);
        }
    }
    Ok(scores)
}

/// Score `entry` with precedents from `project_entries` other than itself
/// (D-04, D-06), in the request the run sends. A `blocking-human` entry is
/// never sent (D-08).
fn score_entry(
    endpoint: &JevEndpoint,
    model: &str,
    project_entries: &[CorpusEntry],
    entry: &CorpusEntry,
) -> ScoredEntry {
    if entry.kind == QuestionKind::BlockingHuman {
        return ScoredEntry::skipped(entry);
    }
    let spec = QuestionSpec {
        question_id: entry.id.clone(),
        question: entry.question.clone(),
        options: entry.options.clone(),
        context: entry.context.clone(),
        kind: entry.kind,
        recommended: None,
    };
    let chosen = corpus::select_precedent_entries(project_entries, &spec, Some(&entry.id));
    let precedent_ids: Vec<String> = chosen.iter().map(|chosen| chosen.id.clone()).collect();
    let precedents: Vec<Precedent> = chosen.iter().map(|chosen| chosen.precedent()).collect();
    let request = triage::jev_request(&spec, &precedents, model);
    let body = match triage_client::post_systemone(endpoint, &request) {
        Ok(body) => body,
        Err(failure) => {
            return ScoredEntry::failed(entry, precedent_ids, failure.fallback_reason().as_str());
        }
    };
    match triage::parse_jev_response(&body, &spec, model) {
        Ok(scores) => ScoredEntry::scored(entry, precedent_ids, &scores, input_tokens(&body)),
        Err((reason, _)) => ScoredEntry::failed(entry, precedent_ids, reason.as_str()),
    }
}

/// `usage.input_tokens`, when the response reports it.
fn input_tokens(body: &Value) -> Option<u64> {
    body.get("usage")?.get("input_tokens")?.as_u64()
}

//! End-to-end proof of the one rate-limit failover pair `--mode auto` admits,
//! Claude → Antigravity at the Code stage (Phase 51, SUPV-02, D-11), through
//! the real `devflow` binary.
//!
//! `devflow start --mode auto --until code` drives Plan and Code under
//! a stub `claude` on the stream shape. The stub is rate-limited on its Code
//! turn; the supervisor hands the stage to a stub `agy`, which must pass its
//! OWN delivery canary inside the supervisor before its stage turn runs. A
//! refused canary ends at the never-silent Code gate (R2-D1), and a run with
//! no fallback waits at that gate without ever invoking `agy`.
//!
//! Every `devflow` child is built by `common::hermetic_devflow`, so its
//! registry entries and sockets live under a tempdir `HOME`. The stubs log to
//! a directory outside the repository, so their logs never dirty the tree the
//! pipeline inspects.
//!
//! What these tests do not establish: the stubs' event shapes are copied from
//! DevFlow's own tests, not from a live Claude or Antigravity CLI; the Claude
//! stub exits after its rate-limit event, so the stream path's idle-timeout
//! route is not exercised; the Plan turn only commits a stub plan and prints success.

mod common;

use devflow_core::canary::CanaryOutcome;
use devflow_core::phase_id::PhaseId;
use devflow_core::state::{AgentKind, State};
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The bound on one whole run: Plan, Code and the handoff.
const RUN_WAIT: Duration = Duration::from_secs(120);

/// The Claude stream `init` event, copied from `V3_INIT_EVENT` in
/// `devflow-core/src/agent_result.rs`. Its presence is what makes Layer 1 read
/// the capture as a Claude event stream.
const CLAUDE_INIT_EVENT: &str = r#"{"type":"system","subtype":"init","cwd":"/tmp/scratchpad/999.64-experiment","session_id":"559fef4d-2053-459e-b7a7-f3200c3b3790","tools":["Task","Bash","Read","Write"],"mcp_servers":[{"name":"github","status":"pending"}],"model":"claude-opus-5[1m]","permissionMode":"bypassPermissions","slash_commands":["gsd-execute-phase"],"capabilities":["interrupt_receipt_v1","interrupt_cancel_queued_v1","msg_lifecycle_v1"],"uuid":"597e1613-77cb-4cdd-a716-2aa75dc58c0b"}"#;

/// `v3_rate_limit_event("rejected")` from `devflow-core/src/agent_result.rs`:
/// the archived `allowed` event with only its top-level `status` replaced. Its
/// `resetsAt` is the `@RESETS_AT@` placeholder, which the [`Rig`] fills with an
/// epoch the test computes from the current time (or `null`, for a denial with
/// no reset time): the reset time the no-fallback gate names.
const CLAUDE_REJECTED_RATE_LIMIT_EVENT: &str = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":@RESETS_AT@,"rateLimitType":"five_hour","overageStatus":"rejected","overageDisabledReason":"out_of_credits","isUsingOverage":false},"uuid":"e73e6774-a79d-4cdf-90bd-53a695f44f5a","session_id":"559fef4d-2053-459e-b7a7-f3200c3b3790"}"#;

/// A Claude stream `result` carrying the success marker.
const CLAUDE_SUCCESS_RESULT: &str = r#"{"type":"result","subtype":"success","is_error":false,"session_id":"559fef4d-2053-459e-b7a7-f3200c3b3790","result":"DEVFLOW_RESULT: {\"status\":\"success\"}"}"#;

/// The Antigravity `init` event, copied from `ANTG_INIT` in
/// `devflow-core/src/agent_result.rs`.
const ANTIGRAVITY_INIT_EVENT: &str = r#"{"event":"init","model":"gemini-3.7-flash-high","inputFormat":"stream-json","outputFormat":"stream-json","printTimeout":"60m"}"#;

/// The Antigravity success result, copied from `ANTG_RESULT_MARKER` in
/// `devflow-core/src/agent_result.rs` (`antigravity_event_result_extracts_marker_from_live_shape`).
const ANTIGRAVITY_SUCCESS_RESULT: &str = r#"{"event":"result","result":{"status":"SUCCESS","response":"DEVFLOW_RESULT: {\"status\":\"success\"}\n"}}"#;

/// The `claude` stub. Each invocation appends one line to `claude.log`:
/// `health` for `--version`, `canary` for the delivery canary, and
/// `stage plan|code` for a stage turn, told apart by the stage prompt
/// (`render_claude_style`: Code runs `/gsd-execute-phase`, Plan
/// `/gsd-plan-phase`), and `unexpected turn` for any other turn, so a sequence
/// assertion sees it. It also appends each invocation's stdin turn (one JSON
/// line) to `claude-stdin.log`, in the same order. Plan commits a minimal plan file with no
/// checkpoint; Code commits once, records that commit, runs `@CODE_TURN_DIRT@`
/// (empty unless the rig leaves uncommitted work), then reports a quota denial
/// and exits.
const CLAUDE_STUB: &str = r#"log='@LOGS@/claude.log'
if [ "$1" = "--version" ]; then echo health >> "$log"; echo "2.1.220 (Claude Code)"; exit 0; fi
IFS= read -r turn
printf '%s\n' "$turn" >> '@LOGS@/claude-stdin.log'
case "$turn" in
  *DEVFLOW_DELIVERY_CANARY_*)
    echo canary >> "$log"
    token=$(printf '%s' "$turn" | grep -o 'DEVFLOW_DELIVERY_CANARY_[0-9a-f]*' | head -1)
    printf '{"type":"result","subtype":"success","is_error":false,"session_id":"s-fake","result":"%s"}\n' "$token"
    exit 0
    ;;
  *gsd-execute-phase*)
    echo 'stage code' >> "$log"
    git commit -q --allow-empty -m 'claude code-turn work' || exit 1
    git rev-parse HEAD > '@LOGS@/claude-code-commit'
@CODE_TURN_DIRT@
    printf '%s\n' '@CLAUDE_INIT@'
    printf '%s\n' '@CLAUDE_RATE_LIMIT@'
    exit 0
    ;;
  *gsd-plan-phase*)
    echo 'stage plan' >> "$log"
    dir='.planning/phases/@PADDED@-failover'
    mkdir -p "$dir" || exit 1
    printf '%s\n' '---' 'phase: @PADDED@-failover' 'plan: 01' 'type: execute' 'autonomous: true' '---' '' '<tasks>' '<task type="auto"><name>stub</name></task>' '</tasks>' > "$dir/@PADDED@-01-PLAN.md"
    git add "$dir/@PADDED@-01-PLAN.md" || exit 1
    git commit -q -m 'plan' || exit 1
    ;;
  *)
    echo 'unexpected turn' >> "$log"
    ;;
esac
printf '%s\n' '@CLAUDE_INIT@'
printf '%s\n' '@CLAUDE_SUCCESS@'
exit 0"#;

/// The `agy` stub. Each invocation appends one line to `agy.log`: `health`
/// for `--version`, `canary` for the delivery canary, `stage` otherwise. The
/// canary answer is `@CANARY_ANSWER@`; the stage turn runs `@STAGE_PROBE@`
/// (empty unless the rig records what the fallback inherited), commits once,
/// records that commit and reports success in the Antigravity shape.
const AGY_STUB: &str = r#"#!/bin/sh
log='@LOGS@/agy.log'
if [ "$1" = "--version" ]; then echo health >> "$log"; echo "agy 0.0.0-stub"; exit 0; fi
IFS= read -r turn
case "$turn" in
  *DEVFLOW_DELIVERY_CANARY_*)
    echo canary >> "$log"
    token=$(printf '%s' "$turn" | grep -o 'DEVFLOW_DELIVERY_CANARY_[0-9a-f]*' | head -1)
    printf '%s\n' '@ANTIGRAVITY_INIT@'
    @CANARY_ANSWER@
    exit 0
    ;;
esac
echo stage >> "$log"
@STAGE_PROBE@
git commit -q --allow-empty -m 'antigravity code work' || exit 1
git rev-parse HEAD > '@LOGS@/agy-code-commit'
printf '%s\n' '@ANTIGRAVITY_INIT@'
printf '%s\n' '@ANTIGRAVITY_SUCCESS@'
exit 0"#;

/// The canary answer in the Antigravity shape the trust predicate accepts:
/// the token inside `event: "result"`'s `result.response`
/// (`canary_antigravity_confirmed_when_token_returns_in_event_result_response`).
const AGY_CANARY_RETURNS_TOKEN: &str =
    r#"printf '{"event":"result","result":{"status":"SUCCESS","response":"%s"}}\n' "$token""#;

/// A canary answer that completes the turn but withholds the token.
const AGY_CANARY_WITHHOLDS_TOKEN: &str = r#"printf '%s\n' '{"event":"result","result":{"status":"SUCCESS","response":"The turn completed without the token."}}'"#;

/// Uncommitted work the primary leaves in the tree when its Code turn is
/// rate-limited: a staged new file, an unstaged edit to a tracked file and an
/// untracked file.
const CODE_TURN_LEAVES_UNCOMMITTED_WORK: &str = r#"    printf 'staged by claude\n' > staged.txt || exit 1
    git add staged.txt || exit 1
    printf 'unstaged edit by claude\n' >> README.md || exit 1
    printf 'untracked by claude\n' > untracked.txt || exit 1"#;

/// What the fallback's stage turn records, before it commits, about the tree
/// it inherited: its working directory, branch, `git status --porcelain`, the
/// staged blob, the two working-tree files, and `.planning/config.json` both
/// as committed and as on disk.
const AGY_RECORDS_WHAT_IT_INHERITED: &str = r#"pwd -P > '@LOGS@/agy-cwd'
git rev-parse --abbrev-ref HEAD > '@LOGS@/agy-branch'
git status --porcelain --untracked-files=all > '@LOGS@/agy-status'
git show :staged.txt > '@LOGS@/agy-index-staged'
cat README.md > '@LOGS@/agy-readme'
cat untracked.txt > '@LOGS@/agy-untracked'
git show HEAD:.planning/config.json > '@LOGS@/agy-config-head'
cat .planning/config.json > '@LOGS@/agy-config-worktree'"#;

/// Hermetic `git` for fixture setup (999.37), never a bare `Command::new("git")`.
fn git(root: &Path, args: &[&str]) {
    let output = devflow_core::test_support::git_command(root)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `git <args>` in `root`, trimmed stdout.
fn git_out(root: &Path, args: &[&str]) -> String {
    let output = devflow_core::test_support::git_command(root)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

/// A repository on `develop` with a roadmap entry for `phase` and the
/// `.planning/config.json` auto mode's unattended check reads at Plan and
/// Code (`preflight::unattended_config_condition`).
fn init_repo(root: &Path, phase: PhaseId) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    fs::create_dir_all(root.join(".planning")).unwrap();
    fs::write(root.join("README.md"), "base\n").unwrap();
    fs::write(
        root.join(".planning/ROADMAP.md"),
        format!("# Roadmap\n\n### Phase {phase}: failover fixture\n"),
    )
    .unwrap();
    fs::write(
        root.join(".planning/config.json"),
        "{\n  \"workflow\": {\n    \"auto_advance\": true,\n    \"_auto_chain_active\": false\n  }\n}\n",
    )
    .unwrap();
    git(
        root,
        &[
            "add",
            "README.md",
            ".planning/ROADMAP.md",
            ".planning/config.json",
        ],
    );
    git(root, &["commit", "-q", "-m", "base"]);
}

/// How the `agy` stub answers its delivery canary.
#[derive(Debug, Clone, Copy)]
enum AgyCanary {
    ReturnsToken,
    WithholdsToken,
}

/// One phase's fixture: a hermetic home, a repository, the two stubs on a
/// fake-bin dir and a log dir outside the repository. Bind a
/// [`common::LaunchedRunReaper`] AFTER this, so the reaper drops first.
struct Rig {
    home: common::HermeticHome,
    _repo: tempfile::TempDir,
    root: PathBuf,
    phase: PhaseId,
    fake_bin: common::FakeBin,
    logs: tempfile::TempDir,
}

/// The current time in Unix epoch seconds, the base every computed reset time
/// starts from (Pitfall 10: no fixed epoch, which ages from future to past).
fn now_epoch() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}

/// A reset an hour away: far enough that no test here reaches its relaunch.
fn a_future_reset() -> Option<i64> {
    Some(now_epoch() + 3600)
}

impl Rig {
    fn new(phase: PhaseId, canary: AgyCanary) -> Self {
        Self::build(phase, canary, "", "", a_future_reset())
    }

    /// A rig whose Claude denial names `resets_at` (epoch seconds), or no reset
    /// time at all for `None`.
    fn with_reset(phase: PhaseId, resets_at: Option<i64>) -> Self {
        Self::build(phase, AgyCanary::ReturnsToken, "", "", resets_at)
    }

    /// A rig whose Claude Code turn leaves uncommitted work behind
    /// ([`CODE_TURN_LEAVES_UNCOMMITTED_WORK`]) and whose `agy` stage turn
    /// records what it inherited ([`AGY_RECORDS_WHAT_IT_INHERITED`]).
    fn with_uncommitted_work(phase: PhaseId) -> Self {
        Self::build(
            phase,
            AgyCanary::ReturnsToken,
            CODE_TURN_LEAVES_UNCOMMITTED_WORK,
            AGY_RECORDS_WHAT_IT_INHERITED,
            a_future_reset(),
        )
    }

    fn build(
        phase: PhaseId,
        canary: AgyCanary,
        code_turn_dirt: &str,
        stage_probe: &str,
        resets_at: Option<i64>,
    ) -> Self {
        let home = common::HermeticHome::new();
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path().canonicalize().unwrap();
        init_repo(&root, phase);
        let logs = tempfile::tempdir().unwrap();
        let logs_path = logs.path().display().to_string();
        let padded = phase.padded();

        let claude = CLAUDE_STUB
            .replace("@CODE_TURN_DIRT@", code_turn_dirt)
            .replace("@LOGS@", &logs_path)
            .replace("@PADDED@", &padded)
            .replace("@CLAUDE_INIT@", CLAUDE_INIT_EVENT)
            .replace("@CLAUDE_RATE_LIMIT@", CLAUDE_REJECTED_RATE_LIMIT_EVENT)
            .replace(
                "@RESETS_AT@",
                &resets_at.map_or_else(|| "null".to_string(), |at| at.to_string()),
            )
            .replace("@CLAUDE_SUCCESS@", CLAUDE_SUCCESS_RESULT);
        let fake_bin = common::fake_bin_dir_for("claude", &claude);

        let answer = match canary {
            AgyCanary::ReturnsToken => AGY_CANARY_RETURNS_TOKEN,
            AgyCanary::WithholdsToken => AGY_CANARY_WITHHOLDS_TOKEN,
        };
        let agy = AGY_STUB
            .replace("@STAGE_PROBE@", stage_probe)
            .replace("@LOGS@", &logs_path)
            .replace("@CANARY_ANSWER@", answer)
            .replace("@ANTIGRAVITY_INIT@", ANTIGRAVITY_INIT_EVENT)
            .replace("@ANTIGRAVITY_SUCCESS@", ANTIGRAVITY_SUCCESS_RESULT);
        write_executable(&fake_bin.path.join("agy"), &agy);

        Self {
            home,
            _repo: repo,
            root,
            phase,
            fake_bin,
            logs,
        }
    }

    /// `devflow start --phase N --agent claude --mode auto --no-worktree
    /// --until code`, plus `extra`, with the stubs first on PATH. Asserts the
    /// start itself succeeded.
    fn start(&self, extra: &[&str]) {
        self.start_with(&[&["--no-worktree"], extra].concat());
    }

    /// [`Self::start`] without `--no-worktree`: the run gets its phase
    /// worktree under `.worktrees/`, and every stage launches there.
    fn start_in_worktree(&self, extra: &[&str]) {
        self.start_with(extra);
    }

    fn start_with(&self, extra: &[&str]) {
        self.start_with_env(extra, &[]);
    }

    /// [`Self::start`] with `env` set on the `devflow start` child, which the
    /// detached supervisor inherits (`DEVFLOW_GATE_TIMEOUT_SECS`).
    fn start_with_timeout(&self, gate_timeout_secs: u64) {
        self.start_with_env(
            &["--no-worktree"],
            &[("DEVFLOW_GATE_TIMEOUT_SECS", &gate_timeout_secs.to_string())],
        );
    }

    fn start_with_env(&self, extra: &[&str], env: &[(&str, &str)]) {
        let existing = std::env::var_os("PATH").unwrap_or_default();
        let path = format!(
            "{}:{}",
            self.fake_bin.path.display(),
            existing.to_string_lossy()
        );
        let output = common::hermetic_devflow(&self.home)
            .args([
                "start",
                "--phase",
                &self.phase.to_string(),
                "--agent",
                "claude",
                "--mode",
                "auto",
                "--until",
                "code",
            ])
            .args(extra)
            .arg(&self.root)
            .current_dir(&self.root)
            .env("PATH", path)
            .envs(env.iter().copied())
            .output()
            .expect("run devflow start");
        assert!(
            output.status.success(),
            "start failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Every event in the run's log, in order.
    fn events(&self) -> Vec<Value> {
        fs::read_to_string(devflow_core::events::events_path(&self.root))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .collect()
    }

    /// The event log as text, for failure messages.
    fn events_text(&self) -> String {
        fs::read_to_string(devflow_core::events::events_path(&self.root)).unwrap_or_default()
    }

    fn events_named(&self, name: &str) -> Vec<Value> {
        self.events()
            .into_iter()
            .filter(|event| event["event"] == name)
            .collect()
    }

    /// Wait, bounded by [`RUN_WAIT`], until the log holds any event in
    /// `ends`. The caller asserts which one; waiting on every possible end
    /// lets a wrong end fail at once with the log, not at the bound.
    fn wait_for_any(&self, ends: &[&str]) {
        common::wait_for(
            || {
                self.events()
                    .iter()
                    .any(|event| ends.iter().any(|end| event["event"] == *end))
            },
            RUN_WAIT,
            &format!(
                "one of {ends:?} in the event log of phase {}; events so far:\n{}",
                self.phase,
                self.events_text()
            ),
        );
    }

    /// The stub's log lines, `health` probes removed. A stub never invoked
    /// has no log, which reads as empty.
    fn invocations(&self, program: &str) -> Vec<String> {
        fs::read_to_string(self.logs.path().join(format!("{program}.log")))
            .unwrap_or_default()
            .lines()
            .filter(|line| *line != "health")
            .map(str::to_string)
            .collect()
    }

    /// How many Code stage turns the `claude` stub ran (its `stage code` log
    /// lines; the log also keeps `canary` and `stage plan`).
    fn claude_code_turns(&self) -> usize {
        self.invocations("claude")
            .iter()
            .filter(|line| *line == "stage code")
            .count()
    }

    /// The `stage_launched` events for the Code stage, in order: the second,
    /// independent count of Code launches.
    fn code_launches(&self) -> Vec<Value> {
        self.events_named("stage_launched")
            .into_iter()
            .filter(|event| event["stage"] == "code")
            .collect()
    }

    /// The prompt of each `claude` invocation, in order: the `content` of each
    /// stdin turn the stub logged, decoded from its JSON so it holds the bytes
    /// the agent was given.
    fn claude_prompts(&self) -> Vec<String> {
        fs::read_to_string(self.logs.path().join("claude-stdin.log"))
            .unwrap_or_default()
            .lines()
            .map(|line| {
                let turn: Value = serde_json::from_str(line).unwrap_or_else(|error| {
                    panic!("a stdin turn is one JSON line ({error}): {line}")
                });
                turn["message"]["content"]
                    .as_str()
                    .unwrap_or_else(|| {
                        panic!("a stdin turn carries its prompt as a string: {line}")
                    })
                    .to_string()
            })
            .collect()
    }

    /// The phase's open Code gate file, parsed.
    fn code_gate(&self) -> Option<Value> {
        let path = devflow_core::gates::Gates::gate_path(
            &self.root,
            self.phase,
            devflow_core::stage::Stage::Code,
        );
        serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
    }

    /// Run `devflow gate <args> <phase> --stage code ...` against the rig.
    fn gate(&self, verb: &str, extra: &[&str]) -> std::process::Output {
        common::hermetic_devflow(&self.home)
            .args(["gate", verb, &self.phase.to_string(), "--stage", "code"])
            .args(extra)
            .arg("--project")
            .arg(&self.root)
            .output()
            .expect("run devflow gate")
    }

    /// A commit id a stub recorded, outside the repository.
    fn recorded_commit(&self, name: &str) -> String {
        fs::read_to_string(self.logs.path().join(name))
            .unwrap_or_else(|err| panic!("{name} was not recorded: {err}"))
            .trim()
            .to_string()
    }

    fn state(&self) -> State {
        devflow_core::workflow::load_state(&self.root, self.phase).expect("the run's state")
    }

    /// Where 3.x wrote its rate-limit cron record. Nothing writes or reads it
    /// now (D-01), so the name is a literal: the absence check below stays a
    /// control that fails if a writer of that file comes back.
    fn cron_file(&self) -> PathBuf {
        self.root
            .join(".devflow")
            // Split so the removed-surface scan does not read this control as a leftover.
            .join(format!(
                concat!("cron-", "instructions-{}.json"),
                self.phase.padded()
            ))
    }
}

/// The index of the first event named `name`, panicking with the log if none.
fn position(events: &[Value], name: &str, log: &str) -> usize {
    events
        .iter()
        .position(|event| event["event"] == name)
        .unwrap_or_else(|| panic!("no {name} event; events:\n{log}"))
}

/// D-11 / criterion 1 at Code (R-2) for the pair auto mode admits: a Claude
/// run rate-limited at Code continues under Antigravity in the same tree on
/// the same branch, after Antigravity's own delivery canary (D-04), and the
/// Code stage completes. Negative control: no resume is scheduled.
#[test]
fn a_rate_limited_claude_run_in_auto_mode_hands_off_to_antigravity_and_completes_code() {
    let phase = PhaseId::new(5121);
    let rig = Rig::new(phase, AgyCanary::ReturnsToken);
    let _reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start(&["--fallback-agent", "antigravity"]);
    let branch = git_out(&rig.root, &["rev-parse", "--abbrev-ref", "HEAD"]);
    assert_eq!(branch, format!("feature/phase-{}", phase.padded()));
    assert_eq!(
        rig.state().fallback_agent,
        Some(AgentKind::Antigravity),
        "start persisted the fallback"
    );

    rig.wait_for_any(&[
        "workflow_finished",
        "gate_fired",
        "rate_limit_resume_scheduled",
    ]);
    let log = rig.events_text();
    let events = rig.events();

    let handoffs = rig.events_named("agent_handoff");
    assert_eq!(handoffs.len(), 1, "exactly one handoff; events:\n{log}");
    let handoff = &handoffs[0];
    assert_eq!(handoff["stage"], "code", "{handoff}");
    assert_eq!(handoff["from_agent"], "claude", "{handoff}");
    assert_eq!(handoff["to_agent"], "antigravity", "{handoff}");
    assert_eq!(handoff["reason"], "rate_limited", "{handoff}");

    let claude_canary = position(&events, "claude_delivery_canary_confirmed", &log);
    let handed = position(&events, "agent_handoff", &log);
    let antigravity_canary = position(&events, "antigravity_delivery_canary_confirmed", &log);
    let finished = position(&events, "workflow_finished", &log);
    assert!(
        claude_canary < handed && handed < antigravity_canary && antigravity_canary < finished,
        "Claude's canary, the handoff, Antigravity's own canary, then the finish; events:\n{log}"
    );
    assert_eq!(
        events[finished]["reason"], "stopped_at",
        "Code completed and the run stopped at --until code; events:\n{log}"
    );

    assert_eq!(
        rig.invocations("claude"),
        ["canary", "stage plan", "stage code"],
        "Claude ran its canary, then Plan and Code"
    );
    assert_eq!(
        rig.invocations("agy"),
        ["canary", "stage"],
        "Antigravity ran its own canary before its stage turn"
    );

    let state = rig.state();
    assert_eq!(state.agent, AgentKind::Antigravity);
    assert!(state.fallback_consumed, "the one hop is spent");

    let claude_commit = rig.recorded_commit("claude-code-commit");
    let agy_commit = rig.recorded_commit("agy-code-commit");
    assert_eq!(
        git_out(&rig.root, &["rev-parse", "--abbrev-ref", "HEAD"]),
        branch,
        "the branch is unchanged across the hop"
    );
    assert_eq!(
        git_out(&rig.root, &["rev-parse", "HEAD"]),
        agy_commit,
        "the fallback's commit is the branch tip"
    );
    assert_eq!(
        git_out(&rig.root, &["rev-parse", "HEAD^"]),
        claude_commit,
        "the fallback built on the primary's Code-turn commit: nothing reset or rebased"
    );

    assert!(
        rig.events_named("rate_limit_resume_scheduled").is_empty(),
        "the handoff arm schedules no resume; events:\n{log}"
    );
}

/// D-04 (never silent) with R2-D1: when the fallback's own canary refuses the
/// relaunch after the handoff commit, the run ends at the Code gate, not the
/// cron arm. Also the control that the canary-order assertion above can fail:
/// without the in-supervisor canary, this run would complete Code.
#[test]
fn a_handoff_to_antigravity_that_fails_its_canary_ends_at_a_gate() {
    let phase = PhaseId::new(5123);
    let rig = Rig::new(phase, AgyCanary::WithholdsToken);
    let _reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start(&["--fallback-agent", "antigravity"]);
    rig.wait_for_any(&[
        "gate_fired",
        "workflow_finished",
        "rate_limit_resume_scheduled",
    ]);
    let log = rig.events_text();
    let events = rig.events();

    let handed = position(&events, "agent_handoff", &log);
    let failed = position(&events, "agent_handoff_relaunch_failed", &log);
    let gated = position(&events, "gate_fired", &log);
    assert!(
        handed < failed && failed < gated,
        "the handoff, then the relaunch failure, then the gate; events:\n{log}"
    );
    assert_eq!(events[handed]["stage"], "code", "{}", events[handed]);
    assert_eq!(events[failed]["stage"], "code", "{}", events[failed]);
    assert_eq!(
        events[failed]["to_agent"], "antigravity",
        "{}",
        events[failed]
    );
    assert_eq!(events[gated]["stage"], "code", "{}", events[gated]);
    let context = events[gated]["context"].as_str().unwrap_or_default();
    let body = context.strip_prefix("[never-silent] ").unwrap_or(context);
    let way_on = body.find(&format!("`devflow resume --phase {phase} --agent <other>`"));
    let cause = body.find("the handoff to antigravity could not relaunch the code stage");
    assert!(
        matches!((way_on, cause), (Some(way_on), Some(cause)) if way_on < cause),
        "the gate names the way on before the cause: {}",
        events[gated]
    );
    assert!(
        !context.contains("a retry or loop-back will not relaunch under"),
        "the removed hand-written lead is gone: {}",
        events[gated]
    );

    assert!(
        rig.events_named("rate_limit_resume_scheduled").is_empty(),
        "a refused relaunch schedules no resume (R2-D1); events:\n{log}"
    );
    assert!(
        !rig.events_named("workflow_finished")
            .iter()
            .any(|event| event["reason"] == "stopped_at"),
        "Code did not complete; events:\n{log}"
    );

    assert_eq!(
        rig.invocations("agy"),
        ["canary"],
        "Antigravity ran its canary and no stage turn"
    );

    let state = rig.state();
    assert_eq!(state.agent, AgentKind::Antigravity);
    assert!(state.fallback_consumed, "the one hop is spent");
    assert_eq!(state.canary, Some(CanaryOutcome::Absent));
    assert!(state.gate_pending, "the never-silent gate is open");
}

/// Drive a run whose fallback failed its canary to the Code gate, answer that gate with
/// `answer` (`gate approve` or `gate reject`, plus its arguments), and wait for the
/// supervisor's response. Returns the event log, parsed and as text.
fn answer_the_failed_canary_gate(rig: &Rig, answer: &[&str]) -> (Vec<Value>, String) {
    let phase = rig.phase;
    rig.start(&["--fallback-agent", "antigravity"]);
    rig.wait_for_any(&[
        "gate_fired",
        "workflow_finished",
        "rate_limit_resume_scheduled",
    ]);
    assert!(
        !rig.events_named("gate_fired").is_empty(),
        "the run reached the Code gate first; events:\n{}",
        rig.events_text()
    );

    let output = common::hermetic_devflow(&rig.home)
        .args(["gate", answer[0], &phase.to_string(), "--stage", "code"])
        .args(&answer[1..])
        .arg("--project")
        .arg(&rig.root)
        .output()
        .expect("run devflow gate");
    // A fast waiter can consume the answer and close its gate before the writer's
    // re-check (backlog 999.177); the event below decides whether it was applied.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() || stderr.contains("published after that gate closed"),
        "the gate answer failed: {stderr}"
    );
    rig.wait_for_any(&["gate_relaunch_refused", "workflow_finished"]);
    (rig.events(), rig.events_text())
}

/// What a retry or loop-back answer leaves when the recorded canary refuses the run's
/// agent: a stopped run naming the refusal and the resume that continues on another
/// agent, a `gate_relaunch_refused` event, no open gate, and no second launch.
fn assert_the_relaunch_was_refused_loudly(rig: &Rig, events: &[Value], log: &str) {
    let refused = position(events, "gate_relaunch_refused", log);
    assert_eq!(events[refused]["stage"], "code", "{}", events[refused]);
    assert_eq!(
        events[refused]["agent"], "antigravity",
        "{}",
        events[refused]
    );
    assert!(
        events[refused]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("ABSENT")),
        "the event names the recorded refusal: {}",
        events[refused]
    );

    let state = rig.state();
    assert!(
        state.stopped,
        "the run is marked stopped, not left live-looking"
    );
    let reason = state.stop_reason.as_deref().unwrap_or_default();
    assert!(
        reason.contains(&format!(
            "run `devflow resume --phase {} --agent <other>`",
            rig.phase
        )),
        "the stop reason names the way on: {reason}"
    );
    assert!(
        reason.contains("the code stage cannot relaunch under antigravity"),
        "the stop reason names the stage and the agent: {reason}"
    );
    assert!(!state.gate_pending, "no gate is left open");
    assert_eq!(state.canary, Some(CanaryOutcome::Absent));
    assert_eq!(
        rig.invocations("agy"),
        ["canary"],
        "the refused relaunch spawned no second canary and no stage turn"
    );
}

/// A retry answer (`gate approve`) to the gate after a failed-canary handoff ends the
/// run loudly instead of relaunching into the recorded refusal and ending the
/// supervisor with no record, gate or event (999.187 at this entry point).
#[test]
fn approving_the_failed_canary_gate_stops_the_run_and_names_the_way_on() {
    let phase = PhaseId::new(5124);
    let rig = Rig::new(phase, AgyCanary::WithholdsToken);
    let _reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);
    let (events, log) = answer_the_failed_canary_gate(&rig, &["approve"]);
    assert_the_relaunch_was_refused_loudly(&rig, &events, &log);
}

/// The loop-back answer (`gate reject` without `abort`) ends the same way as a retry.
#[test]
fn rejecting_the_failed_canary_gate_stops_the_run_and_names_the_way_on() {
    let phase = PhaseId::new(5125);
    let rig = Rig::new(phase, AgyCanary::WithholdsToken);
    let _reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);
    let (events, log) = answer_the_failed_canary_gate(&rig, &["reject", "--note", "try again"]);
    assert_the_relaunch_was_refused_loudly(&rig, &events, &log);
}

/// The way on after the refused relaunch, as an operator would take it. Once the
/// supervisor has ended, `devflow status` names `resume --agent <other>`; a plain
/// `devflow resume` is refused by the recorded canary and leaves the stop reason that
/// names the way on as it was; `devflow resume --agent claude` hands off, which clears
/// the recorded canary, and relaunches Code after Claude's own canary. Negative control
/// for the plain resume: Antigravity ran no second canary or stage turn.
#[test]
fn after_the_refused_relaunch_status_and_resume_lead_to_another_agent() {
    let phase = PhaseId::new(5126);
    let rig = Rig::new(phase, AgyCanary::WithholdsToken);
    let _reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);
    let (events, log) = answer_the_failed_canary_gate(&rig, &["approve"]);
    assert_the_relaunch_was_refused_loudly(&rig, &events, &log);

    let hint = format!("devflow resume --phase {phase} --agent <other>");
    let status = || {
        let output = common::hermetic_devflow(&rig.home)
            .arg("status")
            .arg(&rig.root)
            .output()
            .expect("run devflow status");
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    // `status` prints no hint while the supervisor still answers, so this also waits
    // for it to end before `resume`, which refuses a live run.
    common::wait_for(
        || status().contains(&hint),
        RUN_WAIT,
        &format!("`{hint}` in devflow status"),
    );
    let shown = status();
    assert!(
        shown
            .lines()
            .filter(|line| line.contains(&format!("devflow resume --phase {phase}")))
            .all(|line| line.contains("--agent <other>")),
        "status does not also name the plain resume:\n{shown}"
    );

    let path = format!(
        "{}:{}",
        rig.fake_bin.path.display(),
        std::env::var_os("PATH")
            .unwrap_or_default()
            .to_string_lossy()
    );
    let resume = |extra: &[&str]| {
        common::hermetic_devflow(&rig.home)
            .args(["resume", "--phase", &phase.to_string()])
            .args(extra)
            .arg(&rig.root)
            .current_dir(&rig.root)
            .env("PATH", &path)
            .output()
            .expect("run devflow resume")
    };

    let stop_reason = rig.state().stop_reason;
    let plain = resume(&[]);
    let stderr = String::from_utf8_lossy(&plain.stderr);
    assert!(
        !plain.status.success(),
        "a plain resume is refused: {stderr}"
    );
    assert!(
        stderr.contains(&hint),
        "the refusal names the way on: {stderr}"
    );
    let after_plain = rig.state();
    assert!(after_plain.stopped, "the run is still stopped");
    assert_eq!(
        after_plain.stop_reason, stop_reason,
        "the refused resume left the stop reason that names the way on"
    );
    assert_eq!(after_plain.canary, Some(CanaryOutcome::Absent));
    assert_eq!(
        rig.invocations("agy"),
        ["canary"],
        "the refused resume launched nothing"
    );

    let handed = resume(&["--agent", "claude"]);
    assert!(
        handed.status.success(),
        "resume --agent claude failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&handed.stdout),
        String::from_utf8_lossy(&handed.stderr)
    );
    common::wait_for(
        || {
            let events = rig.events();
            let Some(last_handoff) = events
                .iter()
                .rposition(|event| event["event"] == "agent_handoff")
            else {
                return false;
            };
            events[last_handoff..].iter().any(|event| {
                [
                    "rate_limit_resume_scheduled",
                    "gate_fired",
                    "workflow_finished",
                ]
                .iter()
                .any(|end| event["event"] == *end)
            })
        },
        RUN_WAIT,
        &format!(
            "an end after the resume's handoff in phase {phase}; events so far:\n{}",
            rig.events_text()
        ),
    );
    let log = rig.events_text();
    let handoffs = rig.events_named("agent_handoff");
    assert_eq!(
        handoffs.len(),
        2,
        "the failover, then the resume's handoff; events:\n{log}"
    );
    assert_eq!(handoffs[1]["from_agent"], "antigravity", "{}", handoffs[1]);
    assert_eq!(handoffs[1]["to_agent"], "claude", "{}", handoffs[1]);
    assert_eq!(handoffs[1]["reason"], "resume --agent", "{}", handoffs[1]);
    assert_eq!(
        rig.invocations("claude"),
        ["canary", "stage plan", "stage code", "canary", "stage code"],
        "Claude ran its own canary again, then relaunched Code"
    );
    let state = rig.state();
    assert_eq!(state.agent, AgentKind::Claude);
    assert_eq!(
        state.canary,
        Some(CanaryOutcome::Confirmed),
        "the handoff cleared the recorded refusal and Claude's canary passed"
    );
}

/// CUT-03 (interim) under the same auto-mode rig: with no fallback, the same
/// Claude rate limit at Code waits at the never-silent Code gate, which names
/// the resume command and the reset time from the denial. Both directions in
/// one fixture: the cron-instructions file the old arm wrote is absent, and
/// the gate is present. Negative control: no handoff, and the `agy` stub on
/// PATH is never launched for a canary or a stage.
#[test]
fn a_rate_limited_claude_run_without_a_fallback_waits_at_a_gate() {
    let phase = PhaseId::new(5122);
    let resets_at = now_epoch() + 3600;
    let rig = Rig::with_reset(phase, Some(resets_at));
    let _reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start(&[]);
    rig.wait_for_any(&[
        "rate_limit_resume_scheduled",
        "gate_fired",
        "workflow_finished",
    ]);
    let log = rig.events_text();

    assert!(
        !rig.cron_file().exists(),
        // Split so the removed-surface scan does not read this control as a leftover.
        concat!("no cron-", "instructions file is written; events:\n{}"),
        log
    );
    assert!(
        rig.events_named("rate_limit_resume_scheduled").is_empty(),
        "no resume is scheduled; events:\n{log}"
    );
    let gated = rig.events_named("gate_fired");
    assert_eq!(gated.len(), 1, "one gate; events:\n{log}");
    assert_eq!(gated[0]["stage"], "code", "{}", gated[0]);
    assert!(
        devflow_core::gates::Gates::gate_path(&rig.root, phase, devflow_core::stage::Stage::Code)
            .exists(),
        "the Code gate file is open"
    );

    let shown = common::hermetic_devflow(&rig.home)
        .args(["gate", "show", &phase.to_string(), "--stage", "code"])
        .arg(&rig.root)
        .output()
        .expect("run devflow gate show");
    let shown_text = String::from_utf8_lossy(&shown.stdout);
    assert!(
        shown.status.success(),
        "gate show failed: {}",
        String::from_utf8_lossy(&shown.stderr)
    );
    assert!(
        shown_text.contains(&resets_at.to_string()),
        "the gate names the reset time from the denial: {shown_text}"
    );
    assert!(
        shown_text.contains(&format!("devflow resume --phase {phase}")),
        "the gate names the resume command: {shown_text}"
    );
    assert!(
        shown_text.contains(&format!(
            "once this waiting supervisor has ended, `devflow resume --phase {phase}`"
        )),
        "the resume command is worded as the way on once the waiter is gone: {shown_text}"
    );
    assert_eq!(
        rig.claude_code_turns(),
        1,
        "one Code turn before the gate: {:?}",
        rig.invocations("claude")
    );
    assert_eq!(
        rig.code_launches().len(),
        1,
        "one stage_launched event for Code; events:\n{log}"
    );

    assert!(
        rig.events_named("agent_handoff").is_empty(),
        "no fallback, no handoff; events:\n{log}"
    );
    assert_eq!(
        rig.invocations("agy"),
        Vec::<String>::new(),
        "agy is on PATH and never launched for a canary or a stage"
    );

    let state = rig.state();
    assert_eq!(state.agent, AgentKind::Claude);
    assert_eq!(state.fallback_agent, None);
}

/// How long a test watches for a launch that must not happen.
const NO_MORE_LAUNCHES_WITHIN: Duration = Duration::from_secs(5);

/// The responder name the rate-limit timer writes on its answer
/// (`devflow_core::gates::RATE_LIMIT_TIMER`).
const TIMER: &str = devflow_core::gates::RATE_LIMIT_TIMER;

/// Wait, bounded by [`RUN_WAIT`], until `count` gates have fired in the rig's
/// run. The failure message reads the logs when the wait ends, not when it starts.
fn wait_for_gates(rig: &Rig, count: usize) {
    let started = std::time::Instant::now();
    while rig.events_named("gate_fired").len() < count {
        assert!(
            started.elapsed() < RUN_WAIT,
            "timed out after {RUN_WAIT:?} waiting for {count} gate_fired events in phase {}; \
             claude ran {:?}; events:\n{}",
            rig.phase,
            rig.invocations("claude"),
            rig.events_text()
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// CUT-03 / D-16 / D-17 / D-38, criterion 3: a denial whose reset (plus the
/// margin) has already passed relaunches the Code stage once by itself, and
/// the second denial waits at the gate. Code launches are counted two
/// independent ways: the stub's `stage code` turns and the `stage_launched`
/// events for Code.
#[test]
fn a_passed_reset_relaunches_once_and_a_second_denial_waits() {
    let phase = PhaseId::new(5127);
    let rig = Rig::with_reset(phase, Some(now_epoch() - 3600));
    let _reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start(&[]);
    wait_for_gates(&rig, 2);
    std::thread::sleep(NO_MORE_LAUNCHES_WITHIN);
    let log = rig.events_text();

    assert_eq!(
        rig.claude_code_turns(),
        2,
        "the original Code turn and one automatic relaunch: {:?}; events:\n{log}",
        rig.invocations("claude")
    );
    assert_eq!(
        rig.invocations("claude")
            .iter()
            .filter(|line| *line == "stage plan")
            .count(),
        1,
        "Plan ran once: {:?}",
        rig.invocations("claude")
    );
    assert_eq!(
        rig.code_launches().len(),
        2,
        "two stage_launched events for Code; events:\n{log}"
    );
    let resolved = rig.events_named("gate_resolved");
    assert_eq!(resolved.len(), 1, "one gate was answered; events:\n{log}");
    assert_eq!(
        resolved[0]["responded_by"], TIMER,
        "the timer answered it: {}",
        resolved[0]
    );
    assert_eq!(resolved[0]["gate_kind"], "rate_limit", "{}", resolved[0]);

    let gate = rig.code_gate().expect("the second denial's gate is open");
    assert_eq!(gate["gate_kind"], "rate_limit", "{gate}");
    assert!(
        gate["not_before"].is_i64(),
        "the second gate records its not-before time: {gate}"
    );
    assert!(
        rig.state().rate_limit_auto_relaunch_used,
        "the run's one automatic relaunch is spent"
    );
}

/// `devflow_core::reset_time::RATE_LIMIT_RELAUNCH_MARGIN_SECS`, the margin the
/// timer waits after a reset.
const MARGIN: i64 = devflow_core::reset_time::RATE_LIMIT_RELAUNCH_MARGIN_SECS;

/// The bound a relaunch at a near reset must land within.
const RELAUNCH_WITHIN: Duration = Duration::from_secs(60);

/// Wait, bounded by `limit`, until the run has launched Code `count` times
/// (by `stage_launched` events) and the stub has run that many Code turns.
fn wait_for_code_launches(rig: &Rig, count: usize, limit: Duration) {
    let started = std::time::Instant::now();
    while rig.code_launches().len() < count || rig.claude_code_turns() < count {
        assert!(
            started.elapsed() < limit,
            "timed out after {limit:?} waiting for {count} Code launches in phase {}; claude \
             ran {:?}; events:\n{}",
            rig.phase,
            rig.invocations("claude"),
            rig.events_text()
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// The index of the first event named `name` at or after `from`, if any.
fn position_from(events: &[Value], name: &str, from: usize) -> Option<usize> {
    events
        .iter()
        .skip(from)
        .position(|event| event["event"] == name)
        .map(|at| at + from)
}

/// D-27.2: a reset later than `DEVFLOW_GATE_TIMEOUT_SECS` keeps the gate open
/// past that timeout. The other direction of
/// `a_reset_earlier_than_the_gate_timeout_relaunches_at_the_reset`: a lifetime
/// cut to the gate timeout fails here.
#[test]
fn a_future_reset_outlives_the_gate_timeout() {
    const GATE_TIMEOUT_SECS: u64 = 2;
    let phase = PhaseId::new(5128);
    let rig = Rig::with_reset(phase, a_future_reset());
    let _reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start_with_timeout(GATE_TIMEOUT_SECS);
    wait_for_gates(&rig, 1);
    std::thread::sleep(Duration::from_secs(GATE_TIMEOUT_SECS) + NO_MORE_LAUNCHES_WITHIN);
    let log = rig.events_text();

    let gate = rig
        .code_gate()
        .expect("the gate is still open past the timeout");
    assert_eq!(gate["gate_kind"], "rate_limit", "{gate}");
    assert!(
        rig.events_named("gate_timeout").is_empty(),
        "the gate timeout did not end the wait; events:\n{log}"
    );
    assert!(
        rig.events_named("workflow_aborted").is_empty(),
        "nothing aborted; events:\n{log}"
    );
    assert_eq!(
        rig.claude_code_turns(),
        1,
        "{:?}",
        rig.invocations("claude")
    );
}

/// D-45 rule 5: the armed timer wakes at reset + margin whatever the gate's
/// lifetime, so a reset a few seconds away relaunches within a bounded wait
/// while the gate timeout is ten minutes away, and not before reset + margin.
/// A wake tied to the gate's lifetime fails here.
#[test]
fn a_reset_earlier_than_the_gate_timeout_relaunches_at_the_reset() {
    const GATE_TIMEOUT_SECS: u64 = 600;
    let phase = PhaseId::new(5129);
    let resets_at = now_epoch() - MARGIN + 5;
    let rig = Rig::with_reset(phase, Some(resets_at));
    let _reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start_with_timeout(GATE_TIMEOUT_SECS);
    wait_for_code_launches(&rig, 2, RELAUNCH_WITHIN);
    std::thread::sleep(NO_MORE_LAUNCHES_WITHIN);
    let log = rig.events_text();

    let launches = rig.code_launches();
    assert_eq!(launches.len(), 2, "one relaunch; events:\n{log}");
    assert_eq!(
        rig.claude_code_turns(),
        2,
        "{:?}",
        rig.invocations("claude")
    );
    let relaunched_at = launches[1]["ts"].as_i64().expect("an event timestamp");
    assert!(
        relaunched_at >= resets_at + MARGIN,
        "the relaunch ({relaunched_at}) is not before reset + margin ({}); events:\n{log}",
        resets_at + MARGIN
    );
    let resolved = rig.events_named("gate_resolved");
    assert_eq!(resolved[0]["responded_by"], TIMER, "{}", resolved[0]);
}

/// U8, the dispatcher row: with a 2 s gate timeout and a wake about 6 s away,
/// the gate's lifetime equals the wake, and the wake wins: the Code stage
/// relaunches once at the wake, answered by the timer, with no `gate_timeout`
/// and no `workflow_aborted` before that relaunch. (The second denial's gate
/// has no timer and a 2 s lifetime, so it times out afterwards; that is not
/// this row's gate.) Pins the wiring; the equal-deadline pin is the pure row of
/// `pipeline_gate::tests::the_timer_wakes_at_reset_plus_margin_whatever_the_gate_timeout`.
#[test]
fn a_reset_later_than_the_gate_timeout_relaunches_at_the_reset() {
    const GATE_TIMEOUT_SECS: u64 = 2;
    let phase = PhaseId::new(5130);
    let resets_at = now_epoch() - MARGIN + 6;
    let rig = Rig::with_reset(phase, Some(resets_at));
    let _reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start_with_timeout(GATE_TIMEOUT_SECS);
    wait_for_code_launches(&rig, 2, RELAUNCH_WITHIN);
    let log = rig.events_text();
    let events = rig.events();

    let launches = rig.code_launches();
    let relaunched_at = launches[1]["ts"].as_i64().expect("an event timestamp");
    assert!(
        relaunched_at >= resets_at + MARGIN,
        "the relaunch ({relaunched_at}) is not before reset + margin ({}); events:\n{log}",
        resets_at + MARGIN
    );
    let first_gate = position(&events, "gate_fired", &log);
    let relaunch = events
        .iter()
        .enumerate()
        .filter(|(_, event)| event["event"] == "stage_launched" && event["stage"] == "code")
        .map(|(at, _)| at)
        .nth(1)
        .expect("the relaunch's stage_launched event");
    let resolved =
        position_from(&events, "gate_resolved", first_gate).expect("the first gate was answered");
    assert!(
        resolved < relaunch,
        "answered, then relaunched; events:\n{log}"
    );
    assert_eq!(
        events[resolved]["responded_by"], TIMER,
        "{}",
        events[resolved]
    );
    for end in ["gate_timeout", "workflow_aborted"] {
        assert!(
            position_from(&events, end, first_gate).is_none_or(|at| at > relaunch),
            "no {end} before the relaunch; events:\n{log}"
        );
    }
}

/// D-16: a denial with no reset time is an ordinary blocking gate: nothing is
/// guessed and nothing relaunches by itself.
#[test]
fn a_denial_without_a_reset_time_is_a_plain_gate() {
    let phase = PhaseId::new(5131);
    let rig = Rig::with_reset(phase, None);
    let _reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start(&[]);
    wait_for_gates(&rig, 1);
    std::thread::sleep(NO_MORE_LAUNCHES_WITHIN);
    let log = rig.events_text();

    let gate = rig.code_gate().expect("the gate stays open");
    assert_eq!(gate["gate_kind"], "rate_limit", "{gate}");
    assert!(
        gate.get("not_before").is_none(),
        "no reset, no not-before time: {gate}"
    );
    assert!(
        gate["context"]
            .as_str()
            .is_some_and(|context| context.contains("the denial carried no reset time")),
        "{gate}"
    );
    assert_eq!(
        rig.claude_code_turns(),
        1,
        "{:?}",
        rig.invocations("claude")
    );
    assert_eq!(rig.code_launches().len(), 1, "no relaunch; events:\n{log}");
    assert!(
        rig.events_named("rate_limit_timer_fired").is_empty(),
        "{log}"
    );
}

/// Answer the waiting rate-limit gate with `gate <verb> <extra>` and wait for the
/// relaunch's own denial to open the second gate.
fn answer_and_wait_for_the_relaunch(rig: &Rig, verb: &str, extra: &[&str]) {
    let output = rig.gate(verb, extra);
    // A fast waiter can consume the answer and close its gate before the writer's
    // re-check (backlog 999.177); the relaunch below decides whether it was applied.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() || stderr.contains("published after that gate closed"),
        "gate {verb} failed: {stderr}"
    );
    wait_for_gates(rig, 2);
}

/// D-15 / D-27 / D-38: `gate approve` on the waiting rate-limit gate relaunches
/// the same stage on the same agent, and an operator's answer does not spend
/// the run's one automatic relaunch.
#[test]
fn approving_a_rate_limit_gate_relaunches_the_same_stage_on_the_same_agent() {
    let phase = PhaseId::new(5132);
    let rig = Rig::with_reset(phase, a_future_reset());
    let _reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start(&[]);
    wait_for_gates(&rig, 1);
    answer_and_wait_for_the_relaunch(&rig, "approve", &[]);
    let log = rig.events_text();

    assert_eq!(
        rig.claude_code_turns(),
        2,
        "{:?}",
        rig.invocations("claude")
    );
    let launches = rig.code_launches();
    assert_eq!(launches.len(), 2, "one relaunch; events:\n{log}");
    assert_eq!(launches[1]["agent"], "claude", "{}", launches[1]);
    let resolved = rig.events_named("gate_resolved");
    assert_ne!(resolved[0]["responded_by"], TIMER, "{}", resolved[0]);
    assert!(
        !rig.state().rate_limit_auto_relaunch_used,
        "an operator's approve does not spend the automatic relaunch"
    );
}

/// D-37 / D-38: `gate reject --note` on the waiting rate-limit gate relaunches
/// the same stage on the same agent with the note in the prompt the stub reads
/// on stdin, byte for byte, and leaves the automatic relaunch unspent.
/// Control: the original Code turn's prompt does not hold the note.
#[test]
fn rejecting_a_rate_limit_gate_relaunches_the_same_stage_with_the_note() {
    const NOTE: &str = "wait for the quota,\n\tthen rerun \"cargo test\" and 'nothing else'";
    let phase = PhaseId::new(5133);
    let rig = Rig::with_reset(phase, a_future_reset());
    let _reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start(&[]);
    wait_for_gates(&rig, 1);
    answer_and_wait_for_the_relaunch(&rig, "reject", &["--note", NOTE]);
    let log = rig.events_text();

    assert_eq!(
        rig.claude_code_turns(),
        2,
        "{:?}",
        rig.invocations("claude")
    );
    let launches = rig.code_launches();
    assert_eq!(launches.len(), 2, "one relaunch; events:\n{log}");
    assert_eq!(launches[1]["agent"], "claude", "{}", launches[1]);
    let code_prompts: Vec<String> = rig
        .claude_prompts()
        .into_iter()
        .filter(|prompt| prompt.contains("gsd-execute-phase"))
        .collect();
    assert_eq!(code_prompts.len(), 2, "two Code prompts were logged");
    assert!(
        !code_prompts[0].contains(NOTE),
        "control: the original Code prompt holds no note:\n{}",
        code_prompts[0]
    );
    assert!(
        code_prompts[1].contains(NOTE),
        "the relaunch's prompt holds the note byte for byte:\n{}",
        code_prompts[1]
    );
    assert!(
        !rig.state().rate_limit_auto_relaunch_used,
        "an operator's reject does not spend the automatic relaunch"
    );
}

/// Criterion 1 on the worktree arm (D-10): a run in its phase worktree whose
/// Claude Code turn is rate-limited with a staged file, an unstaged edit and an
/// untracked file in the tree hands off to Antigravity, which launches in the
/// SAME worktree on the phase branch and finds all three exactly as Claude
/// left them: nothing reset, stashed, cleaned or checked out.
#[test]
fn a_handoff_inside_a_phase_worktree_keeps_the_tree_branch_and_uncommitted_work() {
    let phase = PhaseId::new(5124);
    let rig = Rig::with_uncommitted_work(phase);
    let _reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start_in_worktree(&["--fallback-agent", "antigravity"]);
    rig.wait_for_any(&[
        "workflow_finished",
        "gate_fired",
        "rate_limit_resume_scheduled",
    ]);
    let log = rig.events_text();

    // Where `start` puts the phase worktree, so a run that lost track of it
    // still fails on the fallback's cwd rather than on the state.
    let worktree = rig
        .root
        .join(".worktrees")
        .join(format!("phase-{}", phase.padded()))
        .canonicalize()
        .expect("start created the phase worktree");
    let branch = format!("feature/phase-{}", phase.padded());

    let handoffs = rig.events_named("agent_handoff");
    assert_eq!(handoffs.len(), 1, "exactly one handoff; events:\n{log}");
    assert_eq!(handoffs[0]["stage"], "code", "{}", handoffs[0]);
    assert_eq!(
        handoffs[0]["tree_dirty"], true,
        "the handoff measured the worktree's uncommitted work: {}",
        handoffs[0]
    );

    let recorded = |name: &str| {
        fs::read_to_string(rig.logs.path().join(name))
            .unwrap_or_else(|err| panic!("the fallback did not record {name}: {err}"))
    };
    assert_eq!(
        PathBuf::from(recorded("agy-cwd").trim()),
        worktree,
        "the fallback launched in the phase worktree"
    );
    assert_eq!(
        recorded("agy-branch").trim(),
        branch,
        "the fallback launched on the phase branch"
    );
    let inherited: Vec<String> = recorded("agy-status").lines().map(str::to_string).collect();
    assert_eq!(
        inherited,
        [
            " M .planning/config.json",
            " M README.md",
            "A  staged.txt",
            "?? untracked.txt"
        ],
        "the staged, unstaged and untracked changes are all still there, as they were, \
         beside DevFlow's own config write"
    );
    // The config change is DevFlow's auto-chain flag write
    // (`gsd_config::set_auto_chain_active`) and nothing else: with that one
    // key removed, the on-disk config equals the committed one.
    let without_auto_chain_flag = |name: &str| {
        let mut config: Value = serde_json::from_str(&recorded(name))
            .unwrap_or_else(|err| panic!("{name} is not JSON: {err}"));
        let shown = config.to_string();
        config["workflow"]
            .as_object_mut()
            .unwrap_or_else(|| panic!("{name} has no workflow object: {shown}"))
            .remove("_auto_chain_active");
        config
    };
    assert_eq!(
        without_auto_chain_flag("agy-config-worktree"),
        without_auto_chain_flag("agy-config-head"),
        "`.planning/config.json` changed beyond the auto-chain flag"
    );
    assert_eq!(recorded("agy-index-staged"), "staged by claude\n");
    assert_eq!(recorded("agy-readme"), "base\nunstaged edit by claude\n");
    assert_eq!(recorded("agy-untracked"), "untracked by claude\n");

    assert!(
        rig.events_named("workflow_finished")
            .iter()
            .any(|event| event["reason"] == "stopped_at"),
        "Code completed under the fallback; events:\n{log}"
    );
    assert_eq!(rig.invocations("agy"), ["canary", "stage"]);

    assert_eq!(
        git_out(&worktree, &["rev-parse", "HEAD^"]),
        rig.recorded_commit("claude-code-commit"),
        "the fallback built on the primary's Code-turn commit in the worktree"
    );
}

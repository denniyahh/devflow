//! The delivery canary (D-13): the guard that notices when the undocumented
//! CLI behaviour this whole arc rests on has gone away.
//!
//! # Why a planted token rather than a version check
//!
//! Claude Code's `task-notification` delivery — the CLI waking a live session
//! back up after a background task finishes — is **undocumented behaviour**,
//! observed only on `claude_code_version 2.1.220`. A CLI update can withdraw it
//! without any announcement, and if it is withdrawn then every multi-plan wave
//! silently orphans its dispatched work: exactly the 999.64 shape this phase
//! exists to close. Reading the version string would guard a *proxy* for the
//! behaviour, not the behaviour, and would go on reporting healthy the moment
//! the same version number stopped meaning the same thing. So the guard plants
//! a value only DevFlow knows and confirms it comes back.
//!
//! # What a `Confirmed` outcome does and does not mean
//!
//! It means **the notification path is alive**. It NEVER means the dispatched
//! work happened. The agent can read the token out of its own prompt and emit
//! it without doing anything at all — that is 999.67's shape, accepted here
//! deliberately (threat T-31-11) rather than mitigated, because mitigating it
//! needs per-child tokens and D-14 defers those on size. Summaries and merges
//! remain the evidence of work (D-16/D-18). Nothing in this module may be
//! rephrased to imply otherwise.
//!
//! # Where the trust decision is made
//!
//! Not here. The CLI echoes the operator's prompt back into the same stdout as
//! a `user` event, so the planted token **will** appear in the capture whether
//! or not anything was delivered — that echo is what produced the checkpoint
//! false positive 30-05 had to fix. The question "did this token come back from
//! somewhere trustworthy?" is therefore answered by exactly one function in
//! this codebase, [`crate::agent_result::token_reported_in_capture`], which
//! confines the match to events that are both `type: "result"` and
//! orchestrator-authored. This module delegates to it and holds no notion of
//! its own about which lines are trustworthy — a second such notion would be
//! free to drift away from the first, and the drift would be invisible.

use crate::agent_result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The fixed, greppable prefix every declared canary token carries.
///
/// Exposed so the run's provenance can record WHICH guard ran without
/// recording the token itself (T-31-13).
pub const TOKEN_PREFIX: &str = "DEVFLOW_DELIVERY_CANARY_";

/// File name of the canary's own throwaway capture, inside the capture dir.
///
/// Deliberately NOT the phase capture (`.devflow/phase-NN-stdout.log`): that
/// file is the one artifact the entire Layer 1 cascade decides a stage on, and
/// a guard that clobbered it would break the thing it exists to protect.
const CAPTURE_FILE: &str = "delivery-canary.jsonl";

/// Monotonic within one process — the third input to [`declare_token`].
static TOKEN_SEQ: AtomicU64 = AtomicU64::new(0);

/// Declare a fresh success token for one canary run.
///
/// **This is a nonce, not a secret, and must not be "upgraded" into one.** The
/// only property required (RESEARCH § ASVS V6) is that an agent cannot produce
/// the value by chance inside its own generated text. A 64-bit hash of the
/// current wall-clock nanos, this process's pid and a per-process counter
/// clears that bar by a wide margin, and it costs no new dependency — which is
/// why `std::hash::DefaultHasher` is used here rather than a CSPRNG crate.
/// Nothing downstream authenticates anything with this value.
///
/// Two calls in one process differ because the counter feeds the hash. That
/// makes distinctness overwhelming (a 64-bit collision), not absolute; the
/// token is a nonce and nothing breaks on the ~2⁻⁶⁴ tie.
pub fn declare_token() -> String {
    use std::hash::{Hash, Hasher};

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = TOKEN_SEQ.fetch_add(1, Ordering::Relaxed);

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    nanos.hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    seq.hash(&mut hasher);

    format!("{TOKEN_PREFIX}{:016x}", hasher.finish())
}

/// What one canary run established.
///
/// `Absent` and `Unverified` are kept apart on purpose, and collapsing them
/// would be a real loss of information: "the CLI ran and the behaviour is gone"
/// and "the CLI could not be run at all" call for completely different operator
/// action, and a merged variant would report a missing binary as a broken
/// premise (threat T-31-12 — the risk this guard carries is a FALSE refusal).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanaryOutcome {
    /// The declared token came back from a trustworthy place. The notification
    /// path is alive. This says NOTHING about whether work happened.
    Confirmed,
    /// The CLI ran and the token did not come back. The premise this arc rests
    /// on is no longer backed by observed behaviour.
    Absent,
    /// The guard itself could not reach a conclusion — carries the reason. Not
    /// a statement about the CLI's behaviour.
    Unverified(String),
}

/// How the canary gets a child to talk to.
///
/// The seam exists so the matcher can be tested without spawning an agent: every
/// test in this module injects a launcher that writes a canned capture, and none
/// of them runs `claude`. `run` returns `Err` ONLY when the child could not be
/// run to the point of producing a capture — a child that ran and said nothing
/// useful is `Ok`, because that is a fact about the CLI's behaviour and belongs
/// in the `Absent`/`Confirmed` decision rather than in the `Unverified` one.
pub trait CanaryLauncher {
    /// Run one throwaway agent turn against `prompt`, teeing its stdout to
    /// `capture`.
    fn run(&self, prompt: &str, capture: &Path) -> Result<(), String>;
}

/// Where one canary run's throwaway capture lands.
pub fn canary_capture_path(capture_dir: &Path) -> PathBuf {
    capture_dir.join(CAPTURE_FILE)
}

/// The throwaway prompt: dispatch one trivial background task, wait for its
/// completion notification, and only then report.
///
/// The `DEVFLOW_RESULT:` line and the bare token line are separate on purpose.
/// The marker line is what a pipe-owning supervisor's close rule watches for
/// (it must parse as the existing marker grammar, so nothing may be added
/// inside its JSON body); the bare token line is what
/// [`agent_result::token_reported_in_capture`] matches. Folding the token into
/// the marker's JSON would couple this prompt to `AgentResult`'s schema for no
/// gain.
pub fn canary_prompt(token: &str) -> String {
    format!(
        "DevFlow startup check of Claude Code's background-task notification path. \
         Do exactly the following and nothing else — do not read, create or modify any file, \
         and do not run any command.\n\
         \n\
         1. Dispatch ONE background task whose entire job is to reply with the word `ok`.\n\
         2. Wait for that task's completion notification to arrive. Do not finish before it does.\n\
         3. In the turn that follows that notification, end your message with these two lines, \
         each on its own line and exactly as written:\n\
         \n\
         {token}\n\
         DEVFLOW_RESULT: {{\"status\":\"success\"}}\n\
         \n\
         The first line is a single-use token supplied by DevFlow. Reproduce it character for \
         character; do not shorten, summarise, quote or comment on it."
    )
}

/// Run one delivery canary and report what it established.
///
/// Declares a fresh token, plants it in a throwaway prompt, runs `launcher`
/// against a capture inside `capture_dir`, and hands the resulting capture text
/// to [`agent_result::token_reported_in_capture`] — the one function in this
/// codebase that decides whether a token came back from somewhere trustworthy.
/// See this module's header for why that decision is not made here.
pub fn run_delivery_canary<L: CanaryLauncher>(launcher: &L, capture_dir: &Path) -> CanaryOutcome {
    let token = declare_token();
    let capture = canary_capture_path(capture_dir);

    // RED-phase stub: deliberately degenerate. The launcher's verdict is
    // discarded and the capture is never read, so every outcome reads `Absent`.
    // Replaced in this task's GREEN commit.
    let _ = launcher.run(&canary_prompt(&token), &capture);
    let _ = &agent_result::token_reported_in_capture;
    CanaryOutcome::Absent
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- fixtures --------------------------------------------------------
    //
    // Event shapes are taken from the real archived capture at
    // `.planning/phases/30-keep-the-session-alive-past-turn-end/30a-evidence/
    // raw_output_v3.jsonl` (lines 5, 19, 54) by way of 31-RESEARCH.md § "Code
    // Examples": a `system`/`init` line, a top-level `result` carrying the
    // agent's own final text, and the echoed `user` turn the CLI writes back
    // into the same stdout. Identifiers are generalized; shapes are not.

    const INIT_LINE: &str = r#"{"type":"system","subtype":"init","cwd":"/tmp/work","session_id":"s-1","claude_code_version":"2.1.220","uuid":"u-init"}"#;

    /// The CLI's echo of the operator's prompt, re-emitted as a `user` event.
    /// This is the shape that produced the checkpoint false positive 30-05
    /// fixed, and the reason the canary may never substring-scan the capture.
    fn echoed_prompt_line(prompt: &str) -> String {
        serde_json::json!({
            "type": "user",
            "message": { "role": "user", "content": prompt },
            "session_id": "s-1",
            "uuid": "u-echo",
        })
        .to_string()
    }

    /// A TOP-LEVEL `result` event — no `parent_tool_use_id`, so the
    /// orchestrator session authored it.
    fn top_level_result_line(text: &str) -> String {
        serde_json::json!({
            "type": "result",
            "subtype": "success",
            "is_error": false,
            "num_turns": 3,
            "stop_reason": "end_turn",
            "session_id": "s-1",
            "uuid": "u-res",
            "result": text,
        })
        .to_string()
    }

    /// A `result` event forwarded from a SUBAGENT — same type, non-null
    /// `parent_tool_use_id`, therefore not the orchestrator speaking.
    fn subagent_result_line(text: &str) -> String {
        serde_json::json!({
            "type": "result",
            "subtype": "success",
            "is_error": false,
            "session_id": "s-1",
            "uuid": "u-sub",
            "parent_tool_use_id": "toolu_01CanarySubagent",
            "result": text,
        })
        .to_string()
    }

    /// Recover the declared token from the prompt the canary handed the
    /// launcher — the same way the real agent gets it. Keeps the tests honest:
    /// the token is generated inside `run_delivery_canary`, so a test launcher
    /// that hard-coded one would be answering a question nobody asked.
    fn token_in(prompt: &str) -> String {
        let start = prompt
            .find(TOKEN_PREFIX)
            .expect("the canary prompt must carry the declared token");
        let rest = &prompt[start + TOKEN_PREFIX.len()..];
        let suffix: String = rest.chars().take_while(char::is_ascii_hexdigit).collect();
        assert!(
            !suffix.is_empty(),
            "the token in the prompt must have a body after its prefix"
        );
        format!("{TOKEN_PREFIX}{suffix}")
    }

    /// A launcher that writes whatever `lines` the test asked for, given the
    /// token it found in the prompt. Records the prompt it was handed and how
    /// many times it ran.
    struct CannedLauncher<F: Fn(&str) -> Vec<String>> {
        lines: F,
    }

    impl<F: Fn(&str) -> Vec<String>> CanaryLauncher for CannedLauncher<F> {
        fn run(&self, prompt: &str, capture: &Path) -> Result<(), String> {
            let token = token_in(prompt);
            let body = (self.lines)(&token).join("\n");
            std::fs::write(capture, format!("{body}\n")).map_err(|err| err.to_string())?;
            Ok(())
        }
    }

    /// A launcher that could not run at all — a missing binary, a permission
    /// error, a spawn failure. It writes no capture.
    struct FailingLauncher(&'static str);

    impl CanaryLauncher for FailingLauncher {
        fn run(&self, _prompt: &str, _capture: &Path) -> Result<(), String> {
            Err(self.0.to_string())
        }
    }

    /// The token came back inside a top-level `result` — the notification path
    /// is alive.
    #[test]
    fn canary_confirmed_when_token_returns_in_a_top_level_result() {
        let dir = tempfile::tempdir().unwrap();

        let launcher = CannedLauncher {
            lines: |token| {
                vec![
                    INIT_LINE.to_string(),
                    top_level_result_line(&format!(
                        "The background task finished.\n{token}\nDEVFLOW_RESULT: {{\"status\":\"success\"}}"
                    )),
                ]
            },
        };

        let outcome = run_delivery_canary(&launcher, dir.path());

        assert_eq!(
            outcome,
            CanaryOutcome::Confirmed,
            "a token inside a top-level result is the whole point of the guard"
        );
    }

    /// D-13 trap 1, and the single most important test in this module: the CLI
    /// echoes the prompt back, so the planted token appears in the capture
    /// whether or not anything was delivered. A canary that scanned the capture
    /// would certify delivery that never happened.
    #[test]
    fn canary_absent_when_token_appears_only_as_a_prompt_echo() {
        let dir = tempfile::tempdir().unwrap();

        let launcher = CannedLauncher {
            lines: |token| {
                vec![
                    INIT_LINE.to_string(),
                    // The echo carries the token verbatim …
                    echoed_prompt_line(&canary_prompt(token)),
                    // … while the agent's own final word does not.
                    top_level_result_line("I could not dispatch a background task."),
                ]
            },
        };

        let outcome = run_delivery_canary(&launcher, dir.path());

        // Negative control for the assertion below: if the capture did not
        // contain the token at all, `Absent` would be true for an entirely
        // uninteresting reason and this test would be measuring nothing.
        // Checked per LINE and by parsing, not by slicing the raw text —
        // `serde_json` writes object keys in sorted order, so `"type":"user"`
        // lands AFTER the message body that carries the token and a
        // position-based check reads backwards.
        let capture = std::fs::read_to_string(canary_capture_path(dir.path())).unwrap();
        let carrying: Vec<serde_json::Value> = capture
            .lines()
            .filter(|line| line.contains(TOKEN_PREFIX))
            .map(|line| serde_json::from_str(line).expect("fixture lines are JSON"))
            .collect();
        assert!(
            !carrying.is_empty(),
            "fixture must actually contain the echoed token"
        );
        assert!(
            carrying.iter().all(|event| event["type"] == "user"),
            "fixture must place the echoed token ONLY inside a `user` event — \
             if any result event carries it, this test is not exercising the echo case"
        );

        assert_eq!(
            outcome,
            CanaryOutcome::Absent,
            "an echoed token must never satisfy the guard (30-05's false positive)"
        );
    }

    /// Provenance, the second half of trap 1: a `result` forwarded from a
    /// subagent is the right event TYPE and the wrong AUTHOR.
    #[test]
    fn canary_absent_when_token_appears_only_in_a_non_top_level_event() {
        let dir = tempfile::tempdir().unwrap();

        let launcher = CannedLauncher {
            lines: |token| {
                vec![
                    INIT_LINE.to_string(),
                    subagent_result_line(&format!("child reporting: {token}")),
                    top_level_result_line("Done."),
                ]
            },
        };

        let outcome = run_delivery_canary(&launcher, dir.path());

        // Same negative control: prove the token is present before concluding
        // anything from its not being honoured.
        let capture = std::fs::read_to_string(canary_capture_path(dir.path())).unwrap();
        assert!(
            capture.contains(TOKEN_PREFIX),
            "fixture must actually contain the token inside the subagent result"
        );
        assert!(
            capture.contains("parent_tool_use_id"),
            "fixture must actually mark that result as subagent-authored"
        );

        assert_eq!(
            outcome,
            CanaryOutcome::Absent,
            "a subagent-authored result must not certify orchestrator-level delivery"
        );
    }

    /// "The CLI could not be run" is not "the CLI ran and the behaviour is
    /// gone". Collapsing the two would report a missing binary as a broken
    /// premise and send the operator after the wrong problem entirely.
    #[test]
    fn canary_unverified_when_the_launcher_fails() {
        let dir = tempfile::tempdir().unwrap();

        let outcome = run_delivery_canary(
            &FailingLauncher("could not run `claude`: No such file or directory (os error 2)"),
            dir.path(),
        );

        match outcome {
            CanaryOutcome::Unverified(reason) => {
                assert!(
                    reason.contains("No such file or directory"),
                    "the reason the guard could not run must survive into the outcome, \
                     got: {reason}"
                );
            }
            other => panic!("a launcher failure must be Unverified, not {other:?}"),
        }
    }

    /// A token reused across runs would let a stale capture satisfy a later
    /// guard.
    #[test]
    fn declared_tokens_differ_between_runs() {
        let first = declare_token();
        let second = declare_token();

        assert_ne!(
            first, second,
            "each canary run must declare its own token, or a stale capture could satisfy it"
        );
        assert!(
            first.starts_with(TOKEN_PREFIX) && second.starts_with(TOKEN_PREFIX),
            "both tokens must carry the greppable prefix"
        );
    }
}

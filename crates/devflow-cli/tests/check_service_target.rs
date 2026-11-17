//! Behavioural test of `scripts/check.sh service` (D-07b).
//!
//! The target runs the host-only user-unit tests. With no systemd user manager to
//! answer it must fail and say so, never skip: a skipped gate prints nothing a log
//! reader can tell apart from a passing one. The test removes the two variables
//! `systemctl --user` reaches the user bus through, from the script's own
//! environment only, which leaves it no manager to reach on a host that has one and
//! on the pinned container that has none.
//!
//! The passing direction is the host run of `scripts/check.sh service` itself, which
//! runs the `service_e2e` suite.

use std::path::PathBuf;
use std::process::Command;

/// The variables `systemctl --user` reaches the user manager through.
const USER_BUS_VARS: [&str; 2] = ["XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS"];

/// The text the target prints when no user manager answers.
const MISSING_MANAGER_TEXT: &str = "no systemd user manager answers";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root")
}

#[test]
fn check_sh_service_fails_naming_the_missing_user_manager() {
    let root = repo_root();
    let mut command = Command::new(root.join("scripts/check.sh"));
    command.arg("service").current_dir(&root);
    for name in USER_BUS_VARS {
        command.env_remove(name);
    }
    let out = command.output().expect("spawn scripts/check.sh service");
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "service target passed with no user manager reachable; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains(MISSING_MANAGER_TEXT),
        "service target did not name the missing manager; stderr:\n{stderr}"
    );
}

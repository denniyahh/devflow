//! Behavioural test of `scripts/check.sh service` (D-07b).
//!
//! The target runs the host-only user-unit tests. With no systemd user manager to
//! answer it must fail and say so, never skip: a skipped gate prints nothing a log
//! reader can tell apart from a passing one. The test removes the two variables
//! `systemctl --user` reaches the user bus through, from the script's own
//! environment only, which leaves it no manager to reach on a host that has one and
//! on the pinned container that has none.
//!
//! Exit 0 from `systemctl` is not an answer on its own: the pinned devcontainer image
//! ships a `systemctl` shim that prints a notice and exits 0, which the second test
//! reproduces with a fake on `PATH`.
//!
//! The passing direction is the host run of `scripts/check.sh service` itself, which
//! runs the `service_e2e` suite.

use std::path::PathBuf;
use std::process::{Command, Output};

use devflow_core::test_support::write_executable;

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

/// What the pinned devcontainer image's `/usr/local/bin/systemctl` does with no
/// systemd running: print a notice and exit 0.
const CONTAINER_SYSTEMCTL_SHIM: &str =
    "#!/bin/sh\necho '\"systemd\" is not running in this container due to its overhead.'\n";

fn check_sh_service() -> Command {
    let root = repo_root();
    let mut command = Command::new(root.join("scripts/check.sh"));
    command.arg("service").current_dir(&root);
    command
}

/// Asserts the target failed and named the missing user manager.
fn assert_names_the_missing_manager(out: &Output) {
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

#[test]
fn check_sh_service_fails_naming_the_missing_user_manager() {
    let mut command = check_sh_service();
    for name in USER_BUS_VARS {
        command.env_remove(name);
    }
    let out = command.output().expect("spawn scripts/check.sh service");
    assert_names_the_missing_manager(&out);
}

#[test]
fn check_sh_service_refuses_a_systemctl_that_exits_zero_without_a_version() {
    let scratch = tempfile::tempdir().expect("scratch dir");
    let fake_bin = scratch.path().join("bin");
    std::fs::create_dir(&fake_bin).expect("create the fake bin dir");
    write_executable(&fake_bin.join("systemctl"), CONTAINER_SYSTEMCTL_SHIM);
    let parent_path = std::env::var_os("PATH").unwrap_or_default();
    let path =
        std::env::join_paths(std::iter::once(fake_bin).chain(std::env::split_paths(&parent_path)))
            .expect("join PATH");

    let out = check_sh_service()
        .env("PATH", path)
        .output()
        .expect("spawn scripts/check.sh service");
    assert_names_the_missing_manager(&out);
}

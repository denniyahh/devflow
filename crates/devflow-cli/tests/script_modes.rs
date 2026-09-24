//! Git runs only executable hooks, and a `core.fileMode=false` checkout records
//! a new file as `100644`, so a hook committed that way silently never runs on a
//! fresh clone. `scripts/hooks/commit-msg` shipped like that from the day it was
//! added. This pins the committed mode of every hook and top-level script.
//! Sourced fragments under `scripts/lib/` are deliberately out of scope.

use std::path::Path;

#[test]
fn hooks_and_top_level_scripts_are_committed_executable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = devflow_core::test_support::git_command(&root)
        .args([
            "ls-files",
            "-s",
            "--",
            ":(glob)scripts/hooks/**",
            ":(glob)scripts/*.sh",
        ])
        .output()
        .expect("run git ls-files");
    assert!(
        output.status.success(),
        "git ls-files failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let listing = String::from_utf8(output.stdout).expect("utf-8 listing");
    let entries: Vec<&str> = listing.lines().collect();
    assert!(
        entries
            .iter()
            .any(|line| line.ends_with("\tscripts/hooks/pre-push")),
        "the listing must include the hooks, or this test checks nothing: {entries:#?}"
    );
    let not_executable: Vec<&str> = entries
        .iter()
        .filter(|line| !line.starts_with("100755 "))
        .copied()
        .collect();
    assert!(
        not_executable.is_empty(),
        "committed without the executable bit (fix: git update-index --chmod=+x <file>): {not_executable:#?}"
    );
}

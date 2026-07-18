//! Deterministic operator-documentation and runtime-path invariants.

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::{agent_result, events, gates::Gates, lock, ship, workflow};

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("devflow-core must live under <workspace>/crates")
            .to_path_buf()
    }

    fn glob_matches(pattern: &str, value: &str) -> bool {
        let mut rest = value;
        let mut parts = pattern.split('*').peekable();
        let anchored_start = !pattern.starts_with('*');
        let anchored_end = !pattern.ends_with('*');
        let mut first = true;

        while let Some(part) = parts.next() {
            if part.is_empty() {
                first = false;
                continue;
            }
            if first && anchored_start {
                let Some(next) = rest.strip_prefix(part) else {
                    return false;
                };
                rest = next;
            } else if let Some(index) = rest.find(part) {
                rest = &rest[index + part.len()..];
            } else {
                return false;
            }
            first = false;
        }

        !anchored_end || rest.is_empty()
    }

    fn pattern_covers(pattern: &str, relative_path: &str) -> bool {
        let pattern = pattern.trim_start_matches('/');
        if pattern.ends_with('/') {
            relative_path.starts_with(pattern)
        } else {
            glob_matches(pattern, relative_path)
        }
    }

    #[test]
    fn gitignore_covers_all_devflow_paths() {
        let root = Path::new("/representative-project");
        let paths = [
            ("events::events_path", events::events_path(root)),
            ("agent_result::stdout_path", agent_result::stdout_path(root, 16)),
            ("agent_result::stderr_path", agent_result::stderr_path(root, 16)),
            ("agent_result::exit_code_path", agent_result::exit_code_path(root, 16)),
            ("agent_result::agent_pid_path", agent_result::agent_pid_path(root, 16)),
            ("agent_result::history_dir", agent_result::history_dir(root, 16)),
            ("workflow::state_path", workflow::state_path(root, 16)),
            ("workflow::legacy_state_path", workflow::legacy_state_path(root)),
            ("Gates::dir", Gates::dir(root)),
            ("lock::lock_path", lock::lock_path(root, 16)),
            ("lock::project_lock_path", lock::project_lock_path(root)),
            ("ship::cron_instructions_path", ship::cron_instructions_path(root, 16)),
            (
                "ship::legacy_cron_instructions_path",
                ship::legacy_cron_instructions_path(root),
            ),
        ];
        let ignore = std::fs::read_to_string(workspace_root().join(".gitignore"))
            .expect("workspace .gitignore must be readable");
        let patterns: Vec<_> = ignore
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('!'))
            .collect();

        for (constructor, path) in paths {
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .trim_start_matches('/')
                .to_owned();
            assert!(
                patterns
                    .iter()
                    .any(|pattern| pattern_covers(pattern, &relative)),
                "{constructor} produced uncovered runtime path `{relative}`; add a matching .gitignore rule"
            );
        }
    }
}

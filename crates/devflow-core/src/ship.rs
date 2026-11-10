//! Ship bookkeeping.
//!
//! Holds the pure document-finalization transform (CHANGELOG) used on ship
//! completion.

/// Errors produced by ship bookkeeping.
#[derive(Debug, thiserror::Error)]
pub enum ShipError {
    /// Filesystem operation failed.
    #[error("ship I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// JSON parse or serialization failed.
    #[error("ship JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    /// No last-ship record exists.
    #[error("no last-ship record found — nothing to confirm or reject")]
    Missing,
}

/// Prepend a CHANGELOG entry for `version`, creating a standard header if the
/// file did not exist. Pure transform over the existing CHANGELOG contents.
///
/// `body` is the Keep-a-Changelog-grouped content
/// [`crate::version::render_changelog_body`] produces (D-12) — trimmed of
/// trailing newlines and re-terminated with a single `\n`. When `body.trim()`
/// is empty (no version-affecting content, or the caller couldn't compute
/// one), the fallback line `- No changes recorded since the previous
/// release.` is substituted instead, so an entry is never silently blank.
pub fn prepend_changelog(existing: &str, version: &str, date: &str, body: &str) -> String {
    const HEADER: &str = "# Changelog\n\n\
        All notable changes to this project are documented here.\n";
    const FALLBACK: &str = "- No changes recorded since the previous release.";
    let trimmed_body = body.trim_end_matches('\n');
    let body_content = if trimmed_body.trim().is_empty() {
        FALLBACK
    } else {
        trimmed_body
    };
    let entry = format!("## {version} — {date}\n\n{body_content}\n");

    if existing.trim().is_empty() {
        return format!("{HEADER}\n{entry}");
    }
    // Insert the new entry after the header block (first blank line after the
    // top-level title), or at the top if no header is recognized.
    if let Some(idx) = existing.find("\n\n") {
        let (head, tail) = existing.split_at(idx + 2);
        format!("{head}{entry}\n{tail}")
    } else {
        format!("{entry}\n{existing}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepend_changelog_creates_header_when_empty() {
        let out = prepend_changelog("", "0.5.2", "2026-06-18", "- some change\n");
        assert!(out.starts_with("# Changelog"));
        assert!(out.contains("## 0.5.2 — 2026-06-18"));
        assert!(out.contains("- some change"));
    }

    #[test]
    fn prepend_changelog_inserts_after_header() {
        let existing = "# Changelog\n\n## 0.5.1 — 2026-06-17\n\n- old\n";
        let out = prepend_changelog(existing, "0.5.2", "2026-06-18", "- new change\n");
        let new_idx = out.find("0.5.2").unwrap();
        let old_idx = out.find("0.5.1").unwrap();
        assert!(new_idx < old_idx, "new entry should come before old");
        assert!(out.starts_with("# Changelog"));
        assert!(out.contains("- new change"));
    }

    #[test]
    fn prepend_changelog_uses_the_generated_body() {
        let body = "### Added\n\n- add the widget endpoint\n";
        let out = prepend_changelog("", "1.2.0", "2026-07-29", body);
        assert!(out.contains("## 1.2.0 — 2026-07-29"));
        assert!(out.contains("### Added"));
        assert!(out.contains("- add the widget endpoint"));
        // Empty-body fallback (never a silently blank entry).
        let fallback = prepend_changelog("", "1.2.1", "2026-07-30", "");
        assert!(fallback.contains("- No changes recorded since the previous release."));
    }
}

//! The identifier of a phase: `35`, or `35.1`.
//!
//! DevFlow originally carried this as a bare `u32`. GSD's `--insert` mode
//! numbers an inserted phase with a decimal (`35.1`, `35.2`), so a `u32`
//! identifier made every such phase unreachable by `devflow start` — the
//! defect recorded as 999.97 and hotfixed on 2026-08-07.
//!
//! Two renderings exist, and the distinction matters:
//!
//! - [`Display`] is the canonical label — `7`, `35.1`. It is what goes into
//!   prompts and messages a human or a GSD skill reads.
//! - [`PhaseId::padded`] is the zero-padded *path* form — `07`, `35.1`. It is
//!   what names `.devflow/state-07.json`, `feature/phase-07`, and the
//!   `.planning/phases/07-*` glob.
//!
//! `Display` deliberately ignores width specifiers, so a stray `{phase:02}`
//! left over from the `u32` era cannot silently produce a wrong path — it
//! produces the unpadded label and any path built from it fails loudly. Call
//! [`PhaseId::padded`] where a path is meant.

use std::fmt;
use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A phase identifier — a major number, optionally with a minor number.
///
/// Ordering is `(major, minor)` with an absent minor sorting first, so
/// `35 < 35.1 < 35.2 < 36`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PhaseId {
    major: u32,
    minor: Option<u32>,
}

impl PhaseId {
    /// An integer-numbered phase, e.g. `35`.
    #[must_use]
    pub const fn new(major: u32) -> Self {
        Self { major, minor: None }
    }

    /// A decimal-numbered phase, e.g. `35.1`.
    #[must_use]
    pub const fn with_minor(major: u32, minor: u32) -> Self {
        Self {
            major,
            minor: Some(minor),
        }
    }

    /// The major number — `35` for both `35` and `35.1`.
    #[must_use]
    pub const fn major(self) -> u32 {
        self.major
    }

    /// The minor number, if this phase has one.
    #[must_use]
    pub const fn minor(self) -> Option<u32> {
        self.minor
    }

    /// Reads a `phase` field out of a persisted JSON record, in either shape
    /// — a bare number (written before the widening, and still what an
    /// integer phase writes) or a string.
    ///
    /// Returns `None` when the field is absent or is neither shape. Callers
    /// index rather than defaulting, so an absent field cannot read as a
    /// phase that happens to match.
    #[must_use]
    pub fn from_json(value: Option<&serde_json::Value>) -> Option<Self> {
        match value? {
            serde_json::Value::Number(number) => {
                u32::try_from(number.as_u64()?).ok().map(Self::new)
            }
            serde_json::Value::String(text) => text.parse().ok(),
            _ => None,
        }
    }

    /// Whether a persisted JSON `phase` field denotes *this* phase.
    ///
    /// The minor number is part of the identity: phase `35`'s records must
    /// not match phase `35.1`, which is the same cross-matching hazard the
    /// artifact glob has.
    #[must_use]
    pub fn matches_json(self, value: Option<&serde_json::Value>) -> bool {
        Self::from_json(value) == Some(self)
    }

    /// The zero-padded path form: `07`, `35.1`.
    ///
    /// Only the major number is padded. `35.1` is already unambiguous and is
    /// exactly what GSD writes on disk as `.planning/phases/35.1-*`.
    #[must_use]
    pub fn padded(self) -> String {
        match self.minor {
            Some(minor) => format!("{:02}.{minor}", self.major),
            None => format!("{:02}", self.major),
        }
    }
}

impl fmt::Display for PhaseId {
    /// Writes the canonical label, ignoring any width or fill specifier.
    ///
    /// See the module docs: silently honouring `{:02}` here would let a
    /// leftover padding specifier build a path that looks right for `35` and
    /// is wrong for `35.1`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.minor {
            Some(minor) => write!(f, "{}.{minor}", self.major),
            None => write!(f, "{}", self.major),
        }
    }
}

impl From<u32> for PhaseId {
    fn from(major: u32) -> Self {
        Self::new(major)
    }
}

/// Why a string is not a usable phase identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsePhaseIdError {
    input: String,
    reason: &'static str,
}

impl fmt::Display for ParsePhaseIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` is not a phase number ({}) — expected `35` or `35.1`",
            self.input, self.reason
        )
    }
}

impl std::error::Error for ParsePhaseIdError {}

/// Parses one dot-separated component.
///
/// Rejects anything `u32::from_str` would accept but a path or branch name
/// should not — notably a leading `+`, which `"+5".parse::<u32>()` accepts.
fn component(part: &str) -> Option<u32> {
    if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    part.parse::<u32>().ok()
}

impl FromStr for PhaseId {
    type Err = ParsePhaseIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let fail = |reason: &'static str| ParsePhaseIdError {
            input: s.to_string(),
            reason,
        };

        let mut parts = s.split('.');
        let major = component(parts.next().unwrap_or_default())
            .ok_or_else(|| fail("the part before the dot is not a number"))?;
        let minor = match parts.next() {
            Some(part) => Some(
                component(part).ok_or_else(|| fail("the part after the dot is not a number"))?,
            ),
            None => None,
        };
        if parts.next().is_some() {
            return Err(fail("more than one dot"));
        }

        Ok(Self { major, minor })
    }
}

impl Serialize for PhaseId {
    /// Serializes an integer phase as a JSON number and a decimal phase as a
    /// string.
    ///
    /// The number arm preserves the on-disk shape of every `state-NN.json`
    /// written before the widening, so an existing run is not disturbed by
    /// this change.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.minor {
            Some(_) => serializer.serialize_str(&self.to_string()),
            None => serializer.serialize_u32(self.major),
        }
    }
}

impl<'de> Deserialize<'de> for PhaseId {
    /// Accepts either shape: a bare number (state files written before the
    /// widening) or a string (`"35.1"`).
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PhaseIdVisitor;

        impl Visitor<'_> for PhaseIdVisitor {
            type Value = PhaseId;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a phase number such as 35 or \"35.1\"")
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<PhaseId, E> {
                u32::try_from(value)
                    .map(PhaseId::new)
                    .map_err(|_| E::custom(format!("phase number {value} is out of range")))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<PhaseId, E> {
                u32::try_from(value)
                    .map(PhaseId::new)
                    .map_err(|_| E::custom(format!("phase number {value} is out of range")))
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<PhaseId, E> {
                value.parse().map_err(E::custom)
            }
        }

        deserializer.deserialize_any(PhaseIdVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_an_integer_phase() {
        assert_eq!("35".parse::<PhaseId>().unwrap(), PhaseId::new(35));
    }

    #[test]
    fn parses_a_decimal_phase() {
        assert_eq!(
            "35.1".parse::<PhaseId>().unwrap(),
            PhaseId::with_minor(35, 1)
        );
    }

    /// The negative control for the widening: relaxing a `u32` parse is the
    /// kind of change that accepts everything if validation is forgotten, and
    /// this identifier reaches a filesystem path and a git branch name.
    #[test]
    fn rejects_what_is_not_a_phase_number() {
        for input in [
            "",
            ".",
            "35.",
            ".1",
            "35.1.2",
            "-1",
            "+5",
            "35a",
            "thirty-five",
            "35 1",
            "../../etc/passwd",
            "35/../36",
            "1e3",
            " 35",
            "35 ",
        ] {
            assert!(
                input.parse::<PhaseId>().is_err(),
                "`{input}` was accepted as a phase number"
            );
        }
    }

    #[test]
    fn display_is_the_unpadded_label() {
        assert_eq!(PhaseId::new(7).to_string(), "7");
        assert_eq!(PhaseId::with_minor(35, 1).to_string(), "35.1");
    }

    /// A leftover `{phase:02}` from the `u32` era must not silently produce a
    /// path-shaped string — see the module docs.
    #[test]
    fn display_ignores_width_specifiers() {
        assert_eq!(format!("{:02}", PhaseId::new(7)), "7");
    }

    #[test]
    fn padded_is_the_path_form() {
        assert_eq!(PhaseId::new(7).padded(), "07");
        assert_eq!(PhaseId::new(35).padded(), "35");
        assert_eq!(PhaseId::with_minor(35, 1).padded(), "35.1");
        assert_eq!(PhaseId::with_minor(7, 2).padded(), "07.2");
    }

    #[test]
    fn orders_a_decimal_phase_after_its_major() {
        let mut phases = vec![
            PhaseId::new(36),
            PhaseId::with_minor(35, 2),
            PhaseId::new(35),
            PhaseId::with_minor(35, 1),
        ];
        phases.sort();
        assert_eq!(
            phases,
            vec![
                PhaseId::new(35),
                PhaseId::with_minor(35, 1),
                PhaseId::with_minor(35, 2),
                PhaseId::new(36),
            ]
        );
    }

    #[test]
    fn an_integer_phase_still_serializes_as_a_number() {
        assert_eq!(serde_json::to_string(&PhaseId::new(35)).unwrap(), "35");
    }

    #[test]
    fn a_decimal_phase_serializes_as_a_string() {
        assert_eq!(
            serde_json::to_string(&PhaseId::with_minor(35, 1)).unwrap(),
            "\"35.1\""
        );
    }

    /// State files written before the widening hold a bare number.
    #[test]
    fn deserializes_both_persisted_shapes() {
        assert_eq!(
            serde_json::from_str::<PhaseId>("35").unwrap(),
            PhaseId::new(35)
        );
        assert_eq!(
            serde_json::from_str::<PhaseId>("\"35.1\"").unwrap(),
            PhaseId::with_minor(35, 1)
        );
    }

    #[test]
    fn reads_a_phase_field_in_either_shape() {
        assert_eq!(
            PhaseId::from_json(Some(&serde_json::json!(35))),
            Some(PhaseId::new(35))
        );
        assert_eq!(
            PhaseId::from_json(Some(&serde_json::json!("35.1"))),
            Some(PhaseId::with_minor(35, 1))
        );
    }

    /// An absent field must read as absent, never as a phase — the
    /// distinction the whole matcher exists to preserve.
    #[test]
    fn an_absent_or_malformed_phase_field_reads_as_none() {
        assert_eq!(PhaseId::from_json(None), None);
        assert_eq!(PhaseId::from_json(Some(&serde_json::json!(null))), None);
        assert_eq!(
            PhaseId::from_json(Some(&serde_json::json!("nonsense"))),
            None
        );
        assert_eq!(PhaseId::from_json(Some(&serde_json::json!(-1))), None);
    }

    /// The cross-matching hazard: a record belonging to phase 35 must not be
    /// read as belonging to phase 35.1, or either one's history is the
    /// other's.
    #[test]
    fn a_phase_does_not_match_its_decimal_sibling() {
        let integer = serde_json::json!(35);
        let decimal = serde_json::json!("35.1");

        assert!(PhaseId::new(35).matches_json(Some(&integer)));
        assert!(PhaseId::with_minor(35, 1).matches_json(Some(&decimal)));

        assert!(!PhaseId::new(35).matches_json(Some(&decimal)));
        assert!(!PhaseId::with_minor(35, 1).matches_json(Some(&integer)));
    }

    #[test]
    fn round_trips_through_json() {
        for phase in [
            PhaseId::new(7),
            PhaseId::new(35),
            PhaseId::with_minor(35, 1),
        ] {
            let json = serde_json::to_string(&phase).unwrap();
            assert_eq!(serde_json::from_str::<PhaseId>(&json).unwrap(), phase);
        }
    }
}

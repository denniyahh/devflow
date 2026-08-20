//! Public-API integration tests for `AgentKind::Antigravity` (phase 41).
//!
//! Exercises the enum, its Display/FromStr/serde round-trip, and the driver
//! dispatch through the PUBLIC `devflow_core::` surface — the same seam an
//! operator-facing command (`devflow start --agent antigravity`) crosses.

use devflow_core::agents::driver_for;
use devflow_core::state::AgentKind;

#[test]
fn antigravity_parses_from_the_operator_string() {
    assert_eq!(
        "antigravity".parse::<AgentKind>().unwrap(),
        AgentKind::Antigravity
    );
    // Case-insensitive, like every other agent.
    assert_eq!(
        "ANTIGRAVITY".parse::<AgentKind>().unwrap(),
        AgentKind::Antigravity
    );
}

#[test]
fn antigravity_displays_lowercase_for_the_cli() {
    assert_eq!(AgentKind::Antigravity.to_string(), "antigravity");
}

#[test]
fn antigravity_serde_round_trips_through_lowercase() {
    let wire = serde_json::to_string(&AgentKind::Antigravity).unwrap();
    assert_eq!(wire, "\"antigravity\"");
    let back: AgentKind = serde_json::from_str(&wire).unwrap();
    assert_eq!(back, AgentKind::Antigravity);
}

#[test]
fn antigravity_driver_for_returns_the_driver() {
    let driver = driver_for(AgentKind::Antigravity);
    assert_eq!(driver.name(), "Antigravity");
    let (program, args) = driver.build_command(devflow_core::phase_id::PhaseId::new(1), "", &[]);
    assert_eq!(program, "agy");
    assert_eq!(
        args,
        vec![
            "--input-format".to_string(),
            "stream-json".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--print-timeout".to_string(),
            "60m".to_string(),
        ]
    );
}

#[test]
fn antigravity_parse_error_names_it() {
    let err = "bogus-agent".parse::<AgentKind>().unwrap_err();
    assert!(err.to_string().contains("antigravity"), "{err}");
}

//! Frozen Murail gate consumer at 32e218d: same decision body and original assertion.
//! Visibility is widened for this test oracle; project publication/bead resolution is not simulated.
#![expect(
    dead_code,
    reason = "the frozen native result types include publication variants unused by the admission function"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Landing {
    bookmark: String,
    baseline: String,
    candidate: String,
    tree_id: String,
    remote_tip: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Refusal(pub String);

pub enum LandingOutcome {
    Admitted(Landing),
    Published(Landing),
    Refused(Refusal),
    CouldNotDetermine(String),
}

pub fn admit_delta(
    report: &serde_json::Value,
    baseline: &str,
    candidate: &str,
    purchase: bool,
) -> Result<(), LandingOutcome> {
    if report["verb"] != "delta"
        || report["basis"]["baseline"] != baseline
        || report["basis"]["candidate"] != candidate
    {
        return Err(LandingOutcome::CouldNotDetermine(
            "architecture delta does not name the frozen baseline and candidate".into(),
        ));
    }
    let raises = report["policy"]["raises"].as_array().ok_or_else(|| {
        LandingOutcome::CouldNotDetermine(
            "architecture delta has no ceiling-change projection".into(),
        )
    })?;
    match report["verdict"].as_str() {
        Some("REFUSE") => {
            return Err(LandingOutcome::Refused(Refusal(format!(
                "architecture delta refused: {}",
                report["violations"]
            ))));
        }
        Some("PASS") => {}
        _ => {
            return Err(LandingOutcome::CouldNotDetermine(
                "architecture delta has no recognized verdict".into(),
            ));
        }
    }
    if !raises.is_empty() && !purchase {
        return Err(LandingOutcome::Refused(Refusal("ceiling raised without a resolving Purchase: <bead-id> trailer; name the purchase before publication".into())));
    }
    Ok(())
}

#[test]
fn a_pinned_delta_requires_a_resolved_purchase_for_any_raise() {
    let mut report = serde_json::json!({
        "verb":"delta", "basis":{"baseline":"base","candidate":"candidate"},
        "policy":{"raises":[]}, "verdict":"PASS", "violations":[]
    });
    assert!(admit_delta(&report, "base", "candidate", false).is_ok());
    report["policy"]["raises"] = serde_json::json!([{"subject":"surface.core"}]);
    assert!(matches!(
        admit_delta(&report, "base", "candidate", false),
        Err(LandingOutcome::Refused(_))
    ));
    assert!(admit_delta(&report, "base", "candidate", true).is_ok());
    report["verdict"] = serde_json::json!("REFUSE");
    assert!(matches!(
        admit_delta(&report, "base", "candidate", true),
        Err(LandingOutcome::Refused(_))
    ));
    report["verdict"] = serde_json::json!("PASS");
    assert!(matches!(
        admit_delta(&report, "base", "other-candidate", true),
        Err(LandingOutcome::CouldNotDetermine(_))
    ));
    report["policy"]["raises"] = serde_json::Value::Null;
    assert!(matches!(
        admit_delta(&report, "base", "candidate", true),
        Err(LandingOutcome::CouldNotDetermine(_))
    ));
}

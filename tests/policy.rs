//! Authored policy -> snapshot -> observations -> validation -> process exit.
mod support;
use std::process::Command;
use support::Fixture;
const POLICY: &str = "schema=1\nprovenance='authored'\nsurface={fixture=10}\nauthority=[{fact='one home',home='src/lib.rs::Home'}]\ntyped_errors=['fixture']\n[population]\ncore=['fixture']\n[size]\ncore_token_lines=100\ncrate={fixture=100}\n[crate_graph]\nrule=[{name='fixture',allowed=[],law='leaf'}]\n[concentration]\npopulation=['fixture']\nentry_lines=300\nfile_lines=2000\nconventional_names=[]\n";
#[test]
fn policy_command_distinguishes_pass_refusal_and_unavailable() {
    let fixture = Fixture::new(&[
        ("src/lib.rs", "pub struct Home;\n"),
        ("policy.toml", POLICY),
    ]);
    for (text, status, success) in [
        (POLICY.to_owned(), "pass", true),
        (
            POLICY.replace("core_token_lines=100", "core_token_lines=101"),
            "refuse",
            false,
        ),
        (
            POLICY.to_owned() + "amended_by={bead='project-ruling',covers=['entry_lines']}\n",
            "unavailable",
            false,
        ),
    ] {
        fixture.write("policy.toml", &text);
        let output = Command::new(env!("CARGO_BIN_EXE_atlas"))
            .current_dir("/")
            .args(["policy", "--root"])
            .arg(&fixture.0)
            .args(["--policy", "policy.toml", "--json"])
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            success,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["question"], "policy");
        assert_eq!(report["judgment"], "policy-inputs");
        assert_eq!(report["status"], status);
        assert_eq!(report["schema"], 1);
        assert!(
            report["basis"]["sources"]
                .as_array()
                .is_some_and(|sources| !sources.is_empty())
        );
    }
}

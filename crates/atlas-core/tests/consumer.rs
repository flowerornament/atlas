//! The native library works without the application, CLI trait feature or a renderer.
mod support;
use std::{fs, process::Command};
use support::Fixture;

#[test]
fn independent_default_consumer_reads_source_and_neutral_control() {
    let fixture = Fixture::new(&[]);
    let core = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    fs::create_dir(fixture.path().join("src")).expect("consumer source directory");
    fs::write(fixture.path().join("Cargo.toml"), format!("[package]\nname='native-core-consumer'\nversion='0.0.0'\nedition='2024'\n[workspace]\n[dependencies]\natlas-core={{path={:?},default-features=false}}\n", core.to_str().expect("core path"))).expect("consumer manifest");
    fs::write(
        fixture.path().join("src/lib.rs"),
        "pub struct Graph;\npub fn build() -> Graph { Graph }\n",
    )
    .expect("inspected source");
    fs::write(
        fixture.path().join("src/main.rs"),
        r#"
fn main() -> atlas_core::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let captured = atlas_core::discovery::snapshot_workspace(root)?;
    assert!(captured.graph().files.iter().any(|file| file.path == "src/lib.rs"));
    let analysis = atlas_core::analysis::Analysis::new(&captured)?;
    let facts = analysis.representations()?;
    assert!(facts.types.iter().any(|ty| ty.name == "Graph"));
    let (workflow, _) = analysis.expanded("src/lib.rs", "build", 0)?;
    let diagram = workflow.diagram();
    diagram.validate()?;
    assert_eq!(diagram.level, atlas_core::diagram::Level::Declarations);
    assert!(workflow.ascii().contains("build"));
    Ok(())
}
"#,
    )
    .expect("consumer program");
    let target = core.join("../../target/core-consumer");
    let run = Command::new("cargo")
        .current_dir(fixture.path())
        .env("CARGO_TARGET_DIR", &target)
        .args(["run", "--offline", "--quiet"])
        .output()
        .expect("build native consumer");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let tree = Command::new("cargo")
        .current_dir(fixture.path())
        .args(["tree", "--offline", "--edges", "normal", "--prefix", "none"])
        .output()
        .expect("consumer dependencies");
    assert!(
        tree.status.success(),
        "{}",
        String::from_utf8_lossy(&tree.stderr)
    );
    for line in String::from_utf8(tree.stdout)
        .expect("Cargo tree text")
        .lines()
    {
        assert!(
            !matches!(
                line.split_whitespace().next(),
                Some("clap" | "atlas" | "murail-atlas")
            ),
            "unexpected application dependency: {line}"
        );
    }
}

//! A project wrapper, outside atlas-lib, builds the boundary declarations
//! the seams reading needs.
use atlas_lib::checks::stages::seams::BoundaryDeclaration;

#[test]
fn a_wrapper_builds_a_boundary_declaration() {
    let declared = BoundaryDeclaration::new("src/front/mod.rs", "fingerprint")
        .with_listed(["Presentation", "Certified"])
        .with_listed_modules(["check"])
        .with_citation_problems(["`spec/core.md` §Definition is not in the spec"]);
    assert_eq!(declared.file, "src/front/mod.rs");
    assert_eq!(declared.content_fingerprint, "fingerprint");
    assert_eq!(declared.listed.len(), 2);
    assert!(declared.listed_modules.contains("check"));
    assert_eq!(declared.citation_problems.len(), 1);
}

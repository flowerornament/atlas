# Atlas

A read-only Rust source intelligence library and CLI. One source model supports navigation queries, workflow diagrams, declared architecture and lexical type views. Source observations do not prove runtime semantics or ownership.

## Work

Read PROVENANCE.md before changing extracted code and `peat <path|topic>` before changing existing work. One writer per desk; create isolated desks with `~/.nix-config/scripts/desk spawn <purpose>`. Run checks before publishing. Never change an inspected project's source as a side effect of scanning or rendering it.

## Build

Rust edition 2024 on Rust 1.99.1, with rustfmt and the shared workspace lint table aligned with Murail (default and pedantic Clippy denied, plus its safety restrictions). Both packages inherit the workspace lints. Keep the pin, manifests and CI consistent; do not retain an older consumer compiler as an implicit compatibility requirement. Use `cargo add` for dependencies and preserve Cargo.lock. `just check` runs the local gates. `just check-install` verifies an installed binary outside its checkout. Pele needs Node >=20; its pinned licensed bundle ships in the binary. npm is only needed to verify/update the vendor source, not to use Atlas.

## Model

Snapshot owns physical source bytes and lexical facts. SourceGraph records package-file contexts, declaration sites, evidence-labelled relationships and diagnostics; physical reports deduplicate paths. Preserve ambiguity and unresolved names; never select an arbitrary candidate. Account for disconnected files. Document scanner omissions and syntax assumptions.

Workflow anchors select existing function bodies, never restate their calls. Source order and control syntax are observations, not a resolved call graph or execution proof. Preserve branches, loops, fallibility and opaque unsupported forms. Renderers consume a projection and do not inspect source.

Declared dependencies, lexical type mentions and source workflow order have distinct evidence. Abstraction must retain its source provenance and account for summarized observations. Do not add a policy engine or semantic inference merely to improve a diagram.

Architecture checks are a separate first-class capability. Core owns shared
source measurements and fixed rule mechanisms; project wrappers own authored
legislation and local authority resolution. Report-only observations, violations
and unavailable required evidence are distinct. A partial reading never claims
the project's complete PASS.

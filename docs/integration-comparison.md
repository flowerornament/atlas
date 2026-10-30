# Consolidation candidates across projects

The concrete proposed migration and full consumer map are now specified in
[One Atlas](design/consolidation.md) and its
[pinned inventory](design/consolidation-inventory.md), under girard-xqe.

This earlier comparison is a source inventory, not a migration decision. Murail
and Anneal keep their own ownership and qualification gates. No source in either project was changed.

The standalone Atlas already derives its lexical discovery lineage from Murail
(PROVENANCE.md). That shared mechanism is a plausible consolidation boundary.
Similar command names alone do not establish interchangeable behavior.

- **Murail** (`ed1edc4825e1a419d67dfc344d9383926d1f6add`,
  `crates/murail-atlas/src/lib.rs`, `atlas.rs`, `Cargo.toml`): the native crate offers
  source queries and landing-quality views, integrates Murail's FM grammar/render
  crates and carries authored project policy. Reusing standalone discovery, source
  facts and render primitives could remove duplicate mechanics. It does not follow
  that a configuration file can replace its FM/policy consumers or their tests.
- **Anneal** (`2b8e3ba4b9bb362520139ecdaf4184aba13cedd4`,
  `xtask/src/atlas.rs`): census, name/module cards and lexical relationship dump
  overlap directly. Its comment-policy audit, concentration measures and module
  shape diff have additional command/meaning contracts. Standalone contract/source
  comparison is not automatically equivalent to that LOC/block/item shape diff.
- **Standalone Atlas**: qualified calls/signature/field observations, declaration
  adaptation, representation/producer pictures, source-contract comparisons and
  embedded Pele are already separate from the inspected project. Its legacy lexical
  navigation remains labelled approximate. It has no FM/comment policy engine.

A migration should begin with consumer and command-contract inventories, then run
both scanners over the same pinned source and compare file membership, declarations,
test boundaries, imports, ambiguous/unresolved outcomes and exact source witnesses.
Project-policy audits need independent boundary tests. Rendering similarity and
line-count reduction are insufficient evidence of replacement.

Recommendation: evaluate reuse of the source-discovery/fact library first; retain
project-specific policy as the project's concern. Let each owning seat decide
whether a thin adapter, library reuse or a full CLI replacement earns its place.
No new cross-project framework or automatic migration is authorized by this note.

## Read-only Anneal trial, 2026-10-27

Both tools ran over an immutable archive of Anneal
`2b8e3ba4b9bb362520139ecdaf4184aba13cedd4`. Native command:
`cargo run --locked --quiet -p xtask -- atlas dump --json`; standalone command:
`atlas dump ARCHIVE --json`. No Anneal files or policy were changed.

Native dump emits 3,769 items and 3,789 edges. Standalone discovery finds 133
physical Rust files and 3,667 lexical relationships. These are different
populations, not interchangeable totals. After normalizing declaration-kind labels
and excluding synthetic re-exports and impl blocks from the declaration comparison:

- **3,382 sites agree** on physical path, line, name and kind.
- **54 native-only sites** are all present in standalone discovery but marked
  test-only there. Source samples include directly cfg(test)-gated helper functions,
  constants and module declarations. This is a boundary difference, not evidence
  that standalone failed to find them.
- **18 standalone-only sites** belong to Cargo's explicit
  `scripts/measure_code_population.rs` target outside `src/`, omitted by native
  source walking. Its observed path contains parent components, a source-identity
  normalization issue to resolve before migration.
- **Resolution differs.** Native edges: 3,657 unique, 124 ambiguous, 8 unresolved.
  Standalone: 3,290 unique, 117 ambiguous, 260 unresolved. Different populations,
  re-export expansion and external-name treatment mean these are not accuracy
  scores or a qualified-call comparison. Exact edge equivalence is not established.

The trial supports evaluating shared discovery as a library boundary. It does not
qualify replacing Anneal's atlas. Re-export cards, test inclusion, public-only
selection, external-name treatment and source identity need explicit consumer
decisions. Comment audits and concentration/shape metrics remain Anneal policies;
standalone does not replace them. Raw dumps and normalized site differences are
generated evidence under ignored `target/atlas/`, rather than maintained snapshots.

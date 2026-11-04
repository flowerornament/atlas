# Read qualified analysis and move review from their entries

Technical plan for the implementer and reviewer; registered before code on
2026-11-03, against c72a9a08. Work state: girard-xqe.13.

## Model / Replaces / Adds

Atlas captures source, derives conservative observations and reports them.
ARCHITECTURE.md's ownership and visible-workflow principles govern this change.
The public core 0.3 interfaces, CLI, evidence meanings and native policies stay
unchanged. This is an internal ownership and reading repair, not a new analysis.

Replace the mixed responsibilities in index/mod.rs and review/moves/mod.rs with
modules named for their decisions. Move the shared syntax cache up to analysis,
its actual owner. Separate workflow function discovery from expression reading.
Keep the stage/seam layout: roots, paths, interfaces, boundary, writers, view and
ratchets already express different decisions. Splitting those again adds no value.

## Census and final homes

| Responsibility | Current home | Final home | Consumers / contract |
|---|---|---|---|
| Captured bytes and lazy AST/workflow cache | index/mod.rs | analysis/sources.rs | Analysis, index, representation extraction; invocation-local, selected workflow reset |
| Cargo target/module contexts | index constructor | index/mounts.rs | Index construction; all physical/package/target contexts retained, ambiguity refuses |
| Index construction | one long constructor | index/build.rs | Index entry; mount, boundary, function/export, names, method-owner phases in order |
| Call/name resolution | index/mod.rs | index/resolve.rs | workflow projection, representation extraction, application program view; unresolved/ambiguous/invalid caller stay distinct |
| Index public types and entries | index/mod.rs | index/mod.rs | public source_index compatibility reexport, borrowing getters and entry signatures unchanged |
| Move population and native keys | moves/mod.rs | moves/inventory.rs | visibility and moved; match whole crates before narrowing report scope |
| Body/visibility pairing | moves/mod.rs | moves/compare.rs | entries; exact then rerooted then remaining, repeated keys approximate |
| Reports and entry workflow | moves/mod.rs | moves/mod.rs | native wrappers and CLI, existing JSON shape/order and limits |
| Workflow function discovery/selection | workflow/parse.rs | workflow/functions.rs | parse entry; module/impl/import/test evidence unchanged |
| Expression/statement order | workflow/parse.rs | workflow/parse.rs | workflow Step tree; one traversal, opaque unsupported syntax |

Internal analysis imports follow these physical owners. External compatibility
paths remain reexports; there is no adapter layer or second implementation.
Tests follow the decision they exercise, keeping their assertions and fixtures.
The source cache no longer lives under the index just because it was first used
there. Correlated Index tables remain private; construction and resolution are
inherent implementation modules, not new public mutable state.

## Requirements and verification

- R1 must: a reader starting at Index::from_selected_sources can follow named
  phases in order, and find mount policy, construction and resolution independently.
  Verify by source cold read and existing context/flow/representation tests.
- R2 must: moved and visibility show selection, inventory, pairing and reporting
  in order. Their current population, pairing order and diagnostics are preserved.
  Verify existing move tests and fixed-source before/after report comparisons.
- R3 must: workflow's expression traversal remains one mechanism. Discovery and
  selection expose their qualified-name rule separately. Existing workflow/control
  tests and fixed CLI/ASCII/Pele comparisons must remain unchanged.
- R4 must: old public paths/signatures compile and all wire outputs on fixed
  inspected inputs are identical. Do not compare scans of changing Atlas source
  as if source coordinates or fingerprints should remain stable. Use the retained
  c72 source, fixed Girard/Anneal materializations and controlled fixtures.
- R5 must: run just check (including core without default features and rustdoc),
  installed-binary and renderer-vendor checks; run the exact candidate on gates.
  Review broadly for reuse/quality/efficiency, then resolve findings before publish.

No new dependencies, parser/resolver semantics, feature flags or consumer pin
changes. No generic builder framework or splitting per expression variant. Public
schema and package versions need no change for private organization. Size is
reported as an observation, not a target. Retain accepted previous receipts; new
scratch and comparison products go under ignored target/atlas/readability/.

# Atlas responsibility-based layout — 2026-10-31

Outcome `girard-xqe.8`, implementing [the readability plan](readability.md).
Baseline `4104504e4abbbca3a046ca82ee99cc544a30cebf`; the executable code at that
revision is the preceding alias/control increment. This outcome changes physical
organization, private command composition, documentation and manual formatting.
It adds no source-analysis feature or dependency.

## What replaced what

- Core `source.rs`, `scan.rs`, `discovery{,/mounted}.rs`, `workspace.rs`,
  `population.rs`, `model.rs`, `relationships.rs` and `source_text.rs` now live
  under `source/`; the span-text helper is `source/text.rs`.
- Core `analysis.rs`, `source_index.rs`, `representations{,/**}.rs` and
  `workflow{,/**}.rs` now live under `analysis/`. The index's physical home is
  `analysis/index/mod.rs`. Selection-dependent AST/workflow caches retain their
  Analysis-local ownership; shared `Source` remains captured lexical evidence.
- Core `queries{,/**}.rs`, `review{,/**}.rs` and `documentation.rs` now live under
  `queries/`. Navigation and review are distinct families with their earlier
  population/meaning contracts.
- Core components with children use `component/mod.rs`: diagram, revision,
  concentration, delta, policy, stages and seams. Public module paths remain the
  same. Stage root inventory/binding lookup, textual path grammar and interface
  rules now occupy `roots.rs`, `paths.rs` and `interfaces.rs`; original inline
  stage examples move intact to `stages/tests.rs`. Private field/helper visibility
  is adjusted only within the same stage component so those files can cooperate.
- Application command files move to `cli/`, dropping the `_command` suffix.
  The singular names become `dependencies.rs`, `representations.rs` and
  `stages.rs`. Delta retains its presentation child. Map, flow, program and
  source-review operations move out of main beside their argument definitions.
- `cli/args.rs` owns parser grammar/root selection and the original parser tests;
  `cli/mod.rs` owns one total operational dispatch. Navigation owns the named
  legacy-versus-selected-population route. The former second guarded fallback
  dispatch and its long unreachable-command list are removed.
- Application `output.rs`, `artifact_output.rs`, `output_files.rs` move together
  into `cli/output/{mod,artifact,files}.rs`. Shared collision/publication mechanics
  remain shared; distinct artifact versus diagram output defaults are retained.
  The optional explicit profile reader has one private `cli/profile.rs` home.
- Selected application comparison, dependencies, overview, program and
  representations move under `views/`. Native representation/comparison/overview
  presentations move under `render/`. Existing public functions/methods delegate
  or reexport the same implementation; renderers do not open source.
- The one-line application architecture, diagram, discovery, model, queries,
  relationships, revision and workflow facade files become explicit root core
  reexports. The private source-index facade is also a direct reexport. These
  preserve import paths and type identity without duplicate definitions.

Core remains independent of mandatory Clap, Node, renderer and inspected-project
libraries. The application groups existing public models behind its unchanged
facade and keeps command/write options private. Embedded renderer asset paths,
protocols, wire formats and crate versions are unchanged.

## Clarity and review

The shipped [architecture guide](../architecture.md) explains each owner and two
execution stories, including publication/failure and unavailable evidence. Module
headers and the self-inspection query follow the new physical homes. Provenance
retains donor attribution and distinguishes historical extraction paths.

Manual formatting expands delta size/coupling/surface/seam wire projections,
policy/stage/review/overview report envelopes, and workflow/comparison/overview
construction. In particular, unavailable coupling has a named match arm with
explicit null values. No schema field or expression was replaced to format it.

Three read-only review lenses covered reuse, quality and efficiency over the
broad move and neighboring consumers. They found no new duplicate capture/parse
or presentation mechanism. The reuse review found remaining dense maps, which
were expanded. The cold reader found that the first guide incorrectly described
qualified Analysis in the standalone stage/seam path: the code uses Snapshot,
native Workspace/root bindings, and no standalone project-boundary interpreter.
The guide now states that actual path.

From code and shipped docs, the reader located the lexical/import grammar,
Analysis-local selected caches, overview adapter/projection/render/publication
sequence, and native stage/seam selection and unavailable outcomes. Its account
was checked against those files; this supports entry navigation, not a human
readability verdict or proof of every parser branch.

This outcome responds to review findings 5 and 6. It preserves the resolved
finding-1 source/output controls. Index mutation, RootTable traversal state,
Debug-derived tags and fixture lifecycle are unchanged here and retain their
separately registered repair outcome; renderer bounds are also a distinct
behavioral repair. No finding is declared fixed merely by moving its file.

## Verification

- Workspace fmt, warnings-denied Clippy, locked tests, feature-disabled core
  Clippy/tests, warnings-denied rustdoc and renderer tests passed (`just check`).
- Installed executables were exercised outside the checkout (`just check-install`).
  Independent public clients/adapters and source/output-collision controls remain
  part of these checks. Vendor bundle/license verification also passed.
- A fixed Cargo fixture compares old and new executables across 58 cases:
  main/subcommand help; lexical and selected navigation; source review; policy,
  stages/seams; contracts/dependencies/comparison; default, compact and expanded
  flow; program success and refusal; unknown-name/body refusals; two rendered SVGs.
  Exit codes, stdout, stderr and SVG bytes are identical: zero differences.
  There are 53 successful invocations and five preserved refusal cases.
- Probe setup initially differed because the old binary had a different argv[0]
  and artifact files persisted across runs. Both instruments now use argv[0]
  `atlas` and fresh owned output directories; no production code was changed to
  erase those setup differences. No output normalization or field exclusion is
  used in the final comparison.
- The existing self-inspection representation query succeeds with the updated
  physical selectors. Atlas's own source paths/counts necessarily change under
  organization; fixed consumer-source equality is the preservation instrument.
- As-written Rust source, including comments/inline tests: 27,770 to 28,009 lines,
  105 to 108 source files. Core is 21,129 to 21,169; application 6,641 to 6,840.
  Native non-test-module physical lines are 23,593 to 23,833, using Atlas's
  `nontest-loc` report (blank/comments retained, test-only files zeroed). These
  are distinct instruments, not neutral LOC or quality scores. Growth is chiefly
  explicit formatting, headers and command ownership, not a new mechanism.

Evidence, exact executable/transcript inputs, source move map and gate logs are
kept under the ignored `target/atlas/readable-layout/` qualification directory.
No timing run, native project cutover, consumer pin advance or new formal/diagram
feature was performed as part of this outcome.

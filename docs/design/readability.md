# Atlas: readable implementation and review repairs

Technical plan, 2026-10-30. Readers: implementer, reviewer and maintainer.
Baseline: `64ecd2b4162c047496fcf0aa27676c83575c6aec`.
Work and ordering live under `girard-xqe`; this document specifies the target.
The user authorized organization and complete review response before continuing
features, with the Murail cutover proposed next.

## Model / Replaces / Adds

Atlas is a read-only Rust source-analysis library and CLI. Captured source supports
navigation, review, architecture checks and diagrams. A source observation,
a declared rule and an authoritative project judgment have different meanings.

Replace scattered ownership, exposed correlated index state, hidden traversal
state, string-driven declaration decisions and duplicated fixture lifecycle.
Organize existing responsibilities; add no analyzer, command family, plugin
framework, modeling environment or new diagram capability in this work.

## Goals and constraints

- **R1, must:** a directory and module header identify the responsibility, its
  important inputs/results, invariant owner and evidence limits. Public APIs
  explain obligations that callers cannot infer from their types.
- **R2, must:** a reader can follow a command from parsing through capture,
  analysis/query/check, projection, rendering, publication and failure. Coordinating
  functions retain that sequence through named steps and intermediate values.
- **R3, must:** types and visibility protect correlated state. Unknown/ambiguous/
  unavailable observations remain explicit; no silent fallback chooses a source
  target or treats missing evidence as PASS.
- **R4, must:** preserve CLI arguments/defaults, legacy/native wire formats,
  evidence meanings and existing public module/type paths. An intentional Rust
  field/method API change is documented, with enumerated callers migrated together.
- **R5, must:** every finding in the independent seven-item review has a checked
  disposition, and the final accepted revision receives a whole-system read.
- **R6, must:** the renderer has bounded I/O and reliable child/failure ownership,
  while source/query protection remains shared across output routes.

No LOC reduction target, arbitrary file-size split, universal error/type rewrite,
public helper added to shorten main, or abstraction for a variation the design
does not use. Retain Rust 1.99.1, workspace lint policy, donor attribution and the
pinned third-party renderer. New dependencies require a concrete need.

## Responsibility map

Folders group decisions that belong together, rather than turn every processing
step into a new interface. The existing consolidation design cites Parnas's
information-hiding criterion; the research-graph consultation supports keeping
representation and its operations together. It is guidance, not compatibility
or readability evidence. The reader checks below decide whether the map works.

```text
crates/atlas-core/src/
  lib.rs                   public library surface and compatibility reexports
  source/                  bytes, capture, Cargo/mount membership, lexical facts
    mod.rs                 Source and its lexical projection
    discovery/             Snapshot construction and mounted source population
    scan.rs                canonical lexical/import/item grammar
    model.rs               SourceGraph, declarations and lexical evidence
    relationships.rs       lexical relationship derivation
    population.rs          explicit file/item/test selection
    workspace.rs           Cargo-context mechanics
    text.rs                exact captured source-span text
  analysis/
    mod.rs                 Analysis and its selection-dependent syntax/workflow caches
    index/                 qualified names, imports, indexed functions and lookup
    representations/       written types, aliases, signatures and construction
    workflow/              source-control facts and bounded projections
  queries/
    mod.rs                 lexical queries, distinct from selected navigation
    navigation/            native population-aware navigation
    review/                outlines, counts, shape/move/visibility observations
    documentation.rs       comment/citation audits from supplied policy
  checks/
    mod.rs                 structured applicability/findings, shared selections
    policy/                policy input, validation and observations
    stages/                textual paths, root reading and interface checks
      mod.rs               report composition and public stage contract
      roots.rs             immutable RootTable and per-reading traversal context
      paths.rs             existing textual path/import reading
      interfaces.rs        stage/interface requirements
      seams/               boundary, writer, view and ratchet responsibilities
    concentration/         existing concentration policy checks
    delta/                 baseline/candidate judgments
    typed_errors.rs        existing typed-error boundary checks
  diagram/                 renderer-neutral model and quotient projection
  revision/                historical capture, source basis and baseline custody
  architecture.rs          authored boundary declarations

src/
  main.rs                  executable entry and error-to-exit handling
  lib.rs                   application API; direct reexports of core-owned APIs
  cli/
    mod.rs                 one visible dispatch and project-root resolution
    navigation.rs          options plus explicit legacy/native routing
    review.rs              source-review options and report orchestration
    documentation.rs       comment-audit command
    policy.rs              policy command
    stages.rs              stages/seams commands
    delta/                 delta command and native wire presentation
    flow.rs                selected source body, expansion and control detail
    map.rs                 declared/lexical map command
    program.rs             declared program command
    representations.rs     saved representation question command
    dependencies.rs        qualified relationship command
    overview.rs            declaration-adapter overview command
    comparison.rs          source revision comparison command
    output/                destinations, shared validation and artifact writes
      mod.rs               output/evidence route policy
      artifact.rs          native artifact route; distinct defaults retained
      files.rs             common collision checks and filesystem publication
  views/                   application view models and selection/projection
    representations/       selected contracts, fields, producers, preparation
    dependencies.rs        selected qualified file relationships
    overview.rs            source-declared selection/composition
    comparison.rs          stable before/after observation rows
    program/               existing declared call-boundary projection
  render/                  ASCII/Pele and native view-family presentation
    representations/       existing native representation presentations
    comparison.rs          comparison presentation
    overview.rs            overview presentation
  adapter/                 declaration protocol, snapshots and process transport
  bin/atlas-girard.rs       Girard declaration convention

tests/support/             application fixture lifecycle; local examples stay local
crates/atlas-core/tests/support/  core fixture lifecycle; no new fixture crate
```

This names final homes, not mandatory new wrappers. Keep a compact responsibility
in one file until it has a real seam. Use private implementation grouping and
explicit reexports to preserve root public paths and type identities. Existing
facade delegation is intentional compatibility, not a second implementation.
Application command options and write policy remain private binary concerns.
Retain selection-dependent workflow caches within Analysis; do not move them into
shared Arc<Source> or introduce new cross-selection cache sharing. Preserve the
existing AST lifetime and ownership contract during organization.
Renderers consume native view models without reopening source. Core acquires no
mandatory CLI, Node, renderer or inspected-project dependency.

## Fan-out and contracts to carry

- **Capture and lexical facts:** `source.rs`, `scan.rs`, `discovery{,/mounted}.rs`,
  `workspace.rs`, `population.rs`, `source_text.rs`, `model.rs`, `relationships.rs`.
  Readers include navigation, review, typed checks, revision, qualified analysis
  and adapter snapshots. Preserve physical identity versus package/mount contexts,
  orphan files, test ancestry, lexical grammar and original bytes. No new scan.
- **Qualified analysis:** `analysis.rs`, `source_index.rs`, representations and
  workflow. Index readers are core representation extraction/workflow expansion
  and application program/dependency projections; inspect tests and public callers
  before sealing fields. Preserve conservative resolution, ambiguity, alias shape,
  distinct call occurrences and source-control boundaries.
- **Stage reading:** `checks/stages.rs` also serves seam boundary/writer/view/
  ratchet readers. Split ownership without substituting qualified resolution for
  its separately contracted textual reading. Root facts stay immutable; transient
  visited/depth/cycle state belongs to one read.
- **CLI and presentation:** `main.rs`, all `*_command` modules, native JSON
  projections, view renderers and output routes. Preserve lexical compatibility
  versus explicit-population behavior, help/flags/defaults, structured null/
  unavailable distinctions and exit codes. Update test fixtures and shipped
  references when paths move. Manually format dense macro bodies.
- **Embedding and consumers:** root facade, `atlas-core` public API, independently
  compiled adapters/report clients, installed binaries and revision readers.
  Account for `include_str!` paths, packaged renderer/vendor assets and relative
  executable invocation. Anneal's accepted wrapper remains independently qualified;
  the Murail cutover retains the inventory and V1–V7 in `consolidation.md`.
- **Tests and docs:** integration/inline fixtures, CLI wire oracles, documentation
  source citations, examples, `PROVENANCE.md`, README, library/diagram guides and
  install/vendor checks. Preserve examples/assertions while sharing lifecycle.

## Review disposition requirements

The independent review was against `513aebb3`, not this target. The earlier
`girard-xqe.5` qualification remains a historical result at `c9cc7bf3`.
Recheck every finding against the actual implementation before editing.

1. **Output safety:** historically resolved by the issue-5 fix at `eaaf7c06`.
   Preserve the common input inventory/identity policy and current witnesses:
   source, query, architecture, adapter, companions, symlink and hardlink paths.
   Validation failure preserves original bytes and emits no apparent completion.
2. **Public Index:** seal correlated allocation/order behind borrowing accessors;
   document and migrate the deliberate Rust API change. Invalid caller positions
   produce an explicit result instead of panicking or misattributing a function.
   No mutable escape hatch or speculative handle framework.
3. **RootTable:** separate inventory/import/item-path construction phases and a
   per-reading traversal context. Remove hidden `RefCell` traversal state from
   captured facts. Repeat reads after a cycle/failure must remain independent.
4. **Declaration tags:** keep kind/visibility meaning typed internally. Explicit,
   centralized checked conversion preserves legacy wire spellings and unknown
   tags; Debug formatting cannot decide behavior. Cover every supported variant.
5. **Dense macros:** expand JSON objects/closures/matches and name consequential
   unavailable branches. Preserve field names, values, nulls and legacy bytes.
6. **CLI ownership:** move options with workflows, use one named compatibility
   route, and keep ordinary entry success/refusal visible. No distant lists that
   must be edited together or expanded public API solely to make main short.
7. **Fixtures:** one TempDir-based lifecycle helper per package, with useful
   operation-specific failures. Keep manifests, source examples and law assertions
   beside their tests. Exercise parallel runs, drop and panic cleanup.

The separately recorded renderer-I/O finding is issue 2. Declare measured limits
for request/SVG/diagnostics with headroom for useful existing large figures;
concurrently drain streams as necessary. Overflow refuses rather than truncating
success. Reap children on every exit; distinguish malformed output, early failure,
broken pipe and successful output. Reuse transport mechanics only where the
protocols genuinely share them. Keep this behavioral repair separate from moves.

## Acceptance and release boundary

- Compare representative CLI/help/JSON on fixed fixtures and pinned consumer
  sources. Atlas's own file paths/counts can change under its planned moves; record
  those expected self-scan changes rather than weakening fixed-input equality.
  Pure organization preserves representative CLI/help/JSON and public imports;
  use existing tests rather than adding tests that merely restate moved code.
- Invariant fixes have boundary regressions: invalid indices, kind/visibility
  round trips and unknown tags, repeat reads after cycle/failure, preserved
  alias/glob/namespace/test-population and seam judgments. Compare structured
  stage/seam/delta/dump results at pinned inputs; explain every difference.
- Run Rust 1.99.1 `just check`, `just check-install`, no-default-feature core checks,
  warnings-denied docs and vendor verification for the accepted final revision.
  Preserve independent adapter/report clients, safety and source-control controls.
- Qualify renderer transport with disposable fake-Node boundary cases: each request,
  SVG and diagnostic cap; concurrent stdin/stdout/stderr pressure beyond pipe
  capacity; early exit/broken pipe; malformed UTF-8 and non-SVG output; child
  termination/reaping on failure. Bound the test harness itself. Assert refusal,
  unchanged inputs and no successful artifact/evidence publication. Also render a
  representative useful large figure with the embedded renderer as a positive
  control. Existing happy-path renderer tests alone do not satisfy R6.
- Report raw/production/test size and navigation hot spots with named instruments.
  Smaller counts do not establish better design; a split names its responsibility.
- Ship a short architecture/execution guide and runnable README examples, generated
  on demand rather than committing an experiment gallery. A fresh reader uses only
  code/docs to trace an overview through artifact/exits, trace a stage/seam check
  to selected population and findings/unavailable, and locate grammar/cache owners.
  Verify that account against source. Human readability is a separate judgment.
- A reviewer assesses whole-system cohesion and every review disposition at the
  final revision. Publish the accepted interface and receipt, qualify any required
  Anneal pin change with its owner, and adopt Girard's tool pin through land.sh.
  Do not claim a native cutover from shared-library or diagram tests.

Implementation order is the bead dependency graph: organization, substantive
invariant/code-quality repairs, renderer hardening, final whole-system qualification,
then the existing Murail cutover. Do not subdivide these into a growing temporary
compatibility chain. If the fan-out changes materially, remap and amend the plan.
Murail source/policy/FM/budget changes still require its owner's acceptance; this
plan does not authorize them or reopen the completed Anneal cutover.

# Reading Atlas as a system

Atlas captures Rust source, answers questions about that capture, and presents the
result. A diagram is one presentation; a query or architecture check can return
structured evidence directly. Project policy is supplied explicitly.

```text
main -> cli::run -> one command
                      |
                Cargo context + source capture
                      |
          +-----------+------------+
          |                        |
   lexical/native queries    qualified Analysis
   review and checks         index / forms / workflow
          |                        |
          +----- structured result +
                      |
          selected view -> renderer -> Artifact
                                      |
                     output validation -> publication
```

The arrows describe ownership and the ordinary call sequence, not a universal
pipeline every command must traverse. A JSON query can finish before rendering.
Errors return to main, which writes the diagnostic and exits unsuccessfully.
Policy refusal and unavailable evidence retain their distinct structured status.

Three evidence families explain superficially similar paths. **Lexical** views
use declaration spellings and approximate mentions; **qualified** views follow
supported Cargo/import/signature syntax and retain unresolved targets; **native**
reports preserve the original review/check population and counting contracts.
They share source and grammar without claiming interchangeable meanings.
Depth-zero flow deliberately reads explicitly selected bodies, including tests;
it publishes their own workflow evidence and protects those source paths. Expanded
flow uses the selected qualified index and its captured syntax cache.

## Where code belongs

- **`crates/atlas-core/src/source/`** owns source bytes, physical/package/mount
  membership, lexical projection, import/item grammar and the lexical graph.
  `discovery::Snapshot` retains one captured source per physical file plus its
  package observations. A physical file shared by packages has several contexts,
  not an arbitrarily selected owner. `scan.rs` is the canonical lexical grammar.
- **Core `analysis/`** owns the conservative qualified index, representation
  observations and source-control projection. `Analysis` lazily parses captured
  bytes and retains its own selection-dependent workflow cache. `index/` joins
  Cargo/module/import contexts; unsupported or ambiguous targets remain explicit.
  `representations/` preserves written aliases/signatures/fields/construction.
  `workflow/` preserves syntax control and occurrence-specific call returns.
- **Core `queries/`** owns developer reports: lexical queries, selected native
  navigation, review and documentation audits. Lexical and selected native results
  intentionally differ in population/schema. `review/` keeps source metrics and
  token-based move observations; it does not assign an elegance score.
- **Core `checks/`** combines captured observations with explicit policy. The
  `stages/` entry composes a report; `roots.rs` reads root bindings, `paths.rs`
  owns the native textual path reading, `interfaces.rs` applies declared
  interfaces, and `seams/` checks item boundaries/writers/ratchets. This textual
  stage reading is distinct from qualified call resolution. Policy, concentration,
  typed-error and delta checks keep separate entry contracts and reports.
- **Core `revision/`** owns historical source and before/after custody.
  **`diagram/`** owns renderer-neutral models and quotient projections;
  **`architecture.rs`** describes supplied declarations, not observed behavior.
- **Application `src/cli/`** owns options and each command's coordination. `args.rs`
  defines the command grammar and compatibility positional roots. `mod.rs` resolves
  Cargo context and dispatches once. Navigation owns its own compatibility route;
  there is no second catch-all command implementation in main.
- **Application `views/`** selects representation, dependency, program, overview
  and comparison observations. **`render/`** produces ASCII/Pele documents from
  those native models without reopening source. A matrix or comparison table does
  not become a generic dependency graph merely to use a renderer.
- **`cli/output/`** owns destination/default policy, evidence and artifact writes.
  Its artifact and diagram routes retain different documented formats/defaults;
  `files.rs` owns their shared collision policy and publication primitives.
- **`process.rs`** owns concurrent, bounded pipes and direct-child cleanup for
  both renderer and declaration protocols. It observes process status and input
  delivery separately; protocol code decides how to report them.
- **`adapter/`** owns the explicit declaration protocol and executable selection;
  `bin/atlas-girard.rs` supplies Girard's declaration convention. Core does not
  import a project's FM/bead interpretation or execute discovered configuration.

## Invariants at the library boundary

`source_index::Index` owns correlated names, functions, boundaries and entry
positions. Its borrowing `functions()`, `boundaries()`, `roots()` and `inventory()`
accessors preserve their allocation and order. Positions belong to that index;
`resolve` reports an out-of-range caller as `Target::InvalidFunction`, and views
refuse it. A numerically valid position from another index has no portable identity.

Stage `RootTable` holds source facts only. Its constructor inventories modules,
reads imports, inventories root items, then records their textual references.
Each recursive read owns its active-item set, so cycles and failed reads cannot
leave state in the capture. This remains the native textual reading, separately
contracted from qualified call resolution.

`Declaration` retains schema-1 string tags at its serialization boundary,
including unsupported tags. `item_kind()` and `item_visibility()` decode supported
values to scanner enums; unknown tags return `None`. Stable `tag()`/`from_tag()`
conversions define wire spellings explicitly. Diagnostic Debug formatting is not
an encoding or a decision rule.

The root library modules remain public compatibility paths. For example,
`atlas_core::discovery::Snapshot` and `atlas::representations::Views` reexport the
same definitions from their physical owners; the compatibility surface is not a
second source model. Small reexports replace empty wrapper files.

## Trace an overview

Start at `cli/overview.rs::run`. It captures source and creates `Analysis`, reads
normalized declaration syntax from that analysis, and invokes the explicitly
chosen adapter. The host validates protocol custody, source references and
fingerprints; it does not prove the adapter's interpretation.

`views/overview.rs::project` chooses observed contracts from declared surfaces.
Unexpected observed construction routes survive that selection. Verification
instruments retain their direct-call evidence in a separate panel.

The command passes the view to the selected backend. `render/overview.rs` presents
contracts and instrument calls without mixing their edge meanings. The command
then passes the artifact and captured input inventory to `cli/output/artifact.rs`.
Preflight rejects collisions with source, query/adapter inputs and companion paths
before publication. Render/protocol/output errors propagate to main; a successful
render alone does not establish that its files were published.

## Trace a stage or seam check

Start at `cli/stages.rs`. The command reads explicit policy and captures source,
then passes the capture to the check entry in `checks/`. That entry selects its
declared source/test populations and constructs native `Workspace` views and root
bindings from `Snapshot`. Project wrappers can supply source-bound declaration
observations for seams; the standalone command supplies none, keeping required
missing project interpretation explicit.

The stage reader inventories crate-root bindings, reads textual paths in their
module namespace, and applies stage ordering/interface rules. Seam checks consume
that reading and project-supplied boundaries; missing required authority remains
unavailable. Report-only paths are not a complete policy PASS. The command formats
the structured result only after that computation and determines the appropriate
process outcome.

The serialized kinds/statuses, physical source and selected test population are
part of a report's meaning. Do not substitute a qualified call graph for textual
paths, or an authored permission for an observed relationship, to simplify a view.

## Working principles

Keep a representation with the operations that protect its invariants. A public
interface states what callers may rely on; private construction/traversal state
belongs to its owner. Explain responsibilities, inputs/results and evidence limits
in module headers; explain non-obvious caller obligations in API docs.

Read workflows from their entries. Named intermediate results make the sequence
and refusals visible. Hide implementation decisions behind cohesive interfaces;
do not add a forwarding layer, generic framework or smaller file solely to make
an entry point short. Source size is navigation evidence, not a quality verdict.

# Reading Atlas as a system

This document maps Atlas's code and states the rules the code keeps. Each
numbered rule in [Rules](#rules) is one sentence the tree either meets or
breaks. Under it, *Broken by* says what would break it, and *Checked by*
says what catches a break: a field of Atlas's own architecture policy,
[`.config/architecture.toml`](.config/architecture.toml), a test, a compiler
or Clippy lint, or **review** when nothing automatic checks it. `just check`
runs the lints, the tests, `cargo atlas policy` (the policy file, its
authority rows and the crate graph) and `cargo atlas stages`
(`scripts/self-check.sh`); the policy fields that only `cargo atlas delta`
judges are checked when someone runs it (see
[CONTRIBUTING.md](CONTRIBUTING.md#atlas-checks-itself)). Where the tree breaks a
rule today, the rule says so.

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
reports keep fixed line-based population and counting units for review and checks.
A fourth, **semantic**, is a compiler front end's resolution for one build
configuration, read from a SCIP index; it sits beside each qualified target and
never changes one. The index is supplied, or produced by `cli/semantic/indexer.rs`
under `--semantic`: the only place Atlas executes inspected code (rust-analyzer
runs build scripts and proc macros). That runner owns the child's deadline, its
build directory, the before/after input digest and the index cache. They share
source and grammar without claiming interchangeable meanings.
Depth-zero flow deliberately reads explicitly selected bodies, including tests;
it publishes their own workflow evidence and protects those source paths. Expanded
flow uses the selected qualified index and its captured syntax cache.

## Where code belongs

- **`crates/atlas-lib/src/source/`** owns source bytes, physical/package/mount
  membership, lexical projection, import/item grammar and the lexical graph.
  `discovery::Snapshot` retains one captured source per physical file plus its
  package observations. A physical file shared by packages has several contexts,
  not an arbitrarily selected owner. `syntax/` derives written items, imports, visibility and cfg implication with
  syn. `items.rs` holds portable facts; `lexical.rs` owns raw comments/string
  masking. `scan.rs` is an internal compatibility path, with no second grammar.
- **`diagram/`** owns renderer-neutral models and quotient projections. It sits
  before `analysis/`, because the workflow projection builds its `Diagram`.
  **`architecture.rs`** describes supplied declarations, not observed behavior;
  every other module may read it.
- **Core `analysis/`** owns the conservative qualified index, representation
  observations and source-control projection. `Analysis` owns the invocation-local lazy AST and selection-dependent workflow
  cache in `sources.rs`. `index/` joins Cargo/module/import contexts: `mod.rs`
  sequences construction, `mounts.rs` owns Cargo contexts and boundary roots,
  `build.rs` fills correlated name tables, and `resolve.rs` follows supported
  imports and signature types. Unsupported or ambiguous targets remain explicit.
  `representations/` preserves written aliases/signatures/fields/construction.
  `workflow/` preserves syntax control and occurrence-specific call returns.
  `functions.rs` discovers/selects bodies; `parse.rs` reads their ordered expressions.
  `semantic/` reads a SCIP index (`wire.rs`, no dependencies), locates each
  recorded callee name (`locate.rs`) and reports the index's reading and its
  agreement with the qualified target per call. It runs no indexer.
- **Core `relations/`** flattens the lexical graph and the qualified facts into
  the documented tables `facts` exports. `schema.rs` is the v1 contract as data
  (columns, basis column, unknowns companion); `mod.rs` only copies existing
  observations into rows, adds witness ids, and bridges a qualified function or
  type to the lexical declaration at its file, line and name. It resolves
  nothing anew, and it never merges the two families.
- **Core `queries/`** owns developer reports: lexical queries, selected native
  navigation, review and documentation audits. Lexical and selected native results
  intentionally differ in population/schema. `review/` keeps source metrics and
  token-based move observations; it does not assign an elegance score. Its
  `moves/` entry sequences scope, inventory, comparison and report construction;
  `scope.rs`, `inventory.rs` and `compare.rs` own those distinct decisions.
- **Core `query/`** evaluates Datalog over those relations. `syntax.rs` reads
  clauses with their positions; `check.rs` resolves relations and fields
  against the schema, the prelude and rule heads, infers kinds, refuses unsafe
  variables, keeps only the rules a query needs and stratifies them;
  `honesty.rs` owns the basis and absence checks; `eval.rs` runs the strata
  semi-naively over interned rows. `prelude.dl` restates commands as rules,
  each held equal to its command by `tests/query.rs`. It reports rows only:
  no verdict, and no rule adds a resolution the facts did not make.
- **Core `checks/`** combines captured observations with explicit policy. The
  `stages/` entry composes a report; `roots.rs` reads root bindings, `paths.rs`
  owns the native textual path reading, `interfaces.rs` applies declared
  interfaces, and `seams/` checks item boundaries/writers/ratchets. This textual
  stage reading is distinct from qualified call resolution. Policy, concentration,
  typed-error and delta checks keep separate entry contracts and reports.
- **Core `revision/`** owns historical source and before/after custody.
- **Application `src/cli/`** owns options and each command's coordination. `args.rs`
  defines the command grammar and compatibility positional roots. `mod.rs` resolves
  Cargo context and dispatches once. Navigation owns its own compatibility route;
  there is no second catch-all command implementation in main.
- **Application `views/`** selects representation, dependency, program, overview
  and comparison observations. **`render/`** produces ASCII/Pele documents from
  those native models without reopening source. A matrix or comparison table does
  not become a generic dependency graph merely to use a renderer. `render/theme.rs`
  owns every colour: renderers take a palette, and Pele's CSS variables are
  resolved to concrete values in the markup, never in source-derived text.
- **`cli/output/`** owns destination/default policy, evidence and artifact writes.
  Its artifact and diagram routes retain different documented formats/defaults;
  `files.rs` owns their shared collision policy and publication primitives.
- **`process.rs`** owns concurrent, bounded pipes and direct-child cleanup for
  both renderer and declaration protocols. It observes process status and input
  delivery separately; protocol code decides how to report them.
- **`adapter/`** owns the explicit declaration protocol and executable invocation.
  Project conventions live in separately built adapters such as
  `examples/pipeline-adapter.rs`. Core does not import a project's own rule
  vocabulary or execute discovered configuration.

## Rules

### Dependencies between parts

**1. atlas-lib depends on no other workspace package, and cargo-atlas depends
on atlas-lib and no other workspace package.**
- *Broken by:* a dependency from atlas-lib on cargo-atlas or on a new workspace
  member, or from cargo-atlas on a member other than atlas-lib.
- *Checked by:* `[crate_graph]` in the policy, judged by `cargo atlas policy`
  in `just check` (an edge its rule does not allow, or an allowance no edge
  uses, refuses) and by `cargo atlas delta`.

**2. No top-level module of atlas-lib reads a module later in this order:
`architecture`, `source`, `diagram`, `analysis`, `relations`, `queries`,
`query`, `checks`, `revision`, `guide`.**
This is the order the code has, measured with `cargo atlas stages`, not an
ideal one. `diagram` comes before `analysis` because analysis builds the
diagram model (`Diagram`, `Coverage`).
- *Broken by:* a `use` or path in one module that names a later one, or a new
  top-level module missing from the order.
- *Checked by:* `[[stage_order]]` in the policy, judged by `cargo atlas stages`
  in `just check` (`scripts/self-check.sh`) and by `cargo atlas delta`.

**3. Each top-level module of atlas-lib reads only the earlier modules its
`stage_reads` row lists.**
- *Broken by:* a first read between two modules that had none, such as
  `relations` naming `queries`.
- *Checked by:* `[stage_reads.atlas-lib]` in the policy, judged by
  `cargo atlas stages` in `just check` and by `cargo atlas delta`. Adding a
  read means adding it to the row, in review.

**4. In cargo-atlas, no top-level module reads a module later in this order:
`process`, `views`, `adapter`, `render`, `cli`; each reads only the earlier
modules its `stage_reads` row lists.**
Commands own their options and orchestration under `src/cli/`, views select
observations, and renderers present a view: a view never calls a renderer.
- *Broken by:* a view naming `crate::render`, or any read against the order
  or missing from its row.
- *Checked by:* `[[stage_order]]` and `[stage_reads.cargo-atlas]` in the
  policy, judged by `cargo atlas stages` in `just check`.

### Source capture and evidence

**5. `source/syntax/` is the only structural Rust grammar: item nesting,
boundaries, visibility, fields, imports and attributes come from its syn walk.**
The text mask does not recognize declarations, macro payloads stay opaque, and
an unsupported item form refuses rather than disappears from an otherwise
successful inventory. A header comment observation may use the captured raw
text when structural syntax fails, but it cannot supply missing item facts.
Declared architecture, project authority and policy are supplied facts, not
parser inference.
- *Broken by:* a second `parse_file` function outside `source/`, a direct
  `syn::parse_file` call elsewhere, or a declaration read from masked text.
- *Checked by:* the authority row "the structural Rust grammar" (a
  `parse_file` declared outside `source/` fails `cargo atlas policy`); a direct
  `syn::parse_file` call outside `source/syntax/`: review.

**6. `discovery::path_key` is the only conversion from a path to a source key.**
- *Broken by:* a second `path_key`, or an inline `replace('\\', "/")` on a path.
- *Checked by:* the authority row "the source path key" (a `path_key` declared
  outside `source/` fails `cargo atlas policy`); an inline conversion: review.

**7. A physical file that several packages share keeps one context per
package, never one arbitrarily selected owner.**
- *Broken by:* capture or a report choosing one package for a shared file.
- *Checked by:* `crates/atlas-lib/tests/source_contexts.rs`.

**8. A file that does not parse stays in the inventory with its error, and a
missing fact never counts as zero or as a pass.**
`Source::of` is fallible: a `Source` holds valid written-Rust facts, and
discovery keeps invalid bytes, fingerprints and a portable `SyntaxError`
separately. Required failures are selected by package, file and test
population before they are reported; a failed parent cannot establish test
ancestry for its children.
- *Broken by:* dropping an unparsable file, or reporting a count or PASS
  over a population with a missing file.
- *Checked by:* `crates/atlas-lib/tests/syntax_facts.rs`,
  `crates/atlas-lib/tests/policy.rs`, `tests/delta.rs`.

**9. Ambiguous and unresolved names stay visible: no answer picks one
candidate among several.**
- *Broken by:* resolving a duplicate spelling to its first match, or dropping
  an unresolved target.
- *Checked by:* `tests/queries.rs`, `tests/mapping.rs`, `tests/facts.rs`,
  `crates/atlas-lib/tests/navigation.rs`.

**10. A file's syntax facts are a pure function of its relative path and
bytes, and a cache hit returns the facts of exactly the bytes just read.**
`source::FactsCache` keys facts by a hash of the path, the bytes and the
running executable, through private wire types: the public fact types gain no
`Deserialize`. Capture parses cache misses on worker threads into a map keyed
by path, so the order of work never reaches an answer.
- *Broken by:* a fact that depends on another file, the time or the work
  order; a cache key missing the bytes or the executable.
- *Checked by:* `ATLAS_CACHE=verify` in `.cargo/config.toml`, which re-parses
  and compares every cache hit in the whole test suite; `tests/facts_cache.rs`.

**11. Snapshots and their source facts are `Send + Sync`; syn ASTs stay
invocation-local under `Analysis`, with no global AST cache.**
Capture parses to portable facts and drops the AST; qualified commands may
parse the retained bytes again into their local cache.
- *Broken by:* a non-`Send` field in a fact type; a process-wide AST store.
- *Checked by:* `crates/atlas-lib/tests/syntax_facts.rs` (a compile-time
  `Send + Sync` bound); the AST cache: review.

**12. The lexical, qualified, native and semantic families stay separate: a
semantic reading sits beside a qualified target and never changes it.**
- *Broken by:* replacing a qualified target with the index's, or merging two
  families' rows.
- *Checked by:* `tests/semantic.rs`, `crates/atlas-lib/tests/semantic.rs`.

**13. `relations/` copies existing observations into rows and resolves
nothing anew, and each prelude rule answers exactly what its command does.**
- *Broken by:* a relation row that no report observed; a prelude rule that
  drifts from its command.
- *Checked by:* `tests/facts.rs` (rows agree with the reports), `tests/query.rs`
  (each prelude relation equals its command).

**14. Positions in a `source_index::Index` belong to that index: an
out-of-range caller is `Target::InvalidFunction`, and views refuse it.**
The index's borrowing `functions()`, `boundaries()`, `roots()` and `inventory()`
accessors preserve their allocation and order. A numerically valid position
from another index has no portable identity.
- *Broken by:* treating a position from one index as valid in another.
- *Checked by:* `crates/atlas-lib/tests/source_contexts.rs`.

### Checks and project policy

**15. A report that read only part of a project never claims the project's
PASS, and refusal and unavailable evidence stay distinct statuses.**
- *Broken by:* a check reporting PASS when a required input is missing, or
  folding UNAVAILABLE into REFUSE or PASS.
- *Checked by:* `tests/policy.rs`, `tests/stages.rs`, `tests/delta.rs`,
  `crates/atlas-lib/tests/policy.rs`.

**16. Core holds no project's conventions: a project's rules arrive as typed
inputs from its own adapter or wrapper.**
- *Broken by:* a project's names, vocabulary or citation format in
  `crates/atlas-lib` or the CLI.
- *Checked by:* review.

### Reading only, and child processes

**17. Atlas never changes the project it inspects: every output plan refuses
to replace a source file or another input before it writes.**
- *Broken by:* an output path that can land on a captured source, query or
  adapter input, or on a companion file.
- *Checked by:* `tests/output_safety.rs`, `tests/mapping.rs`, `tests/extensions.rs`.

**18. Only `cli/semantic/indexer.rs` runs the inspected project's code, as a
bounded child with a deadline.**
- *Broken by:* another module starting a build, rust-analyzer or a build script.
- *Checked by:* `tests/semantic_indexer.rs` for the runner's bounds; that no
  other module does it: review.

**19. The renderer and adapter protocols exchange with their child process
only through `process.rs::exchange`.**
- *Broken by:* a protocol that writes to or reads from a child's pipes itself.
- *Checked by:* the authority row "child process exchange" (its home exists);
  `tests/renderer_io.rs`, `tests/extensions.rs`; a second spawn site: review.

**20. Every colour is defined in `render/theme.rs`, and renderers take a palette.**
- *Broken by:* a colour value written in a renderer or view.
- *Checked by:* the authority row "every colour" (a `Palette` declared in
  `views/` or `cli/` fails `cargo atlas policy`); a colour literal: review.

### Code organization

**21. Each core fact has one home: the captured workspace (`Snapshot`), the
qualified analysis (`Analysis`), the qualified index (`Index`), the facts cache
(`FactsCache`), the relations schema (`RELATIONS`) and the output collision
policy (`preflight`).**
- *Broken by:* removing or duplicating the home item, or declaring a forbidden
  second one.
- *Checked by:* the `authority` rows in the policy, run by `cargo atlas policy`
  in `just check`.

**22. The root library modules are explicit compatibility re-exports of their
physical owners, never a second definition, and the list only shrinks.**
For example, `atlas_lib::discovery::Snapshot` and `cargo_atlas::types::Views`
re-export the same definitions from their physical owners.
- *Broken by:* a new crate-root re-export or rename, or a wrapper file that
  duplicates its owner.
- *Checked by:* the `alias` rows in `[concentration]`, judged by
  `cargo atlas delta`.

**23. A module name used in more than one place has a ruling: a facet (one
idea seen from several places) or a collision (two ideas, owed a rename).**
Today `facts`, `relations` and `syntax` are collisions.
- *Broken by:* a new module whose name is already used, without a ruling;
  a collision gaining a home.
- *Checked by:* the `name` rows in `[concentration]`, judged by `cargo atlas delta`.

**24. No file grows past its limit: 300 body lines for an entry file, 600 for
any other production file, and no listed larger file grows.**
Size is navigation evidence, not a quality verdict; a split names the decision
it separates, never a line target.
- *Broken by:* a file over its limit, or a listed file growing.
- *Checked by:* `entry_lines`, `file_lines` and the `entry`/`file` rows in
  `[concentration]`, judged by `cargo atlas delta`.

**25. Each package's code and public surface stay within the policy's counts.**
- *Broken by:* growth without a policy edit that says what it buys.
- *Checked by:* `[size]` and `[surface]`, judged by `cargo atlas delta`.

**26. No code uses `unsafe`.**
- *Broken by:* any `unsafe` block, function or impl.
- *Checked by:* `unsafe_code = "forbid"` in the workspace lints.

**27. Code passes Clippy's default and pedantic groups and the workspace's
restriction lints, and every exception states its reason in the attribute.**
CONTRIBUTING.md asks for `#[expect(..., reason = "...")]` rather than `#[allow]`,
so an exception that stops applying is noticed.
- *Broken by:* a new lint finding, or an `#[allow]` without a reason.
- *Checked by:* the `[workspace.lints]` table in `Cargo.toml`, run by
  `cargo clippy -- -D warnings` in `just check` (`allow_attributes_without_reason`
  refuses a missing reason); `#[expect]` over `#[allow]`: review.

**28. A function body fits on a screen: at most 70 lines of code.**
A split names the decision each part makes, as in [Working
principles](#working-principles); 70 is where Clippy stops a body, not a
target to split to.
- *Broken by:* a function body over 70 lines without an `#[expect]`; an
  `#[expect]` left on a function that now fits.
- *Checked by:* `clippy::too_many_lines`, denied in `Cargo.toml` at
  `too-many-lines-threshold = 70` (`clippy.toml`), in `just check`. Functions
  in `checks/` and `analysis/` still over it each carry
  `#[expect(clippy::too_many_lines, reason = "longer than 70 lines; to be split
  along its decisions")]`.

**29. No block nests more than six deep.**
A deeper block usually holds a decision that deserves its own name: an early
return, a helper, or an iterator step.
- *Broken by:* a block nested seven or more deep without an `#[expect]`; an
  `#[expect]` left on a function that no longer nests that deep.
- *Checked by:* `clippy::excessive_nesting`, denied in `Cargo.toml` at
  `excessive-nesting-threshold = 6` (`clippy.toml`), in `just check`. A
  function still over it carries
  `#[expect(clippy::excessive_nesting, reason = "nested deeper than 6 blocks;
  to be flattened along its decisions")]`.

**30. Each module's `//!` header states its responsibility and the important
limits of its result.**
- *Broken by:* a module with no header, or one that no longer says what the
  module does.
- *Checked by:* review; `cargo atlas comments` lists files without a header.

**31. Production code never uses `String` as an error type, as in
`Result<T, String>`: an error keeps the typed cause of the step that produced it.**
Today cargo-atlas meets it; atlas-lib does not yet (its `relations` reader, the
`source::Import` bindings, and sites in `analysis/` and `checks/`).
- *Broken by:* a production `Result<_, String>` in a package the policy lists.
- *Checked by:* `typed_errors` in the policy (cargo-atlas), run by
  `cargo atlas policy` in `just check`; atlas-lib: review, until it is listed.

**32. Renderers present a view without reading source.**
- *Broken by:* a renderer in `src/render/` that reads files or captures source.
- *Checked by:* review.

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

Stage `RootTable` holds source facts only. Its constructor inventories modules,
reads imports, inventories root items, then records their textual references.
Each recursive read owns its active-item set, so cycles and failed reads cannot
leave state in the capture.

## Trace indexing and a move report

Start at `analysis/index/mod.rs::Index::from_selected_sources`. It reads Cargo
mounts, binds supplied boundaries, collects functions and reexports, builds static
names, then rebinds imported method owners. Only the finished index escapes.
`resolve.rs::resolve` distinguishes method receivers from static/value calls;
receiver ownership follows a written type or one initializer's return signature.
No Rust inference or arbitrary choice among candidates is added.

For a move review, start at `queries/review/moves/mod.rs::moved`. Scope selects
whole owning crates, inventory reads their native keys and tokens, comparison
pairs exact/rerooted/remaining bodies, then the entry constructs the owned report.
Scope narrows reporting after matching, so moving across the selected path does
not invent additions/removals. `visibility` shares that inventory but compares
written visibility. It never substitutes Rust's effective visibility.

## Trace capture

Trace capture from `discovery::snapshot_workspace` through `read_metadata` into
`Captured::of`. Its syn walk constructs Source or retains the failure. A native
Workspace borrows successful facts and keeps selected missing evidence. A qualified
Analysis checks selected failures, then reads its cached AST. Both use the same
`TestSelection`; source sites use byte offsets and native line accounting uses
its documented line ranges. Syntax facts use exact byte positions, including
several items on one line. Mixed production/test sites on a shared native line
are unavailable to a line policy; qualified syntax-site selection can
distinguish them, and neither population silently substitutes for the other.

`Declaration` retains schema-1 string tags at its serialization boundary,
including unsupported tags. `item_kind()` and `item_visibility()` decode supported
values to scanner enums; unknown tags return `None`. Stable `tag()`/`from_tag()`
conversions define wire spellings explicitly. Diagnostic Debug formatting is not
an encoding or a decision rule.

## Working principles

Keep a representation with the operations that protect its invariants. A public
interface states what callers may rely on; private construction/traversal state
belongs to its owner. Explain responsibilities, inputs/results and evidence limits
in module headers; explain non-obvious caller obligations in API docs.

Read workflows from their entries. Named intermediate results make the sequence
and refusals visible. Hide implementation decisions behind cohesive interfaces;
do not add a forwarding layer, generic framework or smaller file solely to make
an entry point short. Source size is navigation evidence, not a quality verdict.

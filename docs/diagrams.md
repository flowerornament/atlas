# Diagrams and source evidence

These views explain different relationships in written Rust source. Each keeps
its source witnesses and limitations.

## The default overview

`overview` uses the co-installed `atlas-girard` adapter to interpret literal
`MODULES` and `SURFACES` declarations from `src/lib.rs`
(or `--declarations FILE`). Their shape is the one Girard already uses: module
name, part, layer, allowed dependencies; then stage name, entry, surface types.
The host supplies constant syntax and Cargo module mounts from its shared parse;
the adapter neither reparses nor compiles/executes the inspected crate. Accepted
declarations are checked against their source snapshot before binding.
Computed/nonliteral declarations refuse. Use `--adapter PATH` for a separately
built declaration adapter; its producer, witnesses and diagnostics remain in JSON.

The main forms are successful output types of declared stage entry functions.
Public surface methods join the picture when they produce a form another stage
accepts or construct an entry type. Isolated metadata/error alternatives are not
promoted merely because a surface lists them. All selected type/API/source
witnesses and the selection rule remain in JSON. Unexpected literal constructors
outside the APIs' resolved call reach remain visible.

Verification instruments have a separate panel of observed direct calls. Calls
are not mixed into contract arrows. Declared dependencies do not become observed
edges, and do not filter unexpected relationships out. A declared stage's entry
with no returned representation can still appear as a sink API. This is a contract
view, not an execution or termination proof.

Projects without these declarations use a saved query; Atlas never invents an
architecture for them. Start with `examples/queries/atlas.json` to inspect Atlas
itself:

```sh
atlas representations . --query examples/queries/atlas.json --backend pele
```

## Qualified file relationships

```sh
atlas dependencies ROOT src/backend/admit --relation calls
atlas dependencies ROOT src/backend/admit --relation signatures --backend pele
atlas dependencies ROOT src/backend/admit --relation fields --backend pele
atlas dependencies ROOT src/backend/admit --relation all --json
```

Every displayed edge has positive source witnesses, with file, line, source item,
target item and evidence kind. Calls use conservative path/receiver resolution;
signature types and direct field types use AST type positions, not a lexical word
match. Enum variants and generic parameters do not become same-spelled type edges.
The physical scope selects both endpoints. Observations outside it and unresolved
calls remain in JSON; they are not claims of absence. Disconnected files remain
in the view.

## Representations and producers

```sh
atlas representations ROOT --type Graph --panel producers
atlas representations ROOT --type Graph,Checked,Executable --panel contracts --backend pele
atlas representations ROOT --query question.json --panel all --backend pele
```

A saved question selects source symbols, never arrows:

```json
{
  "title": "From source model to rendered artifact",
  "types": [
    { "name": "SourceGraph", "file": "crates/atlas-core/src/model.rs" },
    { "name": "Diagram", "file": "crates/atlas-core/src/diagram.rs" },
    { "name": "Artifact", "file": "src/render/mod.rs" }
  ],
  "preparation_files": ["crates/atlas-core/src/diagram/projection.rs", "src/main.rs"]
}
```

Optional `apis` selects exact function names and files. Otherwise written-public
functions and trait implementation methods returning selected forms are used;
this syntactic selection does not prove public reachability through reexports.
Ambiguous names refuse. Methods use their written impl spelling, such as
`Generator<'g>::candidates`. Query types and API roots select the figure; extra
literal initializers outside their resolved reach are not erased.

Panels have different visual grammars:

- `contracts`: input/return type relationships, grouped by physical source file.
  Resolved workspace error forms appear as `Err` branches even when not selected
  as main forms; external/primitive errors may remain unresolved. Source titles
  preserve signatures and observed literal sites. Same type does
  not mean same runtime value.
- `producers`: every observed returner and immediate literal constructor of each
  selected type, in separate columns. A forwarding returner is not called a literal
  constructor. A repeated route prompts reading; it is not a duplication verdict.
- `preparation`: returners in selected files beside their source-ordered direct
  producer calls. Repeated calls remain repeated observations.
- `retention`: an optional sorted direct-field matrix. `&` marks references; `*`
  marks nested type syntax. Complete expressions remain in titles/JSON:
  `Arc<Graph>` does not establish ownership of Graph.
- `all`: contracts, producers and preparation in one composite SVG.

Select 1–12 types and at most 40 contracts/preparation rows per question. These
projection limits apply to every backend, including JSON/ASCII; narrow the question
when it exceeds them. Figures refuse without truncating source facts. Retention reserves two lines per cell; longer cell labels
require a narrower question or ASCII.

## Compare a change

```sh
atlas compare ROOT --before HEAD
atlas compare ROOT --before OLD_COMMIT --after NEW_COMMIT --query question.json --backend pele
```

The same selection is applied to both sources. Defaults compare the exact `before`
commit with current working source; `--after` reads another exact revision. Git
objects are read into an owned temporary tree and removed afterward. Atlas neither
checks out nor snapshots the user's working copy. No fetch happens.

Keys use physical file and qualified source symbol, excluding line-number churn.
The Pele comparison aligns before/after values, names both revisions, and labels
added/removed/changed observations. Several rows can describe the same source edit;
unchanged observations are omitted. It shows types, contract positions, field syntax,
literal-site counts and direct producer-call sequences. Removed types remain visible.
Moves/renames are reported as removal/addition; ambiguous identities refuse. Pele
refuses more than 40 changes; narrow the query or use ASCII/JSON. The current reader
requires a repository-root Cargo workspace; subdirectory workspaces and external
path dependencies in a historical snapshot may refuse. Git submodule contents are
not materialized. No semantic equivalence or rename inference is claimed.

## Navigation and source-body views

```sh
atlas census ROOT --json
atlas name ROOT AdmittedPlan
atlas module ROOT src/backend/admit
atlas name ROOT AdmittedPlan --relations --json
atlas flow ROOT --path src/lib.rs --function compile_with --depth 1 --backend pele --format svg
```

`flow` reads a selected function's expression control: operand evaluation, branches,
match patterns and guards, loops, explicit returns and each residual `?`. With
`--depth 1` (up to 6), it expands source-resolved callees and pairs each call
occurrence with its own return continuation. Physical source files enclose nodes;
repeated calls never share a return vertex. Recursion, depth limits and unresolved
calls are labelled boundaries that continue only if they return. Expansion refuses
above 600 nodes without emitting a partial picture; select a smaller entry or depth.
Without `--depth`, it shows one source body without resolving callees.

This remains a source-control picture, not feasibility analysis or a proof of
termination. Panics/unwinding, destructors, implicit operator dispatch and macro
execution are not inferred. Closures, async/try/unsafe blocks, labelled blocks and
unsupported syntax stay opaque; they are not flattened into ordinary work.
Compound assignment order depends on types, so it also stays opaque. Call/return
edges describe the supported ordinary-return path. See the executable controls in
`tests/source_control.rs` and [qualification notes](reader-qualification.md).

`program --architecture PROFILE` and
`map --architecture PROFILE` remain available for existing callers. Their profiles
are declared boundaries/permissions, not observed program behavior.

`name --relations` adds approximate incoming/outgoing lexical evidence to a name
card. The old `related` spelling is a hidden compatibility alias and warns on stderr;
its JSON contract is unchanged. `dump` retains the complete lexical source model for
library and machine consumers. `map` retains declared-boundary/quotient views, which
are different questions from qualified direct file relationships.

The older `map --view types`, `name --relations` and `dump` relationships are explicitly
approximate lexical co-mentions. Use `dependencies` for qualified calls/signature/
field relationships. This distinction is deliberate; legacy evidence is not silently
upgraded into a semantic claim. Saved profiles are optional legacy inputs, never
required for the source-declaration overview.

## Scope and development

The shared source facts cover written production structs/enums, signatures, direct
fields, immediate struct literals and conservative resolved calls. Exact source
expressions preserve containers, references and tuple positions. Macro expansion,
trait/alias normalization, Default/Clone construction, indirect state and dynamic
dispatch are not complete. Deferred literal construction is excluded. Conditional
alternatives remain alternatives. Unknown type spellings include primitives and
external types as well as unavailable resolution. Name resolution needs unambiguous
Cargo mounts; unusual module-declaration formatting may exceed the lexical discovery
scanner. None of these unknowns is a proved missing edge.

The library exposes discovery/source models, representation facts/projections,
qualified dependencies, source-declaration adapters, comparisons, native diagram
models and ASCII/Pele renderers. Renderers do not inspect source. Read
[diagram lessons](diagram-lessons.md) for the failed approaches and the graphical
principles behind the current views, and [PROVENANCE.md](../PROVENANCE.md) for attribution.

`just check` runs formatting, strict Clippy, Rust tests, strict rustdoc and renderer
tests. `just check-install` tests an installed binary outside its checkout.
`just verify-vendor` checks the pinned Pele package. Rust is pinned in
`rust-toolchain.toml`. The crate is MIT OR Apache-2.0; the embedded Pele bundle is MIT.


## Named aliases and control detail

Representation queries include named Rust aliases such as `type Forms = Vec<Form>`.
Their written RHS is shown explicitly; the alias stays the signature's handoff.
RHS references are `alias-target` observations, never fields or ownership claims.
They are not recursively expanded. External/ambiguous RHS names remain unknown;
cycles remain finite written links. Comparison reports RHS changes.

For an entry sequence without separate borrow and synthetic `?` bridge boxes:

```sh
atlas flow --root . --path src/main.rs --function main --detail control --backend pele --format svg --out target/atlas/main.svg
```

`--detail expressions` is the default. Control detail contracts only designated
borrow/bridge nodes with one incoming and one outgoing edge. A residual decision
can join an unexpanded call with one continuation in the same file. Branches,
loops, guards, opaque operations, exits and occurrence-specific call returns remain.
Control detail refuses over 600 raw source-control nodes before folding, also at
depth zero. The original unexpanded expression detail keeps its existing limit.
Folded node labels and witnesses are retained in `coverage.summarized_nodes` in
`diagram.json`; `workflow.json` retains the original selected workflow steps.
Source-control possibilities are not path feasibility or a termination proof.

The Pele preparation panel groups functions by file and connects actual resolved
producer calls to one box per function identity. Two callers sharing a callee are
visible as two routes to that implementation, not a claim of one runtime value.
Locations witness call sites without asserting an evaluation order.
ASCII retains the corresponding comparable preparation rows.

Use `preparation_functions` in a saved query for an exact overview, for example
`[{"name":"least_forms","file":"src/frontend/elaborate/least.rs"},
{"name":"build","file":"src/frontend/elaborate/program.rs"}]`.
Each selector must resolve uniquely and return a selected representation.
`preparation_files`, when also present, limits those selections to those files.
Omit function selection to retain all producer-returning rows in the chosen files.
All source facts remain in JSON; the query selects a view, not an asserted architecture.

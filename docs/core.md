# Using the shared source library

The `atlas-core` workspace package provides source discovery, lexical facts and
structured developer queries without depending on the Atlas CLI or its renderer.
Its compiler baseline is Rust 1.99.1. Native Cargo tools can depend on this package at an exact Git
revision; no sibling checkout or Node runtime is needed.

`discovery::snapshot_metadata` accepts already-loaded Cargo metadata. The returned
snapshot retains normalized physical source paths, original source bytes, lexical
outlines and test-boundary evidence alongside the relationship graph from the same
discovery pass. `source_facts(path)` borrows that captured outline and boundary
evidence; a review does not rebuild its lexical projection. Its graph and source
accessors borrow captured data; later filesystem edits do not change the snapshot.
`discovery::scan_metadata` remains the graph-only convenience interface.

`source::RustSourceProjection` exposes lexical item outlines, declaration lines,
comment lines and test regions. It masks strings and comments before recognizing
Rust structure. Item spans include attached comments and attributes. Lexical
recognition is deliberately approximate and does not expand macros or resolve
Rust types. Comment recognition alone does not interpret an FM annotation.

The native LOC boundary excludes cfg-test modules, including their attached
attributes. Individually cfg-test-gated helpers outside such modules remain in
this counting population. Other views can select a different population; do not
substitute this counter for the production-symbol selection of a qualified view.

`review::outline` consumes the retained snapshot and a physical path scope. It
returns item spans, source fingerprints, non-test lines, top-level item breadth,
largest top-level block and code outside recognized spans. No matching source is
an error. Block depth follows the native review unit: from the declaration line
to the next top-level declaration or test-region boundary. It differs from the
attached span ending at an item's closing brace.

```sh
atlas spans --root . --scope crates/atlas-core/src --json
atlas nontest-loc --root . --scope crates/atlas-core/src
atlas concentration --root . --scope crates/atlas-core/src
```

Scope defaults to the whole workspace; positional PATH remains an alias for
`--scope`. A supplied scope with no matches refuses.

These reports are evidence for reading and review. They do not assign an
architecture verdict. The existing `atlas::discovery`, `atlas::model`,
`atlas::queries`, `atlas::relationships`, `atlas::revision` and
`atlas::architecture` paths forward to the shared implementation.


`review::diff` compares the production line streams and shape reports of two
captured snapshots. Test-module-only edits are excluded; individually gated
helpers retain the native counting boundary. A scope present only in the before
snapshot remains selectable, so deletions remain visible. Changed files carry
before/after fingerprints and optional shape measures; the report does not infer
renames, equivalent behavior or an architectural verdict.

```sh
atlas diff HEAD --root . --scope crates/atlas-core --json
```

The revision is resolved through the shared immutable revision reader. The JSON
basis identifies its exact commit; `after: null` denotes current working source,
whose changed-file fingerprints are in the report. `compare` remains the separate
question about representation contracts and constructors.

Documentation audits use a caller-supplied `documentation::Policy`. It selects
item kinds, written visibility, test population, header population and optional
citation markers. `OutsideTestModules` preserves native counting of individually
gated helpers; `Production` additionally excludes those helpers. Neither changes
the captured test evidence. The source API also exposes projected lines, trait
impl heads and struct-field visibility for native review consumers.

```sh
atlas comments --audit --root . --population pub --json
```

Omitting `--audit` refuses. Citation checking is explicitly unrequested unless
`--citation-markers` supplies the project's own strings. Header selection states whether only the file or any module may supply `//!`.
Missing headers, uncited headers and missing outer `///` comments are separate
observations with source witnesses. This native line-comment metric does not
count doc attributes or block documentation; its limits travel in the report;
this command does not emit an architecture PASS verdict. The shared library
contains no project-specific citation-marker defaults.


`review::moves::{moved, visibility}` match declarations by the native lexical
crate/class/name key, including impl owners and inline modules. A physical path
scope narrows reporting only: matching still spans each selected crate. Duplicate
keys pair approximately and are counted explicitly. Reports retain file
fingerprints, discovery diagnostics and their matching limits.
Move and visibility keying reuse each captured import's parse result. An unreadable
required import fails the query with its source location; it cannot disappear as
an empty binding list or imply that no binding changed.

```sh
atlas moved HEAD --root . --scope src/backend --json
atlas visibility HEAD --root . --scope src/backend
```

`moved` keeps exact normalized tokens, equal-after-rerooting, changed, one-sided
and unreadable spans separate. Exact ignores docs, written visibility and selected layout; macro input tokens
and parenthesis commas are preserved conservatively;
rerooting also removes selected crate-relative module qualifiers, so different
paths can compare equal in that class. Neither proves behavior equivalent.
`visibility` reports the native written-visibility ladder, including restricted
paths; it does not calculate Rust's effective visibility or resolve relative
scopes after a move. Native union-field visibility and glob-import retargeting
remain unmeasured and are named in the report limits. These are report-only
tools, never architecture verdicts.


Qualified type/call extraction (`representations`), conservative name lookup
(`source_index`) and ordered source syntax (`workflow`) now share the core
library. Their renderer-neutral `diagram` values work without a renderer. The
application facade reexports these types and preserves `Workflow::diagram`,
`Workflow::ascii` and the existing representation facts API. Render backends,
artifact transport and project adapters remain in the application.

The default core dependency graph excludes Clap. The optional `clap` feature
adds `ValueEnum` integration for diagram selections; the application enables it
so its existing public enum traits remain available. Native consumers can depend
on `atlas-core` with default features disabled. The gate builds that configuration
and an independent consumer which reads source and produces a neutral control
model without the application or a rendering runtime.


For several qualified questions, capture the workspace once and reuse an
`analysis::Analysis`. It shares the captured source and lazily parses each selected
file once for that invocation. Workflow facts are also shared across its indexes,
rather than traversing every body again for another question. The snapshot owns its Cargo context, so a
later change to the caller's metadata or disk files cannot change its namespace
or observations.

Physical paths and Cargo contexts are distinct. `SourceGraph.files` contains
package-file observations; a path shared by two packages has two observations.
`physical_files()` counts each path once, and `Snapshot::physical_file(path)`
provides a direct lookup. Physical reports prefer a production observation when
a file also has a test-only context. Package navigation and move matching retain
the package context. Package-specific diagnostics carry their package, so one
package's incomplete mount cannot become another package's finding.

Mounts retain test ancestry, and distinguish targets that share a root path.
Qualified function identities include the target and module path. Representation
facts describe physical syntax sites: when contexts resolve a signature or call
differently, its target stays unknown. Declaration diagrams label shared sites
with their package; file diagrams still contain one box per physical path.
Declaration adapters refuse multi-package file authority because their current
protocol cannot express that distinction.

`Snapshot::files(FileSelection::SourceTree)` selects each package's `src`
inventory; `Discovered` also includes the discovered Cargo roots outside it.
`Analysis::with_tests` selects `Production`, `OutsideTestModules` or `All` from
the same retained test evidence. Changing this selection clears workflow facts,
retaining the AST cache. Excluded files need not parse; selecting them makes a
syntax error an explicit failure. Imports use the same population and use-tree
grammar as declarations. Other cfg alternatives are retained without evaluating
feature combinations.

Explicit file workflows (`workflow::read`, `read_many`, `read_file`) remain syntax
queries over the requested bodies, including test bodies. They do not acquire a
production-population filter merely because qualified workspace views use one.

File-module loading remains rustfmt-shaped. File children inside inline modules,
external siblings outside the discovered inventory, symlink mounts and missing
or escaping paths are diagnosed rather than assigned invented namespaces. This
is source evidence, not the compiler's expanded module tree.

```rust,no_run
fn inspect(root: &std::path::Path) -> atlas_core::Result<()> {
    let snapshot = atlas_core::discovery::snapshot_workspace(root)?;
    let analysis = atlas_core::analysis::Analysis::new(&snapshot)?;
    let facts = analysis.representations()?;
    let (workflow, diagram) = analysis.expanded("src/main.rs", "main", 1)?;
    Ok(())
}
```

Application qualified views, program queries and expanded workflows use this
capture path. Unexpanded file-only workflows still read just their selected
files once per batch. Older public graph-plus-root conveniences remain explicit
fingerprint-checked reread adapters for callers that did not retain source; they
are not a fallback selected during analysis. A parse failure names its file and
refuses the qualified analysis rather than producing an empty report.

## Stage paths and policy data

`checks::policy::Policy::parse` reads native schema-1 authored policy data,
rejecting unknown fields. It preserves the native field families and opaque
external references; it does not establish that a written authority is valid.
`stage_references(scope)` returns only the references required by that stage
selection. Local authority resolution belongs to the project's wrapper.

`checks::stages(snapshot, policy, scope)` ranks production textual paths from
the retained source and import facts. Root aliases, reexports, unplaced modules,
forward reads and unknown paths remain distinct. Competing conditional root
imports stay unjudged. Malformed required imports produce unavailable evidence,
never an empty successful reading. The report includes policy findings for the
same selected package; it does not claim complete policy PASS. Captured
file-module locations and directly declared functions/constants/statics can
prove a module/value import pair. Imports bring both namespaces; terminal inline
paths read the value and continued module paths read the module. External or
chained aliases, competing same-namespace bindings and targets already known in
both namespaces remain unjudged. This is bounded source evidence, not complete
Rust namespace resolution.

```sh
atlas stages --root . --policy .config/architecture.toml --scope my-package --json
```

`Snapshot::mounted_sources(package)` retains the native outside-src policy
population separately from Cargo module contexts. Each location and any missing
mount's attempted paths remain available to test-coupling consumers. This does
not expand source-tree budgets or establish compiler namespace resolution.
Unrelated test mounts do not make a production stage reading unavailable.

`checks::seams` derives item debt, unlisted exports, writer calls, test coupling
and monotone baseline ratchets from the same snapshot. Its additional input is
each project's interpreted `BoundaryDeclaration`, keyed by package and stage,
including the root file and captured fingerprint. Missing or stale declarations
cannot construct a seam reading. The native wrapper owns the meaning of its
boundary paragraph and the resolution of formal-model citations; core carries
the resulting citation findings into the ratchets without reinterpreting them.

`SeamReading` exposes native structured observations (`json`, `totals`), listing
changes and `refusals` against a baseline reading and `PublishedStages`. These
are source/path/count rules, not a complete gate verdict. Test-root resolution
uses the same resolver over all-test facts; required unjudged test paths make
the dependent census unavailable. Mounted support and every required source
input retain their fingerprints and diagnostics.

The standalone `atlas seams --policy FILE --json` reports unavailable when a
required project boundary interpreter is absent and directs callers to the
native wrapper. A policy with no selected seam rule reports `not-applicable`.
It never supplies an empty boundary or invents a project authority.

## Written typed-error boundaries

`checks::typed_errors::read(&analysis, packages)` reads the native fixed spelling
rule from the existing AST cache. It visits written type positions in the native
source-tree production population, including callbacks, fields and aliases.
The accepted constructor spellings are bare `Result` or `std::result::Result`
and `core::result::Result`; errors are bare `String` or the `std`/`alloc` string
paths. Qualified forms may start with `::`. Trivia does not hide a match. This
preserves the native written-spelling scope; raw identifier spellings, aliases
and macro expansion are not normalized into these names.

The unit is a distinct source line, even when several matching types appear on
it. Each package retains source fingerprints, matched lines and typed unavailable
inputs. Invalid required syntax, excluded source paths and empty populations
cannot become an empty successful reading. The rule uses no source I/O or
separate parser; an analysis remains valid after its captured source is removed.
It does not establish the project's complete policy verdict.


`checks::policy` validates the native schema-1 **policy inputs**. Configuration
membership and arithmetic, retained production authority homes and forbidden
items, fixed typed-error syntax, and required host/project observations share
one implementation. The command is not a measured landing verdict: size,
surface and concentration caps, dependency judgments and baseline/candidate
ratchets belong to `delta`.

```sh
atlas policy --root . --policy .config/architecture.toml --json
```

The library workflow is `Policy::parse` → capture `Snapshot` → build `Analysis`
→ `policy::observe` → `policy::validate`. Reports keep violations and unavailable
inputs separately. PASS requires both lists to be empty. Required unavailable
evidence takes status precedence while known violations remain visible; both
statuses exit unsuccessfully. The CLI labels the judgment `policy-inputs` in
JSON and text.

The host observer probes normalized workspace-relative forbidden paths and
runs authored patterns with `ast-grep` against a private temporary replica of
captured source bytes. It does not reread original Rust files. Paths crossing
intermediate symlinks are unavailable; a final symlink is an existing forbidden
path. Syntax helpers must exit 0 or 1, emit valid JSON and no stderr, and return
source lines inside the captured query population. Missing, malformed or stale
observations never mean zero matches. File/directory selectors use path-component
boundaries. Pattern populations retain test files as in the native helper;
authority declarations use production test evidence. The helper process has no
Atlas timeout or output-size cap; native wrappers retain control of execution
budgets. Atlas writes no inspected project files.

`Inputs::new(&snapshot)` lets a native wrapper supply explicit filesystem,
pattern and local-authority observations. Pattern readings retain an exact
file/fingerprint manifest, checked against the snapshot. Local references bind
the field path and original written value to `Resolution`; parsing a bead-like
string is not authority. The standalone CLI leaves those project resolutions
unavailable. Use the owning wrapper for the project's complete judgment. This
is a fixed Rust interface, not a dynamic policy-plugin system.


`revision::Revision::read` extracts a resolved Git commit into an owned temporary
directory. The private directory lease owns cleanup independently of the legacy
public `root` path; changing that path cannot redirect deletion. `root`/`commit`
readers remain available, but callers construct revisions through `read`, not
struct literals. Dropping one revision does not delete another extraction or
modify the inspected working tree.


`checks::delta::check` composes policy-input validation, native size/public-surface
and normal-dependency measurements, ceiling changes, stage/seam judgments and
concentration debt ratchets over baseline/candidate captures. The native
`concentration` command remains the separate max-block/breadth shape report.
Body-line debt excludes navigation; size token-lines retain it. Native accounting
keeps individually gated helpers outside test modules. Those units do not change
the shared production population used by stage and typed-error checks.

```sh
atlas delta main --policy .config/architecture.toml --citation-markers .design,fm/ --json
```

The host resolves the baseline and candidate (the current Git/JJ commit by
default), extracts the baseline, captures current source, and verifies the
materialized tree and capture against the owned candidate extraction. It then
loads the tracked policies, observes inputs, judges, and presents the result.
JJ name resolution always uses `--ignore-working-copy`: save working edits
outside Atlas before asking for a pinned delta. A different candidate, dirty
tracked content or extra untracked source refuses. Revision lookup requires
`.git` or `.jj` at the workspace root; ancestor/subdirectory workspaces are not
silently associated with a repository. Extraction omissions and source limits
remain errors/unavailable evidence. Executable-mode custody requires Unix.

JSON retains `verb`, exact `basis.baseline`/`basis.candidate`, `policy.raises`,
`violations`, `verdict` and a human `display`. As in the native gate interface,
JSON REFUSE is data and exits successfully: its consumer must admit the verdict
and any required purchase. UNAVAILABLE exits unsuccessfully; missing measurements
are null. Text refuses unsuccessfully. These are architecture observations and
judgments, not compilation or runtime-equivalence proofs.

A native wrapper supplies `delta::Inputs`: policy authority resolutions,
source-bound before/after stage Boundary declarations, header citations and
project-parsed commit citations. Header facts carry captured file fingerprints;
commit facts carry the exact description fingerprint. Project test-support
features are explicit input data, never inferred from a feature containing
`test`; the shared cfg implication reader handles their permitted remaps.
`--test-support-features` supplies those names for generic CLI use.

The standalone command does **not** interpret project Boundary paragraphs or bead
citations. A policy requiring `stage_seams` or those authorities therefore stays
UNAVAILABLE through this command; use its owning native wrapper for a decisive
project verdict. This keeps project legislation out of the shared parser and
does not introduce a policy-plugin framework or another authored policy file.


The repository's clean Linux gate is `ferry run check --on gates --seat <seat>
--commit <commit> --timeout-seconds 1800`. It runs the same `just check` as local
validation. Timing uses a separate declared instrument and an exclusive bench
lease; the functional gate never queues a test run on the timing host.

The clean Linux recipe obtains Node 24 from the exact Nixpkgs revision in
`ferry.toml` (aligned with the development host's pinned package set). Rust is
selected by `rust-toolchain.toml`; the recipe requires Nix and Just on its host.
It does not install a global Node or skip renderer tests when Node is absent.

## Population-aware native navigation

`queries::navigation::Navigation` reads the same snapshot with an explicit
`VisibilitySelection`. Its structured `census`, `name` and `module` results
provide native tally units, the workspace/package/directory/file ladder,
definitions and hidden matches, reexports, impl sites, approximate textual
references and lexical relationship evidence. Reports include captured source
fingerprints and diagnostics. A shared file retains every package context;
module edges are attached to their own package.

```rust,ignore
use atlas_core::{discovery, population::VisibilitySelection, queries::navigation::Navigation};
let snapshot = discovery::snapshot_workspace(root)?;
let navigation = Navigation::new(&snapshot, VisibilitySelection::RestrictedOrPublic)?;
let ladder = navigation.census(None)?;
let definition = navigation.name("grade::Cost")?;
let module = navigation.module("crates/example/src/grade.rs")?;
```

The CLI selects these reports explicitly:

```sh
atlas census --root . --population pub-ish
atlas census --root . --scope atlas-core --pub-only --json
atlas name grade::Cost --root . --population all --json
atlas module src/lib.rs --root . --population pub
```

`--pub-only` aliases `--population pub`; conflicting selectors refuse. Unflagged
0.5 invocations and graph-only Rust query functions retain their existing report
contract. The native-style query's `all` includes private subjects; test modules
remain outside its population. Directly test-gated helpers retain the native
outside-test-modules boundary. Macros and line accounting do not narrow with
visibility. Native wrappers select `pub-ish` by default and format these facts
into their own established output; project FM binding and basis-key meanings
remain local enrichment.

Name fragments filter both definition and reexport sites, as in the native tool.
Relationships use the existing approximate lexical derivation, with target
visibility independent of source-body selection. Externally shadowed spellings
remain explicitly unresolved, a conservative difference from the old native
tool's omitted edge. They are never promoted to a workspace definition.
`unmentioned_names` retains the native workspace-wide spelling measure: a mention
in one package rescues that spelling everywhere. It does not identify unused
resolved definitions. Fan-in/out are written-use heuristics over retained import
sites. Malformed required imports make name/module reading unavailable.

Text bounds matches, edges and diagnostics and names omitted counts; JSON retains
all observations and ambiguity. Header text is normalized by the shared lexical
projection; the CLI supplies comment decoration. The query context shares one
lazy relationship/site cache and releases its temporary selected graph after
those observations are derived. No source files are reopened.

Capture diagnostics are available separately as `Snapshot::source_diagnostics`;
`Snapshot::graph().diagnostics` keeps the compatibility combined graph view.
Native census reports capture/membership limitations. Name/module reports add
relationship limitations at displayed sites and state the fixed lexical limits,
rather than repeating every unrelated graph warning. Repeated readings preserve
one witness per package/file/line/message. Excluded source remains visible in
all these reports. This selection does not turn unknown source into a zero.

`Navigation::dump()` supplies selected declarations, their file-derived native
homes, normalized export occurrences, and selected ordinary relationship edges.
`ExportKind` distinguishes public conventional-root facades from other reexports.
`BindingEvidence` states whether candidates matched a written qualifier suffix,
a package-local name, or a workspace name. `Resolution` retains every ambiguous
candidate and an explicit reason for a missing target. A unique lexical candidate
is not Rust name resolution; a fallback can be inspected directly in this evidence.
The source model's contextual declaration IDs are retained. Binding IDs append
an occurrence ordinal to the source declaration ID, so grouped aliases and shared
physical files remain distinct.

```sh
atlas dump --root . --population all --json
atlas dump --root . --pub-only --json
```

Native-style CLI JSON includes `schema`, `question`, selection and source basis.
The unflagged graph dump retains its existing schema. A native project wrapper
maps the typed declaration/export facts into its flat `items`/`edges` format,
legacy IDs and visibility labels; these generic facts are not recalculated there.
FM claim and stable-basis meanings remain native enrichment.

## Change-scoped declaration review

`current.declaration_diff(&prior, scope, include_relationships)` compares captured
native declaration blocks, complete attached docs/attributes and their observed
owners. Both navigators must select the same visibility population; a mismatch
refuses instead of inventing additions/removals. The report records that
population. Added, modified and removed are enum variants with exactly the
required source versions. Pure line relocation does not change content, and
same-named items in different packages or observed owners cannot cancel.

```sh
atlas diff HEAD --root . --items --graph --json
atlas diff HEAD --root . --items --pub-only
atlas module src/lib.rs --root . --items --graph
```

Unflagged diff retains its module-shape contract. `--items` defaults to all native
subjects; population controls narrow those subjects, not the line totals.
`--graph` requires `--items` and reports known incident lexical relationships
from prior/current source. An ambiguous target-only incidence is outside that
native rule. `relationships: null` means the relationship query was not requested;
an empty array means it was requested and found no known incidents. Module
`--items` and `--graph` control presentation of already-typed module facts.

The declaration block retains its inherited rustfmt-shaped boundary: an inline
function body shares its captured declaration line, while later body lines are
outside that block. Such an inline body edit can therefore be a captured
modification. This is neither semantic equivalence nor the body-normalized move
query. Full attached doc and attribute payloads are compared; internal blank
lines in a raw attribute string are preserved. Body moves and qualified contract
comparison remain their own questions.

`seams --items` includes the complete item-observation packet in text output.
JSON always retains that packet. This presentation control does not supply
missing boundary/authority evidence or promote a report to a project verdict.
The stages command does not accept this seam-only control.

Revision subprocesses isolate repository selection from inherited `GIT_*`
environment variables without changing the caller's process environment. Ordinary
Git reads verify that its reported toplevel is the requested workspace root;
jj workspaces resolve only their own explicit backing. An ancestor repository
cannot supply a revision merely because the selected source sits beneath it.


## Index API in atlas-core 0.2

The index owns correlated name/function/boundary tables. Its `functions()`,
`boundaries()`, `roots()` and `inventory()` return borrowing slices; public mutable
vectors are no longer exposed. Replace old field reads with these methods and use
`to_vec()` when a client needs its own copy. Numeric positions belong to the index
that produced them. `Target::InvalidFunction` explicitly reports an out-of-range
caller; views must refuse it rather than classify it as an unresolved source call.
Root public module paths and legacy JSON schemas remain stable.

# Extraction provenance

The scanner and relationship mechanism derive from Murail's `murail-atlas`, read
at commit `9463652fa8969b2e41201e529942307348a3cf8a` in the
[upstream Murail repository](https://github.com/flowerornament/murail).

- Original `crates/murail-atlas/src/scan.rs` SHA256:
  `bce594e1489479add0054c7679bf8943cb4d86ba7a3482062f2fb3fbda7b3f6f`.
- `crates/atlas-core/src/source/scan.rs` retains the RustSourceProjection lexical mask, item grammar,
  declaration capture, test-region predicates, module declarations, visibility
  classification and import-binding grammar. It removes the non-test LOC CLI,
  closed-basis parsing and its four tests, citation surfaces, visibility-diff and
  field-diff helpers. LOC helpers remain test-only where original boundary tests
  use them. Sixteen original scanner tests are preserved by name and assertion.
- `crates/atlas-core/src/source/discovery/mod.rs` adapts `FileMap::of`, physical `.rs` walking,
  `child_directory`, `resolve_module_path`, `ModuleMount` and `mark_test_files`
  from `crates/murail-atlas/src/atlas.rs`. It uses Cargo metadata for membership,
  includes extra target roots, classifies integration-test/benchmark roots, marks
  individually gated helpers, and diagnoses the outside-src and symlink limits.
- `crates/atlas-core/src/source/relationships.rs` adapts `build_graph`, `split_signature`, local-candidate
  preference and external import suppression. Its named evidence corresponds to
  Atlas's consumes/produces/composes classes. It preserves all candidate IDs and
  unresolved external bindings rather than reducing these to booleans. Captured
  blocks are lexically masked before co-mention matching, avoiding string/comment
  false edges. Private declarations are included, as in Atlas's `dump` population.
  Cargo target/library names and renamed internal dependencies feed import-root
  classification; grouping unknown nodes preserves exact candidate sets and
  every original relationship ID.
- Local scanner correction: the original byte walk could slice UTF-8 in the middle
  of a character while testing for character literals or ` for `. The extraction
  only slices at an actual quote or a character boundary. A UTF-8 fixture pins it.
- Integration cases adapt the original graph-test claims: parameter/return/field
  evidence, ambiguity, external-import suppression, arrows inside generic bounds,
  comments/string masking and path-mounted test ancestry/production precedence.
  They also test all-file conservation, scopes, cycles, diamonds, renderer evidence,
  label escaping, content drift and portable embedded rendering.

Murail's workspace manifest declares `MIT OR Apache-2.0`. No separate license file
or per-file copyright notice was present in the inspected source paths. The
copied portions retain that license declaration and attribution here and in
NOTICE; no unrelated Murail policy/FM/ledger/revision or rendering machinery was
copied. Newly authored code uses the same license expression. Atlas is maintained independently; upstream attribution is retained in NOTICE.

Pele is an independent MIT-licensed package, exactly `1.8.0`, with package integrity
pinned in `package-lock.json`. Its standalone bundle and full upstream license ship
in `vendor/pele/` and are embedded in the Rust executable. Node >=20 executes the
bundle without npm, a browser, network access or source checkout at runtime.
`renderer/pele.mjs` calls its public synchronous `render` API. References:
https://pele.run/api and https://github.com/obsidianmd/pele .

Architectural abstraction (girard-xs0) retains the scanner and source relationship
mechanism unchanged. A separate versioned project profile supplies declared module
boundaries/allowed dependencies; projection and display aggregate observations with
exact internal/crossing/summarized source IDs. The CLI/package was renamed to atlas.

Whole-program orientation (girard-ptp) adds authored source-call analysis to the
existing workflow AST reader. Calls, signatures, lexical bindings and import
alternatives have that parser as their one home. Cargo module namespaces reuse
the scanner's existing module-mount graph, with separate namespace and test
classification walks. The program map contracts call wrappers and retains exact
chains; it does not reinterpret allowed dependencies or lexical type mentions as
runtime order. No Murail call-graph or policy engine was copied.

Representation views (girard-96b) promote the Girard ignored contract/field/call
experiment into query-independent source facts and three native projections. No new
Murail source is copied. The existing program name/call index moves to
`crates/atlas-core/src/analysis/index/mod.rs`, shared by program and representation queries. File/module
namespaces now also support functionless type files, imported impl owners and
finite refusal of expanding reexport cycles. Existing program boundary semantics
remain at their existing classifier. Extraction preserves written type expressions,
reference/container/tuple positions and unknown observations; it does not promote
signature relationships to runtime value flow or infer ownership through containers.
The Pele subprocess and embedded bundle are shared by both rendering paths; panel
composition adds layout, not source edges. The CLI requires explicit destinations
for these figures and refuses source/query collisions before writing.

The standalone bundle (girard-43q) adds qualified file relationship projections,
literal MODULES/SURFACES adaptation and source-revision comparisons over the same
facts/name index. No further Murail scanner source is copied. Calls, signature
positions, fields and construction remain distinct evidence; lexical legacy views
retain their approximate labels. The default overview's stage selection and layout
are documented rules, not asserted runtime semantics. Source witnesses qualify the
bounded Girard admission audit; unknown resolution is not claimed absent. Historical
revisions are archived into an owned temporary tree, never checked out over source.
Girard-specific exporter/install plumbing is replaced by one pinned recipe; the
source declarations and architectural test dependency remain Girard's authority.

Source-control follow-ups (Atlas #1/#2) strengthen the existing workflow AST reader:
ordered operands, separate residual decisions, guarded match arms, loop headers and
bounded source-call occurrences. The qualified name index still owns callee lookup;
there is no third parser or alternate execution evaluator. Rust's reference defines
operand order and assignment's reversed operand order:
https://doc.rust-lang.org/reference/expressions.html#evaluation-order-of-operands
and https://doc.rust-lang.org/reference/expressions/operator-expr.html#assignment-expressions .
Unsupported type-dependent operator ordering is opaque. Source reach summaries and
control projections remain distinct, and both consume the same source body facts.

Project adapters (Atlas #4, girard-bwk) move the Girard literal declaration grammar
out of the library into the separately compiled atlas-girard executable. No further
Murail code is copied. The existing AST collector emits normalized constant syntax
with source spans; adapters consume it and the shared module mounts, never a second
Rust parser. The host validates snapshot custody and source references, not plugin
truth. ASCII/Pele accept distinct native view families through Renderer::render_view;
the graph-only entry and existing rendering helpers remain compatibility paths.
The contract and its dated clarifications are in docs/design/extensions.md.


The shared-library extraction (girard-xqe.2) moves lexical source, discovery,
relationship, navigation, architecture declaration and revision mechanisms into
`crates/atlas-core`. The application paths reexport these implementations rather
than copying them. Public lexical comment/declaration/outline interfaces preserve
the native evidence consumers' required syntax boundary; FM interpretation is
not added to the library. Native block-depth calculation adapts `max_block` from
Murail `32e218d2773b87e6635d4f3df97f9b7ccca4a65c`, retaining next-item and test-region
bounds. Source-projection regressions and extraction attribution travel with the
mechanism. This provenance statement does not qualify any project cutover.


Rich source snapshots retain the lexical projection, item outline and distinct
module/item test evidence from the same discovery pass. Native review helpers
`impl_trait`, `struct_fields` and their attribute reader derive from Murail
`32e218d2773b87e6635d4f3df97f9b7ccca4a65c`; they reuse the shared UTF-8-safe
splitters and visibility parser. Normalized restricted paths and written visibility
share that parser. Documentation audits take their selection and citation rules
from the caller; no Murail or Anneal citation vocabulary moved into the core.


Move and visibility reports adapt Murail's `atlas/moves.rs` at
`32e218d2773b87e6635d4f3df97f9b7ccca4a65c`. Native lexical keys, enclosing-item
classification, visibility pairing, token normalization and ten regression claims
travel together. File facts come from the shared snapshot; reports own structured
results rather than native text/JSON presentation. The normalizer's rerooted mode
remains explicitly approximate. It is not a compiler resolution or semantic
preservation proof.

The shared normalizer deliberately corrects inherited false-exact cases for
macro/attribute payloads, parenthesized tuple commas and impl-header boundaries.
Attached impl attributes remain part of the header reading. These differences
are pinned by regressions and the dated consolidation amendment; native cutover
acceptance must account for them rather than normalizing them out of comparison.


Qualified source extraction (girard-xqe.2) relocates the existing standalone
AST collector, source index, source-control reader and renderer-neutral diagram
projection into core. Root public paths reexport the same types; actual backends
and adapter transport remain application-owned. No further native source is
copied. The application no longer has direct syn/proc-macro2 dependencies; its
impl-owner query uses the fact type's existing source-name reading. An optional
core Clap integration preserves the facade's public enum traits while default
native consumers do not acquire a CLI dependency. This relocation does not by
itself qualify shared mount/population reconciliation or native project cutover.


Captured analysis joins the existing source/index/collector implementations over
one source snapshot and Cargo context. AST maps borrow the retained bytes rather
than copying or reopening them. Application qualified/program/expanded callers
use that path; legacy public graph conveniences retain their explicit
fingerprint check. Discovery and qualified namespaces share Cargo target path
normalization, including parent components. This is the same grammar and
resolver with a coherent input owner, not a second analysis engine.

Source-context reconciliation shares one retained physical Source across package
observations and preserves target/module/test context in discovery and indexing.
The existing standalone UseTree walk now supplies lexical and qualified imports;
test selection reads the retained source evidence rather than an AST-specific
cfg predicate. Physical reports deduplicate paths; package navigation, matching
and diagnostics keep context. These changes copy no additional native policy or
project interpretation. Native LOC and outline grammar remain independently
qualified against the frozen scanner donor.

Native stage and policy extraction (girard-xqe.2) adapts
`atlas/stages.rs`, the schema-1 data declarations in `atlas/policy.rs`, and
`atlas.rs::outside_mounts` from Murail
`32e218d2773b87e6635d4f3df97f9b7ccca4a65c`. The retained stage regression claims
travel with the mechanism. Policy data decoding accepts the native field
families, but project FM/bead interpretation stays with its owner; strings are
not validated authority. Standalone stage reports explicitly carry unresolved
references and cannot claim a complete policy verdict. This extends the earlier
extraction boundary above; it does not qualify native cutover.

Mounted source traversal shares snapshot bytes and preserves each native module
location, including outside-src test support, rather than deduplicating module
contexts by file. The native traversal is a policy population, not a replacement
for Cargo namespace resolution. Missing mounts retain their attempted paths as
typed evidence. Stage root/reader selection excludes directly gated test items;
native LOC remains separately selectable. Conditional root bindings remain
ambiguous instead of choosing the native reader's last map insertion. Captured
imports are normalized once and reused; parser failures remain explicit. These
differences require source witnesses in conformance and owner acceptance.

Native item-seam extraction (girard-xqe.2) further adapts the same pin's
`atlas/stages/seams.rs` and its boundary export inventory, writers census,
ratchets and structured result projections. Human text formatting is left to
the application/native wrapper. The project-specific `FmIndex`, chapter/claim
parsing and Boundary-paragraph interpretation are not copied into production
core; the wrapper supplies interpreted names and citation findings tied to
captured file custody. Seventeen native mechanics regressions travel with the
port; two native FM-interpretation tests remain native.

Production and all-test roots share one resolver with explicit population
selection. Test-only roots cannot enter the production module/export inventory;
the test population reads its own root aliases and native mounts. Required
unknown test/listed paths, stale/missing boundary declarations, malformed import
syntax and missing test mounts remain unavailable. The discrepancy ledger must
record differences from the native reader's silent omissions. No complete
project verdict or cutover is established by a source-only seam report.

The fixed typed-error rule (girard-xqe.2) adapts the same donor's
`atlas/typed_errors.rs` written constructor/error spelling predicate and three
regression claims. The shared implementation walks the existing retained syn AST
instead of running the donor's fixed ast-grep query and token-string projection.
It preserves production test evidence, qualification/trivia behavior and distinct
source-line units. It does not resolve type aliases, normalize raw identifier
spellings or expand macros. Invalid required syntax and excluded source become
explicit unavailable outcomes. The arbitrary authored-pattern helper is not
replaced by this fixed rule. No new parser or dependency is introduced.


Policy-input validation (girard-xqe.2) adapts the same donor's membership,
ceiling arithmetic, crate-rule and concentration configuration predicates,
authority homes and forbidden selectors. The native `policy` command validates
these inputs; measured budgets and landing ratchets remain `delta`'s question.
Generic mechanisms consume captured source and explicit helper/local-authority
observations. Native BeadId, citation and purchase interpretation stay native.
The fixed typed-error reader replaces only its original helper query; arbitrary
authored patterns still use ast-grep, against a private captured-source replica
owned by tempfile. No inspected project source is written.

The port deliberately refuses overflowing ceiling sums, uses shared path
component boundaries instead of raw prefix selection, and confines forbidden
paths to normalized workspace-relative names without following intermediate
symlinks. Authority homes use retained production evidence across physical
contexts rather than an arbitrary package placement. Missing source/helper
inputs are unavailable. These differences require source witnesses and owner
acceptance before any native cutover; the port does not relax authored policy.


Measured delta (girard-xqe.2) adapts the same donor's `atlas/policy.rs`
ceiling/raise and crate-graph laws, `atlas/concentration.rs` physical body-line,
entry, module-home, alias and header measures and path-fixed ratchets, and its
native stage-landing composition. Twenty-six native concentration mechanics
assertions travel with the port. Its bead grammar test and malformed-bead parser
assertion stay native; shared tests supply explicit interpretations of their
fixed citation inputs. Core reports are typed before the application's native
wire/display projection. The native concentration CLI remains shape-only.

The port corrects shared-path attribution: every production package owner is
retained, and a joining context cannot purchase a row for an established shared
path. It also replaces the remap exception's `contains("test")` heuristic with
the shared cfg implication reader and explicit project support-feature values.
Both corrected verdicts have controlled counterexamples; native owner acceptance
is required before cutover. Source gaps, malformed required imports and stale
header/description observations are unavailable. No FM or bead grammar is copied
into production core.

Revision-store discovery and JJ revset resolution borrow the same pin's
`GitStore`. They replace the older standalone resolver; there is one source of
repository identity. All JJ resolution is read-only, including `@`. The host
checks tracked contents plus captured contexts/dependencies against an exact
owned extraction and refuses mismatched identity. Legacy field readers remain,
while sealed allocation/object identity governs custody and cleanup. The
application uses sha2 for the native policy SHA256 wire field; source facts keep
their existing explicitly non-security fingerprint.


Nine native policy guard/raise/growth/dependency assertions also travel with the
measured delta, with its public-surface and package-membership claims exercised
at the capture boundary. Revision permission checks compare Git's executable
flag, not untracked group/other bits; a real Git fixture witnesses the distinction.


The gate wire regression uses the same pin's `xtask/src/land.rs::admit_delta`
and its original purchase/identity assertion as a test-only oracle. Type and
function visibility is widened for the fixture; the decision body is unchanged.
Actual CLI PASS/REFUSE/UNAVAILABLE and raise packets reach that function. This
qualifies its wire decision, not the native publication workflow or bead resolver.

The 2026-10-29 whole-source review corrected move matching to read normalized
imports retained by Source, replacing two reparses through the legacy convenience
that discarded parse errors. A malformed-import regression fails against the
previous implementation and now refuses both move and visibility inventories
with its source location. Package views group source observations once per
package, preserving their contexts without scanning all files for every package.

Population-aware navigation ports the native `Filter`, `Tally`, census ladder,
name-card subject/reference/reexport/impl selection and module fan/orphan measures
from Murail `32e218d2773b87e6635d4f3df97f9b7ccca4a65c` into snapshot-backed typed
reports. The donor fixture and its census/name/module claims travel through the
public Cargo snapshot boundary. The existing relationship derivation is shared,
parameterizing target selection and declaration-mention collection; no second
lexical resolver is installed. The native spelling-wide orphan unit is called
`unmentioned_names` in the shared report and documented with a two-package
witness. External shadowing retains the shared model's unresolved observation
rather than the native graph's suppressed edge; a controlled witness and the
dated discrepancy require owner acceptance before native cutover. Native FM
binding/basis parsing is not copied. Compatibility graph queries stay unchanged.

Pinned native navigation comparison exposed a test-ancestry correction at
`murail-codegen/src/jit.rs` (cfg-test followed by a multiline lint attribute):
its `jit/tests.rs` is correctly excluded by the shared grammar. The native census
included 75 non-test / 50 token-bearing lines. The dated discrepancy and minimal
snapshot law retain that correction for owner review. Diagnostic witness keys
now include package context and deduplicate across repeated derivations. Source
capture diagnostics are retained before relationships are derived; query reports
use that applicability instead of copying unrelated graph warnings.

The native export/dump query adapts `dump`, its declaration catalog, normalized
qualified suffix, unique-local preference and facade rule from Murail
`32e218d2773b87e6635d4f3df97f9b7ccca4a65c`. The donor dump/facade claims travel
through the public snapshot API with grouped-alias, missing/ambiguous-target,
lookup-priority and shared-context cases. Local preference is one shared lexical
routine, also read by ordinary co-mentions. Candidate lookup reports its actual
suffix/package/workspace evidence instead of claiming Rust resolution. Native
flat JSON serialization remains an adapter boundary. No second import grammar,
FM binding interpreter or source reader was added.

Change-scoped declaration review adapts the fixed inventory/content pairing and
known incident-edge rule from `atlas/declaration_diff.rs` at Murail
`32e218d2773b87e6635d4f3df97f9b7ccca4a65c`. Its two regression claims run through
real captured revisions with typed output. Package/observed-owner keys prevent
cross-context cancellation under R4. Review also found the donor's first-line
summary omits later doc edits; the shared comparison includes the full attached
prefix and attributes, preserving raw payload blank lines. A same-population
precondition protects public API callers. Existing impl-header token syntax,
source IDs and relationship derivation are reused. The inherited inline-function
capture boundary is explicitly pinned as a limitation, not normalized away.


Responsibility-based organization (girard-xqe.8) changes physical homes, not donor
semantics. Capture/lexical evidence live under core source/, qualified observations
under analysis/, and navigation/review/audits under queries/. Existing root public
paths are reexports of those same definitions. The application groups private
commands/output under cli/, selected models under views/, and all native
presentations under render/. Stage path syntax, root inventories and interface
checks are separate private modules; original regression examples remain intact.
The move map and execution contracts are documented in docs/architecture.md.
Older extraction accounts retain historical filenames where they describe the
migration at that date. No additional Murail/Anneal source was copied in this move.

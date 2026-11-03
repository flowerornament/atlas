# One Rust grammar for source facts

Registered 2026-11-03, before implementation. Bead: girard-wt6s.
Baseline: ce2861511da87494f50db1f0f2f2755c451914c2 (Atlas 0.7.0,
atlas-core 0.2.0). The user authorized the census and implementation.

## Model / Replaces / Adds

Source observations describe written Rust, not expanded code, resolved HIR or
runtime behavior. One grammar must determine written item boundaries, nesting,
visibility, imports and attributes. Population and presentation are separate
policies over those facts.

Replace the inherited rustfmt-shaped structural grammar in source/scan.rs,
including bracket-counted item ends, keyword classification, visibility/field/
impl parsing, and line-based cfg/module discovery. Keep the raw text mask and
its explicit comment/line metrics. Add syn-derived portable source facts and
explicit syntax failure; retain Analysis's local AST cache.

## Census and final ownership

The census read changed workflows from capture through their consumers, and
searched structural helper references across core and application source.
The 70 matching source lines include definitions and tests, not 70 consumers.

| Current owner and readers | Observation | Final home |
| --- | --- | --- |
| source/scan.rs; Source::of | handwritten item kinds, ends, indentation-based top level | source/syntax/: syn item walk and byte/line witnesses |
| scan.rs; Source; discovery and mounted.rs | module declarations, path attrs, cfg-test ancestry | same syn walk; discovery retains membership policy |
| scan.rs; population; native Workspace | test module ranges versus individually gated items | same syn walk; population retains the distinction |
| scan.rs; moves/mod.rs | fields, impl trait, restricted visibility | captured syntax facts, or syn fragment parsing for compatibility helpers |
| scan.rs; Source; workflow/facts.rs | UseTree flattening already uses syn | one shared UseTree routine; no second import parser |
| scan.rs; concentration/measure.rs | attributes parsed by syn, cfg payload split as text | syn Meta tree; conservative implication policy retained |
| scan.rs; review/navigation/documentation/checks | exact text, comments, masked lines and native counts | source lexical projection, with no Rust item grammar |
| analysis/index; representations; workflow; typed_errors | full syn AST cached per Analysis | stays local; reads the same capture/population facts |
| checks/stages paths and roots | native textual namespace/path reading | stays native policy, supplied by canonical item/import facts |
| discovery; relationships; review/navigation | declarations and lexical co-mentions | same report contracts, fed by canonical source facts |

Snapshot currently retains Arc<Source>; Analysis's OnceCell<syn::File> is local.
The locked syn 3.0.9/proc-macro2 1.0.107 AST is not Send/Sync. Keep Source and
Snapshot portable. Parse to owned facts at capture and drop that AST; qualified
analysis can parse the retained bytes once into its existing local cache. This
is one grammar with up to two parsing passes, not a claim of one parse. A shared
thread-local cache, Rc conversion or synchronization wrapper is unnecessary.

Source is successfully parsed source. Its fallible constructor returns a typed
syntax error with a location. Discovery retains failed bytes separately from
successful Source facts, fingerprints the failed file, and records a diagnostic.
Snapshot::source continues to return captured bytes; source_facts returns no
facts for an invalid file. Native reports propagate required-source unavailability
instead of fabricating zero counts or a PASS. Typed-error checks retain a Syntax
outcome. Discovery owns retained failures. Consumers apply their package/file/test
selection before translating a failure into their existing error/unavailable
outcome; Analysis must inspect retained failures instead of silently omitting
selected invalid files. Known test-only membership can exclude a failed child.
When a declaring parent's syntax fails, missing ancestry is unknown: no descendant
gets an inferred test exemption. Witness all three: invalid production child,
known test-only child, and declaring parent.

The ordinary-comment policy stays line-based: consecutive preceding // comments
attach unless separated by a blank; outer docs/attributes attach across blanks.
A previous item's ending line stops attachment, so two same-line items never
steal one another's comments. Byte spans identify the syntax separately from
attached line ranges. Preserve the existing native top-level external-mount
population. Nested external module sites are observed by the AST but are an
explicit unsupported native mount where required; do not flatten their parent
namespace or silently invent a physical path.

## Requirements

- **R1 Must:** every written structural fact above comes from syn, including
  same-line items, raw identifiers, multiline attributes and nested declarations.
  No handwritten keyword/bracket structural scanner or parse-failure fallback
  remains. Statement/expression macro bodies stay opaque; their written macro
  site may be reported, never declarations invented from their tokens.
- **R2 Must:** source slices come from original bytes and accurate spans, never
  token pretty-printing. Item spans retain attached ordinary comments/docs and
  attributes. The existing first-/// summary and lexical //! header policies
  remain explicit. File top-level means actual nesting, replacing column zero.
- **R3 Must:** test implication is conservative: test/declared test-only features,
  all/any and #[test] retain their existing policy; no host cfg evaluation.
  Module exclusion and individually gated helpers remain different accounting
  populations. Inherited module mounts continue to merge package observations.
- **R4 Must:** invalid or unsupported required syntax has a source witness and
  an error/unavailable result. No success-shaped empty structural inventory.
  Non-exhaustive/Verbatim items are handled explicitly. Missing facts do not
  erase the file's captured identity or silently remove it from required checks.
- **R5 Must:** multiple imports/items on a line remain distinct. Import consumers
  retain code outside each import's exact byte range. Existing declaration IDs
  remain stable when unique; physical-site collisions receive a byte suffix.
  Consumers whose line-granular policy cannot distinguish a compact boundary
  must explicitly refuse that unsupported reading rather than misattribute it.
- **R6 Must:** report populations, stable wire tags, refusal/unknown distinctions,
  output safety, renderer and project policy authority remain intact. A changed
  count or edge is read against source and classified as intended syntax coverage,
  an explicit limitation, or a defect. No parity-driven weakening of syntax.
- **R7 Must:** root ARCHITECTURE.md is the single canonical system reading map,
  responsibilities and enforceable invariants. Move the existing guide and fix
  incoming links. Keep shipped architecture truthful; this plan owns proposed work.
- **R8 Must:** no new dependency or toolchain/lint weakening. Public source helper
  signatures may become fallible where malformed Rust formerly looked empty;
  document the atlas-core 0.3 migration. CLI flags and existing wire schemas stay.

## Implementation and qualification

Three checkable outcomes under this epic: register this account and architecture
map; replace the structural grammar coherently; qualify and publish the tool.
Native pin adoption is a separate owner decision after product receipts.
Do not create one slice per helper or preserve the old parser as a compatibility
fallback. Compatibility module paths can reexport the canonical implementation.

Boundary tests cover: formatting-independent items/nesting, same-line imports
plus other code, source spans with Unicode/raw strings/comments, associated and
local declarations, fields and visibility, cfg all/any nesting/multiline attrs,
path-mounted tests, opaque macros, BOM/shebang, and malformed/unsupported syntax.
Assertions check facts and native/qualified consumers through Snapshot, not just
private helpers. Mutating a boundary/visibility/gate must make its law fail.
Keep existing regression examples; adjust assertions only for declared semantic
changes, with source-based explanations in the result record.

Run the existing 58-case CLI qualification corpus, whole-source census on Atlas,
Girard and fixed native inputs, plus the accepted Anneal wrapper/product fixtures.
Keep baseline receipts immutable. Compare capture and qualified-command costs on
identical source; label local measurements as local. Report before/after production
size, deleted structural helpers, mismatches, checks not run, and required native
owner work. Prediction: production core stays within +1,000 nonblank/comment/test
lines; replacing grammar should delete more than it adds. Counts are evidence,
never a reason to compress or omit tests.

Run just check, just check-install and just verify-vendor before publication.
Qualified and native stage tests must still expose missing evidence. A fresh
spec reader challenges this account before code; a broad simplify review and
independent final review cover the whole changed workflows, not only new files.

## Sources and decisions

Read the locked syn source and primary docs: [syn 3.0.9](https://docs.rs/syn/3.0.9/syn/),
[Visit](https://docs.rs/syn/3.0.9/syn/visit/index.html),
[Attribute](https://docs.rs/syn/3.0.9/syn/struct.Attribute.html),
[Macro](https://docs.rs/syn/3.0.9/syn/struct.Macro.html), and
[Span](https://docs.rs/proc-macro2/1.0.107/proc_macro2/struct.Span.html).
Outside procedural macros, span-locations provides stable source byte locations;
parse_file accepts BOM/shebang. Macro tokens are not macro expansion.

The user's [Rust guide](https://github.com/lukehutch/skills/blob/main/WRITING_GOOD_RUST.md)
informs boundary types, locked-API verification, ownership and executable gates.
It does not license weakening existing lints, adding error dependencies or
introducing generic frameworks. The research-graph decision screen found no
unsettled general proposition: actual source ownership and the locked syn API
settle this grammar replacement, so a literature search would add no evidence.

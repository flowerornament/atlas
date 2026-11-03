# Structural syntax qualification

2026-11-03. Bead: girard-wt6s. Baseline: Atlas 0.7.0 at
ce2861511da87494f50db1f0f2f2755c451914c2. Implements the
[registered census and plan](2026-11-03-syn-source-facts.md); the current
ownership and invariants live in [ARCHITECTURE.md](../../ARCHITECTURE.md).

## Result against the plan

- The handwritten item, bracket-boundary, visibility, field, impl-header and
  cfg-payload grammars are deleted. syn owns structural facts. Existing UseTree
  flattening is shared with qualified analysis. Native path matching, lexical
  masking and token-based move normalization keep their distinct evidence limits.
- Source captures portable facts; Analysis keeps its local AST cache. Malformed
  or unsupported source retains its bytes, fingerprint and typed syntax failure.
  Required missing facts cannot become zero measurements or a complete PASS.
  Known test-only failures stay outside production selection; failed parents do
  not invent test ancestry. Macro payloads remain opaque, including associated
  macro sites in impls, traits and extern blocks.
- Same-line imports and declarations retain separate byte positions. Stage paths
  keep code around imports. Native site joins no longer overwrite a struct with
  its same-line impl or collapse identical export spellings. Existing IDs remain
  unchanged unless physical sites collide; those receive a byte suffix.
- The one system guide moved to root ARCHITECTURE.md, incoming links were fixed,
  and the file is included in the application package. The source API migration
  is documented in docs/core.md: Atlas 0.8.0, atlas-core 0.3.0. No dependency,
  toolchain, lint, native policy, renderer/vendor or inspected-project change.

## Observed differences and corrections

- The 58-case CLI/ASCII/Pele qualification ran against the fixed owned fixture.
  56 cases were byte-identical. Two JSON reports (`dump` and directory `module`)
  now capture inline function signatures only through the body opener. The same
  body is excluded in multiline source: formatting no longer changes what counts
  as an interface edit. Exit statuses and diagrams were unchanged. The byte
  comparison correctly exits 1 for these two differences; this is not a claim
  that the unmodified parity assertion passed.
- Fixed Atlas source (ce286151) and Girard source (839aa00f) produced identical
  unqualified census reports with both tools. Qualified dependency JSON was
  byte-identical on those sources and on Anneal's fixed 2b8e3ba4 source.
- Anneal's graph inventory loses 12 template-body entries: seven functions and
  five impls inside string_id!/index_id! definitions, which have no independently
  named Rust declaration site. Four accompanying unresolved-Self diagnostics
  disappear; none is added. Generated expansions are not inferred in either
  direction. The graph declaration count is 3,842 -> 3,830.
- The owned Anneal 0.3 API port passes its 21 wrapper tests. Of 40 frozen wrapper
  cases, 30 are byte-identical; ten census/dump/comments cases reflect the same
  macro-template correction, including text/public variants. The default pub-ish
  census loses four functions; the public census loses two. The byte-comparison
  assertion exits 1 and its changed cases are retained, not blessed away.
- Anneal's six own/Murail handle, edge and span product packets remain identical
  with successful exits on both sides. This qualifies those packets, not native
  publication or adoption. Existing consumer pins remain unchanged; owners must
  accept the documented source-observation changes before advancing them.
- An intermediate implementation excluded five extra native lines by extending
  test-module regions into attached comments. That was corrected: native ranges
  begin at the test gate. Native line totals and concentration packets now match.
  Attachment spans still retain full comments/docs for source review.
- syn::parse_file strips script preambles before assigning byte coordinates. A
  law exposed a BOM slice panic. The shared parser now uses syn to recognize the
  preamble and reparses its blanked prefix, preserving original byte positions.
- A new blanket rejection of same-line move items was removed: byte-contained
  bodies and parents allow the existing compact-source move tests to remain valid.
  Malformed required Rust now refuses structural review instead of returning a
  success-shaped partial inventory. Original malformed examples remain as refusal
  cases; a previously invalid file-scope statement is also tested explicitly.

## Verification and review

- Nine boundary laws exercise compact/Unicode source, raw identifiers, imports
  surrounded by code, precise cfg sites, required/test-only/failed-parent captures,
  opaque module/associated macros, BOM/shebang positions, Send+Sync capture,
  distinct same-line IDs/exports, and native gate-start accounting.
- Controlled visibility, exact-gate and import knockouts each changed a passing
  boundary test into a failing test (exit 101), in a separate owned desk. Sources
  were restored after every acquisition. Neither production nor oracle assertions
  were weakened to obtain green results.
- Full workspace tests and Clippy pass. The broader check includes no-default-
  features tests/Clippy, warning-free Rust docs and renderer tests. Installed-binary
  smoke and vendor verification pass. The normally ignored JJ revision-custody
  test was run explicitly and passes.
- The cold plan reader required failure selection and unknown ancestry to be
  specified before code. Three broad simplify reviews covered the full source/
  native/qualified workflows. Their useful efficiency correction removed a joined
  full-file copy; the import-masked view is now lazy. An independent executable
  review at 55b6b836 reproduced the patch hash, passed just check, and challenged
  Unicode/CRLF imports, same-line sites and malformed selected populations.
- Gates check at 55b6b836: Ferry 1793702858045-e30dd9a1480ee350f22cafc401284f9a,
  final ran-pass, exit 0, uncapped output, no surviving processes. Final publication
  and any later code-review/gate receipts are recorded on girard-wt6s.

## Size and cost

- Rustfmt-default, nonblank masked production lines, excluding all test-gated
  source: core 15,738 -> 16,064 (+326, 2.07%); application 6,771 -> 6,771.
  Combined 22,509 -> 22,835 (+1.45%). A small ignored consumer of captured Source
  facts is the instrument; source trees use the default formatter configuration.
- By core owner: source +272, analysis +20, checks +29, queries +5. The registered
  ceiling of +1,000 production lines is met. The hoped-for net deletion is not:
  explicit portable syntax facts and failure custody cost more than the removed
  approximation. Do not describe the overall change as a LOC reduction. The
  deleted mechanism is the second structural grammar.
- Local Mac17,8 / Apple M5 Pro CLI wall time, matched release builds on Rust
  1.99.1, three alternating pairs,
  excluding builds and including Cargo metadata/JSON: census medians in ms are
  Atlas 105 -> 170, Girard 116 -> 206, Anneal 163 -> 325; qualified dependency
  medians are 374 -> 454, 370 -> 458, and 629 -> 791 respectively. These loaded,
  small local samples show added startup cost in these workloads. They are not
  quiet benchmark or causal performance evidence. The deliberate second parse
  remains; no global AST cache was added to erase the figure.
- The first local cost acquisition used a debug baseline against release candidate
  and was incomparable. Its raw receipt remains as an excluded trial; the figures
  above use the release-pinned baseline and release candidate. No bench timing or
  general performance claim is made.

Evidence is consolidated under ignored target/atlas/syntax/: baseline/candidate
CLI packets, corpus outputs and hashes, native wrapper/product comparisons,
knockouts, independent review fixtures, source-line instrument, local/gates logs
and install/vendor receipts. Previous qualification receipts were not overwritten.

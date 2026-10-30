# Shared Atlas qualification, 2026-10-29

This record accompanies [the consolidation design](consolidation.md), its
[pinned consumer inventory](consolidation-inventory.md), and the
[cost protocol](2026-10-29-consolidation-costs.md). Work and acceptance belong to
Girard beads girard-xqe.2 and girard-xqe.5. It qualifies the shared library and
standalone consumer boundary. Murail and Anneal retain their native tools until
their separate owner-accepted cutovers. It does not establish interchangeable
standalone and native JSON schemas or a complete native gate PASS.

## Source and interface evidence

The inspected native implementation and project source are Murail
`32e218d2773b87e6635d4f3df97f9b7ccca4a65c`. Shared implementation evidence below
uses Atlas `163f70d166dd9305abfecdb73a192ca38076f9f3`, Rust 1.99.1. The independent
caller disables core's default features and retains native FM/bead/header/feature
interpretation outside core. It captures both revisions and verifies candidate
source and policy custody before judgment.

- **Census.** All selected subject counts agree across 27 packages. One documented
  ancestry correction changes `murail-codegen` from 21,829 to 21,754 non-test lines
  and from 18,539 to 18,489 token-lines. The 75/50 difference is exactly
  `crates/murail-codegen/src/jit/tests.rs`: multiline attributes between
  `#[cfg(test)]` and the module declaration caused the donor to lose test ancestry.
  A controlled snapshot witness pins the retained correction. No budget changed.
- **Spans.** All 108 native records agree on name, kind, declaration line and
  attached endpoints. The native formatter nests records and folds function-body
  items; the standalone outline exposes 194 flat records, including 86 macro
  sites folded by that formatter. The comparison removes only documented kind
  labels and the displayed macro-name `!`; it compares actual boundaries.
  Native legacy formatting remains its wrapper's responsibility. Span observations
  include tests; line and block metrics use their separate test exclusion.
- **Measured delta.** Exact basis, the complete concentration summary, every
  package's public surface, and core totals agree: 144,029 token-lines and
  1,108,534 tokens. Policy-input validation and raises have no violations or missing
  local authority in the independent caller.
- **Required root glob.** Shared delta reports two unavailable fields for
  `crates/murail-compiler/src/tests.rs`, one per revision: required `super::*` over
  the crate root cannot be ranked. Native omits that limitation and reports PASS.
  Shared output has no seam summary for that unavailable reading. The caller
  exits 1; this is not a successful full gate or equivalent workload. Resolving or
  accepting the difference belongs to the native cutover owner.
- **Other retained differences.** External-name shadowing remains explicitly
  unresolved rather than silently suppressed. Declaration review keys include
  package and observed owner, compare complete attached docs/attributes, and
  reject mismatched visibility populations. These conservative observations have
  controlled witnesses and are specified beside their definitions; native owners
  must accept their effect before cutover. Inline bodies on signature-ending
  lines remain a stated capture limit.

Raw inputs, commands, exits, source fingerprints, archive/binary hashes and outputs
are consolidated under ignored `target/atlas/consolidation/`. The independent
native SDK is an evidence instrument, not a production fallback. Native wrapper
serialization and whole-project migration checks remain part of the native
cutovers, rather than an inferred consequence of these matching observations.

## Costs

The exclusive Linux bench receipt is
`1793278119608-d28a6449847191e141e835f51045f02e`: ran-pass, 154.718 s total,
artifacts fetched, no surviving processes. The instrument commit is
`a0b62f17` in the ignored qualification harness; its receipt records the full
identity and both exact shipped repositories. GNU Time from the registered pinned
Nixpkgs package measured process-tree elapsed/user/system time and maximum RSS.
Query figures exclude compilation and include startup, metadata, capture,
analysis, serialization and writing. Five paired samples used the registered
alternating order, with no discarded warm-up. RSS is GNU Time's maximum, not a
sum of concurrent children.

| Query | Native wall median | Shared wall median | Native peak RSS median | Shared peak RSS median |
| --- | ---: | ---: | ---: | ---: |
| census | 0.39 s | 0.57 s | 107,220 KiB | 155,640 KiB |
| spans | 0.34 s | 0.56 s | 100,272 KiB | 155,672 KiB |
| delta envelope | 1.79 s | 4.30 s | 144,280 KiB | 440,408 KiB |

Census and spans regress in time and peak memory. Their user/system CPU medians
are respectively 0.32/0.07 versus 0.48/0.08 s, and 0.28/0.05 versus 0.48/0.07 s.
The delta row is only a raw envelope: native completes successfully, whereas all
five shared invocations exit 1 with the exact two required-evidence gaps above.
Its user/system medians are 1.89/0.13 versus 3.87/0.58 s. There is no successful
shared gate cost or like-for-like full-gate speed claim. The shared census also
includes source identities and diagnostics; the spans formats retain different
presentation populations. No causal attribution is established by these samples.

Builds used dedicated empty target directories and fetched dependencies before
the clock, each repository's own release profile, then a no-op build and a
specified comment appended only to its owned entry source. These are individual
samples, not build-time distributions.

| Build envelope | Fresh wall | No-op wall | Comment rebuild wall | Fresh peak RSS |
| --- | ---: | ---: | ---: | ---: |
| Native tool | 36.14 s | 0.08 s | 13.72 s | 654,456 KiB |
| Standalone Atlas | 17.44 s | 0.03 s | 2.62 s | 1,112,520 KiB |
| Independent core caller | 14.63 s | 0.03 s | 0.76 s | 1,142,296 KiB |

Fresh user/system CPU was 60.91/3.70 s, 139.25/5.23 s and 99.67/3.85 s,
respectively. Lower wall build time does not imply lower CPU or peak memory.
Native product/render dependencies, standalone CLI/render dependencies and the
independent default-disabled core caller are distinct envelopes. Raw resource
records, manifests, every sample, output byte counts/hashes, source-comment hashes
and compiler/host identities are retained with the receipt. These figures describe
`163f70d1`; the later alias-warning, documentation and 0.6 package-version repairs were not re-timed.

The first smoke failed before measurements because `/usr/bin/time` was absent;
the dated protocol amendment records the pinned replacement. Corrected gates
smoke `1793277882545-e9e34a9ae43ba910e9058553bbaae3d3` passed, artifacts fetched,
no survivors. The final instrument additionally checks the exact unavailable
fields/reasons and absence of violations; a different two-gap packet was refused
by a controlled local check before timing. No timing threshold was registered.
The measured regressions must accompany a native owner's cutover decision.

## Diagram preservation and fresh readers

Published Atlas `71ac882976e5780a92df72424c00cd83572ad9f5` and shared `163f70d1`
ran on the same Girard source `e9e103820a5036513852f8defed2d0ad30f0fa1b`, with
comparison against `57d45f8036c4466fd8b80a7e6ff785ee6af4c77f`. Overview,
Graph-producers and complete comparison SVGs are byte-identical. Comparison JSON
is identical; overview/producers differ only in two copies of the registered
wording about test-implying gates. All underlying observations agree. Existing
positional-root invocations still work. The inspected source remains unchanged.

A fresh proxy reader viewed PNGs first, then JSON and pinned source:

- **Construction.** The figure distinguishes seven Graph returners from the
  literal in calibration and explicit defaults in calibration and elaboration.
  Explaining normal compilation's builder mutation and `mem::take` transfer still
  requires source. This is an explicit observational limit, not inferred freshness.
- **Admission.** The overview communicates borrowed Checked and owned Plan,
  `Ok -> AdmittedPlan`, and `Err -> Box<Refused>`. The exact signature is in
  evidence. Refusal contents and success witnesses require further inspection.
- **Change.** Nine removed observations all belong to `xtask/src/profile.rs`.
  The figure warns that several rows can describe one source edit and does not
  infer replacement behavior. A plain project label would make the before/after
  source-revision context easier to recognize.

Producers was the easiest picture to read. Overview remains tall, with small text
and repeated refusal boxes; comparison exposes raw wrapper notation. The reader
found no false claim. This is proxy qualification, not user visual acceptance;
readable whole-program execution abstraction remains outside this migration.

A separate fresh read of Atlas's code and shipped docs traced overview through
capture, Analysis, adapter custody, projection, renderer/output and exit 1 on
failure. It traced stages/seams' report-only/not-applicable/unavailable outcomes
separately from policy/delta judgments, and located the canonical lexical grammar,
retained source facts and AST/workflow caches. Legibility was rated 7/10: overview
is direct; stage/seam authority and ratchet tracing needs more file hopping.

## Whole-code review dispositions

- **Fix: source reuse and cost.** Move keying now reads retained normalized
  imports rather than reparsing through an error-erasing helper. Malformed required
  imports refuse with a location. Workspace files are grouped once per package,
  in native Path order, rather than filtering all files for every package.
- **Fix: diagnostic identity and relevance.** Capture diagnostics are retained
  separately; native query cards add relationship limitations only at relevant
  sites. Repeated derivation preserves package/file/line/message witnesses without
  merging shared physical files' package contexts.
- **Fix: consumer completeness.** Native population-aware navigation, export
  candidates and declaration/incident review now have typed core results. Wrappers
  need not recreate those mechanisms. Complete attachment comparison and matching
  population guards have red-before/green-after witnesses.
- **Fix: public navigation.** Every operational command's help describes question,
  population, scope, evidence, limits and an example. The generated command list
  remains, with a grouped index including Checks. The deprecated related warning
  is emitted once at the command boundary for both populations; its JSON still
  matches the canonical name query. The shipped self-query and README now point
  at Diagram and projection's actual core files. The documented query failed
  before with zero Diagram declarations and succeeds after the repair.
- **Keep: evidence and authority seams.** Lexical navigation, qualified AST
  relationships and authored architecture declarations answer different questions.
  Compatibility modules delegate; they are not second grammars. Project authority
  stays outside core; renderers consume projections and inspect no source.
- **Keep: model ownership.** Snapshot owns source bytes and lexical facts;
  SourceGraph describes package/file/declaration relationships; Analysis shares
  lazy per-physical-file ASTs and workflow readings; Facts are qualified source
  observations; Diagram is a presentation projection. Native tables and comparison
  rows retain their distinct view models. No cosmetic line-count split was made.

Three broad review lenses plus the fresh reader found no other release-blocking
finding or decision in the shared boundary. This does not approve native product
cutovers, resolve the root glob, or erase the cost and visual limits above.

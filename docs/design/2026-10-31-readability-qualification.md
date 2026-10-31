# Readability qualification and consumer boundary

Outcome of `girard-xqe.11`, 2026-10-31. Baseline: `6cf8bbcb`.
The exact published revision, diff digest and consumer receipts are on the bead.
Atlas is version 0.7.0; atlas-core is 0.2.0. The minor-version boundary marks the
intentional Rust Index field/Target enum change; CLI commands and root public
module paths are retained. Cargo.lock changes only the two workspace versions.

## Model / Replaces / Adds

The shipped responsibility map is the implementation's reading guide: captured
source and grammar, qualified analysis, developer queries, native checks,
revision custody, selected views, rendering and protected publication. Project
interpretation and authority remain outside the generic library. This outcome
qualifies that whole map and completes the independent review response, rather
than treating moved files or passing tests as proof of understandable design.

It replaces remaining dense construction, misleading comments/examples, a
Debug-based relation grouping key and an unused pretty-serialization pass.
Dependency grouping uses the enum with an explicit historical ordering, retaining
wire/diagram order and CLI enum order. Default ASCII stdout returns before pretty
JSON evidence serialization; caller JSON values remain materialized. No benchmark
or elimination of all serialization is claimed.

One bounded behavior repair emerged from exercising the README: comparison
validated every stable source identity before applying a saved question. An
unrelated cfg-specific helper could therefore refuse an otherwise unambiguous
contract question. Validation now covers selected types and every function
contributing input/output, literal/default construction or preparation observations.
Related ambiguity still refuses. This does not evaluate cfg conditions or choose
a duplicate match. This is the sole behavior repair beyond the original findings
in this final qualification; it is recorded before implementation on the bead.

No new analyzer, command family, diagram feature, plugin framework, formal-model
integration or dependency is introduced. Native Murail/Anneal policy and pins do
not change. The release boundary does not authorize a native cutover.

## Independent review dispositions

- **1, source overwrite:** historically repaired at `eaaf7c06`; the current shared
  publication policy and source/query/adapter/companion/symlink/hardlink witnesses
  remain green. The reorganization did not reinstall separate protection rules.
- **2, mutable Index:** repaired at `bf050704`; borrowing slices protect correlated
  storage, invalid positions are explicit and views refuse them. Positions remain
  local to an index; documentation does not promise cross-index numeric identity.
- **3, hidden root walk state:** repaired at `bf050704`; named construction phases
  produce immutable facts and each read owns its cycle state. Native textual
  reading and qualified resolution retain their distinct contracts.
- **4, Debug declaration meaning:** repaired at `bf050704`; scanner enums define
  checked stable wire conversion. Legacy strings, including unknown tags, remain
  round-trippable. This outcome also removes Debug from dependency grouping;
  existing diagnostic/presentation text is intentionally retained.
- **5, dense Rust/macros:** repaired across layout and final qualification. The
  whole-tree reader found further constructors/maps missed by the earlier pass.
  Each is manually formatted and inspected; rustfmt alone accepted visibly bad
  macro-adjacent formatting and did not establish this disposition.
- **6, scattered CLI ownership:** repaired at `3598a56c`; commands own options and
  coordination, dispatch is singular and compatibility routing is explicit.
  Different output and evidence families are preserved rather than merged.
- **7, fixture lifetime:** repaired at `bf050704`; each package uses TempDir
  ownership, including inline native seam/move examples. Examples/assertions stay
  local. A barrier establishes overlapping live directories rather than relying
  on a lazily serial spawn/join iterator.
- **Renderer issue 2:** repaired at `6cf8bbcb`; capped serialization, concurrent
  bounded pipes and direct-child cleanup are covered by hostile-I/O and large
  positive controls. This qualification adds a fake-Node CLI request-cap case
  proving the cap refuses before Node starts, preserving source/profile bytes.
  Existing failed-render source/diagram/Mermaid evidence remains distinct from a
  completed view artifact. No CPU/wall deadline or descendant manager is claimed.

## Cold read and whole-system challenge

A fresh reader, given only shipped documentation and source, finishes within the
registered 20-minute ceiling. It locates SourceGraph construction in discovery,
traces overview from CLI/capture through adapter custody, projection, renderer,
publication and main's unsuccessful exit, and traces stages/seams through their
actual native populations, root reading, findings and unavailable authority.
It locates canonical import/test grammar and Analysis/ParsedSource cache ownership.
The account is checked against those source entries; it is navigation evidence,
not human acceptance or exhaustive verification of every parser branch.

The reader finds misleading blanket scope prose, an outdated renderer header,
a preparation example selecting main after main became a dispatcher, terse
overview names and a less visible cache home. These are corrected. A short
architecture paragraph distinguishes lexical, qualified and native evidence.
A follow-up verifies the corrected account against the current tree.

Reuse and efficiency readers challenge the whole source/capture, cache, query,
check, CLI, view, transport and publication surfaces. They find no warranted
additional mechanism merger. In particular, raw depth-zero workflow reading
intentionally accepts explicit bodies, including tests; it publishes that own
workflow evidence and protects its anchors. Expanded flow uses captured qualified
analysis. No custody defect was demonstrated, and no new API is introduced merely
to erase those distinct selections.

## Verification

- Full Rust 1.99.1 `just check`, `just check-install` and `just verify-vendor` pass:
  warnings-denied workspace/core checks, no-default-feature core, independent
  public clients/adapters, rustdoc, Node and installed-binary controls.
- Fixed 58-case CLI/help/JSON/refusal/SVG comparison against the preceding release
  is exact. Package-version output changes intentionally; no other comparison
  fields are normalized or excluded.
- The accepted Anneal wrapper at `20e52661` is rebuilt in an owned copy with only
  the atlas-core path/version redirected. Its 21 tests pass; all 40 fixed native
  command cases match, and six full-field own/Murail handle/edge/span product
  comparisons match without exclusions. Native source, pin and accepted receipts
  are unchanged. This does not qualify a new Anneal Nix/release pin.
- Sixteen direct fixture-suite invocations pass with four measured overlapping
  test processes; each suite also holds four directories alive at a barrier.
  These are lifecycle/uniqueness observations, not a timing benchmark.
- Comparison boundary tests accept unrelated duplicate identities while retaining
  actual changed contracts. Ambiguous selected contracts, literal constructors
  and default constructors refuse. Restoring the global function-identity guard
  turns the positive boundary test red; the knockout is restored. Independent
  review checks the complete identity-use set and reruns both tests.
- README name, entry-flow, saved-question SVG and revision review examples are
  exercised. Git refs and jj revsets are documented separately. The self-query
  comparison now succeeds after the scoping repair; the source diagram remains
  a generated ignored artifact, not a committed gallery or authored edge map.

Navigation counts, hotspot units and the exact command/digest ledger accompany
this outcome under ignored `target/atlas/qualification/`. Source counts include
comments and inline tests; native non-test-module counts retain individually gated
helpers by contract. Neither is neutral LOC or an elegance score. Largest
functions/files are reading locations, not orders to split by size.

The work adds no runtime/compiler behavior to an inspected project. Girard's
external tool-pin adoption is a separate ordinary gated landing; Murail's native
cutover remains owner-reviewed. Human evaluation of the diagrams and readability remains the user's judgment.


The source-only as-written count is 28,618 lines across 110 Rust files, versus
27,770 across 105 before this repair chain. Comments and inline tests are included.
Native outside-test-module physical lines are 7,071 application plus 17,202 core
(24,273 total); the difference is 4,345 excluded inline/test-file lines. Separate
integration/support tests contain 8,398 as-written lines across 34 files. These
units are explicitly not pure production or neutral LOC.

The largest raw source file is the canonical scanner (1,963 lines, with its inline
regressions), followed by move review (1,363). Several next-largest files are
separate test modules. The largest lexical function spans are representation
collection (258 declaration-to-end lines), expression reading (235), source
capture (229), index construction (204) and representation projection (194).
They retain identifiable contracts; no file is split solely to reduce a count.

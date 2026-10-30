# One Atlas: source, review, checks and diagrams

Technical migration design, 2026-10-27. Readers: implementers, project owners and reviewers. Work and authorization: **girard-xqe**; planning outcome **girard-xqe.1**. This document specifies a proposed end state, not completed adoption. The user's request authorizes this map; project cutovers require the owning project's acceptance.

## Model / Replaces / Adds

Atlas reads source once into evidence-bearing facts, then answers developer questions through navigation, review, checks and diagrams. Project declarations and rules remain distinguishable from observations. Missing evidence is explicit.

The consolidation replaces three copies of source discovery, lexical masking, item/import/test-boundary grammar, navigation and overlapping review calculations. Murail contributes its mature span, move, visibility, concentration and architecture-check mechanisms; standalone contributes qualified relationships, representation views, source control and rendering; Anneal contributes its consumer cases and its differing selection and audit contracts. Existing standalone code has no presumption of being the final implementation.

It adds a shared Rust library boundary usable by native project tooling, a complete standalone command interface, and thin project integration. The existing declaration-adapter protocol remains narrow. It does not become a policy-plugin framework.

## Goals and boundaries

- Preserve all existing production callers, commands and decisive checks while deleting duplicate mechanisms at each project's cutover.
- Make the combined capabilities usable from the standalone CLI, Rust callers, agents and project gates.
- Keep evidence meanings separate: lexical mention, qualified use, declaration, source control, policy finding and decisive verdict.
- Keep source, file membership, visibility and test classification in one shared authority; make differences in consumer populations explicit inputs.

This work does not implement compiler-grade Rust resolution, macro expansion or execution feasibility. It does not make the repository public, migrate Anneal's corpus runtime, replace Murail's FM engine, invent a policy language, or make generated diagrams tracked source. The diagram-reader limitations and Atlas issues #1 and #5 remain their own outcomes. The path-identity defect (#3) must be resolved in the shared source work before project cutover.

## Evidence base

The [inventory](consolidation-inventory.md) records exact pins, paths, command families and direct consumers. It is part of this design. Source was inspected read-only; no cross-project test, timing, migration or output equivalence is claimed here. The earlier Anneal trial is useful counterevidence: 3,382 normalized declaration sites agreed, but test selection, target membership and resolution populations differed. It did not qualify replacement.

Research consulted: the research graph's claim “information hiding is the correct criterion for decomposing systems into modules,” citing Parnas (1972), supports separating source mechanics from changing project rules. This is architectural guidance, not evidence of consumer compatibility. Repository contracts below determine the cutover.

## Final code and dependency homes

```text
Atlas repository
  crates/atlas-core/             Rust library, Rust 1.99.1
    source + discovery          bytes, lexical projection, mounts, items, population
    relationships + queries     distinct evidence; census/name/module/dump
    review                      spans, concentration, shape/move/visibility changes
    checks                      generic architecture checks and structured reports
    revision                    frozen source and baseline/candidate custody
    qualified source analysis   signatures, fields, constructors, calls, control
  src/                          existing atlas package and public facade
    CLI + output                command parsing, help, presentation, artifact writes
    render/                     ASCII/Pele and native view-family rendering
    adapter/                    checked declaration protocol; public entry preserved
    bin/atlas-girard.rs          Girard declaration convention

Murail
  crates/murail-atlas/           native command compatibility + project policy/formatting
    project policy              existing authored TOML, FM references, bead grammar
  xtask                         existing dispatch, FM adapter, land receipt consumer
        └── atlas-core          exact Git revision, no sibling path

Anneal
  xtask                         command compatibility + Anneal comment-policy values
        └── atlas-core          exact Git revision, no sibling path

Girard
  one recipe + exact pin ────── installed atlas + atlas-girard
```

Two packages in the Atlas workspace are sufficient. `atlas-core` contains reusable mechanisms and structured reports without Clap, Pele, Node or dependencies on an inspected project. The existing `atlas` library facade reexports compatible public paths rather than keeping copied implementations; it owns renderers and executable transport. Project libraries depend directly on `atlas-core`; they do not spawn an installed CLI from their library calls. No per-command crate or dynamic plugin registry is introduced.

**2026-10-28 boundary clarification, before qualified-source extraction.**
Renderer-neutral diagram data and source-control projections move with their
source model into core, preserving the existing `Workflow` methods and facade
type identities. ASCII/Pele backends and executable transport stay in the Atlas
application. Core has no mandatory Clap dependency. Its optional `clap` trait
integration preserves the already-public `ValueEnum` implementations on `Level`
and `View` when the application facade enables it; native core consumers leave
it disabled. This is one model with optional foreign-trait integration, not a
second analyzer. Qualify both configurations and verify the default core's
dependency graph has no Clap, renderer or inspected-project dependency.

The source and analysis library use the same Rust 1.99.1 baseline as the Atlas application. Both inherit the shared workspace lint table, aligned with Murail's default, pedantic and safety restrictions while retaining Atlas's existing unsafe-code prohibition. Consumer toolchain alignment is explicit at cutover.

**2026-10-28 amendment, before toolchain-alignment results.** The user directed: “We can be using Rust 1.99.1 these days ... stay in tune with my other projects with similar settings (clippy, etc.).” This supersedes every Rust 1.94 preservation requirement in this design, including the diagram, Anneal outcome and V5. Atlas and atlas-core pin and declare Rust 1.99.1. Both inherit one workspace lint table aligned with Murail's default/pedantic/safety restrictions; necessary exceptions state their reason at the relevant boundary. CI tests the whole workspace on the pinned compiler without an older-compiler job. Anneal's cutover explicitly aligns its toolchain. V5's compiler commands are now `cargo +1.99.1 check --workspace --locked` and `cargo +1.99.1 test --workspace --locked`; all independent/package/consumer checks remain required. The inventory's historical compiler values remain observations of those commits.

The design extends Atlas's earlier “no policy engine merely to improve a diagram” boundary explicitly: generic architecture checks become a first-class product capability. Project legislation and external authority resolution remain outside the core. Update AGENTS.md, PROVENANCE.md and user documentation when that extension is implemented.

## Source contract

- **R1 — must: shared bytes and membership.** One immutable source snapshot owns each physical file's bytes, normalized workspace-relative identity, fingerprint, Cargo mounts and diagnostics. Multiple mounts remain multiple contexts, not duplicated file reads. Current source and pinned revision reads use the same interface. No checkout mutation, implicit fetch or working-copy snapshot occurs during a query.
- **R2 — must: rich lexical facts.** Retain item kind, full attached span including comments/attributes, declaration line, nesting, declaration text, header/doc surfaces, imports/reexports and visibility including the target of `pub(in path)`. Preserve lexical projections needed by FM evidence and native LOC counting. Do not replace this contract with today's smaller public `SourceGraph` record.
- **R3 — must: population as selection.** Record test evidence separately for file/target roots, module ancestry and individually gated declarations. Record top-level and declaration-versus-binding facts. Consumers select evidence categories and item kinds through a typed population value. Murail/Anneal's existing counting boundaries and standalone's current production views must each be reproducible; do not “correct” their budgets by adopting a different default silently.
- **R4 — must: identities and uncertainty.** A source site is tied to snapshot, physical file and span. Comparison keys use crate/module/owner/kind/name and preserve duplicate candidates. Names are not globally unique; line numbers alone are not revision-stable identities. Macro/trait/alias limitations and unknown references remain visible. Keep native approximate matching labelled approximate when reproducing its output.
- **R5 — must: one owner per fact.** Lexical masking/item grammar owns inventory and source spans; the existing AST front end owns qualified syntax and supported control. Both read the same snapshot/population/mounts. Cache a file's AST within an analysis invocation rather than reparsing it per view. An AST projection must not silently establish a competing LOC/test/item census. No daemon/index service is needed.

**2026-10-28 source-context amendment, before context-reconciliation results.**
The source site and the context in which it is compiled are different facts.
Rust assigns a file's module location at the referring module declaration or
crate root ([Reference](https://doc.rust-lang.org/reference/crates-and-source-files.html));
one Cargo package can have several crate targets
([Cargo](https://doc.rust-lang.org/cargo/reference/cargo-targets.html)).
Do not select a primary package or target for a shared file.

Model: the snapshot's physical path map owns one source buffer and its lexical
facts. `SourceGraph.files` records package-file observations, preserving its
existing package-name field; shared paths may have several such observations.
Mounts retain production/test context separately. Qualified function identities
include their target/module context. Representation facts remain physical syntax
sites: a relationship is unique only when every applicable context resolves to
the same physical site. Otherwise it stays unknown. No inference of Rust type
identity from a shared file is added.

Replaces: repeated cross-package reads, rejection of shared package paths,
target-map overwrites, and AST-specific test predicates. Source inventory,
file diagrams, adapter custody, fingerprints and shape reports count physical
paths once; package navigation and native matching retain their package context.
Declaration IDs acquire a package discriminator only on shared physical paths.
Declaration diagrams label that discriminator; physical file boxes stay unique.
Context-dependent diagnostics carry the package and are filtered with it.
The existing adapter protocol cannot express package-qualified module authority;
it must refuse a multi-package file rather than flatten its authority silently.

Adds: typed file-population selection (package `src` inventory versus discovered
Cargo roots), a single test-selection predicate over retained lexical evidence,
and context-aware indexing. Imports and reexports consume the same use-tree
grammar. Root-relative paths preserve leading parent traversal and diagnose
out-of-workspace or symlink mounts. The inherited rustfmt-shaped file-module
grammar is not compiler module loading; unsupported inline/file mounts and
external siblings must carry diagnostics, not fabricated namespaces.

Fan-out checked before code: discovery/snapshot; source index and workflow
cache; representation types, constructors, calls and fingerprints; lexical
relations and name/census/module reports; physical diagram grouping/coverage;
outline/diff/documentation; package move/visibility matching; adapter input
custody; public facade, installed CLI and independent library consumer.
Qualify ordinary pinned output unchanged, plus shared package roots, same-file
lib/bin targets, path traversal, test-only/shared production mounts and
population-dependent imports. The research graph added no useful evidence for
this decision; repository contracts and the primary definitions above govern it.

**2026-10-28 clarification, after controlled population cases.** Production AST
views now read the existing test-implying predicate, including `all(test, ...)`,
rather than the old exact-`cfg(test)` AST predicate. A directly attached inline
attribute gates its declaration, never the following declaration. Use-tree
bindings preserve competing same-local-name candidates instead of overwriting
one. These are deliberate uncertainty/population corrections; record their
witnesses alongside unchanged native LOC/outline evidence. An excluded test
target is not required to parse until the selected population includes it.
Attribute bounds read masked brackets and original text, preserving stacked,
commented and multiline gates. File-module test ancestry consumes those same
facts. Native LOC/outline grammar is unchanged. Explicit file workflow selectors
keep access to test bodies; production selection belongs to qualified workspace
analysis, not to an arbitrary requested syntax body.

Preferred implementation donor is selected per capability, not per repository. Start lexical extraction from Murail's more complete scanner, reconcile standalone's UTF-8 and test/mount fixes and Anneal's cases, and move the union of relevant regressions with it. Retain citation-surface recognition as syntax; interpret `fm:` in Murail. Move Murail's closed-basis row interpretation into its native name-card enrichment, consuming shared declaration syntax; its stable-tag/storage meaning is project-specific.

## Analysis and policy contract

- **R6 — must: native structured results.** Navigation, spans, non-test LOC, concentration, shape differences, visibility differences and body-move classifications return typed reports before formatting. Preserve exact versus rerooted-equal versus changed/unreadable classifications. Renaming is not semantic equivalence. `diff` (shape) and `compare` (representation contracts) remain different questions.
**2026-10-28 correction, after source review and before move-report conformance.**
The frozen native token normalizer can call meaningful macro-input edits exact by
removing `pub`, doc attributes or trailing commas. It can also erase a tuple's
meaningful parenthesized comma. The shared report preserves macro input tokens
and parenthesis commas rather than applying those rewrites without syntax proof.
This is a conservative report correction, not a semantic equivalence claim.
The discrepancy ledger must show these cases and their old/new classifications;
a native cutover's owner accepts the corrected report behavior under V2. Pass
projected code to impl-header classification so comments cannot supply its trait
name. Report selection may resolve an absent current path against the before
snapshot, while current ambiguity still refuses. Native glob-import retargets
and union-field visibility remain unmeasured and explicit in report limits. No
architecture judgment or budget changes under this correction.

**2026-10-28 follow-up, after the first bounded same/relocated comparison.**
That probe agreed on 951 normalized bodies. Further source review requires
preserving non-doc attribute payloads as macro input, including attached impl
attributes in its header reading, finding the impl-body brace outside const
generic arguments, and retaining discovery exclusions even without a captured
file. Regression cases precede the repeated probe; results from the earlier
probe do not qualify these later corrections.

- **R7 — must: generic checks, project inputs.** The core owns arithmetic and traversal for size/surface caps, crate cycles/forbidden edges, authority homes, typed-error source checks, stage order, stage reads, item-level seams, writers/test coupling and ratchets against a published baseline. It consumes a normalized typed policy plus explicitly supplied resolutions of external authorities. The shared policy input keeps the existing field families: population, size, surface, crate_graph, authority, typed_errors, stage_order, stage_reads, stage_seams and concentration. Required external references travel with their source locations and explicit resolution results. Fixed rule kinds are sufficient; no rule scripting language is added.
- **R8 — must: local legislation stays local.** Murail keeps its original `.config/architecture.toml` as the single authored policy, its loading semantics/compatibility with older baselines, BeadId/cited_beads validation, purchase authorization and FM heading/claim/boundary interpretation. Anneal keeps its citation-marker values and documentation requirements. The generic comment-audit mechanism receives these values; marker strings do not live in the shared scanner. Do not generate a second policy file.
- **R9 — must: complete judgments.** Distinguish a report-only finding, a rule violation and unavailable evidence. A project's authoritative PASS includes all of its required generic and local checks. Unknown required source, unresolved required policy inputs or a failed helper process cannot become zero findings or PASS. Lexical unknowns do not automatically fail unrelated rules; each decisive rule states which input population it requires.
- **R10 — must: preserve the gate wire contract.** Murail's `delta --candidate ... --json` continues to supply the exact baseline/candidate identity, `verb`, `display`, `policy.raises`, `violations` and recognized verdict that `xtask/src/land.rs::admit_delta` consumes. Any schema change needs its caller changed and tested in the same Murail commit. No ceiling, debt row, exclusion or authority rule is relaxed to make the migration green.

Mechanism interfaces are ordinary Rust calls, conceptually `load_snapshot(input)`, `analyze(snapshot, population, question)` and `check(before, after, normalized_policy, project_evidence)`. The implementer chooses concrete Rust names using existing vocabulary. Reports carry source/policy witnesses and explicit applicability. Display text and process exit mapping belong to the command edge. Do not pass executable project callbacks into the parser, and do not represent missing project evidence as an empty successful report.

Murail's current typed-error check invokes ast-grep. The shared typed-error analyzer should use the already-present AST type traversal, with side-by-side fixtures covering callbacks, aliases, fields and qualified Result spellings before deleting the native invocation. Any disagreement is adjudicated as a behavior change, never hidden behind a new expected count. Other uses of ast-grep in Murail are untouched.

**2026-10-28 native-stage clarification, after inherited unit tests and review,
before frozen native command comparison.** Production stage paths select the
same retained production facts for root aliases and readers. Directly test-gated
root aliases cannot contribute production edges; native LOC's separate counting
boundary is unchanged. A package scope selects observations, policy findings and
required authority references together. Competing root imports remain unjudged
rather than selecting the last written binding; this is conservative source
analysis, not conditional-compilation resolution. Preserve these deliberate
differences in the native discrepancy ledger and obtain owner acceptance before
cutover. Unreadable captured imports make the required stage reading unavailable.
The stage report neither validates unrelated concentration references nor judges
the mounted test population needed by seam checks. One retained import reading
supplies lexical relations and stage paths; source-control syntax uses the same
UseTree normalization, retaining a written leading `::`.

**2026-10-28 seam-input clarification, before seam-port results.** The native
wrapper supplies each present stage root's interpreted boundary names and
citation findings, bound to its file path and content fingerprint. Core checks
that custody against the captured source, then derives exports, item homes,
writer calls, test coupling and ratchets using the native mechanisms. Core does
not parse the project's Boundary paragraph, read FM chapters or accept an
unresolved citation as a valid boundary. An explicitly supplied declaration with
no listed names differs from an absent declaration. Missing declarations,
stale source custody and unavailable mounted test inputs make the dependent seam
reading unavailable, not zero. Native FM interpretation stays in the native
wrapper and its tests. Source-only regression fixtures may supply those same
interpreted inputs explicitly; they do not establish project authority.

**2026-10-28 seam-population clarification, after unit fixtures/review and before
the frozen native seam comparison.** The same root/path resolver reads production
or all-test facts through typed population selection. Test-only modules cannot
be production targets or empty production boundaries. Test roots include their
own directly gated aliases and native mounts. Required test paths or listed
boundary paths that cannot be ranked make the dependent census unavailable;
they cannot disappear from a zero count. These correct inherited omissions and
must be recorded, with native owner acceptance before cutover. The native
ratchets and their item/count units otherwise remain unchanged.

**2026-10-29 measured-delta clarification, after policy-input conformance and
cleanup qualification, before measured-delta code. Model:** baseline and
candidate source captures feed one measurement and judgment flow; project
legislation and external authority remain caller inputs. **Replaces:** the
application's concentration-as-outline branch when native concentration becomes
available. `spans` continues to expose structural outline/block observations;
concentration measures its different native units. Do not create another
concentration command or a second source census. **Adds:** native body/entry
line, module-home, alias and header measurements over the retained source view;
size/surface/dependency and concentration ratchets over the pair, composed with
the existing stage/seam checks. Missing populations, required source resolution
or external authority prevent a landing PASS. Native citation and purchase
interpretation stay outside core. Carry explicit baseline/candidate custody to
the result and preserve `admit_delta`'s exact identity/raises/verdict/display wire
contract in the wrapper. Format only after typed findings; the CLI's coordinating
function must show capture → measure → judge → present. Qualify this whole
landing path through the frozen native gate consumer rather than declaring a
collection of helper ports complete.

**2026-10-29 correction, after reading the complete donor concentration
entry point and before measured-delta code.** The preceding replacement clause
conflated the native concentration CLI with the `[concentration]` debt judgment.
Native `concentration::report` deliberately reports max-block/breadth shape;
`concentration::landing` measures and judges body-line/name/alias/header debt.
Preserve the CLI shape question and share its outline mechanism. Add the debt
measurement only to delta; do not replace shape reporting or add another
concentration command. Project wrappers supply header-citation observations bound
to captured file fingerprints and parsed commit citations bound to the exact
description bytes. Core does not acquire Murail's marker values or bead grammar.
The standalone host can use explicitly supplied citation markers through the
existing documentation audit. Missing required header/citation evidence is
unavailable, not an uncited count of zero. Malformed cached import syntax makes
alias judgment unavailable instead of silently dropping aliases. These limits
must travel through the complete delta result.

**2026-10-29 context-aggregation correction, after the overlap witness failed
on the initial debt port and before its repair.** Debt rows name physical paths.
A shared path retains every production package owner; a newly joining package
cannot grant a new row to a path already measured in an established population
package. File/entry totals remain physical-path counts, entry membership is the
union of its production contexts, and header/module/alias subjects retain their
native per-package identities. No last package wins ownership. The controlled
nested-workspace witness initially reported PASS; it must refuse after repair.
This corrects a native assumption that source roots never overlap and requires
an explicit discrepancy record for native owner acceptance.

**2026-10-29 revision-host clarification, before delta CLI results.** Reuse
one Git-store discovery/resolution owner for all revision commands. Require an
explicit repository marker at the selected workspace root; do not discover an
ancestor store implicitly. Resolve JJ names with `--ignore-working-copy`,
including `@`; a query never snapshots working edits. The pinned delta host
checks the candidate's tracked tree against current files and checks captured
source/package contexts and normal dependencies against the exact owned
extraction. Dirty or mismatched source is refused, never labelled with the old
commit id. Filesystem policy observations retain the real project root. Sealed
extraction identity is independent of legacy public path/id display fields.
No project checkout, index, bookmark or policy is changed to run the query.

**2026-10-29 remap-guard correction, after a controlled production remap
witness failed and before its repair.** A cfg spelling containing `test` is not
proof of test-only availability: `not(test)` and a feature named `contest` are
production cases. Reuse the shared cfg implication reader, with explicit
project-supplied test-support feature names for the native intentional exception
(such as its test-strategies fixture). No feature vocabulary is built into core.
Preserve the native test-support law with those declared inputs; reject the
substring heuristic and record the corrected verdicts for native owner acceptance.
The generic CLI cannot interpret stage Boundary paragraphs or project bead
citations: policies requiring those inputs remain unavailable and use their
native Rust wrapper. State that scope directly in help and usage documentation.

## User and command behavior

**2026-10-28 policy-pipeline clarification, before generic policy results.** Keep
configuration validation, captured-source checks and helper observations as
named steps. Reuse the native membership/ceiling/graph/concentration rules;
validate source authority homes and forbidden items from retained production
facts. Filesystem and arbitrary syntax-pattern observations have distinct
availability from violations. Missing helpers or local authority resolution
cannot establish PASS. Shared mechanisms have one implementation for the CLI
and native wrappers; no policy-plugin framework or second authored policy is
introduced. Preserve baseline/candidate custody separately for delta.

**2026-10-29 policy-boundary clarification, after the first frozen policy
reading and code review, before correction results.** The `policy` command's
judgment is the native policy-input question: configuration, typed errors,
authority homes and forbidden selectors, plus explicitly resolved project
references. It does not measure size/surface/concentration caps or adjudicate
the landing delta; label that scope in every output. Retain the native split
between input validation and measured delta. Correct the inherited raw-prefix
source selectors to exact-file-or-directory descendants using the shared path
boundary predicate. Require normalized workspace-relative forbidden paths and
refuse observations through intermediate symlinks. These are proposed native
behavior changes, with controlled witnesses and owner acceptance required before
cutover; do not edit the project's authored policy to conceal a difference.

**2026-10-28 typed-error clarification, before fixed-rule port results.** Read
the native written `Result<_, String>` rule through the existing analysis AST
cache, not a second Rust parser or the arbitrary-pattern helper. Preserve the
native constructor/error spellings, qualification/trivia handling, production
test population and distinct source-line unit. Check type syntax (including
callbacks, fields and aliases), not values or string literals; aliases and macro
expansion do not establish a String error. A required file that cannot parse is
unavailable, not a zero. The donor's three regression claims travel with this
rule. General authored `forbid_pattern` queries remain a separate helper input;
this does not invent an AST pattern language or claim complete policy PASS.

**2026-10-28 namespace clarification, after the frozen seam comparison and before
the precision change.** That comparison returned unavailable for twelve uses
of a valid function/module import collision and one crate-root glob. Extend the
existing module-versus-value path handling to imported targets only where the
captured declarations prove the distinction: file-module locations versus
top-level functions, constants and statics. Imports name both namespaces;
terminal inline paths read the value and continued module paths read the module.
Do not infer kinds for external, unresolved or chained aliases. Competing
same-namespace bindings remain unjudged. The root-glob limitation remains
unjudged; this precision change does not relax its required-input outcome.

- **R11 — must: preserve project entry points.** Existing `just atlas`, `cargo xtask atlas`, Murail's native binary/library entry and both `nontest-loc` dispatches continue to work without a separate Node or installed Atlas CLI. Native wrappers adapt arguments, project inputs, legacy JSON and formatting; they do not walk source, classify items or calculate shared metrics.
- **R12 — must: expose reusable tools.** Standalone Atlas adds `nontest-loc`, `spans`, `concentration`, `diff`, `visibility`, `moved`, `comments`, `stages`, `seams`, `policy` and `delta` alongside its current commands. Stage/seam/policy/delta commands require explicit policy input. If a project policy requires an unavailable local authority, they report unavailable and direct the user to that project's wrapper; they cannot claim the wrapper's full verdict. The generic reader uses the shared field shapes from Murail's schema 1, preserves authored provenance and rejects unknown keys. Local authority requirements are explicitly declared, and unavailable ones refuse. It does not implement FM or bead validation by guessing from a string. Murail's wrapper can use this reader but still owns baseline compatibility and all local validation; there is no second authored configuration.
- **R13 — must: coherent help and output.** Top-level help groups navigation, review, checks and diagrams. Every command documents one question, population, evidence kind, scope, limits and example. Shared controls are `--root` (default current directory), `--scope`, supported `--population` selection and `--json`; visual commands also offer `--backend`, `--out`, and applicable `--format`. Preserve existing positional ROOT and native flags through unambiguous aliases in the same parser; refuse conflicting values. Show current syntax in help; do not remove working 0.5 invocations as collateral cleanup.
- **R14 — must: durable machine contracts.** New standalone reports have a schema version, question/selection, source revision or fingerprint basis, observations and diagnostics. Authoritative check reports additionally state rule applicability and verdict. Native wrappers preserve their current JSON contracts; normalized standalone schema does not redefine them. ASCII/Pele render the same report; unsupported view families return typed refusal. Do not require a diagram to run a gate.
- **R15 — must: artifacts stay on demand.** Generated SVG/JSON/Mermaid remain ignored under target/atlas. Optional CI artifacts are uploaded with exact input/policy/tool pins. No project automatically runs expensive visualization during landing. Murail's existing decisive checks remain decisive; Girard gains no new architecture gate implicitly.

For agents, document a short question-to-command guide in Atlas's shipped docs: construction → producers, dependencies → qualified relationships, a move → spans/moved/visibility, structural change → diff/compare, gate result → policy/delta with the correct project wrapper. Include evidence-reading and unknown-handling examples. A dedicated agent skill is optional subsequent work; product usability must not depend on one.

## Ordered implementation outcomes

These are three coherent code outcomes after this planning outcome, not a commitment to recursively subdivide them. Atlas publication and each consuming project's atomic cutover are separate repository commits for ownership reasons.

**2026-10-28 implementation clarification, after private source/review results
and before shared check-pipeline results.** Reuse Murail's existing generic
analysis pipeline and its regression claims over the shared source facts. Adapt
the source boundary, project-authority inputs and presentation boundary; a port
does not require redesigning each command or introducing another rule framework.
Stage-path ranking and qualified call lookup keep their distinct evidence and
unknowns while sharing source, test and import grammar.

Outside-`src` files mounted by native source declarations are required inputs to
the native stage/seam population. Their membership, mount path and test ancestry
must survive the port (R1/R3, V1/V3). A diagnostic for an unsupported Cargo target
does not qualify dropping a file the native tool already reads, nor may missing
required source become an empty successful check.

Qualify a complete native capability from selected source and policy through
measurements/checks to structured results and the command or gate boundary.
Preserve native JSON, baseline/candidate identity, refusal and unavailable
outcomes. New general-purpose interfaces need a concrete consumer in that path.
Private extraction checkpoints are evidence toward the outcome; publication,
Girard adoption and owner-accepted native cutovers retain the completion
boundaries below. This clarifies implementation order and reuse without reducing
the approved features or creating additional outcomes.

1. **Shared Atlas library and complete reusable CLI (girard-xqe.2).** Extract `atlas-core`; reconcile the source contract and import generic native navigation/review/check mechanisms plus their regressions. Preserve root-library facade, current diagrams, external adapter SDK and installed-package behavior. Supply the documented policy reader, command/help inventory and consumer conformance harness. Fix source identity #3 here. Publish one exact Atlas revision; update Girard's pinned recipe/docs in a normal tool-only landing. Existing Murail/Anneal commands remain authoritative until their own cutovers; no runtime dual execution or fallback scanner is installed.
2. **Murail cutover (girard-xqe.3).** At one frozen Murail candidate, switch its crate, direct FM/LOC callers, dispatch and land checks to the published core. Retain native policy parsing/enrichment and formatting, delete local shared implementations in the same commit, and run all native law/knockout and gate-consumer tests. Owner accepts evidence before landing. The old executable is used only as a test oracle from its frozen pin, never as production fallback.
3. **Anneal cutover (girard-xqe.4).** Independently after outcome 1, replace its xtask atlas/scanner with core calls and a thin command/policy wrapper. Preserve its seven commands, comment markers, non-test LOC dispatch and the aligned Rust 1.99.1 build. Delete both local mechanisms in the same commit; retain its exact-output consumer tests as compatibility projections. Owner accepts before landing. This outcome can proceed before Murail's landing because it does not depend on Murail's wrapper.

If a newly discovered consumer invalidates one of these boundaries, re-map the whole affected outcome and amend this design before continuing; do not accumulate scanners, feature flags or unplanned nested slices. Owners can split repository publication from adoption, but cannot declare a project migrated with an old production mechanism still active.

## Qualification and deletion checklist

- **V1 / R1–R5: source conformance.** At the pins in the inventory, run old/new over identical immutable source. Compare exact membership, mounts, attached spans, visibility scopes, cfg/test evidence, import bindings, headers and selected LOC/items. Exercise UTF-8, inline/path-mounted modules, reexports, shared mounts, external targets, same-spelled variants/types and malformed syntax. Record every difference with a source witness and a disposition. Zero unexplained differences in a migrated consumer's selected population.
- **V2 / R6,R11,R14: command conformance.** Capture each native command/flag combination below on a small executable fixture and the pinned project. Compare parsed JSON and exit behavior; normalize only known paths, tool revision and duration fields. Formatting-only differences require a reviewed transcript. Do not normalize findings/counts/ambiguity into agreement. Keep native output assertions that consumers depend on.
- **V3 / R7–R10: decisive checks.** Run Murail's existing policy, delta, concentration, stage/seam and typed-error suites and `admit_delta` tests through the migrated boundary. Knock out source span/test membership, a forward stage path, a seam/writer/test debt increment, invalid owner/purchase, an authority home, a widened surface/ceiling and a String error: each relevant check must refuse. Missing evidence, malformed reply and wrong candidate identity remain unavailable/refused. Test the actual gate input boundary, not only a helper predicate.
- **V4 / R2,R8,R11: direct callers.** FM evidence discovery and unowned-source custody tests exercise line comments, citation attributes, fake annotations in strings, declaration attachment and test gates through shared lexical projection. Both native `nontest-loc` commands retain their law cases. Closed-basis cards retain valid/malformed row tests under Murail's enrichment.
- **V5 / R11–R14: packaged consumers.** `cargo +1.99.1 check --workspace --locked`, `cargo +1.99.1 test --workspace --locked` and native Anneal xtask tests pass. An independent Cargo crate builds the public library; installed Atlas works outside its checkout, with no sibling source/npm. Preserve existing adapter adversarial and graph/table backend tests. Exercise all old CLI examples and new command help with positive and invalid combinations.
- **V6 / R15: product preservation.** At the existing pinned Girard figure input, compare source facts and the overview/producer drawings; explain any intended changed observation. A fresh reader answers the three existing reader questions from figures plus evidence. This does not require whole-program flow to become readable in this migration.
- **V7: costs and final deletion.** Record same-instrument scan time, peak memory and fresh/incremental build costs before/after for representative navigation, review and native gate commands. This plan asserts no performance result. Regressions are reported to the consumer owner before cutover. Search production code/manifests for the retired grammars/walkers and obsolete dependencies; all surviving copies have an explicit single home or test-oracle purpose. Product crates have no Atlas dependency. Generate final help/API/dependency inventory and record the revisions and native landing results.

Acceptance commands include Atlas's full workspace `just check`, `just check-install`, `just verify-vendor`; Murail's `just atlas-check`, policy/delta fixture tests, FM evidence and land-consumer tests, plus its owner-directed landing checks; Anneal's `cargo test --locked -p xtask`, strict xtask fmt/Clippy and ordinary owner-directed `just check`. Test/package recipes must be updated for the new Atlas workspace; fmt, Clippy, tests and rustdoc explicitly select the whole workspace so the new core cannot escape the gate. Published timing comparisons use bench through Ferry with an exclusive lease and exact input/tool commits; non-timing remote checks use gates. Add at most a bounded probe recipe where an existing project recipe cannot supply the instrument. Local observations must be labelled as local. No performance or remote check is implied merely by listing it here; actual receipts belong on the epic.

## Native navigation completion

**2026-10-29 clarification, after the whole-code review and before native
navigation results. Model:** R6/R11 require the native generic navigation
mechanisms to be in core, not just the bytes from which wrappers could recreate
them. **Replaces:** the missing population-aware tally/ladder/name/module layer
that would otherwise force arithmetic and source classification back into each
native wrapper. **Adds:** a snapshot-backed navigation context with typed reports,
using the existing source/outline/import owners and lexical relationship mechanism.
The current graph-only query functions remain compatibility APIs.

The native tally selects type/function/alias subjects by visibility; macros and
LOC retain their native independent rules. Relationship target registries use
the selected visibility while source bodies include every production item outside
test modules, including directly gated helpers. These are separate selections.
The native ladder groups package, directory and module rows without choosing one
owner for a shared physical file. Name cards preserve path-fragment selection,
hidden matches, reexports, impl sites and approximate textual references. Module
cards retain header, tally, item, fan and orphan evidence. FM claim/basis-key
interpretation stays in the project wrapper.

Standalone population controls select these richer native-style reports without
silently changing unflagged 0.5 query behavior or existing graph-only library
calls. Core reports retain source identities, selection and uncertainty; wrappers
only format/enrich them. This completion introduces no source walker or alternative
name resolver. Carry the donor's native census/name/module assertions through the
public snapshot boundary; compare the pinned commands and document conservative
source-reading corrections before accepting any native cutover. Finish this
implementation before V7 cost measurements, so timings describe the actual shared
consumer path.

**2026-10-29 navigation discrepancy, after donor source review and controlled
navigation fixtures, before the pinned full-project command comparison.** The
shared relationship mechanism already reports an explicitly unresolved
observation for a workspace type spelling shadowed by `use ext::Type` or
`use ext::*`; the native `build_graph` suppresses it (`atlas.rs` at `32e218d`,
external_bindings.suppresses at the token loop). Reuse the common mechanism and
retain its uncertainty; do not add a native suppression mode. Controlled name and
module cases pin the difference, and native cutover requires owner acceptance
of the changed observation. No architecture verdict or budget is relaxed.

The native orphan measure is deliberately workspace-wide by spelling, not by a
resolved definition. Preserve that unit and expose it as `unmentioned_names`
with its scope stated; a two-package same-name witness pins the limitation. A
native serializer can retain its existing `orphans` label. Package identity on
actual declaration/relationship sites remains intact. The source header strings
are already normalized by the shared grammar; comment decoration is formatting.

**2026-10-29 diagnostic applicability repair, after pinned navigation results
at d2784b5a and the successful bounded witness-deduplication repair, before
capture/applicability repair results.** Those JSON cards carried thousands
of unrelated relationship diagnostics, and recomputing the selected relationship
projection duplicated witnesses from the default graph. A controlled shared-file
case also showed two packages' same-site diagnostics merging into one. Preserve
one witness per package/file/line/message across repeated derivations.

Keep capture diagnostics separately on the existing Snapshot at discovery time;
source availability and a relationship limitation are different observations.
The compatibility SourceGraph still carries its combined diagnostics. Native
census carries capture diagnostics, and name/module add relationship diagnostics
applicable to their displayed sites. Their report states the fixed lexical
limits. No required source failure or requested relationship ambiguity is hidden,
and no diagnostic category is guessed from message text. This is an additive
source API and query-applicability correction, not a new source scanner or a
changed architecture verdict.

**2026-10-29 pinned census discrepancy, after command results at d2784b5a.**
All subject counts in the 27-package pub-ish and strict-public census agree.
`murail-codegen` alone differs in line units: native 21,829 non-test lines /
18,539 token-lines versus shared 21,754 / 18,489. The exact difference is
`crates/murail-codegen/src/jit/tests.rs`: 75 / 50. At the pin, `jit.rs:866–871`
puts a multiline `allow` between `#[cfg(test)]` and `mod tests;`; the native
module-gate reader loses the test ancestry and the retained shared grammar keeps
it. Preserve the correct ancestry, pin a minimal public-snapshot census witness,
and require owner acceptance of this changed population at native cutover. No
policy ceiling or registered budget is edited. Package file iteration uses the
native Path ordering, retaining fan/reexport output order rather than normalizing
arrays into agreement at comparison time.

**2026-10-29 export completion, before its implementation results.** R6/R11
also cover the native dump's generic declaration/reexport/facade observations.
Add a typed `Navigation::dump` using retained normalized imports and the existing
lexical candidate preference; leave the old graph dump API/CLI unchanged without
an explicit population. Root-public facade classification retains the donor's
conventional lib/main root rule. Reexport lookup records its evidence: written
qualified suffix first, then package-name preference, then workspace-name
preference. These are lexical candidates, not Rust resolution. Preserve all
ambiguous IDs and typed unresolved outcomes. Contextual declaration IDs come from
the snapshot; binding occurrence IDs include the source declaration and ordinal.
Native serialization maps these facts into its existing IDs, labels and schema.
Carry the donor's dump and facade claims through the public snapshot boundary,
including grouped aliases, local/global ambiguity and package-shared source.
No FM interpretation, source walker or independent name parser is added.

**2026-10-29 declaration-review completion, before implementation results.**
The registered inventory includes native `atlas/declaration_diff.rs`; complete
its typed change-scoped declaration and incident-relationship report before
R6/R11/R13 qualification. Keep signature/doc changes distinct from body-preserving
move comparison. Preserve added/modified/removed outcomes, unchanged line
relocation, duplicate candidates, prior/current evidence and native test/visibility
selection. Keys include package and observed lexical owner to satisfy R4 rather
than cancelling same-named declarations from unrelated contexts. Reuse existing
impl-header syntax and source identity, with a controlled owner-move witness for
that conservative correction. Native incident selection is known incidence by
changed source or uniquely selected changed target; ambiguous target-only edges
are outside that rule, and the report states that limit.

The standalone module/diff controls supply meaningful native `--items`/`--graph`
views without replacing unflagged 0.5 results. The new diff graph option requires
items; population controls select declaration subjects, not line totals. Native
project wrappers retain their own existing parser/serialization contracts until
owner-accepted cutover. No source walker, body evaluator or policy framework is
added.

**2026-10-29 declaration-review corrections, after controlled review witnesses
and before pinned declaration-review comparison.** The native content comparator
only sees a first-line doc summary, so later doc-line edits can disappear. The
shared report compares the complete attached prefix, including attributes and
internal payload blank lines; a red-before/green-after public-boundary witness
pins this correction. It also refuses comparing two different visibility
populations and states the chosen population in the result, rather than
classifying selection changes as source changes. Observed owner keys prevent
identical declarations moved between inline modules from cancelling. Native
cutover must accept these stronger source observations explicitly.

Retain and state the inherited capture boundary: the scanner takes a whole
signature-ending line, so an inline function body remains in its declaration
block while later body lines do not. Controlled cases distinguish those edits;
this query promises captured-source change, not body independence or semantic
equivalence. Do not rewrite shared lexical grammar or FM consumers under this
review feature. Native module/diff flags are meaningful opt-in standalone views;
legacy native parsing remains the owner's cutover boundary.

## Revision cleanup ownership repair

**2026-10-29 clarification, after source review and before the controlled repair
witness. Model:** a `Revision` is an owned temporary extraction of an exact Git
commit; cleanup authority belongs to that allocation. **Replaces:** the custom
`Drop` that deletes the mutable public `root` path, the predictable scratch name
and the hand-built allocation lifetime. **Adds:** a private `tempfile::TempDir`
lease, using the same owned scratch mechanism as policy helpers. The lease cleans
its own allocation on success or error. Public `root` and `commit` field readers
and `Revision::read` remain source-compatible; struct-literal construction is
intentionally sealed because an arbitrary path is not an owned extraction. No
inspected-project path is a cleanup target. Prove this with tool-owned fixture
repositories and a tool-owned decoy directory: changing the legacy public root
must preserve the decoy while the allocated extraction is removed. This bounded
repair does not complete the whole-repository organization review.

## Costs, alternatives and authority

This is a substantial extraction and contract reconciliation, spanning native scanners, reports, gate behavior and packaging. It is not a thin adapter task. The largest risks are population changes altering ceilings, lost FM citation surfaces, legacy JSON interpretation and accidental resolution claims. Toolchain and renderer subprocess limitations are independent qualification constraints.

Rejected: forcing Murail through the 0.5 declaration protocol (wrong inputs/outputs); replacing native semantics with current standalone defaults (changes policy populations); generic dynamic policy plugins now (no demonstrated need); keeping three scanners behind a common CLI (does not consolidate); copying all FM concepts into the shared library (creates a Murail-specific core).

The user owns approval of implementation scope and any intentional changed product behavior. Murail/Anneal owners accept their cutovers and any population/schema or measured-cost change; the Atlas implementer owns module layout within these boundaries. The default proposal preserves native behavior. A disagreement that would change a decisive verdict blocks that cutover and is recorded on the epic as a decision, not silently resolved by adjusting a test or policy budget. No additional product decision is needed merely to publish this planning document.

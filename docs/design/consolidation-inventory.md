# Atlas consolidation: pinned consumers and replacement map

Source inventory, 2026-10-27, accompanying [the design](consolidation.md). Paths below are relative to the named repository. Work: girard-xqe. This is the map used to specify a migration, not a report of completed equivalence tests.

## Pins and inspection boundary

| Repository | Inspected published commit | Toolchain |
| --- | --- | --- |
| Standalone Atlas | `71ac882976e5780a92df72424c00cd83572ad9f5` | 1.99.0 |
| Murail | `32e218d2773b87e6635d4f3df97f9b7ccca4a65c` | 1.99.0 |
| Anneal | `2b8e3ba4b9bb362520139ecdaf4184aba13cedd4` | 1.94.0 |
| Girard Atlas adoption | `1ba6bea1727427146c4d0312c2c03e9bd37414d8` | 1.99.x |

The compiler values above describe the inspected pins. The user's 2026-10-28 amendment in the design sets the target to Rust 1.99.1 and shared Murail-style lints; it supersedes preserving Anneal's historical compiler.

Murail and Anneal anchor working copies were at different revisions during inspection. Their Atlas source directories were checked with `jj diff --from PUBLISHED --to WORKING --stat`: zero changes for `crates/murail-atlas` and `xtask`, respectively. Published files/commands are the baseline. Re-pin a changed project before implementing its cutover; do not assume this table describes its future tip.

Inspection covered each Atlas command dispatcher, production source modules, manifests, project dispatch, Rust imports of the native scanner and named landing consumers. It did not census arbitrary external scripts or users' private shell commands. External unknown consumers retain the existing CLI/library compatibility boundary; the cutover record names that limit.

## Command contract inventory

Common controls differ today. Murail has a single handwritten usage surface with `--json`, `--pub-only`, `--items`, `--graph`, `--audit` and delta's `--candidate`; validity is command-specific. Anneal validates the analogous combinations for its seven commands and disallows public-only on concentration/diff. Standalone uses Clap, an explicit ROOT argument, per-command help and distinct render controls. Shared names do not establish interchangeable schemas.

| Question | Murail | Anneal | Standalone 0.5 | Final common home |
| --- | --- | --- | --- | --- |
| Population ladder | census | census | census | core queries |
| Non-test source lines | xtask nontest-loc | xtask nontest-loc | absent | core counts + CLI |
| Name/binding card | name | name | name | core queries + local enrichment |
| Module card | module | module | module | core queries |
| Lexical relationship dump | dump | dump | dump | core lexical model + native serializers |
| Documentation audit | comments --audit | comments --audit | absent | core audit, project rules |
| Concentration | concentration | concentration | absent | core review |
| Module shape delta | diff REV | diff REF | absent | core review |
| Attached item spans | spans PATH | absent | absent | core review |
| Visibility changes | visibility REV [SCOPE] | absent | absent | core review |
| Body-preserving moves | moved REV [SCOPE] | absent | absent | core review |
| Stage-order findings | stages [CRATE] | absent | absent | core checks |
| Item-level stage seams | seams [CRATE] | absent | absent | core checks + project boundaries |
| Policy validation | policy | absent | absent | core checks + native policy reader |
| Decisive landing delta | delta REV --candidate REV | absent | absent | core checks + Murail verdict assembly |
| Representation overview | absent | absent | overview | existing projection + adapter |
| Qualified file relationships | absent | absent | dependencies | core qualified facts |
| Representation panels | absent | absent | representations | core facts + existing projections |
| Contract/constructor change | absent | absent | compare | core revision/facts + row projection |
| Declared/lexical abstraction | absent | absent | map, program | existing projections |
| Source-control flow | absent | absent | flow | core control facts + existing projection |

Standalone's hidden `related` alias forwards to `name --relations`; retain that compatibility without adding a second mechanism. Murail/Anneal `--graph` means lexical co-mention evidence, not qualified call resolution. `--pub-only` is strict written public selection; their default “pub-ish” population includes restricted visibilities. Grouped reexports/bindings and production/test selection need dedicated cases, not only total-count comparisons.

Native `dump` is JSON-only. Murail's name card includes closed-basis rows and binding/reexport information; the former stays native enrichment. Native diff measures LOC/block/items; standalone compare does not replace it. Murail's stages/seams findings participate in delta ratchets, while spans/visibility/moved remain report-only.

## Producer/mechanism fan-out

Each bullet names source producers, existing readers, target home, and the deletion/check boundary.

- **Lexical source and item grammar.** Murail `crates/murail-atlas/src/scan.rs`; Anneal `xtask/src/scan.rs`; standalone `src/scan.rs`. Read by each tool's FileMap/discovery and lexical relationship builder. Murail's public RustSourceProjection additionally serves FM evidence, and both native scanners serve LOC commands. Target: core source projection, outline/import/cfg/visibility APIs. Delete the duplicate scanner implementations after all listed readers switch; compatibility reexports/adapters contain no grammar. Check attached comment/attribute spans, UTF-8, masks, cfg/path ancestry, outline-only items and private imports.
- **Discovery and membership.** Murail `atlas.rs::{Workspace,CrateMap,FileMap,ModuleMount,mark_test_files,outside_mounts}`; Anneal's corresponding structures/functions in `xtask/src/atlas.rs`; standalone `src/discovery.rs`. Read by every query and native policy metric. Target: core snapshot/discovery with physical identity plus mount contexts. Reconcile outside-src targets, symlinks, shared mounts and renamed dependencies before deleting walks. The known standalone parent-component path defect is Atlas #3.
- **Lexical name and relation lookup.** Native `atlas.rs::{Filter,build_graph,split_signature,binding_map,name_card,module_card}` and corresponding Anneal functions; standalone `src/relationships.rs`, `queries.rs`, `model.rs`. Target: core lexical evidence and structured query results. Preserve ambiguous/reexport/external distinctions and native binding cards. Native wrappers keep legacy JSON/formatting, not lookup algorithms.
- **Qualified forms and source control.** Standalone `representations/{extract,collect,types}.rs`, `source_index.rs`, `dependencies.rs`, `workflow.rs`, `workflow/` and `program/` source-call reading. Target: core syntax facts, one supported call lookup and source-control model; native diagram/layout remains application-side. Scanner/AST syntax have different evidence contracts; the migration must not turn the lexical graph into a qualified graph by relabelling edges.
- **Revision reads.** Native `atlas.rs::GitStore` and Anneal `git`/diff readers; standalone `revision.rs`, `comparison.rs`. Read by native move/shape/delta and standalone compare. Target: core immutable revision input. Preserve each query's basis: native shape selection uses its historical contract; standalone contract comparison reads each revision's source. Never read candidate policy from a different tree or silently materialize external dependencies from the live checkout.
- **Source metrics and moves.** Murail `atlas/concentration.rs`, `atlas/spans.rs`, `atlas/moves.rs`, `atlas/declaration_diff.rs` and `atlas.rs` shape diff; Anneal `atlas.rs::{concentration,diff}`; standalone currently lacks these reports. Target: core review. Project ceilings/exception rows remain inputs, not generic defaults. Preserve token/body/LOC units exactly and carry unreadable/ambiguous outcomes.
- **Architecture checks.** Murail `atlas/policy.rs`, `atlas/stages.rs`, `atlas/stages/seams/{ratchets,view,writers}.rs`, `atlas/typed_errors.rs`. Readers: policy, stages, seams, delta, `just check` policy invocation, and land's architecture tail. Target: core check mechanisms with structured applicability/findings, plus native policy/resolution wrapper. Keep old fail-closed rules and published baseline ratchets; delete generic calculations from the wrapper once equivalent.
- **Project interpretation.** Murail `atlas/stages/seams/boundary.rs::{FmIndex,paragraph,boundaries}`, policy BeadId and concentration cited_beads, `scan.rs::basis_keys`; Anneal `atlas.rs::CITATION_MARKERS` and documentation requirements; Girard `src/lib.rs::{MODULES,SURFACES}` interpreted by `atlas-girard`. Target: owning project's interpretation of supplied source/docs, or the existing official Girard adapter. Core owns syntax/counters/traversals. Shared rendering must not import Murail's FM/bead crates.
- **Presentation.** Murail `atlas.rs` ASCII tables use murail-render; Anneal has native table formatting; standalone `src/render/`, `diagram/`, representation/overview/comparison projections. Target: core typed reports and application/native renderers. Retain murail-render where exact native text needs it; do not copy its general rendering system into Atlas to remove one dependency. Source-backed facts remain independent of ASCII/SVG layout.

## Direct consumers beyond the commands

| Reader | Current call/input | Must survive |
| --- | --- | --- |
| Murail xtask main | murail_atlas::run | native arguments and exit meaning |
| Murail native atlas binary | murail_atlas::run | standalone native entry |
| Murail LOC dispatch | scan::nontest_loc | counting/test boundary |
| Murail FM Rust evidence | RustSourceProjection | attachment and citation surfaces |
| Murail FM custody audit | line_comment_lines | fake-directive exclusion |
| Murail land run_delta | delta JSON subprocess | frozen basis + policy.raises + verdict |
| Murail land admit_delta | parsed JSON | purchase and refusal semantics |
| Murail just policy gate | cargo xtask atlas policy | authoritative policy validation |
| Anneal xtask main | atlas::run | seven verbs, accepted/rejected flags |
| Anneal LOC dispatch | scan::nontest_loc | existing output and boundary |
| Girard justfile | exact installed package pin | both binaries; cache refusal |
| Atlas Rust consumers | public modules/Renderer/adapter | documented facade and SDK |
| Atlas renderer subprocess | embedded Pele | installed standalone operation |

Specific FM readers: Murail `xtask/src/fm/evidence/rust.rs::discover_file` reads declaration_lines and citation_surface; `xtask/src/fm/evidence/custody.rs::audit_unowned_rust_file` reads line_comment_lines and original text. Neither is replaced by a representation diagram or a declaration-adapter JSON reply.

Murail `xtask/src/land.rs::{run_delta,admit_delta}` consumes `verb == delta`, exact `basis.baseline/candidate`, display text, `policy.raises`, PASS/REFUSE and violations. The land layer separately enforces an authorized purchase. Replacing this with “command exited zero” would weaken the gate.

## Files changed or removed by each project cutover

- **Atlas:** introduce workspace/core manifest; move source/query/revision/generic analysis homes; replace root implementations with calls/reexports; extend CLI/help and tests; update Cargo.lock, package include paths and installed-consumer build fixtures, justfile, CI, README, PROVENANCE and extension docs. Preserve both installed binaries and embedded renderer. No checked-in generated experiment corpus is added.
- **Murail:** native crate manifest/lock, `scan.rs` compatibility exports, `atlas.rs` dispatch/formatting, project policy interpretation and native enrichment; delete migrated generic source/review/check bodies. Switch FM evidence imports if the compatibility path is removed. Update xtask manifest/imports, justfile help/check selection and land tests in the same commit where required. Preserve `.config/architecture.toml` budgets and history. No product crate gains a development-tool dependency.
- **Anneal:** xtask manifest/lock, main dispatch, native wrapper and documentation; delete `xtask/src/scan.rs` and the duplicated Atlas mechanisms from `atlas.rs` once both command and LOC callers use core. Preserve `.cargo/config.toml` xtask invocation and `just atlas`; no change to anneal-core/md/cli/mcp or its released install.sh behavior.
- **Girard:** exact tool revision, recipe if command compatibility requires it, usage guide and dated tool record. No machine source changes. Rendering remains opt-in, output ignored.

## Consumer conformance matrix to implement

Rows are test populations, not claimed completed runs. Existing project fixtures travel with their claims; transient old/new captures belong under ignored target/atlas/consolidation.

| Population | Required observations | Decision |
| --- | --- | --- |
| One pinned project tree per tool | commands, selection, membership | explain every difference |
| Inline/path-mounted tests | file/item test evidence, counts | exact native population |
| Grouped reexports and aliases | binding identity and ambiguity | preserve cards/JSON |
| Attached comments/attributes | outline, citations, FM attachment | no lost evidence |
| External/shared Cargo targets | physical identity and mounts | diagnostic or correct membership |
| Move/rename/reroot fixtures | body tokens and visibility scopes | original classifications |
| Policy positive/negative fixtures | applicable checks and witnesses | same verdict/ratchet |
| Missing helper/source/policy | unavailable/refusal | never PASS |
| Every native flag combination | JSON, text, exit/rejection | compatible entry contract |
| Girard saved pictures | facts and explanatory layout | preserve or explicitly review |
| Installed/independent caller | package/API/toolchain | no checkout coupling |

## Deliberate differences and costs

The earlier Anneal trial found extra standalone Cargo-target sites, differing individually test-gated helpers and different lexical resolution populations. They are a required discrepancy ledger, not a reason to select whichever total is smaller. No native budget is recalibrated by this plan.

Murail scanner features omitted during standalone extraction (citation surfaces, visibility/field helpers, LOC command and closed-basis parsing) were checked in PROVENANCE.md. Some now return to the shared source contract; closed-basis meaning stays Murail-local. This reverses a deliberate extraction boundary with named consumers, rather than adding an unrelated third implementation.

Expected work is mostly relocation, public-contract enrichment and consumer qualification, with new CLI/check integration. No numeric code-size or speed improvement has been measured. The final record must report moved/new/deleted production code separately and distinguish temporary test oracles from production duplication. A common crate name alone is not evidence of consolidation.

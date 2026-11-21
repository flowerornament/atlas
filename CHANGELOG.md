# Changelog

Atlas has two packages: `cargo-atlas` (the `cargo atlas` command and its
library) and `atlas-lib`. Before 0.11.0 they were named `atlas` and
`atlas-core`. Versions below name the package they apply to.

## Unreleased

- Two call and path readings Rust resolves differently now follow Rust: a
 name a glob import brings in shadows a workspace crate of that name
 (`use crate::adapters::*; json::parse()` calls `adapters::json`), and a
 path with a leading `::` names a crate, never a module in scope
 (`::json::parse()` calls the crate even beside a local `mod json`). Both
 used to resolve to the other item; callers, flow, facts and
 `allowed_readers` sites read the same way.

- A glob over the crate root (`use crate::*`, or `use super::*` reaching
 it) is read through each root name it brings in, where the name is used,
 in stage readings and seams test coupling alike, instead of making the
 file's paths unjudged and the crate's seams unavailable. Production
 root globs are now judged too, so a stage reading may newly refuse a read
 that was unjudged before. When lib.rs itself re-exports a glob, the root's
 names cannot be listed, and the glob stays unjudged.
- `[concentration] function_population`: the crates whose functions
 `function_lines` and `function_nesting` measure, when they differ from
 `population` (absent: `population`), checked as `population` is.
- A function's identity (`flow --function` and `concentration` function
 rows) names an impl's self type and trait by their last segment with
 their type arguments, without lifetimes and with normalized spacing
 (`<Graph<Cell> as From<Seed>>::from`): a lifetime rename or a respelled
 import no longer orphans a row. One normalizer serves the measure, flow's
 names and the reading of selectors and rows, so an older spelling still
 names the function. A qualified self type (`impl a::T`) in an older row
 keeps its leading segments.
- An authority home `FILE::ITEM` naming a `mod` declaration is that module,
 as a directory home is: `allowed_readers` treats the module's files as the
 home's own, not readers, and `forbid_new_*` counts no debt in them. Each
 file of such a module used to count as a reader, of paths that did not
 lead into the home.
- **Library:** `BoundaryDeclaration::new(file, content_fingerprint)` with
 `with_listed`, `with_listed_modules` and `with_citation_problems`: a wrapper
 outside atlas-lib can now build the boundary declarations seams and
 `delta` read (the type is `#[non_exhaustive]` and had no constructor).
 `BoundaryDeclaration::listed_modules` names child modules a stage exposes
 wholesale: every public item of each, and of the modules below it, is a
 boundary item for interior reads, unlisted exports, writers and test
 coupling.
- The Node renderer and declaration adapters run under a wall-clock limit
 (120 s for a drawing, 60 s for an adapter exchange): past it the child is
 stopped and the command refuses, naming the limit, instead of waiting
 forever on a stalled child. A process the child left holding its output is
 not waited for, and the refusal says so.
- A `--semantic` run past its deadline claims that the processes the
 indexer started were stopped only when Atlas confirmed it: it now checks
 each one is gone after the kill, and where the process list cannot be read
 (a restricted host), or one survives, it says the cleanup is unconfirmed.
- `--semantic` holds the index to the source the command read: a captured
 file whose bytes differ, before indexing (a cached index included) or
 after, refuses. A change between the capture and the indexer's first
 digest used to be accepted, so the index described other bytes than the
 qualified reading it was compared with.
- A call inside a test-gated statement of a production function
 (`#[cfg(test)] { ... }`, a gated `let`) is test code by its own site: a
 production reading (`callers`, `facts`, `query` by default) no longer
 records it, and with `--with-tests` its `call` or `unknown_call` row is
 `test_only`. Test-gated statements are recorded as test bytes in the
 syntax facts, and the lines they wholly occupy as test lines, so native
 line accounting agrees with the sites on them; a gated statement sharing
 a line with production code leaves that line mixed.
- The query absence check requires every unknowns relation a negated
 relation inherits: a rule negating a relation derived from `any_call` that
 reads `unknown_call` (through `possible_call`) but not `semantic_unknown`
 is now refused, naming the one it does not read, instead of passing.
- `cargo atlas orient --json` holds every row of every section; it no
 longer applied the text's row budget (5 by default, or `--rows`), which
 left a JSON consumer an incomplete ranking. `--rows` and `--all` now
 change only the text.
- atlas-lib has no production `Result<_, String>`: the last twelve sites in
 `analysis/` and `checks/` carry typed causes (a SCIP index that is
 malformed or unusable, a source that does not parse, a home that names no
 one module, a spelling that cannot be counted, an alias the census cannot
 read), and Atlas's own policy lists both packages in `typed_errors`. No
 public signature or message changes.
- docs/checks.md gains "Your first delta" (commit the policy, citation
 markers, `header_uncited`, hints) and "Recording what is broken today" (a
 table of where each kind of rule records today's breaks); `delta --help`
 points at both.
- `cargo atlas policy`'s `N authority homes`, and `delta`'s `authorities`,
 count module homes as well as item homes. **Library:**
 `policy::Report::module_homes`, each resolved module home by fact.
- `cargo atlas delta` with a policy absent at the baseline says so, naming
 the baseline, instead of reporting a missing file: it reads the policy
 from both commits, and the first commit of a policy is checked with
 `cargo atlas policy`.
- `cargo atlas policy` and `delta` print the policy's hints in text too
 (`hint:` and `hint      ` lines, such as a reader row that can be lowered),
 not only in JSON, so a row above its measure no longer looks unchecked.
- The tutorial policy uses the new authority rules: only `check` reads the
 syntax tree (`allowed_readers`), only `main` writes to standard error
 (`forbid_new_pattern`), and `function_lines`/`function_nesting` hold its
 functions. docs/checks.md, getting-started and the README quote its
 outputs, and gain examples for `forbid_new_pattern` with `allowed_in`,
 `pipeline` and `stage_seams` writers and final stages. `delta`'s
 `spelling` line and the `forbid_new_*` refusals use plainer words.
- **Breaking:** `stage_seams` writers rows count calls apart by
 population, per calling owner: `test_sites` and `production_sites` are
 both required, each exact to its measure and only falling (a new owner is
 held to 0). A candidate row writing the combined `sites` refuses (in
 `stages`, `delta` and `policy`) with the new spelling; a published row
 that has it bounds each population by it and by the base tree's measure.
 `Writers::sites` is an `Option`; `seams::Totals::writer_sites` is
 `test_writer_sites` and `production_writer_sites`, and the seams JSON's
 `writers.*.sites` is `test_sites` and `production_sites`.
- Mounts are classified by population: `FileMount::test_only` (a test-gated
 chain, or one starting in a test file) and `FileMount::gates`, the
 attributes on each declaration of the chain. A gate needing test or only
 a declared test-support feature is test code for seams
 (`checks::seams_with_test_support`, `cargo atlas seams
 --test-support-features`, and `delta`'s own flag). A production file
 mounted from outside `src` leaves the seam reading unavailable
 (`DiagnosticCode::ProductionMountUnread`). `Policy::unjudged` is now empty.
- `stage_seams.final_stages` is judged by `stages` and `delta`: no stage
 but the composition (the last stage of `stage_order`) reads a final stage,
 by its `stage_reads` row or by an observed production path (one refusal
 per reader, at its first site); a final stage is a staged, non-floor
 stage other than the composition. Once published, the list and the
 composition stay unless `stage_seams.amended_by = { bead, covers =
 ["final_stages"] }` (new `Knob::FinalStages`), cited in the description,
 rules the change.
- `stage_order.pipeline` is judged by `policy::validate` (so `cargo atlas policy` and
 `delta`): each function, chosen with `flow`'s selector among production
 functions, makes exactly its `calls` as written (paths, `.method`,
 `macro!`; nested calls and macro arguments count, `?` belongs to its
 call, one `Ok(..)` around the final expression is free), in straight-line
 code; `loops = true` allows `for` loops, any number, each body judged
 alike. Branches, other loops, early exits, closures and `async` blocks,
 `&`/`|` merges, struct update, `?` on a value, turbofish and unreadable
 macros or callees, and `cfg`-gated statements refuse. A line-number
 selector, a duplicate call or a call not spelled as a path, `.method` or
 `macro!` is a configuration error.
- `authority.forbid_new_pattern`, `forbid_new_observed` and `allowed_in`
 are judged by `delta`: held spelling debt outside a home, counted per
 (file, owning item, spelling) on the base (under the published policy)
 and the candidate, may only fall; debt that falls is freed only when its
 item left its file and only for that item and spelling arriving
 elsewhere; a new or raised `allowed_in` bound is a `Raise` that must equal
 its measure; a guard removed or a pattern changed while the base holds
 debt refuses. `delta::Report::spellings` gives each fact's debt on both
 sides and the files with macro invocations a pattern cannot see into.
 **Library:** `delta::Inputs::before_policy`, `policy::Inputs::{new_patterns,
 observed}`, `SpellingState`, `Occurrence`, `Census`, `policy::observe_spellings`.
 `cargo atlas delta` observes `forbid_new_pattern` on both sides (the base
 under the published policy) and shows each fact's debt; a
 `forbid_new_observed` class is supplied by a wrapper (see the library
 guide) and is unavailable otherwise. `authority.amended_by = { bead,
 covers }`, cited in the description, is the one way to remove a spelling
 or change its pattern while the base holds debt under it; removing an
 authority entry that held debt refuses.
- `authority.allowed_readers` is judged: the only files whose production
 code may read a home, each with its exact count of reading sites (each
 `use` leaf and each written path leading into the home, re-exports, glob
 re-exports and aliases followed; later uses of an imported name are not
 new sites). An unlisted reader or more reads refuse; fewer is a hint, or a
 refusal for an owned row; a row with no reads refuses; a site Atlas
 cannot pin that might reach the home, or a macro's spelled path that
 does, is `UNAVAILABLE` with the site and the fix. A home may now be a
 module (a file or a directory) as well as `FILE::ITEM`. Authority homes
 and `forbid_item` now read every discovered production file, including
 those outside `src/` (a `build.rs` module, a feature-gated `#[path]`
 mount): a broader check, so a policy that passed may newly refuse when
 such a file declares a forbidden item or a second home. **Library:** `policy::Report` gains `authority_readers`
 (`ReaderReading`) and `hints`; `Analysis::path_reading`.
- **Library:** `Analysis::path_sites` reads every path written in
 production code as a site (`analysis::index::sites::PathSite`): each `use`
 tree leaf and each whole written path in code, a macro's input read as
 code when it parses as comma-separated expressions and otherwise as the
 paths it spells, each resolved through the qualified index in every
 production scope of its file, glob re-exports (`pub use self::m::*`) and
 type aliases followed, with what binds its first segment (an import, a
 glob or an alias) and, for an ambiguous path, its candidates. The
 groundwork for counting the readers of a home.
- `delta` judges function limits over a change: a raised or removed
 `function_lines`/`function_nesting` refuses, as does a new `generated`
 path; a new or raised `function` row is a `Raise` for the host to admit
 (with the row's `owner`); a function that grows within an unchanged row
 refuses (a row holds the lesser of its value and the base measure). A row
 follows a function moved unchanged (same fingerprint, one match), and a
 row on a repeated name (`f#2`) stays with its function when a twin is
 inserted. **Library:** `Raise` lives in `checks::policy` (still
 `checks::delta::Raise`), and `concentration::Verdict` gains `raises`.
- Function limits are judged: `[concentration] function_lines` and
 `function_nesting` hold every production function of the population (and
 its build script) to Clippy's `too_many_lines` and `excessive_nesting`
 measures, `function` rows hold a function above a limit at its measure
 (owned rows exact, as file rows), and `generated` paths are left out.
 `cargo atlas policy` judges them on the tree; the law over a change comes
 with `delta`'s. A Clippy golden test (`just clippy-golden`, run in CI)
 holds the measure to Clippy's own flags. Atlas's policy adopts 70 and 6,
 its Clippy settings, with a row for each function it holds above them.
- atlas-lib: two public errors are typed. **Breaking:**
 `relations::Relations::read` returns `Result<Relations, relations::ReadError>`
 instead of `Result<Relations, String>`, and `source::Import::bindings` is
 `Result<Vec<ImportBinding>, source::UnreadableImport>` instead of
 `Result<Vec<ImportBinding>, String>`. Both print the same message as before
 (`Display`) and implement `std::error::Error`.
- cargo-atlas: views no longer re-export renderers, so a view never calls a
 renderer and cargo-atlas has a stage order of its own. **Breaking:** the
 renderers' true paths are public and the view-module paths are gone:
 `cargo_atlas::types::{ascii, pele}` (and `representations::{ascii, pele}`)
 are `cargo_atlas::render::types::{ascii, pele}`, and
 `cargo_atlas::overview::{ascii, pele}` are
 `cargo_atlas::render::overview::{ascii, pele}`. `types::Panel` and
 `Comparison::pele` are unchanged.
- Function measures: every function with a body is measured once in the
 syntax pass, as Clippy's `too_many_lines` (code lines in the body) and
 `excessive_nesting` (deepest block level from the file's root) read it,
 and kept in the facts cache (`Source::functions`, `FunctionMeasure`, with
 an identity spelled as `flow --function` prints it and a token
 fingerprint). The facts `function` relation gains `lines` and `nesting`
 columns (schema 1, appended), the numbers the concentration law will
 read. Atlas expands no macro: functions a `macro_rules!` writes are not
 measured, and blocks inside macro arguments are not counted, so Atlas
 can read lower than Clippy there.
- An extension field written empty (`final_stages = []`, a writer's
 `test_sites = {}` or `production_sites = {}`) is a claim and is reported
 like any written field, no longer read as absent. **Library:** those
 three fields are `Option`s.
- `cargo atlas policy` refuses a second `stage_order` for one package, as
 `stages` already did.
- `[size] approve_growth = true` is judged: no stored ceilings; `delta`
 reports every core crate's token lines before and after
 (`Report::size_changes`), and each crate that grew as a `Raise` from its
 base size (`before = None` for a crate the base lacks) for the host to
 approve. Shrinking is free.
- Crate graph: an allowed package the rule's package does not depend on is
 refused (UNUSED ALLOWANCE), and so is a vendored package that depends on a
 workspace member (VENDORED EDGE). `cargo atlas policy` now judges the
 graph as `delta` does, from Cargo's declared dependencies. **Breaking:** a
 crate rule without `allowed` is refused (it allowed any dependency); write
 `allowed = []` for none, or `allowed = "any"` to keep the old meaning.
 **Library:** `CrateRule::allowed` is `Option<Allowed>` (`Allowed::Any`,
 `Allowed::Crates`).
- Policy owners and citations. A concentration `entry` or `file` row may
 name an `owner`; an owned row must equal its measure (a stale owned row
 refuses with the lower value or the deletion it owes), while a row without
 an owner keeps the hint. **Breaking:** a crate leaving `population.core`
 now needs `[population] amended_by = { bead = "<reference>", covers =
 ["population"] }`, cited in the description, as a concentration
 loosening does; without it `delta` refuses and names that line. Row owners
 and the amendment's reference join the authority a project wrapper
 resolves (`Policy::external_references`). `Raise` gains `owner` (none for
 ceiling raises).
- Policy: the format reads the 0.21 extensions (`[size] approve_growth`,
 `population.amended_by`, concentration `function_lines`,
 `function_nesting`, `generated`, `function` rows and row `owner`,
 authority `allowed_readers`, `forbid_new_pattern`, `forbid_new_observed`
 and `allowed_in`, `stage_order` `pipeline` (a list: a package may have
 several functions that run its stages), `stage_seams` `final_stages`
 and writers' `test_sites`/`production_sites`, and `allowed = "any"` in a
 crate rule). Their rules are not judged yet: a policy that writes one is
 `UNAVAILABLE` naming each field (`Policy::unjudged`), never a complete
 `PASS`. Policies without them are judged as before. `[size]`'s
 `core_token_lines` and `crate` may be left out with `approve_growth`, and
 written together with it are refused. **Library:** `Policy` and several
 of its section structs gain public fields (`StageOrder::pipeline`,
 `SizeCeilings::approve_growth`, ...), and new types `Pipeline`, `Reader`,
 `NamedPattern`, `Allowance`, `Generated`, `FunctionRow`.
- **Breaking (atlas-lib, whose next version is 0.10.0):** every policy
 schema type is `#[non_exhaustive]` (`Policy`, `CrateRule` and `Amendment`
 join the rest), so 0.21's remaining fields and later ones are additive.
 Build a policy, or any part of one, by deserializing it
 (`Policy::parse`, or `toml::from_str` for a section) rather than with a
 struct literal.
- Fixed: on Unix a source file whose name holds a backslash
 (`src/odd\name.rs`) was keyed as `src/odd/name.rs`, so output protection
 guarded a path that does not exist and `--out 'src/odd\name.rs'` could
 overwrite the file Atlas read. Paths are converted to `/` only on Windows,
 where a backslash is a separator (`discovery::path_key`, used by
 discovery, the checks and the command line alike), and a `.rs` file whose
 name is not valid UTF-8 is excluded with a diagnostic rather than read
 under a lossy name.
- Fixed: `callers`/`callees` with an index added the index's target as a
 second call wherever it disagreed with the qualified reading (a stale
 index after an import retarget, say), so one site counted twice and a
 stale caller appeared. The index now adds rows only for calls Atlas left
 unresolved; a disagreement keeps the qualified row and is stated from
 both ends ("the index: disagrees at N call sites", JSON
 `semantic.disagrees`). `orient`'s "+N by the index" counts the same way.
 The `semantic_call` facts keep disagreeing rows, marked `disagrees`.
- Fixed: the semantic reading found a method call's name by searching the
 text after its receiver, so a comment before the method, or a call of the
 same name inside the receiver's arguments, made the outer call take the
 inner occurrence (a wrong semantic edge). Method calls now carry their
 name's site as parsed; one read without it gets no semantic position. On
 a large private workspace 6 outer calls in multi-line chains now read
 their own method; this repository and a mid-size workspace are unchanged.
- Fixed: `#[cfg_attr(PREDICATE, path = "...")] mod m;` was mounted from
 the default file only, so a call into `m` could resolve to the file a
 configuration does not read (a wrong edge). Each conditional path is now
 a candidate beside the unconditional one, inside or outside `src`, and
 every candidate file is mounted as `m`; a name more than one defines is
 ambiguous. No call, mount or diagnostic changes on this repository or
 two private workspaces (none uses one).
- Fixed: a path starting with a workspace crate's name was read as that
 crate even where the caller's module declares a module or type of that
 name, which Rust reads first (a wrong edge). A relative path now names
 the module's own module or type first; a local function of that name
 still does not shadow the crate. The same holds for a `pub use` target,
 read once every module is known. No call changes on this repository or
 two private workspaces.
- Fixed: a method receiver's type was read through the module's item even
 when the body shadows it (`type A = B; let v: A = ...; v.run()` or a
 body-local `fn make()` as the initializer), giving a wrong edge to the
 module's item. Such a receiver is now `local-shadow`, naming what the body
 binds, as a direct call through a shadowed name already was; so is an
 initializer that is a local value. No call changes on this repository or
 two private workspaces.
- Fixed: a local binding typed a method receiver beyond its lexical scope,
 so a call after a closure or `async` block, or in another block, branch or
 match arm, could resolve through a binding it cannot see (a wrong edge,
 from 0.19's closure reading). Bindings now carry their scope (block,
 closure body, arm, `if let`/`while let` branch, `for` body). On this
 repository and two private workspaces no resolved call is lost, and 5 to
 66 calls per workspace that two out-of-scope bindings had made ambiguous
 now resolve; the SCIP index agrees with every one.
- Glob imports of workspace modules are followed (`use super::*`, `use
 crate::model::*`). A glob of the module itself or an ancestor binds every
 name there, private items and that module's `use` imports included; a
 glob of another module binds its non-private items and re-exports. A name
 the module declares or imports by name shadows every glob, two globs
 binding different items are ambiguous (never picked), and a glob of
 another package is not followed; the unresolved-path reason then says a
 glob may bind it. No resolved call is lost or changes target on this
 repository or two private workspaces; with `--with-tests` resolved calls
 grow by 148 (Atlas), 7,848 (+19%, large workspace) and 87 (mid-size); the
 SCIP index disagrees with none of them.
- `--with-tests` on `callers`, `callees`, `facts` and `query` reads the
 test population beside production code: test targets, the module files
 they declare, `#[cfg(test)]` modules and test-gated items. Every row from
 them is marked: `function`, `type` and `constant` get a `test_only`
 column, and `call`, `unknown_call`, `reference`, `unknown_type`,
 `construction` and `default_call` one that is their `from` item's (schema
 v1, appended columns). `callers` marks test rows `[test]` and counts them
 ("; 10 in test code"); JSON rows carry `test_only`. The default stays
 production code. `query --facts FILE --with-tests` is refused: an export
 has the population it was exported with. On Atlas, the test population
 adds 805 functions and 2,479 resolved calls (5,768 unresolved).
- Test targets outside `src` now have the module files they declare read
 (`tests/common/mod.rs` and what it declares), mounted under every target
 that declares them. Mount-missing diagnostics: Atlas 35 to 0, and two
 private workspaces 52 to 0 and 197 to 0.
 **Library:** `representations::{Function, Type, Constant}::test_only`;
 the qualified relations' `test_only` columns.
- `orient` no longer gives a share of call sites resolved: most unresolved
 calls are into other packages, which Atlas does not resolve, so a ratio
 over all call sites read as poor coverage. It gives the resolved and
 unresolved counts and says how to tell them apart. With `--scip FILE` or
 `--semantic` it splits the unresolved calls into other packages, workspace
 functions, workspace items that are no captured function, and calls
 neither reading pins (by the index's reading), gives Atlas's share of the
 workspace-function calls (Atlas: 2,004 of 2,750, 72%; the index the other
 746), and marks each most-called row with the callers the index adds
 (`+21 by the index`). JSON: `resolution.semantic`, `Row::by_index`.
 **Library:** `queries::orient::{semantic_orient, SemanticCalls}`.
- `callers` and `callees` answer from the semantic reading with `--scip
 FILE` or `--semantic` (which select `--evidence semantic`). The calls the
 index names a captured function for that Atlas left unresolved are added as rows with reading `semantic` (marked
 `[semantic]` in text); a qualified row is never removed. Each answer's
 unresolved calls are settled: confirmed, ruled out (the index names
 another function, an external item, or a workspace item that is no
 captured function), or left open, and only the open ones stay listed as
 possibly / unresolved, each with what the index read (`trait-method`,
 `local`, `no-definition`, `not-indexed`, ...). JSON answers carry a
 `semantic` summary and the report the index's provenance. Across this
 repository and two larger private workspaces, the index rules out 90-96%
 of possible-caller rows (mostly calls into other packages), confirms
 0.6-3.7%, and leaves 2.5-6.5% open. `--evidence semantic` without an
 index, and `--evidence qualified` with one, are refused.
 **Library:** `queries::callers::{semantic_callers, semantic_callees,
 Settled}`, `Unresolved::semantic`, `Report::semantic`,
 `analysis::semantic::{Reading::named, Reading::describe, Named}`.
- Elsewhere `--evidence semantic` is refused as a reading the command does
 not answer from (it no longer says "arrives in 0.20").
- Semantic facts: `query` and `facts` read a SCIP index (`--scip FILE`, or
 `--semantic` to run rust-analyzer) into a third family, `semantic`.
 `semantic_call{from, to, spelling, path, line, column, agreement}` holds
 each recorded call the index names a captured workspace function for;
 `semantic_unknown` holds every other reading (external, trait method open,
 local value, no definition, ambiguous, not indexed, mismatch) as its
 unknowns companion. The prelude's `any_call{from, to, reading}` unions
 `call` and `semantic_call` and says which reading named the pair;
 `calls_function`, `reaches` and `possible_call` stay qualified. A program
 reading the family without an index is refused; an unusable index is
 refused with its reason. A stale index is caught per call: the index's
 symbol must name what is written there (an import alias or a re-export the
 qualified reading followed counts), and a document whose text the index
 carries must be the tree's, or the call reads `mismatch` / not indexed.
 Share of today's unresolved calls the index names a workspace function
 for: Atlas 735 of 13,689 (5%); a large private workspace 9,766 of 89,851
 (11%); a mid-size one 2,789 of 19,418 (14%). Methods on receivers Atlas
 cannot type are the largest part (Atlas 406 of 8,001; the others 4,881 of
 42,544 and 1,448 of 9,341, with receiver-initializer-unknown 25% and 30%).
 No qualified call disagrees with the index on any of the three.
 **Library:** `Relations::with_semantic`, `relations::schema::Family::Semantic`,
 `Semantic::Mismatch`.
- `callers` and `callees` index the calls once per question instead of
 scanning every call for each matching function and depth level: `callers
 new --depth 4` on a large workspace takes half the time. Output is
 unchanged. Every command Atlas suggests (tails, run lines) is now built and
 shell-quoted in one place. **Library:** `relations::code` is public.

## cargo-atlas 0.19.1 / atlas-lib 0.9.1

- The Atlas Agent Skill ships with the binary: `cargo atlas help skill`
 prints `skills/atlas/SKILL.md` exactly (listed in `help topics`), and the
 README's "Use with agents" section saves it for Claude Code
 (`~/.claude/skills/atlas/SKILL.md`) or Codex (`~/.agents/skills/atlas/`).
- **Fix:** a method on a receiver whose type is named through a restricted
 re-export (`pub(crate) use standing::Standing;`, then `standing:
 &mut select::Standing<'_, Entry>` with `standing.could_place()`) stayed an
 unresolved path; only `pub use` re-exports were followed. Restricted ones
 (`pub(crate)`, `pub(super)`, `pub(in path)`) are now followed too: code
 that names an item through one compiles only where it may. Private `use`
 imports are still not exports. Resolved calls: Atlas 1,908 to 1,945; a
 large private workspace 23,418 to 24,103; a mid-size one 5,962 to 6,023.
- `types` chooses between types that share a name, the way `name` and
 `callers` do: `module::Name` (the end of its module path), `FILE:Name`, or
 `--in PATH`. A shared bare name is refused with a runnable spelling for
 each type (`"Checked" names 2 types; choose one: cargo atlas types
 src/check.rs:Checked, ...`), where it used to say "qualify its file" and
 accept no qualified form.
- **Fix:** calls inside a closure were recorded nowhere, neither resolved nor
 unknown, so `callers` could report "no unresolved call is written as X" for
 a function called only from closures. A closure's body is now part of the
 function that writes it: its calls are that function's calls, resolved with
 the function's bindings and the closure's typed parameters; a method called
 on an untyped closure parameter is recorded unresolved, not guessed. `flow`
 still draws a closure as one opaque step. Construction sites follow the
 same rule: a struct literal and a `Type::default()` call inside a closure
 are now construction rows of the enclosing function in `types` and the
 `construction`/`default_call` facts. An `async { }` block's body follows the
 same rule: its calls and literals are the enclosing function's (they were
 recorded nowhere before). Few blocks in the measured workspaces: only the
 large private one changed, by 18 unknown calls. Recorded calls (resolved / unknown), before
 and after: Atlas 1,621 / 11,108 to 1,902 / 13,570; a large private
 workspace 20,637 / 76,453 to 23,418 / 90,518; a mid-size one 1,722 / 4,455
 to 1,948 / 5,085. The callers caveat now names what stays unrecorded: calls
 in macros, and through a function passed elsewhere as a value.

## cargo-atlas 0.19.0 / atlas-lib 0.9.0

0.19 gives every command one grammar. The bare argument is the command's
subject (a scope, a name, a function, the types or a revision), `--in` and
`-p` choose part of the project, `--vs` and `--at` the revisions, `--as` the
output, and `--rows`/`--all` how much text. New `callers` and `callees`
answer "who calls X" from resolved calls and say what they could not
resolve. Every 0.18 spelling still works; the superseded ones print one note
on stderr. JSON outputs are unchanged or gain fields, apart from the
suggested commands in `orient` and `compare`'s notes.

- **Library:** `atlas-lib`'s crate docs say what it is, where to start and
 how its evidence differs, with an example that runs as a doctest (capture,
 census, and the same count as a query); the module `atlas_lib::guide` is the
 library guide (`cargo atlas help library`), and a test keeps the crate's copy
 equal to it.
- **Behaviour change:** `facts --schema` prints a readable listing by
 default (each relation with its family and meaning, each column with its
 kind and meaning, its basis column and unknowns companion); `--schema --as
 json` (or `--format json`) prints the schema as data, as `--schema` did.
- From an agent's second session:
  - The prelude's `possible_call` joins on a name's last segment through
  `function_segment` and `call_segment` instead of testing every pair: on a
  workspace with 40,729 such calls it went from 85 s to under a second. A
  test bounds its join steps, independent of the machine.
  - `cargo atlas --help` (and `help`) opens with "Run `cargo atlas help
  agent` first".
  - `function.public` is defined (a bare written `pub`; restricted and
  trait-impl methods are false) and an empty `mount.target` explained.
  - The briefing names `@--` and other jj revisions.
  - Query text folds the facts' standing limits into one note pointing at
  `cargo atlas help concepts`; program-specific notes stay; JSON keeps all.
- From an agent's first session with only the binary:
  - The prelude gains `possible_call{to: F, call: C}`, the unresolved calls
  written with F's name, as `callers` reads "possibly" (a test holds them
  equal). The absence check names it when a program negates calls.
  - A query that writes `n = count(C)` in a body, or a comma before `order
  by`, is refused with where a count goes and how a query ends.
  - `function.qualified` and `type.qualified` are described as they are:
  the name with its inline `mod` blocks and, for a method, its owner type;
  the file's module path is `mount.module_path`. Values are unchanged.
  - `dependencies` text ranks the busiest file pairs (10, most first), and
  each JSON edge carries `count`, the observations behind it (additive).
  - `callers` and `callees` with more than 3 matches say how to name one,
  built from the first match.
  - `diff --items` counts source limits by kind, one line each with an
  example, instead of one line per file; JSON lists each.
  - Help examples use names from the tutorial project, so they run there.
- Help that cannot drift: `query --help` lists every prelude relation, read
 from the prelude by section; a test runs every `Example:` line in `--help`
 and every `cargo atlas …` example in the agent briefing and the query guide
 on the tutorial project (an example that needs an adapter, another
 project's files or a jj workspace must still parse); and each link between
 guides also names the topic that prints it (`cargo atlas help checks`).
- `cargo atlas help agent` prints a one-page briefing for an agent's first
 session: what an answer means (resolved, possibly, unobserved; unknown is
 not absent), which command answers which question, how to name and narrow
 a subject, output and tails, a query cookbook whose every recipe a test runs
 on the tutorial project, and common pitfalls. `skills/atlas/SKILL.md` is an
 Agent Skill that points at it and at `orient`.
- The guides are in the binary: `cargo atlas help topics` lists them and
 `cargo atlas help TOPIC` prints one, exactly as its `docs/` file reads, so a
 reader with only `cargo install` has them. `query` and `facts` are commands
 too: `cargo atlas help query` prints the command's help, and their guides
 are `cargo atlas help --topic query`. `cargo atlas help COMMAND`
 prints what `COMMAND --help` does. Help text points at topics, not at
 `docs/*.md` paths.
- `callers`, `callees`, `dependencies` and `flow` take `--evidence
 auto|lexical|qualified|semantic`, reserved for 0.20: `auto` and `qualified`
 are their reading; `lexical` and `semantic` are refused with what to run.
 Rows of `callers` and `callees` carry `reading: "qualified"`.
- `orient`'s tails read `… N more`, as every bounded list does.
- New commands `callers FN` and `callees FN`: the functions that call FN, or
 that FN calls, from the calls Atlas resolved, one row per call site. An
 unresolved call written with FN's name is listed apart as a *possible*
 caller, counted by why it stayed unresolved, and never counted as a caller;
 `callees` counts the calls it could not follow as external or unknown, not
 absent. A function with neither says so and never reads as unused. FN is
 `FILE:LINE`, a declaration id, `Type::method`, `module::name` or a name;
 several matches are each answered on their own. `--in`/`-p` keep the
 calling (or called) functions in part of the project; `--path FILE
 --function NAME` also works. JSON schemas `atlas.callers/1` and
 `atlas.callees/1`. **Library:** `queries::callers::{callers, callees}`.
- `callers` and `callees` take `--depth N` (1 to 16): they follow resolved
 calls level by level, list each function at the first depth it is reached
 (with `via`, the function one level nearer), and count the unresolved calls
 beyond depth 1 per depth instead of listing them. The prelude gains
 `calls_function` (one step) and `reaches` (any depth); a test holds the
 commands to them.
- `query` follows the shared output rules: `--rows N`/`--all` bound its text
 tables (default 20, with a runnable tail; `--show N` still works with a
 notice), `--as jsonl` prints a header and one line per row, and `--in`,
 `-p`, `--vs` and `--at` are refused, since the rules choose what they read.
- `name` reads `Type::method` (`cargo atlas name Snapshot::graph`), the
 spelling `orient` and `callers` print; it refused it before (with
 `--population` it still does). `name
 --relations` text ends with `for calls, cargo atlas callers X`, since a
 co-mention is not a call.
- `compare` no longer refuses a project where one type has several impls of
 one generic trait (`impl From<&str> for Cell`, `impl From<bool> for Cell`:
 both define `Cell::from`), which made it unusable on most real projects,
 Atlas included. Functions that share a symbol are keyed by their signature
 too, on both sides; a key still shared, such as `cfg` alternatives with one
 signature, still refuses, and no candidate is picked. The JSON `notes` say so.
- One output flag on every command: `--as FORMAT`, with the formats that
 command can produce (`--help` lists them). `--as text` is the default
 wherever text exists; `--as json` is the same as `--json`, which stays a
 permanent spelling; `--as svg` and `--as html` draw an image or a page
 (replacing `--backend pele` and `--format svg|html`); `facts --as jsonl`
 replaces `--format jsonl`. Each `--as` form prints exactly what its old
 spelling prints. `--as` beside an old spelling that asks for something
 else is refused (`atlas: --as svg conflicts with --backend ascii`).
- **Behaviour change:** a bare argument is the command's subject. On
 `orient`, `census`, `spans`, `concentration`, `nontest-loc`, `comments` and
 `dependencies` it is a scope, read as a path, then a unique file or
 directory name, then a package. 0.18 read a lone directory as the project
 root, so `census crates/atlas-lib`, `census src` and `dependencies crates`
 answered for the whole workspace and dropped what was typed; they now
 answer for that scope. A lone directory is still the project root when it
 holds a `Cargo.toml` and lies outside the workspace being read
 (`census ../other-project`), and of two bare words the first is the root
 (`census . src`, `name . Snapshot`), each with a notice. A name that is both
 a path and a package elsewhere, or that matches several paths, is refused
 with the choices and a command to run; so is a bare argument on a command
 with no subject (`dump src`).
- `--in PATH` (a path or unique name) and `-p PACKAGE` (a package) on the
 scope commands; `--in` also on `diff`, `moved`, `visibility` and `map`.
 `--scope` still works there with a notice; on `stages` and `seams` it names
 a package and is unchanged.
- `types` takes its types as the bare argument (`cargo atlas types
 Tree,Checked`); `--type` still works with a notice. `types NAME --root DIR`
 now reads DIR, where 0.18 failed to start `cargo metadata`.
- `flow --format svg|html` without `--backend pele` still draws text, as in
 0.18; the notice points at `--as`, which draws the image.
- `--vs REV` on `diff`, `moved`, `visibility` and `delta` is the same as the
 bare revision, which stays first-class; two different revisions are
 refused (`atlas: give the revision once: REV or --vs REV`). `compare` takes
 `--vs REV` and `--at REV` for `--before` and `--after`.
- Text output is bounded: each list shows at most `--rows N` rows (a default
 per command, chosen so a default page stays within about 60 lines on Atlas
 and on syn), and a list that leaves rows out ends with one line,
 `… N more; <command>`, whose command shows the rest: the same run with
 `--all`, or a narrower subject (`cargo atlas spans src/lib.rs --all`).
 `--all` prints every row; `--rows` and `--all` conflict. Bounded: `orient`
 (adds `--all`), `census`, `name`, `module`, `spans`, `concentration`,
 `nontest-loc`, `comments`, `diff`, `moved`, `visibility`. JSON is never
 bounded. `spans` now shows 8 files by default, with the items of each
 sharing the page; `diff --items` shows each declaration's first line, not
 its whole body. `orient`'s suggested commands say `--in`, not `--scope`.
 **Library:** `Names::text_bounded`, `ModuleCard::text_bounded` and the
 `queries::More` alias; `text()` is unchanged.
- Superseded spellings (`--backend`, output `--format`, `--scope`, `--type`,
 `compare --before/--after`,
 a positional ROOT, `flow`'s positional ANCHOR, the `representations`
 alias) still work until 1.0 and print one line on stderr before any other
 output: `atlas: note: --backend is now --as (the old spellings work until
 1.0; ATLAS_NOTICES=off hides this)`. The note is never on stdout or in
 JSON and never changes the exit code.

## cargo-atlas 0.18.1 / atlas-lib 0.8.1

- Refusals end with a command to run, built from real names: an ambiguous
 scope suggests this run with the first candidate (`...; run: cargo atlas
 dependencies --scope src/cli`), `types` without a selection suggests the
 struct or enum the most declarations mention, and `flow` without a body
 suggests a binary's `main` (or the first function in the library root).
- `spans`, `concentration` and `nontest-loc` text show the first 20 files
 (`--rows N` for another number), then `... 1285 more files (1305 in all);
 to see them: cargo atlas spans --all`. `--all` lists every file as before,
 and JSON always holds every file. On a 1,305-file workspace `spans` text
 goes from 47,501 lines to 484.
- `visibility` text says `No visibility changes against <commit>.` when
 there are none, instead of a row of zero counts, and states repeated keys
 as a caveat: `(422 items share a name and were paired approximately;
 --json lists them)`. JSON is unchanged.
- `name` text counts impl blocks apart from the declarations they belong
 to (`Snapshot: 2 declaration sites (ambiguous), and 2 impl blocks`, not
 `4 declaration sites`), so one type with an impl is no longer ambiguous.
 Kinds and visibility read as Rust spells them (`[pub struct]`, `[impl]`,
 not `[Pub Struct]`), and a mention's target as its declaration ID, `one
 of ...` or `unresolved: ...`, not `Unique("...")`. JSON is unchanged.
 **Library:** `ItemKind::keyword`, `Visibility::keyword`, and `Display` for
 `model::Resolution`.
- `flow` text heads each body once (`main (src/main.rs:8)`, not `main` and
 then that line) unless a profile anchor's name differs from its function,
 and shows `if let P = e` as one branch step after `e`'s own steps, not
 also a separate `let P = e` step. The workflow and diagram JSON keep every
 step.
- `dependencies` text (and the file-level `map` and `program` views): an
 edge names each file by the shortest path suffix no other file shares
 (`index/mod.rs`, not `mod.rs`). The header says what follows, such as
 `137 files; 281 links between files`, leaving out zero counts of things
 not shown, and an empty `COVERAGE` section is left out. JSON is unchanged.
- Help: `--json` is described on every command that has it, as are
 `flow --architecture` and `--path`, `--backend`, `--format` and
 `--evidence-dir`. `--pub-only` is marked as the deprecated spelling of
 `--population pub`. `comments` runs its audit without `--audit`, which is
 still accepted.
- A positional project root (`cargo atlas census path/to/project`) still
 works but prints a warning naming `--root`. One that is a directory without
 a `Cargo.toml`, such as `cargo atlas dependencies crates` inside a
 workspace, is refused with the command to run instead (`cargo atlas
 dependencies --scope crates ...`). Before, Cargo found the enclosing
 workspace, the directory was silently ignored and the whole default scope
 was read.
- `dependencies` without a scope draws every package's production files.
 Before, it drew `src` only, which left out a workspace's member packages
 (atlas-lib on Atlas), and refused on a workspace with no root `src` (`scope
 "src" is ambiguous`). With several packages it ends with a stderr line
 naming one to draw: `cargo atlas dependencies --scope crates/<largest>/src`.
 The JSON shape is unchanged.
- An unresolved call whose path starts with a name nothing in scope binds
 (no import, item, module or crate root), such as `Box::new`, `Ok(..)` or
 `std::fmt::Formatter::write_str`, is reported as written. Before, the reason
 read `unresolved or external path murail_abi::gpu::Box::new`, a path no
 source text says. Its subject is the written path, and the message says it
 may be a prelude or external name.
- `orient` scoped to a package counts the same diagnostics as `census` and
 `module`: on a file another package also reads, only this package's (and
 the file's own) diagnostics. Before, it also counted the other package's.
- `dump --json --semantic` and `flow --semantic` run `rust-analyzer scip` on
 the workspace and read its index as semantic call evidence, as `--scip FILE`
 does for an index you supply. This runs the workspace's build scripts and
 procedural macros, so it is opt-in only. The indexer is an owned child that
 builds under `<target>/atlas/semantic` and is stopped, with every process it
 started, after `--semantic-timeout` seconds (default 1800). Atlas refuses
 before running anything when rust-analyzer (`ATLAS_RUST_ANALYZER`, or
 `rust-analyzer` on `PATH`) cannot report its version or the toolchain has no
 `rust-src`, and refuses an index made while the workspace's manifests,
 configuration, build scripts or sources changed. Produced indexes are cached
 in the user cache directory, keyed by those inputs, the indexer and
 toolchain versions and the Atlas executable; `--no-cache` turns that off.
 The output gains a `semantic_indexing` record (`indexing` in `flow`'s
 evidence file); `--scip` output is unchanged. **Library:**
 `SemanticIndex::read_indexed`, whose provenance `origin` is `indexed`.
- New command `query`: Datalog over the `facts` relations, for questions no
 command asks. Rules and `?-` queries name fields (`call{from: F, to: G}`)
 and may recurse, negate (`!`), compare, test text (`under`, `starts_with`,
 `ends_with`, `contains`, `last_segment`) and count distinct bindings; a
 query may `order by` its columns and take a `limit`. A program captures
 only the families it reads, or reads a saved export with `--facts FILE`
 and notes which files have changed since. A prelude (`--prelude`) restates
 `dependencies --relation calls` (`file_calls`), `name --relations`
 (`name_match`, `name_card`) and `census`, each tested equal to its
 command. Two honesty checks refuse a rule that drops a candidate's
 `basis`, or negates calls, references or mentions without reading their
 unknowns relation, unless `--accept basis` or `--accept absence`, which
 the answer reports. The command exits 0 whenever the program ran,
 whatever the rows; it has no pass or fail. Row and time limits refuse with
 the relation that grew. See docs/query.md.
- **Library:** new `query::{parse, check, run}` with `Program`, `Checked`,
 `Options`, `Check`, `Limits`, `Answer`, `Table`, `Error` and `PRELUDE`, and
 `relations::Relations::read` for an export written by `write_json` or
 `write_jsonl`.

## cargo-atlas 0.18.0 / atlas-lib 0.8.0

- **Breaking (library):** `source_index::Target::Summarized` holds a
 `Summary { kind, subject, message }` instead of a `String`; its `Display`
 is the message. New: `model::DiagnosticCode`, `Diagnostic::code`,
 `source_index::{Summary, SummaryKind}`, `UnknownCall::reasons`.
- **Breaking (CLI):** the `representations` command is now `types`, and its
 Pele output defaults to `target/atlas/types.svg`. `representations` still
 works as an alias.
- **Breaking (library):** cargo-atlas's `representations` view module is now
 `types` (the old path remains as a hidden re-export), and
 `render::View::Representations` is `View::Types`. atlas-lib's
 `representations` module and `Analysis::representations()` are unchanged.
- **Library:** `FactsCache::open` no longer creates the cache directory or
 prunes it. That happens on first use by a capture of a workspace that does
 not contain the directory, so a library caller can no longer create a
 directory inside the project it reads. New: `FactsCache::is_inside`, and
 `FactsCacheStats::unavailable` for a directory that could not be created.
- Diagnostics have stable codes. Every diagnostic in graph JSON, report
 bases and check `unavailable` lists carries `code` (such as `mount-missing`
 or `self-unresolved`) beside `message`, whose wording may change; JSON
 without a code reads as `unclassified`. Every unknown call carries
 `reasons`, each with a stable `kind` (such as `unresolved-path` or
 `receiver-type-unknown`), the `subject` it concerns and its `message`;
 `reason` is unchanged. `facts` adds `diagnostic.code` and
 `unknown_call.kind` and `subject`. docs/facts.md lists every code.
- `orient` groups diagnostics and unknown-call reasons by these codes, each
 with its first message as an example, instead of by message text.
- `Self` in a lexical mention now means the type it names: in an impl's
 items, the impl's self type (by the name it ends in); in a struct or enum,
 that type. It becomes a mention with spelling `Self`, resolved as if that
 name were written, and no longer a "Self is not resolved" diagnostic. Only
 a `Self` with no such type, such as in a trait's own items, stays one. On
 three large workspaces this removes 94-97% of those diagnostics (Murail
 2,098 to 63) and adds the mentions (Murail 2,055). **Library:**
 `model::Declaration` has `self_type`, in graph JSON when present.
- Each unresolved type mention in the type facts (`unknown_types` in
 `dependencies --json` and `types --json`, `unknown_type` in `facts`) names
 the parameter, return or field it occupies and the line and column of its
 spelling, so every mention is its own row. Before, two `u32` fields of one
 struct were two identical rows; on one large workspace 6,843 of 39,926 rows
 were exact duplicates. **Library:** `representations::UnknownType` has
 `slot`, `line` and `column`.
- New command `facts` exports what Atlas found as flat relations, in JSON or
 JSON Lines: files, module paths, declarations, `use` statements, mentions
 and diagnostics (lexical), and functions, types, fields, calls, references,
 constructions and constants (qualified). An ambiguous mention is one row
 per candidate with a `basis` column; calls, references and mentions Atlas
 could not resolve are rows in an unknowns relation that each resolved
 relation names. Qualified functions and types name the lexical declaration
 at their site. `facts --schema` prints every relation and column; see
 docs/facts.md. Lexical declarations carry their enclosing inline modules.
- **Library:** `atlas_lib::relations` builds the same relations
 (`Relations::capture`), with the schema as data in `relations::schema`.
- A file module declared inside inline modules, such as syn's
 `mod gen { pub mod fold; }`, is now mounted at the path Rust loads it from:
 each inline module adds a directory (its `#[path]` if it has one), after the
 file's own stem in a non-mod-rs file, and a `#[path]` inside one is relative
 to that directory. Before, the file was outside every target's module tree
 and `census --population pub` refused the declaring file. A file module
 inside a function body is still refused, now on its own. A raw-identifier
 module such as `mod r#match;` is loaded from `match.rs`.
- `dump` and other graph JSON list a declaration's enclosing inline modules
 as `modules`, when it has any, so a graph read back from JSON keeps them and
 `MODULE::NAME` selectors match through them. **Library:**
 `model::Declaration::modules` is public.
- New `orient` command, the place to start on an unfamiliar project. One page
 lists the Cargo targets, non-test lines by directory, the largest blocks, the
 most-called functions (resolved calls only, a lower bound) and the
 most-mentioned types, then what Atlas could not resolve, including the share
 of call sites it resolved. Each list states its unit, order and total, and
 ends with the command that shows the rest. The page closes with follow-up
 commands built from the names it found; a tie or several binaries get a
 placeholder instead of a pick. `--scope` takes a package or a path, `--rows`
 sets the list length, `--json` gives the same report with a stable section
 `id`. `cargo atlas --help` now starts with it and a question-to-command map.
- **Library:** `queries::orient::orient` builds the same report.
- `dump --json --scip FILE` adds a `semantic` field, and `flow --scip FILE`
 writes a `.semantic.json` evidence file beside its workflow evidence: each
 call read against a SCIP index you supply (for example from
 `rust-analyzer scip .`), with the index's provenance and coverage. `dump` also
 compares each call with its qualified target. Without `--scip` nothing
 changes; with it nothing else changes. Atlas runs no indexer. An unreadable
 or unusable index is reported unavailable and the command still answers.
- **Library:** `analysis::semantic` reads semantic call evidence from a SCIP
 index you supply (for example from `rust-analyzer scip`), with no new
 dependencies, and `Analysis::semantic` compares it with each call's qualified
 target: `agrees`, `extends`, `disagrees`, `ambiguous` or `silent`. The
 qualified targets are unchanged. Every reading names its producer, its single
 host configuration and an index digest; each site is checked against the
 captured text. An index with no standard-library symbols (what an indexer
 writes without rust-src), another position encoding, a repeated document or
 malformed bytes is reported unavailable with its reason, never read in part.

- A trait impl for a type the workspace does not declare, such as
 `impl From<Slot> for usize` or `impl Root for f64`, no longer makes a call
 like `usize::from(port)` or `self.sqrt()` resolve to the workspace's impl.
 Rust picks the impl by argument type, or prefers an inherent method, so the
 call stays summarized and names the type. Only trait impls are read this
 way, and only when no workspace type has the owner's name; a trait impl for
 a type declared only by a macro is also left unresolved. A workspace type
 alias of a foreign type, such as `type Rational = Ratio<i128>` with
 `use num_rational::Ratio`, is read as that foreign type, and so is an impl
 owner imported from another crate, even when the workspace declares a type
 of the same name elsewhere. Before, `program`,
 `dependencies` and call expansion drew these as workspace calls, including
 false self-recursion.
- **Library:** `workflow::ParsedFile` has a `types` field listing the types
 and traits a file declares.
- Commands reuse each file's syntax facts from a cache in the user cache
 directory when the file's bytes, path and the Atlas executable are unchanged,
 and parse the files it does not hold on several threads. Answers are
 byte-identical with the cache on, off or empty. `ATLAS_CACHE=off` or
 `--no-cache` turns it off; `ATLAS_CACHE_DIR` moves it; `ATLAS_CACHE=stats`
 reports hits and misses on stderr; `ATLAS_CACHE=verify` parses every file
 anyway and fails the run if an entry differs. A cache directory inside the
 inspected project is never created or used. The cache prunes itself below
 1 GiB and drops other builds' entries after a week unused.
- atlas-lib: `source::FactsCache` lets a library caller opt in to the same
 cache with `FactsCache::open(dir, mode)?.install()`. Without it, capture is
 unchanged apart from parsing a package's files on several threads.

## cargo-atlas 0.17.1 / atlas-lib 0.7.1

- `diff`, `moved` and `visibility` refuse, naming the directories, when the
 revision is extracted inside another Cargo workspace (for example with TMPDIR
 inside a checkout) and the revision's own package declares no workspace.
 Before, Cargo refused with a message about the unrelated workspace.
- `diff` no longer lists a file whose text changed but whose measured shape
 did not (same non-test lines, top-level items and largest block): it counts
 them in one line, with or without `--items`. The JSON keeps every changed
 file and marks each with `shape_changed`.
- `flow` draws a statement under `#[cfg(...)]`, or under a `cfg_attr` that adds
 one, as a branch named by the attribute as written: its steps on one arm,
 nothing on the other. The predicate is not evaluated.
- In a jj workspace, a revision jj cannot resolve says Atlas read it as a jj
 revision. A git `HEAD`, `HEAD~N` or `HEAD^` spelling also gets its jj
 spelling: jj keeps git's HEAD at `@-`, so `HEAD~1` is `@--`.

## cargo-atlas 0.17.0 / atlas-lib 0.7.0

- **Breaking (library):** atlas-lib's public report, fact, step and view
 types are `#[non_exhaustive]`, so later fields and variants are not breaking
 changes. Code outside atlas-lib can no longer build them with struct
 literals or match their enums without a `_` arm; read their fields instead.
 These stay exhaustive because callers build them, or because their set is
 deliberately closed:
 - inputs: `architecture::{Architecture, Module, Kind, WorkflowAnchor}`,
   `population::Population`, `checks::policy::{Policy, CrateRule, Amendment}`,
   `checks::policy::validation::{Match, ReferenceResolution}`,
   `checks::concentration::{Inputs, Citations}`, `checks::delta::Basis`,
   `documentation::Policy`;
 - the diagram model renderers and views build:
   `diagram::{Diagram, Node, Edge, Coverage, Surface, Query}`;
 - the closed verdict `checks::policy::validation::Status` (pass, refuse,
   unavailable).
- **Breaking (JSON):** without `--all-facts`, a scoped `dependencies --json`
 omits facts unrelated to the scope; the `view` is unchanged. The `facts`
 keys are no longer sorted alphabetically within the document.
- **Breaking (library):** `workflow::Step` has a `Block` variant.
- **Breaking (library):** `workflow::Workflow` has a `declarations` field of
 the new `workflow::Declared` type. The workflow JSON reads it as empty when
 missing.
- **Breaking (JSON):** `name --json` without `--population` reports the input
 as `query`, not `name`, and adds `question: "name"`, as the `--population`
 report already did. **Breaking (library):** `queries::Names::name` is renamed
 `query`.
- `program --json` refuses `--out` and `--evidence-dir`, as `dependencies`
 does, instead of ignoring them.
- Text diagrams say `(1 site)`, not `(1 sites)`. `name --population` headings
 read `1 match in population all`. The `--bare` help says a `dependencies`
 drawing is always bare, and the top-level help says `flow` and `map` write
 their JSON beside the drawing rather than taking `--json`.
- docs/library.md no longer says a snapshot keeps syntax trees.
- `flow` without `--depth` reads only the selected file and no longer captures
 the whole workspace: on a 1,400-file workspace it takes 0.09 s instead of
 2.6 s (16 s in a debug build). It still refuses to write over any `.rs` file
 under a workspace package, found by walking the directories without reading
 them.
- **Library:** `discovery::source_paths` lists those files.
- `dependencies --json` with a `--scope` lists only the facts incident to the
 scope: calls, references and constructions with an end in a scoped file, the
 unresolved calls and types written there, and the functions and types at
 both ends. `--all-facts` restores the workspace-wide list. On a 1,400-file
 workspace, a one-file scope's JSON goes from 82 MB to 57 KB, and peak memory
 halves because each file's syntax tree is released once its facts are read.
- **Library:** `representations::Facts::incident` selects the facts incident
 to a scope.
- `flow` draws `future.await` as the future's own steps, then one await step,
 instead of one opaque step. It does not show when or how often the future is
 polled.
- `flow` draws a labelled block (`'found: { ... }`) with its steps, and
 `break 'found` goes to the step after the block. Before, the whole block was
 one opaque step. An unlabelled `break` or `continue` inside still targets the
 enclosing loop, and `continue 'found` reaches no target.
- `flow` draws the steps inside an `unsafe` block after an `unsafe block,
 through line N` marker, instead of one opaque step.
- `flow` draws an array repeat `[value; N]` with its element expression as a
 step, evaluated once, instead of one opaque step.
- `flow` no longer draws an item or `use` written inside a body as an opaque
 step: it runs nothing. Text output lists each one under the heading as
 `declares (not a step)`, and diagrams list it in their notes.
- `flow` draws a compound assignment (`+=`, `<<=` and the rest) with its
 operands as steps, including any `?` inside them, instead of one opaque step.
 Rust evaluates the right operand first for built-in operators and the left
 first for overloaded ones. Atlas does not infer types, so when both sides have
 steps it draws both orders as a branch. The assignment itself stays opaque.
- `flow` draws a range's start and end as steps before the range, instead of
 one opaque step that hid both.
- `name` reads one selector grammar with or without `--population`: `NAME`,
 `MODULE::NAME` (the end of the module path, inline `mod` blocks included,
 such as `parse::Tree` or `nest::a::repeated`) or `FILE:LINE`. Before, the
 default population refused `parse::Tree`, and inline module paths failed in
 both.
- `flow --function` selects a trait's default body (`Trait::method`), a trait
 impl's method as `<Type as Trait>::method`, and any function by the line
 (or `LINE:COLUMN`) where its name is written. When a selector matches several
 functions, the error lists each one's site and the selector that picks it.
 A default body is never treated as a call target, because an impl may
 override it.
- `flow` follows a labelled `continue` or `break` written in a `while`
 condition to the loop it names; before, it led to an unresolved target. An
 unlabelled one there is not valid Rust and stays unresolved.

## cargo-atlas 0.16.0 / atlas-lib 0.6.0

- `moved` no longer reports relocations against unchanged source. Equal bodies
 now pair within a file before across files; before, a repeated name with
 equal bodies in several files could pair across them.
- `moved` lists each body that moved file unchanged as a `relocated:` row (JSON
 `exact_relocated`) with both sites; before, it was only counted. A repeated
 name's cross-file pairs are marked ambiguous.
- A call through a name that the function body itself declares, with a
 nested `fn`, another item or a `use` (including a glob), is left unresolved
 instead of being drawn to the module's item of that name. This affects
 `dependencies`, `flow --depth` and the program overview.
- **Breaking (library, atlas-lib 0.6.0):** `review::moves::MoveReport` has an
 `exact_relocated` field and `workflow::Workflow` has a `local_names` field.
 Code that builds either struct or destructures every field must add them.
 The workflow JSON reads `local_names` as empty when it is missing.

## cargo-atlas 0.15.0

- `--bare` draws only the diagram for `representations`, `compare` and
 `overview`: no title, notes or panel headings around it. The JSON evidence
 keeps the notes. Other diagrams have nothing to leave out.
- **Breaking (library):** `render::Pele` has a `bare` field, and
 `representations::pele`, `overview::pele` and `Comparison::pele` take
 `&Pele` instead of a `Theme`, so every drawing option travels together.

## cargo-atlas 0.14.0

- The overview also keeps each panel within 40 API contract rows, the other
 figure limit, so a workspace with many public methods splits into more panels
 instead of refusing. Every contract still appears. A module that alone needs
 more than 12 types or 40 rows is refused with both counts.
- **Breaking (library):** `representations::MAX_TYPES` and `MAX_ROWS` name the
 figure limits; `overview::PANEL_TYPES` is removed in their favour. User
 `representations` queries keep both limits and their messages.

## cargo-atlas 0.13.0

- `cargo atlas overview` no longer refuses when the declared stages select more
 than 12 types. It draws several panels of at most 12, each holding whole
 declaring modules in declared order and titled with their names. A function
 taking or returning a panel's type is drawn there too, so forms passed
 between stages stay visible. Every selected type appears; nothing is cut. A
 single module declaring more than 12 selected types is refused by name.
- **Breaking (JSON):** the overview's `views` is replaced by `panels`, each
 with its `modules` and `views`, and a `split` listing each module's type
 count and panel. The `cargo_atlas::overview::Overview` type changes the same
 way.
- `cargo atlas representations` keeps its limit of 1 to 12 types per question.
- `atlas-lib` is unchanged at 0.5.0; the adapter protocol is still version 2.

## cargo-atlas 0.12.0 / atlas-lib 0.5.0

Declaration adapters must be rebuilt against `cargo-atlas` 0.12.0: the adapter
protocol is now version 2, and an adapter built for version 1 is refused with
"unsupported adapter protocol".

- Adapters can resolve a module in the package that declares it:
 `snapshot.module_root_from(&input.source, "name")`. A module with the same
 name in another workspace package is no longer a competing candidate.
 `module_root` keeps its meaning (every package). Each snapshot file lists its
 `packages`.
- **Breaking (atlas-lib):** constants with call syntax whose callee is a plain
 path, such as `Some(Part::Host)` or `Wrapper(1, X)`, arrive as
 `Literal::Call { path, args }` (JSON kind `call`) instead of `Opaque`. This
 is syntax only; nothing is evaluated. Method calls, macros and callees with
 generic arguments stay `Opaque`.
- **Breaking (atlas-lib):** `Literal` is `#[non_exhaustive]`. Add a wildcard arm
 to matches on it.

## cargo-atlas 0.11.0 / atlas-lib 0.4.0

The packages are renamed for crates.io, where `atlas` and `atlas-core` were
already taken, and the command becomes a Cargo subcommand. To migrate:

| Before | After |
| --- | --- |
| `cargo install --git … atlas` | `cargo install --locked cargo-atlas` |
| binary `atlas`, run as `atlas <command>` | binary `cargo-atlas`, run as `cargo atlas <command>` (there is no `atlas` binary) |
| package `atlas`, `use atlas::…` | package `cargo-atlas`, `use cargo_atlas::…` |
| package `atlas-core`, `use atlas_core::…` | package `atlas-lib`, `use atlas_lib::…` |
| delta JSON `"command": "atlas delta …"` | `"command": "cargo atlas delta …"` |

- **Breaking:** help, examples, report headings and suggested commands say
 `cargo atlas`. The delta report's `command` field names the command to re-run,
 so it uses the binary that now exists.
- **Breaking:** declaration adapters depend on `cargo-atlas` instead of `atlas`.
- Both packages can be published to crates.io. Their `include` lists are
 anchored to the package root, so untracked or linked directories in a working
 copy are never packaged.

## atlas 0.10.0

- Diagram images are dark by default. `--theme light` restores dark text on a
 white background; every command with `--backend pele` accepts `--theme`.
- SVG images carry concrete colours and an opaque background instead of CSS
 variables, so viewers and converters that ignore CSS variables (such as
 `rsvg-convert`) no longer draw solid black boxes.
- **Breaking (library):** `render::Pele` has a `theme` field, and
 `representations::pele`, `overview::pele` and `Comparison::pele` take a
 `render::Theme`. `Pele::default()` is dark.

## atlas 0.9.0

- **Breaking:** the project-specific overview adapter that earlier versions
 installed beside `atlas` is no longer part of this package, and
 `atlas overview` now requires `--adapter PATH`. Projects keep their
 declaration adapter in their own repository; see
 [`examples/pipeline-adapter.rs`](examples/pipeline-adapter.rs).
- The example adapter is renamed from `foreign-adapter` to `pipeline-adapter`
 and reports the producer name `example-pipeline`.
- Documentation is rewritten for new users: a getting-started tour built on a
 small example project (`examples/tutorial`), a concepts guide with a glossary,
 and guides to diagrams, architecture checks (including a policy file
 reference), the library and extensions.
- `atlas compare` indents multi-line values, such as a struct's fields, under
 their label.

## atlas 0.8.0 / atlas-core 0.3.0

- Structural source facts come from syn. Invalid files stay in the inventory as
 syntax errors instead of disappearing.
- `Source::of`, projection `outline`/`declaration_lines`/`test_regions`, and the standalone `non_test_loc`, `TestRegions::of`, `struct_fields`, `impl_trait`, `restricted_path` and `import_bindings` readers return Result. A parse failure cannot masquerade as an empty inventory. `struct_fields` returns named `Field` values; `declaration_lines` returns owned `(line, Item)` facts. `LineItem` is retired. Obtain Item and navigation Site values from source/navigation rather than constructing partial struct literals. Public compatibility module paths still name the same implementations.

## atlas 0.7.0 / atlas-core 0.2.0

- The index owns correlated name/function/boundary tables. Its `functions()`, `boundaries()`, `roots()` and `inventory()` return borrowing slices; public mutable vectors are no longer exposed. Replace old field reads with these methods and use `to_vec()` when a client needs its own copy. Numeric positions belong to the index that produced them. `Target::InvalidFunction` explicitly reports an out-of-range caller; views must refuse it rather than classify it as an unresolved source call. Root public module paths and legacy JSON schemas remain stable.
- Embedded rendering bounds its child process I/O and keeps failure diagnostics.

## atlas 0.5.0

- The library's project-specific `architecture::source::read` is removed in
 favor of the declaration adapter protocol.

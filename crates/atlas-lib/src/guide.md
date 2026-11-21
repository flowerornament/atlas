# Using the library

Everything `cargo atlas` does is also available from Rust, through the
[`atlas-lib`](https://crates.io/crates/atlas-lib) crate. Use it to build your
own reports, write tests that check your project's structure, or run checks the
command line cannot express.

`atlas-lib` has no command-line, rendering or Node.js dependencies:

```toml
[dependencies]
atlas-lib = "0.9"
```

The optional `clap` feature adds command-line parsing support for diagram
options. The `cargo-atlas` package re-exports all of `atlas-lib` under the
same paths, as the `cargo_atlas` library, and adds the renderers and the
[declaration adapter](extensions.md) (`cargo atlas help extensions`) protocol.

## The basic pattern

Every use of the library follows the same steps:

1. **Take a snapshot** of the workspace. This reads every Rust file once.
2. **Ask questions** of that snapshot, directly or through an `Analysis` that
   adds qualified name resolution.
3. **Use the typed result.** Reports are ordinary Rust values. Formatting them
   and choosing exit codes is up to you.

```rust,no_run
use atlas_lib::{analysis::Analysis, discovery};
use std::path::Path;

fn tour(root: &Path) -> atlas_lib::Result<()> {
    // Read the workspace once. Every later question uses this snapshot.
    let snapshot = discovery::snapshot_workspace(root)?;

    // Build a qualified index over it, then ask for the representation facts:
    // every type, function signature, field, construction site and call.
    let analysis = Analysis::new(&snapshot)?;
    let facts = analysis.representations()?;
    println!("{} functions, {} types", facts.functions.len(), facts.types.len());

    // The steps of `main`, expanding the functions it calls one level deep.
    let (workflow, _diagram) = analysis.expanded("src/main.rs", "main", 1)?;
    println!("{}", workflow.ascii());
    Ok(())
}
```

A snapshot never changes. Editing files after taking it does not affect any
answer, and Atlas never writes to the files it read. Take a new snapshot to see
new changes.

## Where to start

| You want to | Start with |
| --- | --- |
| Read a workspace | `discovery::snapshot_workspace(root)`, or `snapshot_metadata(&metadata)` if you already ran `cargo metadata` |
| Ask qualified questions | `analysis::Analysis::new(&snapshot)` |
| Look up names and modules | `queries::navigation::Navigation` |
| Get a one-page arrival report | `queries::orient::orient(&snapshot, scope, rows)` |
| Ask who calls a function, or what it calls | `queries::callers::{callers, callees}(graph, &facts, selector, file, depth, within)`; with a semantic reading, `semantic_callers` and `semantic_callees`, which take it last |
| Measure files | `review::outline` |
| Compare two versions | `review::diff`, `review::moves::moved`, `review::moves::visibility` |
| Read an earlier commit | `revision::Revision::read` |
| Audit documentation | `documentation::audit` |
| Check an architecture policy | `checks::policy`, `checks::stages`, `checks::delta` |

The Rust documentation (`cargo doc --open -p atlas-lib`) describes every type.
The sections below explain how the pieces fit together.

## Snapshots

`discovery::snapshot_workspace` runs `cargo metadata`, finds each package's
files, and reads them. The snapshot keeps each file's bytes, the facts read
from its syntax, and a record of which packages and targets each file belongs
to. It does not keep syntax trees: an `Analysis` parses the files it needs
again, once each.

- A file that fails to parse stays in the snapshot. `Snapshot::source` still
  returns its bytes, and `syntax_error` explains the failure. Questions that
  need that file return an error instead of an empty answer.
- A file shared by several packages is stored once, with one entry per
  package. `physical_files()` counts each file once.
- `Snapshot::files(FileSelection::SourceTree)` selects each package's `src`
  directory; `Discovered` adds other files Cargo knows about.
- Modules come from written `mod` declarations. Unusual layouts, such as
  `#[path]` attributes pointing outside the package or symlinked modules, are
  reported as diagnostics rather than guessed at.
- Capture parses a package's files on several threads; the snapshot is the
  same as a serial read.
- To reuse syntax facts across runs, as the `cargo atlas` command does, open
  and install a cache once per process:
  `FactsCache::open(&dir, FactsCacheMode::Reuse)?.install()`. An entry is
  keyed by a file's exact bytes, its path and the running executable, so a
  hit holds the facts of the bytes just read. Without an installed cache
  nothing is stored. The cache is never used for a workspace that contains
  its directory.

## Qualified analysis

`Analysis` resolves names through `use` statements, modules and Cargo targets.
Build one per snapshot and reuse it for several questions; it parses each file
at most once and shares the work between questions.

- `representations()` returns `Facts`: types, function signatures, struct
  fields, construction sites and resolved calls. Anything it cannot resolve is
  kept as unknown.
- `expanded(path, function, depth)` returns the steps of one function, with
  calls expanded to the given depth, plus a diagram of them.
- `semantic(&SemanticIndex::read(&bytes))` reads every production call against
  a SCIP index you supply, such as one written by `rust-analyzer scip`. Each
  call keeps its qualified target and gains a second, labelled reading: a
  workspace definition, a trait method (dispatch open), an external item, a
  local value, no definition, ambiguous, or not indexed. Their `agreement` is
  reported, never resolved. An index that names no standard-library item, uses
  another position encoding or is malformed is `unavailable`, with the reason.
  `SemanticIndex::read_calls` reads calls without the comparison.
- `queries::callers::callers` and `callees` take the snapshot's graph and
  the `Facts` from `representations()`, not the `Analysis`: build the facts
  once and ask several call questions of them. `within` keeps the calling
  (or called) functions by file; `depth` is 1 to `MAX_DEPTH` (16).
- `with_tests(selection)` chooses whether test code is included:
  `Production`, `OutsideTestModules` or `All`.

Production code is the default, and `cfg` alternatives are kept side by side.

## Navigation

`Navigation` answers the same questions as `cargo atlas census`, `name`, `module`
and `dump`. Choose which visibility to include when you create it.

```rust,no_run
use atlas_lib::{discovery, population::VisibilitySelection, queries::navigation::Navigation};
use std::path::Path;

fn navigate(root: &Path) -> atlas_lib::Result<()> {
    let snapshot = discovery::snapshot_workspace(root)?;
    let navigation = Navigation::new(&snapshot, VisibilitySelection::RestrictedOrPublic)?;
    let census = navigation.census(None)?;       // everything, or Some("package")
    let tree = navigation.name("Tree")?;          // a name, or a path like parse::Tree
    let module = navigation.module("src/check.rs")?;
    let _ = (census, tree, module);
    Ok(())
}
```

Navigation relationships are *lexical*: they come from matching names, so
they are approximate. Every ambiguous candidate is kept, and a name that
matches nothing is marked unresolved rather than dropped. `dump()` returns all
the declarations, re-exports and relationships in one value.

## Relations

`relations::Relations::capture(graph, Some(&facts))` flattens a snapshot's
lexical graph and its qualified facts into the tables `cargo atlas facts`
exports, described in [facts.md](facts.md) (`cargo atlas help --topic facts`). Pass `None` for the facts to
capture only the lexical family. `relations::schema::RELATIONS` lists every
relation's columns, its `basis` column and its unknowns companion as data.

```rust,no_run
use atlas_lib::{analysis::Analysis, discovery, relations::Relations};
use std::path::Path;

fn export(root: &Path) -> atlas_lib::Result<()> {
    let snapshot = discovery::snapshot_workspace(root)?;
    let facts = Analysis::new(&snapshot)?.representations()?;
    let relations = Relations::capture(snapshot.graph(), Some(&facts));
    let calls = relations.get("call").map_or(0, |call| call.rows.len());
    relations.write_jsonl(std::io::stdout().lock())?;
    let _ = calls;
    Ok(())
}
```

Read columns by name with `Relation::records()` or `schema.position(name)`:
within one schema version, columns are only ever appended.
`Relations::read(text)` reads an export back, JSON or JSON Lines.

## Queries

`query::parse`, `query::check` and `query::run` evaluate the Datalog of
[query.md](query.md) (`cargo atlas help --topic query`) over a `Relations`. `check` refuses a program it cannot
resolve against the schema, and an honesty finding that `Options` does not
accept; `Checked::families()` names the families the program reads, so you
can capture only those. `run` returns each query's rows with the accepted
findings and the notes that say what the rows cannot show.

```rust,no_run
use atlas_lib::{discovery, query, relations::Relations};
use std::path::Path;

fn edges(root: &Path) -> atlas_lib::Result<()> {
    let program = query::parse(r#"?- file_calls{from: A, to: B, calls: N}, under(A, "src")."#)?;
    let checked = query::check(&program, &atlas_lib::relations::Family::ALL, &query::Options::default())?;
    let snapshot = discovery::snapshot_workspace(root)?;
    let facts = atlas_lib::analysis::Analysis::new(&snapshot)?.representations()?;
    let relations = Relations::capture(snapshot.graph(), Some(&facts));
    let answer = query::run(&checked, &relations, query::Limits::default())?;
    for row in &answer.tables[0].rows {
        let _ = row;
    }
    Ok(())
}
```

## Comparing versions

`revision::Revision::read(root, "HEAD~1")` copies one commit into a private
temporary directory, which is deleted when the `Revision` is dropped. It never
touches your working copy. Take a snapshot of `revision.path()` to compare it
with the current code:

```rust,no_run
use atlas_lib::{discovery, review, revision::Revision};
use std::path::Path;

fn review(root: &Path) -> atlas_lib::Result<()> {
    let current = discovery::snapshot_workspace(root)?;
    let previous = Revision::read(root, "HEAD~1")?;
    let before = discovery::snapshot_workspace(previous.path())?;

    let shape = review::diff(&before, &current, ".")?;              // file size and shape
    let moves = review::moves::moved(&before, &current, None)?;     // exact, moved, changed bodies
    println!("{} changed bodies", moves.changed.len());
    let _ = shape;
    Ok(())
}
```

- `review::diff` compares the size and shape of each file, excluding test
  modules.
- `review::moves::moved` compares item bodies. *Exact* means the tokens are
  the same, ignoring docs, visibility and layout. *Rerooted* means they are the
  same once module paths such as `crate::` and `super::` are ignored.
  *Changed* means anything else. Equal bodies pair within a file first;
  `exact_relocated` lists the exact ones that moved file.
- `review::moves::visibility` lists items whose written visibility changed.

None of these claims two pieces of code behave the same; they compare how the
code is written.

`Revision::read` needs Git and `tar`. In a jj workspace, it finds the
underlying Git store with `jj git root`. The repository must be at the
workspace root.

## Documentation audit

`documentation::audit(&snapshot, scope, &policy)` lists files without a `//!`
header, headers without a citation, and items without a `///` comment. The
`documentation::Policy` says which items to check (kinds, visibility, test
code) and which strings count as a citation. Atlas has no built-in citation
format.

## Architecture checks

The checks described in [architecture checks](checks.md) (`cargo atlas help checks`) are functions you can
call directly. This is also how you write a *project wrapper*: a small program
that supplies the project-specific inputs the command line cannot.

```rust,no_run
use atlas_lib::{analysis::Analysis, checks, checks::policy::{self, Policy}, discovery};
use std::path::Path;

fn check(root: &Path) -> atlas_lib::Result<()> {
    let text = std::fs::read_to_string(root.join(".config/architecture.toml"))?;
    let policy = Policy::parse(&text)?;
    let snapshot = discovery::snapshot_workspace(root)?;
    let analysis = Analysis::new(&snapshot)?;

    // Gather the observations the policy needs, then judge them.
    let inputs = policy::observe(&snapshot, &policy);
    let report = policy::validate(&analysis, &policy, &inputs)?;
    println!("{} violations, {} unavailable", report.violations.len(), report.unavailable.len());

    // Read stage paths for one package.
    let stages = checks::stages(&snapshot, &policy, Some("tutorial"))?;
    println!("{} stage violations", stages.policy_violations.len());
    Ok(())
}
```

Each report keeps violations and unavailable evidence apart. Treat a check as
passed only when both are empty.

**Supplying project inputs.** `policy::observe` gathers what Atlas can see on
its own. A wrapper can build `policy::Inputs::new(&snapshot)` itself and add
what only the project knows, such as whether an issue ID cited in the policy
really exists (`ReferenceResolution`). In the same way, `checks::delta::Inputs`
carries stage boundary declarations, header citations and commit-message
citations into `checks::delta::check`, and
`checks::stages::seams::BoundaryDeclaration` describes one stage's boundary:
`BoundaryDeclaration::new(root_file, content_fingerprint)` binds it to the
stage root's captured bytes, `with_listed` names its boundary items,
`with_listed_modules` names child modules it exposes wholesale (every public
item of each, and of the modules below it, is a boundary item), and
`with_citation_problems` reports what the project found wrong with its
citations.
Atlas checks these inputs against the snapshot (for example, that a
declaration's file has not changed) but leaves their meaning to your project.

**Held spellings.** `delta` counts `forbid_new_*` on both sides, so it needs
each side's occurrences: `delta::Inputs::policy` for the candidate and
`delta::Inputs::before_policy` for the base. Atlas fills the
`forbid_new_pattern` entries (`new_patterns`) with `policy::observe` and, for
the base under the published policy, `policy::observe_spellings`. A wrapper
fills `observed` for each `forbid_new_observed` class, per authority fact and
class name, on both sides. Each entry is a `SpellingState::Read` whose
`sources` maps every production file of that side's snapshot to its content
fingerprint, and whose `Occurrence`s give the file, byte range, line and the
exact text there. The text is the spelling (`class:text` is its key), so
supply the written form you mean to count. Atlas, not the wrapper, decides
which item owns each occurrence and drops test code and literals. If the
sources differ from the snapshot's production files, or an occurrence's text
is not the bytes at its range, the class is unavailable on that side. A
class you cannot observe is `SpellingState::Unavailable` with the reason;
leaving it out reports the same.

Other check functions:

- `checks::typed_errors::read(&analysis, packages)` finds `Result<_, String>`
  and similar error types in production code.
- `checks::delta::check` compares a baseline and a candidate, as `cargo atlas delta`
  does.

## Limits

The library has the same limits as the command line: no macro expansion, no
`cfg` evaluation, no type inference and no trait dispatch. See
[how Atlas works](concepts.md#limits) (`cargo atlas help concepts`).

# Changelog

Atlas has two packages: `cargo-atlas` (the `cargo atlas` command and its
library) and `atlas-lib`. Before 0.11.0 they were named `atlas` and
`atlas-core`. Versions below name the package they apply to.

## Unreleased

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
- **Breaking (library):** `source_index::Target::Summarized` holds a
 `Summary { kind, subject, message }` instead of a `String`; its `Display`
 is the message. New: `model::DiagnosticCode`, `Diagnostic::code`,
 `source_index::{Summary, SummaryKind}`, `UnknownCall::reasons`.
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
- **Breaking (CLI):** the `representations` command is now `types`, and its
 Pele output defaults to `target/atlas/types.svg`. `representations` still
 works as an alias.
- **Breaking (library):** cargo-atlas's `representations` view module is now
 `types` (the old path remains as a hidden re-export), and
 `render::View::Representations` is `View::Types`. atlas-lib's
 `representations` module and `Analysis::representations()` are unchanged.
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

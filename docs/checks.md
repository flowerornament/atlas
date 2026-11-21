# Architecture checks

As a project grows, its structure tends to drift. A module starts reaching into
another module it was never meant to use, a file keeps growing, a crate gains
public items nobody planned. Each change looks harmless on its own; together
they make the code harder to understand.

Atlas lets you write your intended structure down as rules, then checks the
code against them, for example on every pull request. The rules live in a TOML
file called a **policy**. The checks themselves are built into Atlas; there is
no scripting language to learn.

| Command | Question it answers |
| --- | --- |
| `policy` | Is the policy file valid, and does it match the code? |
| `stages` | Do the stages of the program only use earlier stages? |
| `delta` | Does a change break a rule or make anything worse than before? |
| `seams` | Which items cross stage boundaries, and which tests reach into other stages? Needs a [project wrapper](#project-wrappers); on its own it reports `UNAVAILABLE`. |
| `comments` | Which files and items are missing documentation? |

## Three possible outcomes

Every check sorts what it finds into three groups:

- **Violations**: the code breaks a rule.
- **Unavailable**: Atlas needed some evidence and could not get it. For
  example, a file did not parse, or a rule needs information only your project
  can supply (see [project wrappers](#project-wrappers)).
- **Observations**: facts Atlas reports without judging them.

A check passes only when there are no violations *and* nothing is unavailable.
Missing evidence is never counted as a pass.

## A first policy

The [tutorial project](getting-started.md) (`cargo atlas help getting-started`) is a small program in three stages:
`parse` turns text into a `Tree`, `check` turns a `Tree` into a `Checked`, and
`emit` turns a `Checked` into text. Its policy,
[`examples/tutorial/.config/architecture.toml`](../examples/tutorial/.config/architecture.toml),
says that the stages run in that order and that each stage only uses the one
before it. It also says that only `check` reads the syntax tree, that only
`main` writes to standard error, and how long and deeply nested a function
may be:

```toml
schema = 1
provenance = "authored"

# Each fact has one home. Atlas checks that the home exists, that only the
# listed files read it (each with its exact number of reads), and that the
# listed spellings do not spread or grow outside it.
authority = [
  { fact = "the syntax tree", home = "src/parse.rs::Tree", allowed_readers = [
    { file = "src/check.rs", reads = 1, reason = "checks parsed trees" },
  ] },
  { fact = "error output", home = "src/main.rs::main", forbid_new_pattern = [
    { name = "stderr", pattern = "eprintln!($$$)" },
  ] },
]

# Packages the size and surface rules apply to.
[population]
core = ["tutorial"]

# Upper limits on size, in lines that contain code.
[size]
core_token_lines = 200
crate = { tutorial = 200 }

# Upper limit on the number of public declarations per package.
[surface]
tutorial = 20

# Which workspace packages each package may depend on.
[crate_graph]
rule = [{ name = "tutorial", allowed = [] }]

# The stages, earliest first. A stage may only use earlier stages.
[[stage_order]]
crate = "tutorial"
stages = ["parse", "check", "emit"]

# What each stage may use from earlier stages.
[stage_reads.tutorial]
check = { reads = ["parse"] }
emit = { reads = ["check"] }

# Limits on how large files may grow, and other shape rules.
[concentration]
population = ["tutorial"]
entry_lines = 300
file_lines = 2000
conventional_names = []
header_uncited = { tutorial = 2 }
# The longest a function may be, in lines of code, and how deeply it may
# nest: the measures of Clippy's too_many_lines and excessive_nesting.
function_lines = 40
function_nesting = 4
```

The [policy reference](#policy-reference) below explains every section. First,
check that the policy is valid:

```console
$ cargo atlas policy --policy .config/architecture.toml
policy inputs: PASS · 1 core crates · 2 authority homes
```

## Checking a change with `delta`

`delta` is the check to run on every change. It compares two commits: the
**baseline**, an earlier commit you have already accepted, and the
**candidate**, the change being checked (by default, the current commit). It
measures both and refuses the candidate if it breaks a rule or makes a measured
number worse.

Suppose someone adds a function to `emit.rs` that uses `Tree` from `parse`
directly. The policy only lets `emit` use `check`, and only lets `check`
read the tree:

```console
$ cargo atlas delta HEAD~1 --policy .config/architecture.toml --citation-markers docs/
atlas: cargo atlas delta 8d58f188991e -> e0b7a38e0663 · 152ms
size      +4 core token-lines · 45/200 · 9.8 tokens/line
shape     src/emit.rs::emit block 3->5 · items 1->2 · 1 touched · DENSER
tree      entry files 0 lines over 300 (0) · files 0 lines over 2000 (0) · 0 ruled names · 0 aliases · 2 uncited roots
coupling  +0 -0 edges · 0 graph violations
surface   +1 public declarations · tutorial 11/20 · 2 authorities
spelling  error output: debt 0 -> 0 outside the home · 0 in tests or literals left out · files with macros a pattern cannot see into: 1
verdict   REFUSE
refusal   authority `the syntax tree`: src/emit.rs:3 reads the home `src/parse.rs::Tree` through `crate::parse::Tree`; src/emit.rs is not an allowed reader; read it through a listed reader, or add { file = "src/emit.rs", reads = 1, reason = "..." }
refusal   stage interface `tutorial`: src/emit.rs:3 emit reads parse (`crate::parse::Tree`), which its stage_reads row does not list; read it through the stage's input, or add `parse` to the row's reads with the reason in a comment
```

The one line breaks two rules: `emit` uses a stage it may not, and reads a
home it is not listed as reading.

Each line summarizes one kind of measurement:

- **size**: token-lines (lines that contain code), against the limit in `[size]`.
- **shape**: how the changed files' largest block and number of items changed.
- **tree**: file sizes and module names, against `[concentration]`.
- **coupling**: dependencies between workspace crates, against `[crate_graph]`.
- **surface**: the number of public items, against `[surface]`.
- **spelling**: for each `forbid_new_*` rule, how many of its spellings sit
  outside the home before and after the change.

The verdict is one of:

- `PASS`: no rule broken and nothing got worse.
- `REFUSE`: at least one rule is broken. Each `refusal` line says where, and
  how to fix it.
- `UNAVAILABLE`: Atlas could not get evidence a rule needs. Each `unavailable`
  line says what is missing. This verdict wins even when a rule is also broken,
  so supply the missing evidence first.

`DENSER` on the shape line means the largest changed block grew, or the code
got shorter but packed more onto each line. `core` means the packages listed
in `[population] core`.

Some projects require each module's `//!` header to point to the design note
that explains it, such as `See docs/parser.md`. The `[concentration]` section
counts headers that point nowhere. Atlas cannot tell what counts as a citation in your
project, so you tell it with `--citation-markers`. Without that option, the
verdict is `UNAVAILABLE`:

```console
$ cargo atlas delta HEAD~1 --policy .config/architecture.toml
...
verdict   UNAVAILABLE
unavailable concentration.before.source.src/lib.rs: required project header-citation observation unavailable
...
```

`delta` exits with an error for both `REFUSE` and `UNAVAILABLE`, and then prints
its report on standard error, so it can gate a CI job directly. With `--json`, a `REFUSE` verdict is returned as data and the
command exits successfully; read the `verdict` field. `UNAVAILABLE` still exits
with an error.

The policy file must be committed, because `delta` reads it from both commits.
The candidate must match your files on disk: if you have uncommitted changes to
tracked files, or new untracked source files, `delta` refuses rather than
guessing which version you meant. In jj, Atlas never snapshots your working
copy, so commit your work first.

### Spellings that may not spread

The tutorial's `error output` fact says `main` is the one place that writes
to standard error. If `check` starts printing a warning with `eprintln!`,
`delta` refuses:

```console
$ cargo atlas delta HEAD~1 --policy .config/architecture.toml --citation-markers docs/
...
spelling  error output: debt 0 -> 1 outside the home · 0 in tests or literals left out · files with macros a pattern cannot see into: 2
verdict   REFUSE
refusal   authority `error output`: `src/check.rs::check` newly holds 1 `stderr` outside its home at src/check.rs:12; spell it through the home, or give the item an `allowed_in` row (a count stays with its item, so a renamed, copied or new item starts at 0)
```

A project adopting such a rule usually has spellings outside the home
already. They may stay: `delta` counts them in the baseline, per item, and
refuses only growth (see [Authority](#authority-one-home-for-each-fact)).

### Rules that only go down

Many numbers in a policy are *ratchets*: they record how much of something the
code has today, and may only go down. For example, `header_uncited = { tutorial = 2 }`
says two files currently lack a citation. A change that adds a third is
refused, and a change that fixes one should lower the number to 1. Ratchets let
you adopt a rule in a project that does not follow it yet, and improve over
time.

## Checking stages with `stages`

If `parse.rs` used something from `emit`, `stages` would report:

```console
$ cargo atlas stages --policy .config/architecture.toml --scope tutorial
stage order `tutorial`: src/parse.rs:2 parse → emit names `crate::emit::emit`, a later stage; move the type to the stage that produces it, or amend the stage_order row if the order is wrong; never alias it
1 selected packages; no complete project policy verdict
```

`stages` reads every `use` and path in each stage and reports reads that go the
wrong way (a stage using a later stage), reads the policy does not allow, and
paths it could not place. `--scope` names the Cargo package. Add `--json` for
the full report, including every read it judged.

`stages` is a report. It always exits successfully and never claims a full
pass, because it checks only part of the policy. Use `delta` to gate changes.

## Checking documentation with `comments`

```sh
cargo atlas comments src --population pub-ish
cargo atlas comments --citation-markers docs/,RFC- --json
```

This lists files with no `//!` header, headers that cite nothing, and items
with no `///` comment. (`--audit`, which earlier versions required, is still
accepted.) `--citation-markers` lists the strings that count as a
citation, such as a path prefix (`docs/`) or an identifier prefix (`RFC-`).
Without it, citations are not checked. The audit reads `//!` and `///`
comments only, not `#[doc]` attributes or block comments. It reports coverage
and gives no verdict.

## A larger example: Atlas's own policy

Atlas checks itself with a policy of its own,
[`.config/architecture.toml`](https://github.com/flowerornament/atlas/blob/main/.config/architecture.toml).
It shows the sections a real project grows into: a crate graph for two
packages, a measured stage order with exact `stage_reads` rows, ten authority
homes with `forbid_item` rows, and a `[concentration]` section with ratchet
rows for today's large files, rulings for every repeated module name and the
crate-root re-exports kept for compatibility. Its
[ARCHITECTURE.md](https://github.com/flowerornament/atlas/blob/main/ARCHITECTURE.md#rules)
states each rule in words and names the policy field, test or lint that
checks it.

## Policy reference

A policy is a TOML file, usually at `.config/architecture.toml`. Atlas rejects
unknown fields, so a typo is an error rather than a silently ignored rule.

### Required fields

| Field | Meaning |
| --- | --- |
| `schema` | Always `1`. |
| `provenance` | Always `"authored"`: the file was written by people, not generated. |
| `authority` | A list of facts and the one place each is defined (see below). Must not be empty. |
| `[population] core` | The workspace packages that the size and surface rules cover. A package leaves `core` only with `amended_by = { bead = "<reference>", covers = ["population"] }` in `[population]`, cited in the change's description, as a loosening of `[concentration]` is. |
| `[size] core_token_lines` | The most lines containing code allowed across all `core` packages. |
| `[size] crate` | The most lines containing code allowed in each package. |
| `[size] approve_growth` | Instead of `core_token_lines` and `crate` (writing both is refused): no stored limits. `delta` reports each `core` package's lines of code before and after the change, and each package that grew as a raise for the project to approve, from its earlier size (a new package raises its whole size). Shrinking needs nothing. |
| `[surface]` | The most public declarations allowed in each package. |
| `[crate_graph]` | Which workspace packages may depend on which (see below). |
| `[concentration]` | Limits on file size and module naming (see below). |

### Authority: one home for each fact

```toml
authority = [
  { fact = "the syntax tree", home = "src/parse.rs::Tree" },
  { fact = "error codes", home = "src/error.rs::Code", forbid_item = ["ErrorCode"] },
]
```

Each entry names a fact and the item that defines it. This stops a second,
competing definition from appearing elsewhere: Atlas checks that the home
exists, and that the forbidden duplicates below do not. Optional lists catch duplicates elsewhere:

- `forbid_item`: item names that must not be declared.
- `forbid_path`: files or directories that must not exist.
- `forbid_pattern`: [ast-grep](https://ast-grep.github.io/) patterns that must
  not match. These need `ast-grep` installed.

A home is an item, `FILE::ITEM`, or a module: a file (`src/parse.rs`) or a
directory for the module rooted there (`src/parse/`).

`allowed_readers` lists the only files whose production code may read the
home, each with its exact number of reading sites:

```toml
authority = [
  { fact = "the parser", home = "src/parse/", allowed_readers = [
    { file = "src/check.rs", reads = 2, reason = "checks parsed trees" },
  ] },
]
```

A reading site is a `use` that names something in the home (each leaf of a
`use` list counts) or a written path that leads into it, followed through
`use`, `as`, `super`, re-exports (globs included) and type aliases, as
Atlas resolves calls. Later uses of an imported name are the import's
reading, not new ones. Test code is not read; code behind a feature gate
is. The home's own files are not readers. A file not listed, or reading
more than its row, refuses; a row its file reads fewer times can be
lowered (with an `owner`, the row must equal its count); a row whose file
no longer reads the home refuses, since an unused allowance would allow a
future read. `[]` allows no reader. A path Atlas cannot pin to one target
that might reach the home (competing imports, a `use` it cannot read), or
a path a macro spells without parsing as code, makes the check
`UNAVAILABLE`, naming the site and the fix; a macro's arguments that
parse as expressions are read as code.

`forbid_new_pattern` names [ast-grep](https://ast-grep.github.io/)
patterns that may not appear anew or grow outside the home; the ones
already there may stay. `allowed_in` lets a named item hold some:

```toml
[[authority]]
fact = "narrowing casts"
home = "src/width.rs::Width"
forbid_new_pattern = [{ name = "narrow", pattern = "$X as u8" }]
allowed_in = [
  { file = "src/render.rs", item = "render", counts = { "" = { narrow = 2 } }, reason = "one byte per colour channel" },
  { file = "src/show.rs", item = "impl Display for Width", counts = { fmt = { narrow = 1 } }, owner = "issue-12", reason = "prints the narrowed value" },
]
```

- `delta` counts each spelling on both sides, per file, owning item and
  spelling, each side under its own policy. An item's count may only fall.
  Test code and the text of string and character literals never count.
- A count moves only with its item: moving an item to another file, or
  renaming a whole file, is neutral. A fix in one item never pays for
  growth in another (not even one of the same name), and a renamed or
  copied item starts at 0.
- `allowed_in = [{ file, item, counts, owner, reason }]` lets an item hold
  a spelling up to `counts`, keyed by the path inside the item (`""` for
  the item itself, `"fmt"` for a method of an impl). `item` is a function,
  type, const or static, or a top-level trait impl by its header (generics
  and `where` kept, whitespace collapsed). A new or raised bound is a raise
  for the project to approve, and must equal what the item uses.
- `forbid_new_observed = [name]` does the same for spellings a project
  wrapper observes, each written text its own spelling (`name:text` in
  `counts`). The command line observes `forbid_new_pattern` on both sides;
  a `forbid_new_observed` class needs a wrapper to supply it, and is
  unavailable from the plain command line.
- A pattern cannot see inside a macro's input; `delta` reports how many
  files invoke macros, and a project that needs those counted supplies
  them as an observed class.
- Removing a spelling, or changing its pattern, while the baseline holds
  it refuses, unless `amended_by = { bead, covers = ["narrow"] }` in the
  authority entry names the ruling, covers only what the change alters,
  and the change's description cites it. Removing the whole authority
  entry leaves nowhere to amend, so it refuses while the baseline holds
  any.

### Crate graph

```toml
[crate_graph]
rule = [
  { name = "core", allowed = [] },
  { name = "cli", allowed = ["core"], law = "the CLI is a thin layer over core" },
]
```

Every workspace package needs a `rule`, unless it is listed as `vendored` or
`terminal`. `allowed` lists the workspace packages it may depend on (`[]`
for none), or is `"any"` for no limit; every rule must write it. Each
listed package must be one the package really depends on: an allowance it
does not use is refused, since a later dependency would use it without
anyone deciding to. `law` is a note shown when the rule is broken.
Dependency cycles are always refused. `cargo atlas policy` and `delta` both
judge the graph, from the dependencies Cargo declares.

Optional lists:

- `vendored`: packages copied into the workspace from elsewhere. They need no
  rule, and may depend on no other workspace package.
- `terminal`: packages nothing else may depend on, such as a binary or a test
  harness.
- `transparent`: packages anything may depend on. Each still needs a rule.
- `forbidden_members`: package names that must not be in the workspace.

### Stages

This variant of the tutorial's rules lets `emit` use only `check`'s boundary:

```toml
[[stage_order]]
crate = "tutorial"
stages = ["parse", "check", "emit"]

[stage_reads.tutorial]
check = { reads = ["parse"] }
emit = { input = ["check"] }
```

`stage_order` lists one package's top-level modules in the order data flows
through them, earliest first, once per package. A stage may never use a
later stage.

```toml
[[stage_order]]
crate = "compiler"
stages = ["parse", "check", "emit"]
pipeline = [
  { file = "src/lib.rs", function = "compile", calls = ["parse::parse", "check::check", "emit::emit"] },
]
```

`pipeline = [{ file, function, calls, loops }]` in a `stage_order` entry
names each function that runs the stages (a package may have several). The
body must make exactly `calls`, in straight-line code. `function` uses the
same selector as `flow` (a name, `Type::method`, an inline module's
`module::f` within the file, or `<Type as Trait>::method`), never a line
number and never a `#[cfg(test)]` function; one that names none or several
refuses, listing the candidates. A call is listed as written, never
resolved: a path (`parse::parse`, `Some`), `.method` for a method call, or
`macro!`. Every call counts wherever it appears, including in arguments,
receivers and `for` heads. A `?` on a call belongs to it, and one `Ok(..)`
around the body's final expression is not a call (what it wraps is). A
macro's tokens are read as expressions, so the calls in its arguments
count, but its expansion is not seen: listing a macro trusts it as a step.
A macro whose tokens are not expressions refuses. `loops = true` allows
`for` loops, each body judged by these same rules. Anything else refuses:
`if`, `match` and `let ... else`; `while` and `loop`; `return`, `break` and
`continue`; closures and `async` blocks; `&`, `&&`, `|`, `||` (and `&=`,
`|=`, or-patterns); struct update `..base`; `?` on a value; a turbofish,
`<T as Tr>::f(..)` or a callee that is not a path; a `#[cfg]` statement;
and labelled blocks, `try` and `const` blocks. References (`&x`), ranges,
arithmetic, comparisons, `as`, `.await`, assignments and literals are
allowed. Items written inside the body are declarations and are not read.

`stage_reads` says which earlier stages each stage may use, and how:

- `reads`: stages it may use freely.
- `input`: stages it may use only through their *boundary*, the items that
  stage's root module declares or re-exports.
- `debt`: known exceptions, as rows like
  `debt = { parse = { items = 3, owner = "issue-12" } }`. `items` counts the
  items read past the boundary and may only go down; `owner` names the issue
  that will remove them.

### Concentration

```toml
[concentration]
population = ["tutorial"]
entry_lines = 300
file_lines = 2000
conventional_names = ["error", "tests"]
header_uncited = { tutorial = 2 }
```

- `entry_lines`: the most lines of code an *entry file* may have. An entry
  file is one that declares child modules, such as `lib.rs`. Keeping entry
  files short keeps them readable as a table of contents.
- `file_lines`: the most lines of code any production file may have.
- `entry` and `file`: files allowed above those limits, as rows like
  `{ path = "src/big.rs", lines = 2400 }`. The recorded size may only go down.
  A row may name an `owner`, the reference that answers for it (an issue,
  say). An owned row must equal its file's measure: where a row without an
  owner is only told it can be lowered or deleted, an owned one is refused
  until it is. A project wrapper resolves each owner, as it does stage debt
  owners.
- `function_lines` and `function_nesting`: the longest a function body may
  be, in lines holding code, and how deep its blocks may nest, counted from
  the file's root (`impl`, `trait`, inline `mod` and each `{}` block add a
  level). They are the measures of Clippy's `too_many_lines` and
  `excessive_nesting`, so a project can use the same numbers as its Clippy
  settings. Production functions of the `population` packages and their
  build scripts are measured; tests are not. Atlas expands no macro: a
  function a `macro_rules!` writes is not measured, and blocks inside a
  macro's arguments are not counted, so Atlas can read lower than Clippy
  there. `cargo atlas facts` has the same numbers as `function.lines` and
  `function.nesting`.
- `function`: functions allowed over those limits, as rows like
  `{ file = "src/big.rs", function = "Parser::expr", lines = 140 }` (or
  `nesting = 8`, or both). `function` is written as `cargo atlas flow
  --function` prints it: `Type::method`, `<Type as Trait>::method`,
  `module::name`, with `#2` for the second function of one name in a
  file. Rows behave as `entry` and `file` rows, `owner` included. Over a
  change, `delta` refuses a raised or removed function limit and a new
  `generated` path, holds a row to the lesser of its value and its
  function's earlier measure, and reports a new or raised row as a raise
  for the project to approve. A row follows its function to another file
  only when the function moved unchanged; where a file has two functions
  of one name, a row stays with the one it was written for.
- `generated = [{ path, reason }]`: files or directories whose functions are
  not measured.
- `conventional_names`: module names any package may reuse, such as `error`.
- `name`: other module names used in more than one place, each with a
  `ruling`: `facet` (one idea seen from several places, which is fine) or
  `collision` (two ideas sharing a name, owed a rename).
- `alias`: re-exports that give an item a second home or name. Rows may only be
  removed.
- `header_uncited`: per package, how many module roots have a `//!` header that
  cites nothing. May only go down.

### Optional sections

- `typed_errors`: packages whose production code must not use `String` as an
  error type, as in `Result<T, String>`. (The tutorial does, so it does not
  enable this rule.)
- `stage_seams`: item-level boundaries between stages: what each stage
  exports, who may build shared types, and how far tests reach into other
  stages. This needs a [project wrapper](#project-wrappers).

  ```toml
  [stage_seams.compiler]
  floor = ["vocabulary"]
  final_stages = ["report"]

  [stage_seams.compiler.writers.Graph]
  stage = "lower"
  reason = "every later stage reads the graph"
  producers = ["lower"]
  owner = "issue-30"
  test_sites = { "check" = 2 }
  production_sites = {}
  ```

  `final_stages` lists stages no stage reads except the composition, the
  last stage of the crate's `stage_order`. A stage reads a final stage
  when its `stage_reads` row names it or a production path into it is
  observed; each such reader refuses once, at its first site. A final
  stage must be a stage, outside the floor, and not the composition.
  `final_stages = []` claims there are none. Once published, the list is
  the policy author's: `delta` refuses a landing that adds a final stage,
  drops one, removes the list, or moves the composition, unless
  `amended_by = { bead, covers = ["final_stages"] }` in the crate's
  `stage_seams` names the ruling and the landing's description cites it.
  Moving the composition changes who may read the final stages, so it is
  the same decision and the same cited amendment admits it.
  A `writers` row records the calls outside its producers that build or
  change its type, per calling owner (a stage or test module) and apart by
  population: `test_sites = { "<owner>" = n }` and `production_sites`, both
  required, `{}` for none. Each count must equal what the census measures
  and may only fall from the published one (an owner the published row
  does not name had 0), so a production writer cannot hide behind a test
  count. A row still written with the combined `sites` refuses; a
  published row that has it bounds each population by that count and by
  what the base tree measures there. A file mounted from outside `src`
  counts as test code when its mount is test-gated, or its gate needs test
  or only a declared test-support feature (`--test-support-features` on
  `seams` and `delta`); a production file mounted from outside `src`
  leaves the seam reading unavailable, since the stage reading reads `src`
  alone.

#### Test code, by check

Each check draws the line between test and production code itself:

| Check | Counts as test code |
|---|---|
| `concentration` | test modules, test functions and targets, and module remaps gated to test or a declared test-support feature |
| `stage_seams` | test files and modules, and mounts gated to test or a declared test-support feature |
| `allowed_readers`, `forbid_new_*` | test modules and functions only: code gated to a test-support feature is production on purpose, since reaching a home through test support is still coupling to it |

The Rust documentation for `atlas_lib::checks::policy::Policy` describes each
field in more detail.

## Project wrappers

Some rules depend on conventions only your project knows. For example: what
makes a module's boundary list valid, or which issue IDs a commit message may
cite to justify loosening a rule. Atlas does not guess these. Instead, the
library accepts them as typed inputs from a small program your project owns,
called a **project wrapper**:

- `checks::policy::Inputs` for policy observations and authority,
- `checks::delta::Inputs` for boundary declarations, header citations and
  commit citations,
- `checks::stages::seams::BoundaryDeclaration` for stage boundaries.

The `cargo atlas` command line cannot supply these, so a rule that needs one
reports `UNAVAILABLE` there. That is deliberate: Atlas reports the gap
instead of passing a rule it cannot check. See the
[library guide](library.md#architecture-checks) (`cargo atlas help library`) to build a wrapper.

## Limits

- Stage checks follow written paths and `use` statements. This is close to,
  but not the same as, the compiler's name resolution. Paths Atlas cannot
  place are reported as unjudged, never as allowed.
- Checks measure how code is written. They do not prove it compiles or
  behaves the same.
- Revisions are read from the repository in the workspace root (`.git` or
  `.jj`). Atlas does not use a repository in a parent directory.

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

The [tutorial project](getting-started.md) is a small program in three stages:
`parse` turns text into a `Tree`, `check` turns a `Tree` into a `Checked`, and
`emit` turns a `Checked` into text. Its policy,
[`examples/tutorial/.config/architecture.toml`](../examples/tutorial/.config/architecture.toml),
says that the stages run in that order and that each stage only uses the one
before it:

```toml
schema = 1
provenance = "authored"
authority = [{ fact = "the syntax tree", home = "src/parse.rs::Tree" }]

[population]
core = ["tutorial"]

[size]
core_token_lines = 200
crate = { tutorial = 200 }

[surface]
tutorial = 20

[crate_graph]
rule = [{ name = "tutorial", allowed = [] }]

[[stage_order]]
crate = "tutorial"
stages = ["parse", "check", "emit"]

[stage_reads.tutorial]
check = { reads = ["parse"] }
emit = { reads = ["check"] }

[concentration]
population = ["tutorial"]
entry_lines = 300
file_lines = 2000
conventional_names = []
header_uncited = { tutorial = 2 }
```

The [policy reference](#policy-reference) below explains every section. First,
check that the policy is valid:

```console
$ cargo atlas policy --policy .config/architecture.toml
policy inputs: PASS · 1 core crates · 1 authority homes
```

## Checking a change with `delta`

`delta` is the check to run on every change. It compares two commits: the
**baseline**, an earlier commit you have already accepted, and the
**candidate**, the change being checked (by default, the current commit). It
measures both and refuses the candidate if it breaks a rule or makes a measured
number worse.

Suppose someone adds a function to `emit.rs` that uses `Tree` from `parse`
directly. The policy only lets `emit` use `check`:

```console
$ cargo atlas delta HEAD~1 --policy .config/architecture.toml --citation-markers docs/
atlas: cargo atlas delta 08811eb74b74 -> 81668df96124 · 376ms
size      +4 core token-lines · 45/200 · 9.8 tokens/line
shape     src/emit.rs::emit block 3->5 · items 1->2 · 1 touched · DENSER
tree      entry files 0 lines over 300 (0) · files 0 lines over 2000 (0) · 0 ruled names · 0 aliases · 2 uncited roots
coupling  +0 -0 edges · 0 graph violations
surface   +1 public declarations · tutorial 11/20 · 1 authorities
verdict   REFUSE
refusal   stage interface `tutorial`: src/emit.rs:3 emit reads parse (`crate::parse::Tree`), which its stage_reads row does not list; read it through the stage's input, or add `parse` to the row's reads with the reason in a comment
```

Each line summarizes one kind of measurement:

- **size**: token-lines (lines that contain code), against the limit in `[size]`.
- **shape**: how the changed files' largest block and number of items changed.
- **tree**: file sizes and module names, against `[concentration]`.
- **coupling**: dependencies between workspace crates, against `[crate_graph]`.
- **surface**: the number of public items, against `[surface]`.

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
cargo atlas comments --scope src --population pub-ish
cargo atlas comments --citation-markers docs/,RFC- --json
```

This lists files with no `//!` header, headers that cite nothing, and items
with no `///` comment. (`--audit`, which earlier versions required, is still
accepted.) `--citation-markers` lists the strings that count as a
citation, such as a path prefix (`docs/`) or an identifier prefix (`RFC-`).
Without it, citations are not checked. The audit reads `//!` and `///`
comments only, not `#[doc]` attributes or block comments. It reports coverage
and gives no verdict.

## Policy reference

A policy is a TOML file, usually at `.config/architecture.toml`. Atlas rejects
unknown fields, so a typo is an error rather than a silently ignored rule.

### Required fields

| Field | Meaning |
| --- | --- |
| `schema` | Always `1`. |
| `provenance` | Always `"authored"`: the file was written by people, not generated. |
| `authority` | A list of facts and the one place each is defined (see below). Must not be empty. |
| `[population] core` | The workspace packages that the size and surface rules cover. |
| `[size] core_token_lines` | The most lines containing code allowed across all `core` packages. |
| `[size] crate` | The most lines containing code allowed in each package. |
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

### Crate graph

```toml
[crate_graph]
rule = [
  { name = "core", allowed = [] },
  { name = "cli", allowed = ["core"], law = "the CLI is a thin layer over core" },
]
```

Every workspace package needs a `rule`, unless it is listed as `vendored` or
`terminal`. `allowed` lists the workspace packages it may depend on; leave it
out to allow any. `law` is a note shown when the rule is broken. Dependency
cycles are always refused.

Optional lists:

- `vendored`: packages copied into the workspace from elsewhere. They need no
  rule.
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
through them, earliest first. A stage may never use a later stage.

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
[library guide](library.md#architecture-checks) to build a wrapper.

## Limits

- Stage checks follow written paths and `use` statements. This is close to,
  but not the same as, the compiler's name resolution. Paths Atlas cannot
  place are reported as unjudged, never as allowed.
- Checks measure how code is written. They do not prove it compiles or
  behaves the same.
- Revisions are read from the repository in the workspace root (`.git` or
  `.jj`). Atlas does not use a repository in a parent directory.

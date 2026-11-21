# How Atlas works

This page explains the ideas that every Atlas command shares. Once you know
them, `cargo atlas <command> --help` tells you the rest. A [glossary](#glossary) at
the end defines the words Atlas uses in its output.

## Reading a project

When you run a command, Atlas does three things:

1. **Finds the code.** It asks Cargo (`cargo metadata`) which packages the
   workspace has and where their source files are. It then follows the `mod`
   declarations in those files.
2. **Takes a snapshot.** It reads every Rust file once, into memory. Every
   question in that command is answered from this snapshot, so editing files
   while Atlas runs cannot change the answer.
3. **Parses the code.** It turns each file into a syntax tree with
   [syn](https://docs.rs/syn), the parser most Rust macros use. It does not run
   the compiler.

Atlas never writes to the source files it reads.

A file that does not parse is not skipped. It stays in the results as a syntax
error, so a broken file cannot quietly turn into "zero problems found".

One file can belong to more than one Cargo target, for example a file used by
both a library and a binary. Atlas keeps one copy of the file and notes each
target it belongs to. It does not pick one as the "real" owner.

## Three kinds of evidence

Atlas can know something about your code in three different ways. Similar
looking commands can rely on different kinds, and each answer says which kind
it uses.

| Evidence | How Atlas knows | Used by |
| --- | --- | --- |
| **Lexical** | The same name is written in two places. Quick, but two things with the same name can be confused. | `census`, `name`, `module`, `dump`, `map --view types` |
| **Qualified** | Atlas followed `use` statements, modules and Cargo targets to work out exactly which item a call or type refers to. If it cannot work it out, the item is marked *unresolved*. | `dependencies`, `callers`, `callees`, `types`, `compare`, `overview`, `flow --depth` |
| **Declared** | You wrote it down: an architecture policy, or a table in your code that an adapter reads. | `stages`, `seams`, `policy`, `delta`, `program`, `map --architecture` |

A glob import (`use super::*`, `use crate::model::*`) is followed into
workspace modules only. A glob of the module itself or of an ancestor binds
every name there, private items and that module's own `use` imports
included; a glob of another module binds its non-private items and
re-exports. A name the module declares or imports by name shadows every
glob; two globs binding different items leave the call ambiguous; a glob of
another package, or a glob inside the globbed module, is not followed.
Items a macro writes are not seen, so a glob can appear to bind a name such
an item shadows.

A fourth kind, **semantic**, comes from outside Atlas: a SCIP index that a
compiler front end such as rust-analyzer wrote (`rust-analyzer scip .`). Pass
it with `--scip FILE` to `dump --json` or `flow`, and each call gets a second,
labelled reading beside Atlas's own: which item the indexer resolved it to, a
trait method whose implementation is chosen at run time, an item of another
package, or nothing usable. `dump` also says whether the two readings agree.
With `--scip` Atlas does not run the indexer: producing the index is your
step, and it runs your project's build scripts and procedural macros.

`--semantic` instead of `--scip FILE` asks Atlas to run `rust-analyzer scip`
itself. That is the one Atlas option that **executes your project's code**:
rust-analyzer builds and runs its build scripts and procedural macros, so use
it only on code you would build. On a large workspace of 26 crates it took one
to four minutes and over 6 GB of memory; a small crate takes seconds. Atlas builds under `<target>/atlas/semantic`, never in your normal
build directories, and stops the indexer (and everything it started) after
`--semantic-timeout` seconds (default 1800). It digests your manifests,
configuration, build scripts and sources before and after the run and
refuses an index made while any of them changed. It also refuses, before
running anything, when rust-analyzer is not installed (`ATLAS_RUST_ANALYZER`
names it if it is not `rust-analyzer` on `PATH`) or the toolchain has no
`rust-src`. The index is kept in the user cache directory, keyed by those
inputs and the indexer and toolchain versions, so asking again about an
unchanged workspace takes a second; `--no-cache` turns that off. Its
`semantic_indexing` record says how the index was obtained. The index reflects
one build configuration, the indexer's host target and default features, and
every reading says so. Atlas checks each call's name against the source it
read; an index of different text reads as *not indexed* there. An index that
names nothing from the standard library (what rust-analyzer writes when it
cannot find `rust-src`) is reported *unavailable*.

`callers`, `callees`, `dependencies` and `flow` take `--evidence` to name the
reading they answer from. `auto` (the default) and `qualified` are their
current reading. `callers` and `callees` answer from the semantic reading
too when given `--scip FILE` or `--semantic` (`--evidence semantic` names
it, and needs one of them): the calls the index names a function
for are added as rows marked `semantic`, and only the unresolved calls it
pins to nothing stay listed. Elsewhere `lexical` and `semantic` are refused
with what to run instead.

Atlas never mixes these up. A rule you declared is never drawn as if it had
been observed in the code. A relationship observed in the code is never hidden
just because your rules do not allow it.

## Unknown is not the same as absent

Atlas cannot work out everything. Macros, trait methods and code it cannot
parse all leave gaps. When there is a gap, Atlas says *unresolved*,
*unknown* or *unavailable*. It never reports a gap as "no relationship" or
"zero". In the same way, a check that is missing evidence reports
`UNAVAILABLE`, not `PASS`.

`callers` shows this in one answer. Its resolved rows are the calls Atlas
followed to the function. A call it could not resolve, but that is written
with the function's name, is listed apart as a *possible* caller, counted by
why it stayed unresolved, and never counted as a caller. A function with
neither is reported as having no resolved callers, never as unused: calls
inside macros, or through a function passed elsewhere as a value, are not
recorded. (A closure's own calls are recorded, as calls of the function that
writes it.) `--depth N` follows resolved calls up to N levels, and
counts the unresolved ones beyond the first level instead of listing them.

## Choosing what to read

### The project root

Commands read the Cargo project in the current directory. To read another
one, pass `--root path/to/project`.

### Scope

A scope narrows a command to part of the project. Name it after the command,
as in `cargo atlas census crates/core/src`. Atlas reads the name as the first
of these that matches:

1. a path relative to the project root, such as `crates/core/src`;
2. a file or directory name that is unique in the project, such as `parse.rs`;
3. a Cargo package name.

A name that is both a path and a package somewhere else, or that matches
several paths, is refused with the choices. `--in PATH` reads only a path or
name, and `-p PACKAGE` only a package. A directory inside the project is
always a scope; to read another project, use `--root`.

- `diff`, `moved` and `visibility` take a path with `--in`, since their bare
  argument is the revision.
- Stage and seam checks take a Cargo package name with `--scope`.
- The 0.18 spelling `--scope PATH` still works until 1.0, with a one-line
  note on stderr.

A scope that matches nothing is an error. Atlas does not report an empty result
in that case, because an empty result would look like a real answer.

### Population

Every count depends on what is included. Atlas calls the set of items it counts
the *population*, and lets you choose it:

- `--population all` counts every item, including private ones.
- `--population pub` counts only items marked `pub`.
- `--population pub-ish` counts `pub` items and items with limited visibility,
  such as `pub(crate)`.

Defaults differ by command. Without `--population`, `census`, `name` and
`module` show every declaration they found; with it, they print a summary of
items with that visibility, per package. `comments` defaults to `pub-ish`.

Code inside a test module (`#[cfg(test)] mod tests { ... }`) is left out by
default. A single function marked `#[cfg(test)]` outside a test module is
still counted by line-counting reports such as `nontest-loc`, but not by the
qualified views. Each report says which rule it used.

## Comparing revisions

Review commands compare your current code with an earlier commit:

```sh
cargo atlas diff HEAD~1                 # Git: the previous commit
cargo atlas diff @-                     # jj: the previous change
cargo atlas compare --vs v1.2.0 --at v1.3.0
```

Atlas turns the name you give into an exact commit, then copies that commit's
files into a private temporary directory. It never checks out a commit,
stashes your changes, fetches from a remote or takes a jj snapshot.

The repository (`.git` or `.jj`) must be in the project's root directory.
Atlas does not look for a repository in a parent directory.

## Output

Output is text by default. Long lists are shortened, and Atlas says how much
it left out.

- Text lists are bounded so a page stays short: each list shows a few rows,
  and a list that leaves rows out ends with `… N more;` and a command that
  shows them. `--rows N` chooses how many; `--all` shows every row.
- `--as json` (or `--json`) prints everything, including ambiguous and unresolved items. Each
  JSON report has a `schema` number and describes what it selected and which
  source it read.
- `--as svg` draws an SVG image, and `--as html` (on `flow`, `map` and
  `program`) an HTML page around it. Drawing needs Node.js 20 or newer; the
  drawing code is built into Atlas. The 0.18 spellings `--backend` and output
  `--format` still work until 1.0, with a one-line note on stderr
  (`ATLAS_NOTICES=off` hides it).
- `--out path` chooses the output file. Relative paths start at the project
  root. Without it, text goes to the terminal and images go under
  `target/atlas/`.

Each diagram is saved with a [Mermaid](https://mermaid.js.org/) version next to
it, and a JSON file of the evidence behind it. `flow` and `map` write their JSON
evidence under `target/atlas/`, or to the directory given by `--evidence-dir`. Atlas refuses to write output over
any file it read, such as a source file or a saved question. If drawing fails,
no half-finished image is left behind.

## Commands at a glance

| Task | Commands |
| --- | --- |
| Find your way around | `orient`, `census`, `name`, `module`, `callers`, `callees`, `dump`, `facts`, `query` |
| Review a change | `diff`, `moved`, `visibility`, `compare` |
| Measure | `spans`, `concentration`, `nontest-loc`, `comments` |
| Draw diagrams | `dependencies`, `types`, `flow`, `overview`, `program`, `map` |
| Check architecture | `delta`, `stages`, `policy`, `seams` |

`cargo atlas orient` is the place to start on an unfamiliar project. It prints
one page: the Cargo targets, where the non-test lines are, the largest blocks,
the most-called functions and most-mentioned types, and how many calls Atlas
could resolve. Each list states its unit and order and names the command that
shows the rest, and the page ends with commands to run next. Tied rows and
several binaries get a placeholder rather than an arbitrary pick.

`cargo atlas --help` lists every command. Each command's `--help` ends with a short
note: the question it answers, what it counts or what evidence it uses, and an
example.

## Limits

These limits apply to every command:

- **Macros are not expanded.** A macro call appears as one opaque step.
  An initializer-free `#[distributed_slice]` or `#[linkme::distributed_slice]`
  static with a written slice type is retained as a written declaration. Atlas
  does not resolve the attribute's origin, expand it, or infer linked members
  or the generated type.
- **`cfg` options are not evaluated.** When code has alternatives for different
  features or platforms, Atlas keeps all of them side by side.
- **Types are not inferred.** Atlas knows a type only where the code writes it.
- **Dynamic dispatch is not resolved.** Atlas does not decide which trait
  implementation a call will use.
- **Paths are not proved.** `flow` shows what the code's structure allows. It
  does not prove that a branch can be taken or that a loop ends.
- **Modules come from the source.** Atlas follows written `mod` declarations
  and Cargo targets. Modules created by macros or build scripts are not seen.

When one of these limits affects an answer, the report says so.

## Glossary

**Ambiguous.** A name matches more than one declaration. Atlas lists all of
them.

**Baseline and candidate.** In a check that compares two commits, the
baseline is the earlier, accepted commit, and the candidate is the change
being checked.

**Block.** The lines from one top-level declaration to the next. "Largest
block" is the size of the biggest one in a file.

**Declaration.** Anything the code defines with a name: a function, struct,
enum, trait, module, constant and so on. A *declaration site* is the place
where it is defined.

**Evidence.** The reason Atlas believes something: lexical, qualified or
declared. See [three kinds of evidence](#three-kinds-of-evidence).

**Opaque.** A step Atlas knows is there but does not look inside, such as a
macro call or a closure.

**Physical file.** A file on disk. One physical file can belong to several
Cargo targets.

**Population.** The set of items a report counts, for example "public items
outside tests".

**Policy.** A TOML file where you write your project's structural rules. See
[architecture checks](checks.md) (`cargo atlas help checks`).

**Production code.** Code outside tests.

**Rerooted.** Two item bodies that are the same once module paths such as
`crate::` and `super::` are ignored. Code moved between modules is usually
rerooted.

**Representation.** A type that carries data from one part of the program to
another, such as `Tree` or `Checked` in the tutorial. A representation's
*contracts* are the function signatures that take or return it.

**Rescoped.** An item whose restricted visibility now names a different
module, such as `pub(in crate::a)` becoming `pub(in crate::b)`.

**Scope.** The part of the project a command looks at.

**Snapshot.** The copy of the source that Atlas reads into memory at the start
of a command.

**Stage.** One step of a program that runs in a fixed order, such as parse,
then check, then emit. Stages are declared in a policy.

**Token-line.** A line that contains code, not just whitespace or comments.
Size limits in a policy count token-lines.

**Unavailable.** A check needed evidence that it could not get. Unavailable
is never treated as a pass.

**Unresolved.** Atlas could not work out what a call or type refers to. It is
not treated as absent.

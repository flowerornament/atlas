# Getting started

This tour uses a tiny example project to show what Atlas can do. It takes about
fifteen minutes. You need Atlas installed (see the [README](../README.md#install))
and Git.

## The example project

[`examples/tutorial`](../examples/tutorial) is a very small "compiler" in
four steps:

1. `parse.rs` splits text into words and stores them in a `Tree`.
2. `check.rs` rejects an empty `Tree` and wraps a good one in `Checked`.
3. `emit.rs` turns a `Checked` program back into text.
4. `main.rs` runs the three steps on a command-line argument.

Get the Atlas repository, copy the project somewhere else, and put the copy
under Git so you can make changes and compare them later:

```sh
git clone https://github.com/flowerornament/atlas.git
cp -r atlas/examples/tutorial /tmp/tutorial
cd /tmp/tutorial
git init && git add -A && git commit -m "start"
```

Every command below runs inside `/tmp/tutorial`. Atlas reads the project in the
current directory. To read a project somewhere else, add `--root path/to/project`.

## 1. See what the project contains

```console
$ cargo atlas census
workspace: 5 files (0 test-only), 11 production declarations
  Const: 1
  Enum: 1
  Fn: 4
  Mod: 3
  Struct: 2
5 lexical relationships; 0 ambiguous, 0 unresolved; 0 diagnostics
Lexical declaration evidence; candidate identity is approximate, not runtime use or ownership.
```

A *declaration* is anything the code defines with a name: a function, a
struct, a module, a trait and so on. The project has eleven of them across five
files. "Production" means code outside tests.

The last line says how much to trust these numbers. Atlas found the
relationships by matching names in the text ("lexical" evidence). That is
quick and works everywhere, but two things with the same name can be confused.
The [concepts guide](concepts.md#three-kinds-of-evidence) explains the
different kinds of evidence Atlas uses.

## 2. Look up a name

```console
$ cargo atlas name Tree
Tree: 1 declaration site

src/parse.rs:4:Tree [pub struct]
  The parsed program.
  0 outgoing, 3 incoming candidate-unique, 0 incoming ambiguous
  pub struct Tree {
```

Atlas shows where `Tree` is defined, its doc comment and its first line. Three
places in the code mention `Tree` ("incoming"), and each mention matches only
this one definition ("candidate-unique"). "Outgoing" counts the names that
`Tree`'s own definition mentions.

Now try a name that is used twice:

```console
$ cargo atlas name check
check: 2 declaration sites (ambiguous)

src/check.rs:10:check [pub fn]
  Check a parsed tree.
  2 outgoing, 0 incoming candidate-unique, 0 incoming ambiguous
  pub fn check(tree: Tree) -> Result<Checked, String> {

src/lib.rs:2:check [pub mod]
  0 outgoing, 0 incoming candidate-unique, 0 incoming ambiguous
  pub mod check;
```

`check` is both a module and a function. Atlas lists both instead of picking
one. This is a rule throughout Atlas: when something is ambiguous, you see
every candidate. To pick one, give the end of its module path,
`cargo atlas name check::check`, or its site, `cargo atlas name
src/check.rs:10`.

Add `--relations` to see the mentions themselves.

## 3. Look inside a file

```console
$ cargo atlas module src/check.rs
src/check.rs: 1 physical files, 4 incident lexical relationships

src/check.rs (2 production declarations)
  src/check.rs:5 Struct Checked
  src/check.rs:10 Fn check
```

`module` accepts a file, a directory or a Cargo package name. "Incident
lexical relationships" are the name mentions that start or end in this file.

## 4. Follow a function

`flow` shows the steps inside one function, in the order they are written,
including every branch.

```console
$ cargo atlas flow --path src/main.rs --function main
main (src/main.rs:4)

-> std::env::args() [src/main.rs:5]
-> std::env::args().nth(1) [src/main.rs:5]
-> text = std::env::args().nth(1).unwrap_or_default() [src/main.rs:5]
-> &text [src/main.rs:6]
-> parse(&text) [src/main.rs:6]
-> check(parse(&text)) [src/main.rs:6]
-> match check(parse(&text)) [src/main.rs:6]
   | pattern Ok(program)
    -> println!("{}", emit(&program)) [opaque] [src/main.rs:7]
   | pattern Err(message)
    -> eprintln!("error: {message}") [opaque] [src/main.rs:8]

return from main → () (if the body completes)
```

The `match` has two branches, one for each pattern. `println!` is a macro.
Atlas does not expand macros, so the whole call is one *opaque* step: Atlas
knows it is there but does not look inside it. That is why the call to `emit`
inside `println!` is not shown as its own step.

`flow` also writes two JSON files under `target/atlas/` that record exactly
what it saw. Atlas writes only inside the project's `target` directory, or
wherever you point `--out`. It never changes your source files.

## 5. Draw how files depend on each other

```console
$ cargo atlas dependencies --relation calls
Source Calls: workspace
5 files; 2 links between files

+-- src
|  [n1] check.rs
|  [n2] emit.rs
|  [n3] lib.rs
|  [n4] main.rs
|  [n5] parse.rs
+

RELATIONSHIPS
  [main.rs] --calls: main calls check (1 site)--> [check.rs]
  [main.rs] --calls: main calls parse (1 site)--> [parse.rs]

EVIDENCE / LIMITS
  - ... 11 unresolved/external call observations are not proved absent.
  ...
```

These relationships are *qualified*: Atlas followed the `use` statements and
module structure to work out which function each call refers to. The call to
`emit` is missing because it sits inside the `println!` macro. The limits
section says that 11 calls could not be resolved (mostly calls into the
standard library), and that this does not mean they are absent.

Try `--relation signatures` to see which files use another file's types in
their function signatures:

```console
$ cargo atlas dependencies --relation signatures
...
RELATIONSHIPS
  [check.rs] --signature names: check.tree : Tree (1 site)--> [parse.rs]
  [emit.rs] --signature names: emit.program : &Checked (1 site)--> [check.rs]
```

If you have Node.js installed, add `--backend pele` to draw the same diagram
as an SVG image. Atlas saves it under `target/atlas/` and prints the path.

## 6. Follow a type

The example includes a small saved question, [`pipeline.json`](../examples/tutorial/pipeline.json),
that names the two main types. `types` shows which functions take
and return them:

```console
$ cargo atlas types --query pipeline.json --panel contracts
From text to output
Contracts and direct fields, not runtime value flow.

A  REPRESENTATION CONTRACTS
  src/check.rs:10  check
    (Tree [tree]) -> (Checked [return · Ok])
    literal Checked at line 14
  src/parse.rs:9  parse
    () -> (Tree [return])
    literal Tree at line 10
```

Read the first entry as: `check` takes a `Tree` (in its argument `tree`) and
returns a `Checked` inside `Ok`, and it builds a `Checked` value directly at
line 14. The input of `parse` shows as `()` because `&str` is not one of the
selected types. `emit` is not listed at all: without a list of functions in
the saved question, Atlas picks public functions that *return* a selected
type, and `emit` only takes one.

## 7. Review a change

Make a change: limit the length of a program, and make the `words` field
visible only inside the crate.

```sh
# In src/parse.rs, change `pub words` to `pub(crate) words`.
# In src/check.rs, add a second rule before `Ok(Checked { tree })`:
#     if tree.words.len() > 100 {
#         return Err("program too long".into());
#     }
git commit -am "Limit program length; narrow Tree.words"
```

Now compare against the previous commit (`HEAD~1`; in jj, use `@-`):

```console
$ cargo atlas diff HEAD~1
Module shape against a0914cf... (current source)
src/check.rs
  before: 15 non-test lines; 2 top-level items; largest block 6
  after: 18 non-test lines; 2 top-level items; largest block 9
src/parse.rs
  before: 13 non-test lines; 2 top-level items; largest block 5
  after: 13 non-test lines; 2 top-level items; largest block 5

$ cargo atlas moved HEAD~1
Token bodies against a0914cf...: 7 exact; 0 rerooted; 1 changed; 0 only before; 0 only after; 0 unreadable ...
changed: tutorial item check · src/check.rs:10 (pub) -> src/check.rs:10 (pub)

$ cargo atlas visibility HEAD~1
Written visibility against a0914cf...: 0 widened; 0 rescoped; 1 narrowed; 3 ambiguous keys
narrowed: tutorial field Tree.words · src/parse.rs:5 (pub) -> src/parse.rs:5 (pub(crate))
```

- `diff` shows how each file's size and shape changed. The largest block in
  `check.rs` grew from 6 to 9 lines.
- `moved` compares the body of every item. Seven are the same and one changed.
  If you had moved a function to another file without changing it, it would
  be listed as exact but relocated. *Rerooted* means a body is the same once
  module paths such as `crate::` and `super::` are ignored, which is what
  usually happens when code moves between modules.
- `visibility` lists items that became more public ("widened"), less public
  ("narrowed"), or public to a different module path ("rescoped"). Items are
  matched by package, kind and name; "ambiguous keys" counts names that appear
  more than once, which can only be matched approximately.

Atlas reads the old commit into a temporary directory. It never checks out
commits or touches your working files.

## 8. Check the structure

The example declares its structure in
[`.config/architecture.toml`](../examples/tutorial/.config/architecture.toml):
the steps run in the order `parse`, `check`, `emit`, and each step may only use
the one before it. To see a rule broken, make `emit` use `parse` directly:

```sh
# In src/emit.rs, add this line below the existing `use` line:
#     use crate::parse::Tree;
# and add this function at the end of the file:
#     /// Count the words in a tree.
#     pub fn count(tree: &Tree) -> usize {
#         tree.words.len()
#     }
git commit -am "emit reads the tree directly"
```

```console
$ cargo atlas delta HEAD~1 --policy .config/architecture.toml --citation-markers docs/
...
verdict   REFUSE
refusal   stage interface `tutorial`: src/emit.rs:3 emit reads parse (`crate::parse::Tree`), which its stage_reads row does not list; read it through the stage's input, or add `parse` to the row's reads with the reason in a comment
```

`delta` compares the current commit (the *candidate*) with the commit you name,
here `HEAD~1` (the *baseline*). It refuses the candidate because `emit` now
uses `parse`, which the rules do not allow. You can fix the code, or change the
rules if the new dependency is intended.

`--citation-markers docs/` tells Atlas what text in a `//!` header counts as a
reference to a design document. One of the policy's rules counts headers
without one; without this option, that rule cannot run and the verdict is
`UNAVAILABLE`. [Architecture checks](checks.md) explains the policy file and
each check.

## Next steps

- Run `cargo atlas orient` in your own project: one page of where its code is
  and what to run next.
- Run `cargo atlas census` and `cargo atlas dependencies --relation calls` in your own
  project.
- Read [how Atlas works](concepts.md) to understand scopes, populations and the
  different kinds of evidence.
- Browse the [diagrams](diagrams.md) to see every view.
- Run `cargo atlas --help` for the full list of commands.

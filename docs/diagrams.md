# Diagrams

Each Atlas diagram answers one question about your code. Every box and arrow
comes from a specific place in the source, and Atlas keeps a record of that
place. Atlas never draws an arrow it did not see in the code or in a rule you
wrote.

| Question | Command |
| --- | --- |
| Which files call each other, or use each other's types? | [`dependencies`](#which-files-depend-on-which) |
| Which functions take, return and build a type? | [`types`](#which-functions-handle-a-type) |
| What paths can one function take? | [`flow`](#what-a-function-does) |
| How did a type's functions and fields change between two commits? | [`compare`](#how-a-change-affected-a-type) |
| How do the stages my project declares hand data to each other? | [`overview`](#an-overview-from-declared-stages) |
| How do the groups in my architecture profile connect? | [`map`, `program`](#older-views-map-and-program) |

The examples on this page use the [tutorial project](getting-started.md).

## Text and images

Every diagram prints as text by default. To draw an image, add
`--as svg` (this needs Node.js 20 or newer):

```sh
cargo atlas dependencies --relation calls --as svg
```

Atlas saves the image under `target/atlas/`; use `--out path/to/file.svg` to
choose another location. Atlas also saves:

- a `.mmd` file next to the image, with the same diagram in
  [Mermaid](https://mermaid.js.org/) format,
- a `.json` file with all the evidence, including things it could not resolve.
  `flow` and `map` write it under `target/atlas/`, or to `--evidence-dir`.

`flow`, `map` and `program` can also wrap the image in an HTML page with its
evidence: use `--as html`.

Images are dark by default. Add `--theme light` for dark text on a white
background, for example to print a diagram or embed it in a light page:

```sh
cargo atlas dependencies --relation calls --as svg --theme light
```

The colours are written into the SVG itself, so the image looks the same in
a browser, an image viewer or a converter such as `rsvg-convert`.

`types`, `compare` and `overview` write a title, notes on what the
figure does not prove, and panel headings around the drawing. Add `--bare` to
draw only the diagram in a Pele image, for example to put it in a slide or a
README. Text output is unchanged, and the notes stay in the JSON evidence.

## Which files depend on which

```sh
cargo atlas dependencies --relation calls
cargo atlas dependencies --relation signatures
cargo atlas dependencies --relation fields
cargo atlas dependencies src/backend --relation all
```

`dependencies` draws one box per file and one arrow for each way a file uses
another. There are three kinds of use, and you choose which to draw:

- **calls**: a function in one file calls a function in the other.
- **signatures**: a function's parameters or return type name a type from the
  other file.
- **fields**: a struct or enum field has a type from the other file.

```text
RELATIONSHIPS
  [check.rs] --signature names: check.tree : Tree (1 site)--> [parse.rs]
  [emit.rs] --signature names: emit.program : &Checked (1 site)--> [check.rs]
```

Read the first arrow as: in `check.rs`, the parameter `tree` of function
`check` has type `Tree`, which is defined in `parse.rs`. "1 site" means this
happens in one place.

These are *qualified* relationships. Atlas followed the `use` statements and
modules to find exactly which item each name refers to; it did not just match
names. Calls are resolved only when the path or receiver is written clearly in
the code. A name the function body declares itself, with a nested `fn` or a
`use` inside the body, is not resolved, so the call is never drawn to a
module item of the same name. Calls Atlas cannot resolve are counted in the limits section and kept
in the JSON. They are never treated as "no call".

The scope after the command (or `--in PATH`) selects which files to draw. Files with no arrows still appear, so
you can see code that nothing else uses.

With `--json`, the evidence lists the facts behind the drawing: the calls,
types and unresolved names that touch the scoped files, and the functions and
types at the other end. Atlas still reads the whole workspace to resolve
them. Add `--all-facts` to list every file's facts.

## Which functions handle a type

```sh
cargo atlas types Tree --panel producers
cargo atlas types Tree,Checked --panel contracts --as svg
cargo atlas types --query pipeline.json --panel all --as svg
```

A *representation* is a type that carries data through your program. This view
shows, for the types you choose, which functions take them, return them and
build them. Name the types after the command, comma separated, or give a saved question
(`--query`, described below).

The view has several *panels*. Each shows a different side of the same types:

- **`contracts`** (output heading `REPRESENTATION CONTRACTS`) lists the
  selected functions, grouped by file, with the parameters and return values
  that use a selected type. Without an `apis` list, Atlas selects the public
  functions that *return* a selected type; a function that only takes one is
  not shown. For example, `(Tree [tree]) -> (Checked [return · Ok])`
  means the function takes a `Tree` in its parameter `tree` and returns a
  `Checked` inside `Ok`. Error types defined in your workspace appear as `Err`
  branches.
- **`producers`** (output heading `OBSERVED RETURNERS / IMMEDIATE LITERAL
  CONSTRUCTION`) lists, for each type, the functions that return it and the
  places that build it directly (`Checked { ... }`). A function that only passes
  a value along is not counted as building it.
- **`preparation`** (output heading `PREPARATION RETURNERS / DIRECT PRODUCER
  CALLS`) lists the functions in chosen files, next to the calls they
  make that produce a selected type, in source order.
- **`retention`** is a table of which selected types hold which others in
  their fields. `&` marks a reference and `*` marks a type nested inside
  another, such as `Vec<Tree>`.
- **`all`** draws contracts, producers and preparation together.

Two functions taking the same type does not mean they receive the same value
at run time. This view shows how the code is written, not how data moves when
it runs.

### Saved questions

A saved question is a JSON file that selects types and functions. It makes a
diagram repeatable: you can commit it and draw the same view after every
change.

```json
{
  "title": "From text to output",
  "types": [
    { "name": "Tree", "file": "src/parse.rs" },
    { "name": "Checked", "file": "src/check.rs" }
  ]
}
```

Optional fields:

- `apis`: exact functions to include, each as `{"name": ..., "file": ...}`.
  Without it, Atlas includes public functions and trait methods that return a
  selected type.
- `preparation_files`: the files whose functions appear in the preparation
  panel.
- `preparation_functions`: exact functions for the preparation panel, each as
  `{"name": ..., "file": ...}`. Each must match one function that returns a
  selected type.

A name that matches more than one item is refused rather than guessed. Methods
use the spelling of their `impl` block, such as `Generator<'g>::candidates`.
[`examples/queries/atlas.json`](../examples/queries/atlas.json) is a larger
saved question that describes Atlas itself.

A question may select 1 to 12 types and at most 40 rows per panel. If a
question selects more, Atlas refuses and asks you to narrow it, instead of
cutting the diagram short.

### Type aliases

A type alias such as `type Forms = Vec<Form>` is shown with what it stands
for. The alias stays the name used in signatures. Aliases are not expanded
recursively.

## What a function does

```sh
cargo atlas flow --path src/main.rs --function main
cargo atlas flow --path src/main.rs --function main --depth 1 --as html
cargo atlas flow --path src/main.rs --function main --detail control --as html
```

`--function` takes the function's name, or a qualified name when the file has
several: `module::name`, `Type::method`, `<Type as Trait>::method`, or
`Trait::method` for a trait's default body. Two bodies with the same written
name, such as `#[cfg]` alternatives, are picked by the line where the name is
written (`--function 12`, or `12:8` with the column). When a selector matches
more than one function, the error lists each one with the selector that picks
it.

`flow` shows the steps inside one function in the order they are written:
each expression, every branch of an `if` or `match` (including guards), loops,
early `return`s and every `?` that may return early.

```text
-> check(parse(&text)) [src/main.rs:6]
-> match check(parse(&text)) [src/main.rs:6]
   | pattern Ok(program)
    -> println!("{}", emit(&program)) [opaque] [src/main.rs:7]
   | pattern Err(message)
    -> eprintln!("error: {message}") [opaque] [src/main.rs:8]
```

A compound assignment such as `total[i()] += next()?` evaluates its operands
in an order that depends on their types, so when both sides have steps, Atlas
draws both orders as a branch.

A statement under `#[cfg(...)]` is drawn as a branch named by the attribute
as written: its steps on one arm, nothing on the other. Atlas does not decide
which targets compile it.

An item or `use` written inside the body, such as a nested `fn`, runs nothing,
so it is not a step: the text output lists it under the heading and a diagram
lists it in its notes.

Some steps are *opaque*: Atlas knows they are there but does not look inside.
Macro calls, closures, `async` and `try` blocks and syntax Atlas does not
support are opaque, and Atlas does not guess what they do. An `unsafe` block
shows its steps after a marker that names the line where the block ends. A
labelled block (`'found: { ... }`) shows its steps, and `break 'found` leaves
it. `x.await` shows `x`'s steps, then one await step; when the future is
polled is not shown.

**Expanding calls.** By default `flow` shows one function body. With
`--depth 1` (up to 6), it also opens the functions that body calls, when it can
resolve them, and shows where each call returns. Recursion, the depth limit and
calls it cannot resolve are marked as boundaries. A diagram with more than 600
steps is refused; choose a smaller function or depth.

**Less detail.** `--detail control` hides small borrow steps (`&text`) and the
extra steps Atlas adds for `?`, leaving branches, loops, calls and exits. The
hidden steps are still listed in the JSON evidence.

`flow` shows what the code's structure allows. It does not prove that a branch
can actually be taken, that a loop ends, or what a panic, destructor or
operator overload does. The test file
[`tests/source_control.rs`](../tests/source_control.rs) shows each construct
`flow` supports.

## How a change affected a type

```sh
cargo atlas compare --vs HEAD~1 --query pipeline.json
cargo atlas compare --vs v1.0.0 --at v1.1.0 --query pipeline.json --as svg
```

`compare` applies the same saved question to two versions of the code and
lists what changed: types, function signatures, fields, the number of places a
type is built, and the order of calls in preparation functions. By default it
compares a commit (`--vs`) with your current files; add `--at` to compare
two commits. The 0.18 spellings `--before` and `--after` still work until 1.0.

For example, after adding a `length` field to `Checked`:

```text
[type] src/check.rs::Checked
  before: tree: Tree
  after: tree: Tree
    length: usize
```

Items are matched by file and name, not line number, so moving code within a
file does not show up as a change. A renamed or moved item appears as one
removal and one addition. The image backend refuses more than 40 changes;
narrow the question, or use the text or JSON output.

Atlas reads the old commit into a temporary directory and deletes it
afterwards. It does not check out commits, fetch, or touch your working files.
The Cargo workspace must be at the repository root, and Git submodules are not
read.

## An overview from declared stages

```sh
cargo atlas overview --adapter path/to/adapter --as svg
cargo atlas overview --adapter path/to/adapter --declarations src/stages.rs --json
```

Some projects describe their own structure in code, for example with a
`const PIPELINE` table listing each stage and its entry function. `overview`
draws that structure: which types each stage hands to the next.

Atlas does not know your table's format, so you supply a small program called
a *declaration adapter* that reads it. Atlas parses the table and passes it to
the adapter; the adapter says what each row means; Atlas checks the answer
against the source and draws the diagram. Tables computed at run time are
refused. See [extending Atlas](extensions.md) to write an adapter.

In the diagram, the main types are what each stage's entry function returns
on success. Public functions also appear when they produce a type another
stage takes. Modules marked as instruments (such as logging or measurement
code) get a separate panel. Your declarations decide what counts as a stage,
but they never add or hide observed relationships.

One figure holds at most 12 types and 40 function rows. When the stages
select more of either, the overview draws several panels, one after another. Each panel holds the types declared
by whole modules, taken in the order you declared them, and is titled with
those modules' names. A function that takes or returns a type in a panel is
drawn in that panel even when its own stage is elsewhere, so a type handed from
one stage to the next stays visible. Every selected type appears; nothing is
cut. The JSON's `split` lists each module, how many types it declares and which
panel holds them. If one module alone needs more than 12 types or 40 rows,
the overview refuses and names that module with both counts, because splitting
it would separate types that belong together.

## Older views: map and program

`map` and `program` are older views.

- `map --architecture PROFILE` and `program --architecture PROFILE` draw the
  groups and permissions declared in an *architecture profile*: an older JSON
  format, separate from the [policy file](checks.md), described by the types in
  the `cargo_atlas::architecture` module. They show what you declared, not what the
  code does. For new projects, use a policy and `delta` instead.
- `map --view types`, `name --relations` and `dump` show relationships found by
  matching names (*lexical* evidence). They are approximate. For exact
  relationships, use `dependencies`.

## Limits

- Diagrams show how code is written, not what happens when it runs.
- Macros are not expanded, types are not inferred and trait calls are not
  resolved. The [concepts guide](concepts.md#limits) lists all the limits.
- The image renderer accepts at most 16 MiB of input and produces at most
  32 MiB of output. A request larger than that is refused before Node.js
  starts, and output that is too large is refused rather than cut short.
- If drawing fails, Atlas does not leave a partial image. `flow` and `map`
  still save their JSON evidence and Mermaid file, and the command exits with
  an error.

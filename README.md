# Atlas

Atlas answers questions about how your Rust codebase is put together, and it
can stop a change that breaks the structure you agreed on.

You point it at a Cargo project and ask, from your terminal, a script or CI:

- Where is this type defined, and how much of the code depends on it?
- I just refactored a lot of files. What actually changed?
- Which files call into which?
- Does this change break our architecture rules?

Atlas only reads your code. It never edits, compiles or runs it, and every
answer points to the file and line it came from, so you can check it yourself.

## What it looks like

### Draw how your types move through the code

Choose some types, and Atlas draws every function that takes, returns or builds
them, grouped by the file each function lives in. Here is Atlas drawing its own
three central types:

```console
$ cargo atlas types --query examples/queries/atlas.json --panel contracts --backend pele --bare
```

<!-- To refresh: run the command above once with --theme dark and once with
     --theme light, each with --out target/atlas/contracts-THEME.svg, and copy both
     into docs/images/. -->
<a href="https://raw.githubusercontent.com/flowerornament/atlas/main/docs/images/contracts-light.svg">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/flowerornament/atlas/main/docs/images/contracts-dark.svg">
    <img alt="Atlas drawing its own source: which functions produce and consume its three central types, SourceGraph, Diagram and Artifact, grouped by the file each function lives in" src="https://raw.githubusercontent.com/flowerornament/atlas/main/docs/images/contracts-light.svg">
  </picture>
</a>

Every arrow is a function signature or a place where the type is built. Click
the image for full size.

### Find a type, and see how much depends on it

Here is Atlas looking at its own source:

```console
$ cargo atlas name Snapshot
Snapshot: 4 declaration sites (ambiguous)

crates/atlas-lib/src/source/discovery/mod.rs:38:Snapshot [Pub Struct]
  Source bytes and their derived lexical graph from one discovery pass.
  4 outgoing, 42 incoming candidate-unique, 0 incoming ambiguous
  pub struct Snapshot {

src/adapter/snapshot.rs:23:Snapshot [Pub Struct]
  2 outgoing, 3 incoming candidate-unique, 0 incoming ambiguous
  pub struct Snapshot {

(and the two `impl Snapshot` blocks)
```

There are two different types named `Snapshot`. About 42 places refer to the
first and 3 to the second, so you know which one a change would ripple
through. When a name could mean more than one thing, Atlas shows you every
match. It never picks one for you.

### See what a big change really did

You moved code across files and renamed things. The text diff shows hundreds
of changed lines. Atlas compares every function and type body with the earlier
commit and sorts them:

```console
$ cargo atlas moved HEAD~4
Token bodies against d3e4e42d4a95…: 1186 exact; 0 rerooted; 10 changed; 0 only before; 0 only after
changed: atlas method Comparison::text · src/views/comparison.rs:271
changed: atlas-lib item header_counts · crates/atlas-lib/src/checks/concentration/ratchets.rs:559
...
```

Out of 1,196 items, 1,186 are identical apart from where they live or how
they're formatted. You only need to review the 10 that changed.

### See which files depend on which

```console
$ cargo atlas dependencies --relation calls
RELATIONSHIPS
  [main.rs] --calls: main calls check (1 site)--> [check.rs]
  [main.rs] --calls: main calls parse (1 site)--> [parse.rs]
```

Add `--backend pele` to draw the same thing as an SVG image. Atlas can also
draw which functions produce and consume a type, and every path a function can
take. See [diagrams](https://github.com/flowerornament/atlas/blob/main/docs/diagrams.md).

### Catch a change that breaks your rules

You write your project's rules in a small TOML file. For example: "the code
that writes output may use the checker, but not the parser". Then Atlas checks
each change against those rules. Here someone made the output code use the
parser directly:

```console
$ cargo atlas delta HEAD~1 --policy .config/architecture.toml --citation-markers docs/
cargo atlas delta dbb03fd281f2 -> 7e0ff3f307aa · 427ms
size      +4 core token-lines · 45/200 · 9.8 tokens/line
...
surface   +1 public declarations · tutorial 11/20
verdict   REFUSE
refusal   stage interface `tutorial`: src/emit.rs:3 emit reads parse (`crate::parse::Tree`),
          which its stage_reads row does not list; read it through the stage's input, or add
          `parse` to the row's reads with the reason in a comment
```

The refusal names the exact line and gives you two ways to fix it. In CI this
fails the build, so the rule holds without anyone having to remember it.
[Architecture checks](https://github.com/flowerornament/atlas/blob/main/docs/checks.md) explains how to write rules, including
what `--citation-markers` is for.

## Is it for you?

Use your editor and rust-analyzer when you're working on one piece of code.
Reach for Atlas when your question is about the whole project, or when you want
the answer in a script, a review or a CI job:

- You're new to a codebase and want a map of it.
- You're reviewing a large refactor and want to know what really changed.
- You want structural rules ("this crate stays under 2,000 lines", "the parser
  never uses the code generator") checked automatically instead of in review.
- You're building your own tooling and want a Rust library that reads a
  workspace for you. See [using the library](https://github.com/flowerornament/atlas/blob/main/docs/library.md).

## Install

You need Rust 1.99.1 or newer.

```sh
cargo install --locked cargo-atlas
cargo atlas --version    # should print "cargo-atlas 0.17.1" or later
```

Atlas is a Cargo subcommand: run it as `cargo atlas` from inside your project.

Some features need more tools:

- **SVG diagrams** need Node.js 20 or newer. You don't install anything else;
  the drawing code is built into Atlas.
- **Comparing commits** needs Git and `tar`. If your project uses
  [jj](https://github.com/jj-vcs/jj), Atlas also needs `jj`, and you name
  commits the jj way: `@-` is the change before the one you are working on.
  If you give a git spelling such as `HEAD~1`, Atlas says what jj calls it.

Atlas keeps the syntax facts of each file it reads in your user cache
directory (`~/Library/Caches/atlas` on macOS, `~/.cache/atlas` or
`$XDG_CACHE_HOME/atlas` on Linux, `%LOCALAPPDATA%\atlas` on Windows), so the
next command skips files whose bytes have not changed. An entry is keyed by
the file's exact bytes, its path and the Atlas executable, so answers are the
same with the cache on or off. It holds names, doc lines and declaration text
from your code, is readable only by you on Unix, and stays under 1 GiB. Set
`ATLAS_CACHE=off` or pass `--no-cache` to turn it off, `ATLAS_CACHE_DIR` to move
it, `ATLAS_CACHE=stats` to see what it served, and `ATLAS_CACHE=verify` to check
every entry against a fresh parse. Atlas never puts the cache inside the
project it reads.

To get the same Atlas every time, for example in CI, add `--version <version>`
to the install command. To try unreleased changes, install from the
repository instead: `cargo install --locked --git
https://github.com/flowerornament/atlas.git cargo-atlas`.

## Try it in five minutes

This repository includes a tiny project to practise on. The `dependencies`
and `delta` examples above come from it.

```sh
git clone https://github.com/flowerornament/atlas.git
cp -r atlas/examples/tutorial /tmp/tutorial
cd /tmp/tutorial

cargo atlas census                 # what does this project contain?
cargo atlas name Tree              # where is Tree defined, and what uses it?
cargo atlas module src/check.rs    # what is in this file?
cargo atlas dependencies --relation calls               # which files call which?
cargo atlas flow --path src/main.rs --function main     # what paths can main take?
```

[Getting started](https://github.com/flowerornament/atlas/blob/main/docs/getting-started.md) walks you through it step by step,
ending with the rule check shown above.

On your own project, run the same commands from its root directory, or pass
`--root path/to/project`. Add `--json` to any command for machine-readable
output, and run `cargo atlas <command> --help` to see what a command counts.

## What Atlas can't see

Atlas reads code the way a careful person would, without the compiler. So it
doesn't:

- expand macros (a macro call shows up as one step it doesn't look inside),
- decide which `#[cfg(...)]` options are active,
- work out types the code doesn't write down,
- decide which trait implementation a call uses at run time,
- prove that a path through a function can actually happen.

When it can't work something out, it tells you instead of leaving it out. A
missing answer is reported as unknown, never as "nothing found".

## Documentation

- [Getting started](https://github.com/flowerornament/atlas/blob/main/docs/getting-started.md): a guided tour of the example project
- [How Atlas works](https://github.com/flowerornament/atlas/blob/main/docs/concepts.md): the ideas behind every command, and a glossary
- [Diagrams](https://github.com/flowerornament/atlas/blob/main/docs/diagrams.md): each kind of diagram and how to read it
- [Architecture checks](https://github.com/flowerornament/atlas/blob/main/docs/checks.md): writing rules for your project and checking them
- [Using the library](https://github.com/flowerornament/atlas/blob/main/docs/library.md): calling Atlas from your own Rust code
- [Extending Atlas](https://github.com/flowerornament/atlas/blob/main/docs/extensions.md): teaching Atlas your project's conventions
- [Contributing](https://github.com/flowerornament/atlas/blob/main/CONTRIBUTING.md), [code architecture](https://github.com/flowerornament/atlas/blob/main/ARCHITECTURE.md) and the
  [changelog](https://github.com/flowerornament/atlas/blob/main/CHANGELOG.md)

## License

Atlas is available under either the [MIT](https://github.com/flowerornament/atlas/blob/main/LICENSE-MIT) or the
[Apache 2.0](https://github.com/flowerornament/atlas/blob/main/LICENSE-APACHE) license, at your option. The bundled
[Pele](https://github.com/flowerornament/atlas/blob/main/vendor/pele/README.md) diagram renderer is MIT licensed.

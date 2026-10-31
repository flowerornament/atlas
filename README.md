# Atlas

Read Rust code as a system. Atlas is a read-only source analysis library and CLI
for Cargo projects. It helps you find declarations, inspect dependencies, review
changes, and draw how representations move through a program.

Every relationship carries its evidence. A signature, a written call, a retained
field and an authored architecture rule mean different things; Atlas keeps them
separate. Unresolved and ambiguous observations remain visible.

## Install

```sh
cargo install --locked --git https://github.com/flowerornament/atlas.git atlas
```

Rust 1.99.1 is required. For reproducible project tooling, add `--rev FULL_COMMIT`.
The package installs `atlas` and `atlas-girard`; install the whole package.

Cargo is needed to discover source. Diagram rendering with Pele also needs Node
20 or newer. The renderer is embedded; npm and network access are not needed to
render. Revision comparisons need Git and tar. Reading a jj workspace also needs the
`jj` executable; revision reads never snapshot its working copy.

## Read a project

Run these from a Cargo project, or add `--root /path/to/project`:

```sh
atlas census --population pub-ish              # source and documentation counts
atlas name MyType --population all --json      # declarations and lexical references
atlas spans --scope src                        # items and their source boundaries
atlas concentration --scope src                # block depth and item breadth
atlas diff HEAD~1 --scope src --items --graph  # changed declarations and relationships
atlas moved HEAD~1 --scope src                 # exact and rerooted body matches
atlas visibility HEAD~1 --scope src            # written visibility changes
```

`--json` gives structured observations for scripts and agents. `--scope` selects
files inside the Cargo workspace; `--population` selects supported declaration
populations. Run `atlas COMMAND --help` for each question, selection and limit.

## Draw a question

```sh
atlas dependencies --scope src --relation calls --backend pele
atlas representations --type MyType --panel producers --backend pele
atlas compare --before HEAD~1 --backend pele
atlas flow --path src/main.rs --function main --detail control --backend pele --format svg
```

ASCII is the default. Pele produces a standalone SVG with JSON evidence and a
Mermaid companion under `target/atlas/`; use `--out PATH.svg` to choose a path.
These are generated artifacts, not a website or a source file to maintain.

For a reusable question, save a JSON selection of types and entry functions.
Named aliases stay visible as handoffs. Select `preparation_functions` in a saved
question to see which passes share producer implementations; `--detail control`
folds expression mechanics while retaining branches, loops and early exits.
Try `examples/queries/atlas.json` on Atlas itself. `overview` can instead read
explicit project declarations through an adapter; its bundled Girard adapter
understands `MODULES` and `SURFACES`. Atlas does not invent a project's stages.

See [diagram examples and meanings](docs/diagrams.md) and
[project adapters and renderers](docs/extensions.md).

## Check declared architecture

`comments`, `stages`, `seams`, `policy` and `delta` provide documentation audits
and fixed architecture checks. Policy checks require explicit authored input;
project-specific authority, such as formal-model citations, belongs to the
project's wrapper. Missing required evidence is unavailable, never PASS.
These commands do not install themselves into a project's landing gate.

## Use the library

`atlas-core` exposes captured source, navigation, qualified analysis, review and
check reports without the CLI, renderer, Node or a dependency on the inspected
project. Depend on an exact Git revision:

```sh
cargo add atlas-core --git https://github.com/flowerornament/atlas.git --rev FULL_COMMIT
```

The [library guide](docs/core.md) explains snapshots, populations, evidence and
embedding. The `atlas` package retains compatibility library paths and owns the
CLI, adapters and ASCII/Pele rendering.

## Limits

Atlas reads written source. It does not expand macros, fully resolve Rust's type
system, prove runtime ownership, or establish behavioral equivalence. Source-flow
views retain supported control syntax, with explicit boundaries for unsupported
or unresolved work. Large diagrams can still be difficult to read. The source
and uncertainty behind a picture are part of its result.

## Develop

```sh
just check          # fmt, Clippy, workspace/core tests, rustdoc and renderer tests
just check-install  # installed tools outside the checkout
just verify-vendor  # embedded Pele bytes and license
```

See [architecture and execution flow](docs/architecture.md),
[provenance](PROVENANCE.md) for the scanner's origins and evidence boundaries,
and [diagram lessons](docs/diagram-lessons.md) for the design choices.
Atlas is MIT OR Apache-2.0. Its bundled Pele renderer is MIT.

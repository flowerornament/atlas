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

`--json` gives structured observations for scripts and agents. Review and diagram
commands use `--scope` for workspace paths; stage/seam checks use it for Cargo
packages. Native navigation also has package/directory selectors. `--population`
selects each command's supported declaration population; run `atlas COMMAND --help`
for the exact selection and evidence meaning. Revision examples below use Git's
`HEAD~1`; in a jj workspace pass a revset such as `main-`, or an exact commit ID.

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

## Try it on Atlas

From a checkout of this repository, with Atlas installed:

```sh
atlas name SourceGraph --population all
atlas flow --path src/main.rs --function main --detail control
atlas representations --query examples/queries/atlas.json --panel contracts --backend pele --out target/atlas/atlas.svg
atlas compare --before HEAD~1 --query examples/queries/atlas.json --json
```

The name card locates the captured source model and its lexical references. The
flow shows the CLI entry's supported branches and exits. The saved question
follows `SourceGraph`, `Diagram` and `Artifact`, with files and signature witnesses;
open `target/atlas/atlas.svg` to inspect it. The comparison asks that same question
of the previous commit and current source. It reports changed contracts and
construction sites, rather than claiming changed runtime behavior.

These are existing source questions, not a profile containing authored arrows.
Use their source links to check an observation or read what a view leaves unknown.

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
and uncertainty behind a picture are part of its result. Contract comparison
refuses duplicate stable identities among observations in the selected question;
it does not choose a matching arbitrarily.

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

# Extending Atlas

Atlas has two extension points:

- A **declaration adapter** teaches Atlas how your project describes its own
  structure, so `cargo atlas overview` can draw it.
- A **rendering backend** draws Atlas's diagrams in a new output format.

To build tools on Atlas's analysis instead, see the [library guide](library.md).

## Declaration adapters

### The idea

Many projects already describe their structure somewhere in their code: a
table of pipeline stages, a list of plugins, a registry of commands. Each
project writes this differently, so Atlas cannot understand all of them. A
declaration adapter is a small program that understands one project's
convention.

The [tutorial project](getting-started.md) lists its stages in `src/lib.rs`:

```rust
pub enum Role {
    Pass,
}

pub const PIPELINE: &[(&str, &str, Role, &str)] = &[
    ("parse", "parse", Role::Pass, "Parse text"),
    ("check", "check", Role::Pass, "Check rules"),
    ("emit", "emit", Role::Pass, "Emit text"),
];
```

Each row gives a module, its entry function, its role and a label. The
example adapter in [`examples/pipeline-adapter.rs`](../examples/pipeline-adapter.rs)
understands exactly this format. Build it, then point `overview` at it:

```console
$ cargo build --example pipeline-adapter     # in your clone of Atlas
$ cd /tmp/tutorial                             # your copy of the tutorial
$ cargo atlas overview --adapter path/to/atlas/target/debug/examples/pipeline-adapter
Representation contracts from declared stage APIs
Contracts and direct fields, not runtime value flow.

A  REPRESENTATION CONTRACTS
  src/check.rs:10  check
    (Tree [tree]) -> (Checked [return · Ok])
    literal Checked at line 14
  src/emit.rs:5  emit
    (Checked [& program]) -> ()
  src/parse.rs:9  parse
    () -> (Tree [return])
    literal Tree at line 10
...
```

Compare this with `cargo atlas types` in the
[getting-started tour](getting-started.md#6-follow-a-type): there, `emit` was
missing because it does not return a selected type. Here the adapter named
`emit` as a stage's entry function, so it appears. Modules the adapter marks
as instruments, such as logging, would get a separate panel.

### What happens when `overview` runs

1. Atlas reads the project and finds the declaration file (`src/lib.rs` by
   default; choose another with `--declarations`).
2. It parses the file's constants and sends them to the adapter as JSON on
   standard input. The adapter does not need to parse Rust or compile the
   project.
3. The adapter replies on standard output, saying which modules are stages,
   what their entry functions are and where each answer came from. Or it
   refuses, with a reason.
4. Atlas checks the reply against its own reading of the source: the modules
   and constants must exist, and the source must not have changed in between.
5. Atlas draws the diagram from what it observed in the code, organized by the
   stages the adapter named.

Declarations decide how the diagram is organized. They never add or remove an
observed relationship. The JSON evidence records the adapter's name, version
and reply, so you can always tell which parts came from it.

Atlas runs only the adapter you name with `--adapter`. It never runs programs
named in a project's files, and it never downloads anything. An adapter is
ordinary code running on your machine, not a sandbox, so only use adapters you
trust.

### Write an adapter

An adapter is a Rust binary that depends on the `cargo-atlas` package. Implement
`adapter::ProjectAdapter` and call `adapter::serve` from `main`:

```rust,ignore
use cargo_atlas::{
    Result,
    adapter::{self, ProjectAdapter, Request},
    architecture::{Architecture, Kind, Module},
};

struct Pipeline;

impl ProjectAdapter for Pipeline {
    fn name(&self) -> &'static str {
        "example-pipeline"
    }
    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
    fn declarations(&self, input: &Request) -> Result<Architecture> {
        // Find the constant in the declaration file Atlas already parsed.
        let pipeline = input.snapshot.constant(&input.source, "PIPELINE")?;
        // ... turn each row into a Module ...
    }
}

fn main() {
    if let Err(e) = adapter::serve(&Pipeline) {
        eprintln!("pipeline adapter: {e}");
        std::process::exit(1);
    }
}
```

The full example is about 75 lines. A good way to start is to copy it into your
own repository as a small binary crate that depends on an exact
`cargo-atlas` version:

```toml
[dependencies]
cargo-atlas = "=0.18.1"
```

Useful pieces of the `Request`:

- `input.snapshot.constant(&input.source, "NAME")` finds a constant. It refuses
  if the constant is missing or defined twice.
- The constant's `value` lets you walk literal syntax: `.array()`, `.tuple()`,
  `.string()`, `.path()` and `.call()`. A call whose callee is a plain path,
  such as `Some(Part::Host)` or `Wrapper(1, X)`, arrives as its path and
  arguments; Atlas records the syntax only, so a tuple struct, an enum variant
  and a function call look the same. Anything else, such as a method call, a
  macro or a callee with generic arguments, stays opaque. Adapters cannot
  evaluate Rust.
- `input.snapshot.module_root_from(&input.source, "name")` finds a module's
  file in the package that declares it. It accepts a full module path, or a
  short name if only one module in that package has that name. A module with
  the same name in another workspace package, such as a build tool, is not a
  candidate. `module_root("name")` does the same across every package.

Constants reach the adapter as JSON, tagged by `kind`:

```json
{"kind": "string", "value": "build"}
{"kind": "path",   "value": ["Role", "Pass"]}
{"kind": "array",  "value": [ ... ]}
{"kind": "tuple",  "value": [ ... ]}
{"kind": "call",   "value": {"path": ["Some"], "args": [{"kind": "path", "value": ["Part", "Host"]}]}}
{"kind": "opaque", "value": "make().finish()"}
```

New kinds may be added in later versions, so match with a fallback arm.

Return an `Architecture` listing each `Module` with its `kind` (`Stage`,
`Support`, `Instrument` and so on) and its entry function. Its `provenance`
names the source constants the answer came from (`pipeline.id()`), and Atlas
checks that they exist. When your convention is ambiguous,
return an error rather than guessing.

### Protocol details

You only need these if you write an adapter without the `cargo-atlas` crate.

- One JSON request on standard input, one JSON reply on standard output.
  Diagnostics go to standard error.
- Protocol version `2`, interface `project-declarations/v1`. Version 2 added
  each snapshot file's `packages` and the `call` literal kind. The reply echoes
  the request's snapshot ID and names its producer.
- Size limits: request 16 MiB, reply 2 MiB, diagnostics 64 KiB.
- A reply that is malformed, too large, for another snapshot or for another
  protocol version is refused before anything is drawn.

To use an adapter inside another Rust program without a separate process, call
`adapter::adapt` directly and check the result with `adapter::validate_reply`.
[`tests/extensions.rs`](../tests/extensions.rs) has working examples, including
the cases Atlas refuses.

## Rendering backends

Atlas builds each diagram as a *view*: a description of what to draw, already
selected from the source. A renderer turns a view into an output document. It
never reads source code itself.

`render::View` is one of four families: a graph, a set of representation
tables, a list of comparison rows, or an overview. Implement
`Renderer::render_view` for the families you support, and return `Artifact`
documents. Return the typed `UnsupportedView` error for the others. Do not
reshape a view into another family to claim support, such as turning a table
into a graph with no edges.

The built-in ASCII and Pele renderers both work this way. A custom renderer is
used from the library; `cargo atlas` only offers the built-in two, as
`--as text` and `--as svg` or `--as html`.

## Limits

- An adapter reads only literal constants. It cannot see computed values or
  macro output.
- A file shared by several packages cannot be used as a declaration file yet;
  the protocol cannot express which package a declaration belongs to.
- Fingerprints detect changed source but are not cryptographic. An adapter's
  name and version are whatever it reports.

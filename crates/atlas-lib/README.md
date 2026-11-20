# atlas-lib

The library behind [`cargo-atlas`](https://crates.io/crates/cargo-atlas). It
reads a Rust workspace once, without compiling or running it, and answers
questions about how the code is put together:

- where a name is declared, and what refers to it,
- which items changed, moved or narrowed their visibility between two commits,
- which functions produce and consume a type, and which files call which,
- whether a change keeps the architecture rules a project has written down.

Every answer points to the file and line it came from. When a name could mean
more than one thing, every candidate is reported; when something cannot be
worked out without the compiler (macro expansion, `cfg`, inferred types, trait
dispatch), it is reported as unknown rather than as absent.

```toml
[dependencies]
atlas-lib = "0.9"
```

```rust,no_run
use atlas_lib::{analysis::Analysis, discovery};

fn main() -> atlas_lib::Result<()> {
    let snapshot = discovery::snapshot_workspace(std::path::Path::new("."))?;
    let facts = Analysis::new(&snapshot)?.representations()?;
    println!("{} functions", facts.functions.len());
    Ok(())
}
```

See [using the library](https://github.com/flowerornament/atlas/blob/main/docs/library.md)
for a guided tour. Licensed under either MIT or Apache 2.0, at your option.

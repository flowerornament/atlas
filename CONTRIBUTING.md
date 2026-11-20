# Contributing to Atlas

Thank you for helping. Bug reports with a small reproducing project are the
most useful contribution. Atlas's job is to report source evidence honestly, so
a report that shows Atlas overclaiming (a missing ambiguity, an invented edge,
a parse failure counted as zero) is especially valuable.

## Set up

- Rust 1.99.1. `rust-toolchain.toml` selects it, along with rustfmt and Clippy.
- Node.js 20 or newer, for renderer tests.
- [just](https://github.com/casey/just), for the recipes below.
- npm, only to verify or update the vendored Pele renderer.

```sh
just check          # format, strict Clippy, all tests, rustdoc, renderer tests;
                    # each run's output is kept in target/gate/, the newest
                    # linked as target/gate.log, and a failure names its step
just check-install  # install the binary and exercise it outside the checkout
just verify-vendor  # check the vendored Pele bundle against its lockfile
```

CI runs the same steps.

`.cargo/config.toml` points the syntax-facts cache of every process Cargo
starts here (tests, `cargo run`) at `target/facts-cache` and sets
`ATLAS_CACHE=verify`, so the whole suite checks each cache hit against a
fresh parse and never touches your user cache directory.

## Where to start reading

[ARCHITECTURE.md](ARCHITECTURE.md) maps the code and traces the main command
paths. In short:

- `crates/atlas-lib/` is the library: source capture, qualified analysis,
  navigation and review queries, architecture checks and revision reading.
- `src/cli/` defines commands and their options. `src/views/` selects
  observations for a diagram; `src/render/` draws them as ASCII or SVG.
- `src/adapter/` is the declaration-adapter protocol.
- `tests/` holds end-to-end tests through the public API and the CLI.

## Conventions

- **Evidence stays honest.** Keep ambiguity and unresolved names; never pick an
  arbitrary candidate. A lexical mention, a qualified use and a declared rule are
  different kinds of evidence. Missing evidence is reported as unavailable, never
  as zero or PASS.
- **Read-only.** Atlas must never modify a project it inspects, including while
  writing diagrams.
- **No project-specific rules in core.** A project's conventions belong in its
  own declaration adapter or check wrapper, which supplies typed inputs.
- **Module headers** state the module's responsibility and the limits of its
  results. Public API docs state the obligations a caller must meet that the
  type system cannot enforce.
- **Lints.** The workspace denies Clippy's default and pedantic groups plus
  several safety lints, and forbids `unsafe`. Fix the code rather than adding an
  `#[allow]`; when an exception is justified, use `#[expect(..., reason = "...")]`.
- **Dependencies.** Add them with `cargo add` and commit `Cargo.lock`.
- **Tests** come with behavior changes. Prefer a small fixture project that shows
  the exact source shape.

## Pull requests

Keep each pull request to one change. Describe what a user can now observe, and
any change to report schemas or command output. Note breaking changes in
[CHANGELOG.md](CHANGELOG.md).

By contributing, you agree that your contribution is dual-licensed under the
MIT and Apache 2.0 licenses, like the rest of Atlas.

---
name: atlas
description: "Answer questions about how a Rust workspace is put together with the cargo-atlas CLI, from source and without building it: where an item is defined, who calls a function and what it calls, which files depend on which, what takes or returns a type, what changed since a commit, and architecture checks. Use when exploring an unfamiliar Rust project, tracing calls, reviewing a change's structure, or writing a Datalog query over the code's facts."
---

# Atlas

Atlas is the `cargo atlas` command (`cargo install cargo-atlas`). Everything
it can teach is in the binary:

1. Run `cargo atlas help agent` once: what its answers mean (resolved,
   possibly, unobserved), which command answers which question, how to
   narrow a name, and a query cookbook.
2. Run `cargo atlas orient` in the project: its entry points, where the code
   is, and commands to run next.

Then `cargo atlas help COMMAND` for one command and `cargo atlas help topics`
for the guides. Add `--as json` when you will read the answer in code.

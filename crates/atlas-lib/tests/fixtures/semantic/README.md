# Semantic evidence fixture

A small library whose calls a compiler front end and Atlas's qualified
resolver read differently. `index.scip` is rust-analyzer's SCIP index of
exactly these bytes; the tests in `tests/semantic.rs` copy the package to a
temporary directory and read the committed index against it.

After changing `src/lib.rs`, regenerate the index from a copy, so no
`Cargo.lock` or `target/` is written here:

```sh
tmp=$(mktemp -d)
cp -R Cargo.toml src "$tmp"
(cd "$tmp" && CARGO_TARGET_DIR="$tmp/target" rust-analyzer scip . --output index.scip)
cp "$tmp/index.scip" index.scip
```

The index records its producer's version. Its readings reflect the host
target and default features of the machine that wrote it.

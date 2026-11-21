fmt:
    cargo fmt
lint:
    cargo clippy --workspace --all-targets -- -D warnings
test:
    cargo test --workspace --locked
# The local gate; scripts/check.sh lists its steps and keeps each run's log.
check:
    scripts/check.sh
build:
    cargo build --release --locked
verify-vendor:
    npm ci --ignore-scripts --no-audit --no-fund
    npm run verify-vendor
check-install:
    renderer/check-install.sh
# The function measure against Clippy itself (runs Clippy on a fixture crate)
clippy-golden:
    cargo test -p atlas-lib --test clippy_golden --locked -- --ignored
# Publish the described jj change: run `just check` on exactly it, then push it
land *args:
    scripts/jj-land.sh {{args}}

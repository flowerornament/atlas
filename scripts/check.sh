#!/usr/bin/env bash
# The local gate: every step `just check` runs, in order, with its output kept.
#
# Each run writes target/gate/<UTC timestamp>.log, so an earlier failure is
# never overwritten, and points target/gate.log at the newest one. On failure
# it names the step that failed and its exit status: cargo's 101 alone does
# not say whether formatting, Clippy, a test or rustdoc refused.
#
# The self-check step checks Atlas against its own architecture policy with
# the binary the test step just built (scripts/self-check.sh).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
mkdir -p target/gate
log="target/gate/$(date -u +%Y%m%dT%H%M%SZ)-$$.log"
ln -sfn "gate/$(basename "$log")" target/gate.log

steps=(
  "fmt|cargo fmt --check"
  "clippy|cargo clippy --workspace --all-targets --locked -- -D warnings"
  "test|cargo test --workspace --locked"
  "clippy (no default features)|cargo clippy -p atlas-lib --no-default-features --all-targets --locked -- -D warnings"
  "test (no default features)|cargo test -p atlas-lib --no-default-features --locked"
  "self-check|scripts/self-check.sh"
  "doc|RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked"
  "npm|npm test"
)
for step in "${steps[@]}"; do
  name="${step%%|*}"
  command="${step#*|}"
  echo "check: $name: $command" | tee -a "$log"
  status=0
  bash -c "$command" 2>&1 | tee -a "$log" || status=$?
  if [ "$status" -ne 0 ]; then
    echo "check: FAILED at $name (exit $status); full output: $root/$log" | tee -a "$log" >&2
    exit "$status"
  fi
done
echo "check: passed; output: $root/$log" | tee -a "$log"

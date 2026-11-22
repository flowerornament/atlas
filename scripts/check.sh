#!/usr/bin/env bash
# The local gate: every step `just check` runs, in order, with its output kept.
#
# Each run writes target/gate/<UTC timestamp>.log, so an earlier failure is
# never overwritten, and points target/gate.log at the newest one. On failure
# it names the step that failed and its exit status: cargo's 101 alone does
# not say whether formatting, Clippy, a test or rustdoc refused.
#
# Each step's elapsed seconds are logged after it, and the last line sums
# them, so every change to the gate can be measured.
#
# The self-check step checks Atlas against its own architecture policy with
# the binary the test step just built (scripts/self-check.sh).
#
# The gate writes only under target/ (and npm's node_modules/): every other
# file of the checkout is hashed before the steps and after, and a change
# fails the gate, naming the files. A test that types a path without setting
# its directory to its fixture would otherwise write into the checkout.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
mkdir -p target/gate
log="target/gate/$(date -u +%Y%m%dT%H%M%SZ)-$$.log"
ln -sfn "gate/$(basename "$log")" target/gate.log

checkout() {
  find . \( -path ./target -o -path ./node_modules -o -path ./.jj -o -path ./.git \
    -o -path ./.beads -o -path ./.peat -o -path ./.claude \) -prune -o -type f -print0 |
    LC_ALL=C sort -z | xargs -0 cksum
}
before="$(checkout)"

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
timings=()
for step in "${steps[@]}"; do
  name="${step%%|*}"
  command="${step#*|}"
  echo "check: $name: $command" | tee -a "$log"
  started=$SECONDS
  status=0
  bash -c "$command" 2>&1 | tee -a "$log" || status=$?
  elapsed=$((SECONDS - started))
  timings+=("$name ${elapsed}s")
  echo "check: $name took ${elapsed}s" | tee -a "$log"
  if [ "$status" -ne 0 ]; then
    echo "check: FAILED at $name (exit $status) after ${SECONDS}s; full output: $root/$log" | tee -a "$log" >&2
    exit "$status"
  fi
done
changed="$(diff <(echo "$before") <(checkout) | sed -n 's/^[<>] [0-9]* [0-9]* //p' | LC_ALL=C sort -u)"
if [ -n "$changed" ]; then
  echo "check: FAILED: the gate changed files of the checkout; a step must write only under target/:" | tee -a "$log" >&2
  echo "$changed" | tee -a "$log" >&2
  exit 1
fi
summary="$(IFS=,; echo "${timings[*]}")"
echo "check: passed in ${SECONDS}s (${summary//,/, }); output: $root/$log" | tee -a "$log"

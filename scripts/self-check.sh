#!/usr/bin/env bash
# Atlas checks itself: its own architecture policy, judged by the binary this
# checkout builds. `just check` and CI run this script; see CONTRIBUTING.md,
# "Atlas checks itself", and the rules in ARCHITECTURE.md.
#
# `policy` checks the policy file and each authority row, and exits with an
# error for REFUSE or UNAVAILABLE. `stages` is a report that always exits
# successfully when it finds a violation, so this script fails unless its
# whole output is the one summary line it prints when every read between
# atlas-lib's top-level modules follows the declared order and stage_reads.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
policy=.config/architecture.toml
atlas() { cargo run --locked --quiet -- atlas "$@"; }

atlas policy --policy "$policy"

clean="1 selected packages; no complete project policy verdict"
report="$(atlas stages --policy "$policy" --scope atlas-lib)"
printf '%s\n' "$report"
if [ "$report" != "$clean" ]; then
  echo "self-check: stages reported the reads above; fix them or amend the stage order in $policy and ARCHITECTURE.md together" >&2
  exit 1
fi

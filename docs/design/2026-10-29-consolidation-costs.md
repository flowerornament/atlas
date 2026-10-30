# Consolidation cost protocol

Registered 2026-10-29 before any cost measurements for girard-xqe.2. This is V7
of [the consolidation design](consolidation.md). Functional conformance remains
separate; cost does not turn missing authority into PASS.

## Question and inputs

What does the shared implementation cost to build and to answer representative
navigation, review and gate questions, compared with the pinned native tool?
There is no registered speed or memory threshold. Report regressions and differences
in workload/output before a consuming project's cutover.

The native implementation and inspected source are Murail
`32e218d2773b87e6635d4f3df97f9b7ccca4a65c`. The shared implementation is the
frozen Atlas qualification commit named in the instrument's input ledger before
submission. Rust is 1.99.1 for both. Release builds use each repository's own
release settings, recorded with the raw commands; do not change optimizer flags
to improve a result. Preserve compiler, host, input and tool identities.

The before tool is the native `murail-atlas` binary. After navigation/review use
the standalone Atlas binary with explicit native populations. After the gate
uses an independent default-feature-disabled core caller with the original pinned
project's FM/bead/header/feature interpretations. That caller includes paired
source capture and candidate custody, not only an already-loaded check loop.
Native project source and published tool revisions remain unchanged.

## Instruments and runs

Run one bounded, exclusive Ferry lease on bench, at most 1,800 seconds, with a
named Atlas seat. All functional checks and instrument smoke checks run on gates.
The ignored qualification harness declares exact extra Git inputs through Ferry;
preparation only materializes owned copies. Instrument source, checksums, receipt
and raw outputs stay under ignored `target/atlas/consolidation/`.

Use Linux `/usr/bin/time -v` for each process tree: elapsed wall time, user/system
CPU and maximum resident set size. Preserve its raw record and the actual process
exit status. A failed or timed-out operation has that outcome and no successful
cost interpretation. Record output bytes and hashes, not only a derived ratio.
Use the same instrument for both sides. Do not sum nested measurements.

Build measurements cover the native tool, standalone tool, and independent core
gate caller separately. For each, fetch dependencies before the clock, build once
with an empty dedicated target directory (fresh build with warm registry), then
run a no-op incremental build. Finally append one specified source comment in
the owned tool/caller entry source and measure a rebuild. Record the edit and its
hash; never edit the inspected source or the original oracle. This is one sample
of each build question, not a build-time distribution.

The query population is the same pinned full Murail source for both sides:

- Navigation: `census --json`, pub-ish subjects. The shared command explicitly
  requests `--population pub-ish`.
- Review: `spans crates/murail-atlas/src/scan.rs --json`.
- Gate: `delta PIN --candidate PIN --json`, with the independent core caller
  supplying the same pinned project interpretations.

After builds, run five repetitions per query per side, alternating the order:
before/after, after/before, before/after, after/before, before/after. No per-query
warm-up is discarded. Each invocation includes startup, Cargo metadata, source
capture, analysis, serialization and output writing; compilation is excluded.
The build edit is a comment only and is stated with the query binary identity.

## Interpretation

Report raw samples and medians for elapsed/CPU/RSS, plus fresh/no-op/rebuild
costs. Distinguish the standalone package's rendering/CLI dependencies from the
default-disabled core caller's dependencies. The native tool also has product
dependencies; that is a measured build envelope, not a claim about parser speed.

These envelopes are not identical algorithms or outputs. The shared navigation
reports source identities and explicit diagnostics; native reports are smaller.
The documented JIT-test ancestry correction changes 75 non-test / 50 token lines.
The shared gate preserves the required test root-glob as unavailable while native
omits that limit. Record these differences beside the figures. Do not claim whole
native gate equivalence or attribute a cost change to one mechanism without a
separate causal instrument. Keep the raw collected material even when it misses.

If the lease or build budget is insufficient, retain partial results and identify
the unmeasured cells. Do not silently shorten repetitions, remove a slow case,
change inputs or reuse an unmatched binary. Any extension is a dated amendment
stating which results were already seen.

## Instrument availability amendment

2026-10-29, after the first instrument smoke failed before any build or query
measurement: the Linux host has no `/usr/bin/time`. Use GNU Time from the pinned
Nixpkgs revision `7fd4004d0511991fff3a0524856a9be09e28c5f0` through its `time` package
for both sides. Record the resolved executable and version. Keep `-v`, raw resource
records, populations, repetitions and lease budget unchanged. No cost result had
been seen when this amendment was made.

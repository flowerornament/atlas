# Bounded embedded rendering

Outcome for readability repair `girard-xqe.10` and Atlas issue 2, 2026-10-31.
Baseline: `bf050704f8af5cdcd297913e555d2bd641d7bba9`.

## Model / Replaces / Adds

Rendering consumes a selected source model and either returns an artifact or a
failure. It cannot accept truncated output or incomplete request delivery as
success. Replace the renderer's write-before-read, unbounded wait_with_output
route with the existing adapter's concurrent exchange, now shared under one
private process owner. Protocols keep their own meaning and configured limits.
No source query, dependency, vendor bundle, project policy or CLI feature is added.

- Before implementation, 48 existing qualification SVGs were measured: maximum
  347,197 bytes (`followups/render.svg`). Thirty-six reconstructed requests had
  a maximum 763,557 serialized bytes; the embedded bundle is 695,333 bytes.
  The registered caps are 16 MiB request, 32 MiB SVG and 64 KiB diagnostics.
  Those request/SVG caps have about 22/96 times the observed maxima as headroom;
  the diagnostic cap matches the existing adapter. The pinned renderer's own
  50,000-character source limit is retained.
- Borrowed JSON request fields avoid copying source and bundle into a JSON value.
  A capped writer counts escaped serialized bytes and refuses before Node starts.
- One exchange owns three concurrent pipe workers. Each read retains at most its
  cap plus one overflow-detection byte, then refuses and stops the child; it no
  longer waits for an unbounded stream to end. Completion messages avoid polling.
  A guard stops/reaps the direct child on setup, thread creation, I/O and join
  failures; workers are joined after that child is stopped on failure.
- The exchange retains observed child status/stderr and request-delivery failure
  separately. Protocol callers preserve startup diagnostics and report the write
  failure alongside them. Successful exit with incomplete request input refuses.
  Status is the observed result, including termination if the child was still
  live; no unobserved exit code is inferred.
- Pele checks complete captured output for UTF-8 and an SVG root/closing tag.
  This is a protocol check, not a complete XML validation or sanitizer.
  Protocol failures reach existing output policy; successful publication remains
  behind the shared source/query/adapter/companion collision checks.

This is an I/O bound, not a wall-time/CPU limit or process-tree manager. Explicitly
invoked programs must close their pipes and not leave descendants holding them.
The pinned embedded bundle does not delegate child processes. This contract and
per-invocation limits are stated in the shipped diagram guide.

## Verification

- Three CLI boundary tests exercise oversized SVG, oversized diagnostics while
  stdin is unread, streams exceeding pipe capacity before request consumption,
  immediate startup refusal, a live child closing stdin, successful exit without
  consuming the request, invalid UTF-8 and non-SVG output. A 20-second test-only
  deadline bounds each witness; no production deadline is claimed.
- These tests verify direct-child disappearance/reaping, unchanged source/query
  bytes, unsuccessful status/empty success stdout and no completed view artifact
  or evidence on failure. A pipe-pressure success publishes its complete SVG.
- Request overflow is tested at the protocol input boundary with escaping that
  expands beyond the serialized cap. A below-cap request round-trips its exact
  non-ASCII/quoted payload. The actual pinned renderer accepts a labelled
  550-stage pipeline (43,220 source characters), producing an SVG larger than
  the largest previously measured figure.
- Review catches an early-failure diagnostic regression in the first cancellation
  design. The revised exchange carries observed output and input error separately;
  an immediate refusal no longer disappears behind a broken-pipe message. The
  strengthened tests include both immediate failed and successful exits.

- Independent quality review verifies the repaired startup case and runs the three
  CLI boundary tests, 11 adapter/extension tests and two renderer protocol/control
  tests. Reuse and efficiency reviews find the shared mechanism appropriately
  narrow: no new capture, request clone, polling, generic process framework or
  added dependency. The new three-worker route matches the existing adapter's
  worker cost; it replaces the renderer's unsafe sequential pipe ordering.
- Full `just check` and `just check-install` pass, including feature-disabled core,
  public independent clients, source/output controls, rustdoc and Node tests.
  The fixed 58-case CLI/SVG comparison remains byte-identical to `3598a56c`,
  already shown identical through the preceding invariant repair. No fields or
  output bytes are normalized or excluded. Existing finite rendering success
  therefore retains its artifacts; newly tested hostile-I/O outcomes refuse.
- As-written Rust source under src and core/src, including comments/inline tests,
  is 28,249 to 28,446 lines (+197), 108 to 110 files. The two new files separate
  protocol interpretation from shared process ownership. Integration boundary
  tests are outside this source-only count. No line-reduction target is used.

Ignored custody: `target/atlas/renderer-io/` holds the preregistered measurements,
full gate/install logs, boundary/control logs and exact CLI comparison. There is
no new total runtime/deadline guarantee, descendant cleanup, native cutover,
consumer-pin change, runtime performance claim or vendor change in this outcome.

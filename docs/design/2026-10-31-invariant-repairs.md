# Index, root reading and declaration invariants

Outcome for the readability plan (`girard-xqe.9`), 2026-10-31.
Baseline: `3598a56cef02bb07c6c1f6061cfa652f9262a245`.

## Model / Replaces / Adds

Atlas captures source once and derives evidence from it. Correlated index tables
must retain their order, source facts must not hold a walk's temporary state,
and declaration meaning must not depend on diagnostic formatting. This repair
responds to independent review findings 2, 3, 4 and 7, preserving the existing
qualified and native textual readings rather than unifying their unlike meanings.

- `source_index::Index` now owns private function, boundary, root and inventory
  vectors. Borrowing slice accessors replace public mutable fields. Allocation
  order cannot be changed through those accessors; cached names retain their
  referents. Function and boundary positions belong to the index that produced
  them, not a global or cross-index identity.
- `resolve` returns `Target::InvalidFunction(position)` for an out-of-range
  caller. Program, representation and expanded-workflow consumers refuse that
  outcome; it is distinct from a supported call whose source target is unknown.
  Internal canonical lookup also returns no result for an invalid position.
- This intentionally changes the Rust field API and extends the exhaustive
  Target enum. Index readers in core representations/workflow, application
  program projections and source-context tests migrate together. Existing root
  public module/type paths and wire formats remain. Downstream Rust clients
  replace `.functions`, `.boundaries`, `.roots`, `.inventory` with borrowing
  method calls; clone a slice with `to_vec()` when ownership is needed, and
  handle the invalid-position variant explicitly.
- `RootTable::of` reads inventory, imports, declared items and item references
  through named phases. `RootTable` holds no RefCell or walk-local visited set.
  Each `Reading` borrows those immutable facts and owns its active-item set.
  Cycles, alias-depth bounds, ambiguous bindings, namespace pairs, populations,
  missing-root behavior and unjudged results retain the previous grammar.
- Scanner kinds and written visibility have explicit stable `tag`/`from_tag`
  conversions. Legacy Declaration string fields retain all serialized tags;
  checked `item_kind`/`item_visibility` accessors expose supported enums or None.
  Relationship decisions use those enums. Capture, outline and review identity
  keys no longer use Debug output to encode declaration meaning. No unsupported
  wire tag is defaulted to a supported declaration or discarded on serialization.
- Each package's test support owns temporary directories through TempDir. Core's
  inline seam and move examples use that same helper; integration tests retain
  their authored manifests, source examples and assertions. The application
  retains its pre-existing common fixture manifest. No fixture crate, production
  dependency, test-only policy or fallback scanner was added.

The new Reading type owns a real lifetime distinction; stable tag functions are
wire conversions, not another grammar. Borrowing accessors are the index's only
new storage interface. Native stage reading and qualified call resolution remain
separate because their source evidence and unknown outcomes have different
contracts. No second implementation of either is introduced.

## Evidence

- Full `just check` passes: formatting, strict workspace Clippy/tests,
  feature-disabled core Clippy/tests, warnings-denied rustdoc, renderer tests.
  Installed binaries pass `just check-install` outside their source checkout.
- A frozen Cargo fixture compares the baseline and repaired executable across
  58 CLI/help/data/refusal/render cases: exit codes, stdout, stderr and two SVGs
  are byte-identical, with no field exclusions or normalization. Five refusals
  remain refusals; 53 invocations succeed.
- Independent quality review runs the root cycle/failure test, invalid-index
  boundary test, both package lifecycle tests, privacy doctest, 22 seam tests
  and 21 move-review tests. All pass. Reuse and efficiency reviews read the full
  changed surfaces and surrounding callers; no production defect is found.
- Reviews catch two qualification gaps: the initial lazy spawn/join test was
  serial, and three inline fixture routes still used PID-based directories.
  The repaired test collects handles before joining and holds four directory
  lifetimes at a barrier. The remaining inline routes use the same RAII owner.
- Root facts are Sync; concurrent independent reads repeatedly traverse a cycle
  and an unresolved root item. Removing the cycle guard makes that test red.
  Removing the invalid-index guard makes its public boundary test panic/red.
  Both knockouts are restored. Two compile-fail doctests reject direct field
  mutation and mutation through a borrowing accessor.
- Every supported kind/visibility tag round-trips against its exact schema-1
  spelling. Unknown kind and visibility tags survive an actual Declaration
  deserialize/serialize round trip with neither acquiring supported meaning.
- Parallel fixture lifetimes, normal drop and panic unwinding are exercised in
  each package. Existing capture-after-directory-deletion assertions remain.

Exact commands, binaries, frozen fixture, stdout/stderr comparisons, knockout
logs and gate logs are under ignored `target/atlas/invariant-repairs/`.
No project pin changes or native policy decisions belong to this repair.

- The accepted Anneal wrapper at `20e52661` is rebuilt in an owned copy with only
  its atlas-core source redirected to this repair. Its 21 tests pass, and all
  40 native command cases match the accepted binary exactly (stdout, stderr,
  exit; fixed source at `2b8e3ba4`). Six full-field product comparisons over the
  frozen own/Murail corpora also match byte-for-byte: handle, edge and span,
  no exclusions. The actual Anneal repository, its pin and accepted receipts
  are unchanged. These are local compatibility receipts, not native publication
  or Nix/release qualification.
- As-written Rust source under both src trees, including comments/inline tests,
  grows from 28,009 to 28,249 lines across the same 108 files (+240). Explicit
  conversion mappings, API docs and boundary tests account for the growth;
  fixtures and RootTable become clearer without a line-reduction target.
  Integration test/support files are not included in that source-only count.

No timing, new renderer transport, native Nix build or consumer-pin adoption was
run here; renderer bounds and final consumer release have separate outcomes.
